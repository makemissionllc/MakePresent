use crate::logging::{Level, LogEntry};
use crate::project::{
    is_first_run, now_iso, Background, BroadcastView, ClientState, Library, LibrarySlide,
    BoxGeometry, LibrarySong, Look, OutputView, Overlay, PlaylistTemplate, Positioning, Project,
    Settings, Slide, StageView, TemplateItem, TextPosition, Transition, write_settings,
};
use crate::scripture::ScriptureMatch;
use crate::state::AppState;
use crate::triggers::TriggerAction;
use crate::windows::{self, DisplayInfo};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

fn snapshot(app: &AppHandle) -> ClientState {
    let state = app.state::<AppState>();
    let settings = state.current_settings();

    // Single consistent read — cloning once avoids a torn view where `live`
    // changes between the `current`/`next` lookups and the final `project`
    // clone, and also halves lock acquisitions on the hot path (every mutate).
    let (project_snapshot, current, next, on_deck, looks) = {
        let project = state.project.read().unwrap();
        let current = project
            .live
            .as_deref()
            .and_then(|id| project.find(id))
            .cloned();
        let next = project
            .live
            .as_deref()
            .and_then(|id| project.next_slide(id))
            .cloned();
        let on_deck = project.on_deck().cloned();
        let looks = project.looks.clone();
        let cloned = project.clone();
        (cloned, current, next, on_deck, looks)
    };

    let snap = ClientState {
        project: project_snapshot,
        notice: state.notice.read().unwrap().clone(),
        output: OutputView {
            visible: windows::output_visible(app),
            monitor_index: settings.output_display_index,
            monitor_name: settings.output_display_name,
            fullscreen: settings.output_fullscreen,
        },
        stage: StageView {
            visible: settings.stage_visible,
            monitor_index: settings.stage_display_index,
            monitor_name: settings.stage_display_name,
        },
        broadcast: BroadcastView {
            enabled: settings.ndi_enabled,
            source_name: crate::broadcast::NDI_SOURCE_NAME.to_string(),
        },
        first_run: is_first_run(&state.app_data_dir()),
        default_transition: settings.default_transition,
        current,
        next,
        on_deck,
        looks,
        output_look_id: settings.output_look_id,
        stage_look_id: settings.stage_look_id,
        ndi_look_id: settings.ndi_look_id,
        midi_enabled: settings.midi_enabled,
        midi_device_id: settings.midi_device_id,
        osc_enabled: settings.osc_enabled,
        osc_port: settings.osc_port,
        triggers: settings.triggers,
        stage_network_enabled: settings.stage_network_enabled,
        stage_network_port: settings.stage_network_port,
        stage_message: state.stage_message.read().unwrap().clone(),
        overlay: state.overlay.read().unwrap().clone(),
        audio: state.audio.get_status(),
    };

    // Keep connected phones/tablets in lock-step with the desktop windows: after
    // every state rebuild (mutation, trigger advance, IPC snapshot), push the
    // Stage Display snapshot to any local-network WebSocket clients. No-op when
    // the server is off.
    state.network.broadcast(&crate::network::stage_broadcast(app));

    snap
}

fn log(app: &AppHandle, level: Level, message: &str) {
    app.state::<AppState>().logger.log(level, message);
}

/// Snapshot the current state and broadcast it to every window.
/// Used by the self-healing handler after recreating the Output window so the
/// frontend immediately renders the live slide.
pub fn snapshot_and_emit(app: &AppHandle) -> ClientState {
    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    snap
}

// ---------------------------------------------------------------------------
// Per-slide auto-advance timer — backend-driven (dumb-renderer principle).
// When a live slide has `auto_advance_secs = Some(n)`, the backend spawns a
// thread that sleeps N seconds and then advances to the next playlist item via
// the same `make_live` path the UI/triggers use. The generation counter on
// AppState cancels any previous timer when the operator manually advances.
// ---------------------------------------------------------------------------

/// Cancel any pending auto-advance timer by bumping the generation.
fn cancel_auto_advance(app: &AppHandle) {
    app.state::<AppState>().bump_auto_advance();
    log(app, Level::Info, "auto-advance: cancelled");
}

/// Schedule an auto-advance for the given live slide when `secs` is Some and >0.
/// Bumps the generation so any previous timer is cancelled; the new timer captures
/// this generation and checks it after sleeping.
fn schedule_auto_advance(app: &AppHandle, live_id: &str, secs: u64) {
    if secs == 0 {
        cancel_auto_advance(app);
        return;
    }
    let live_id_owned = live_id.to_string();
    let gen = app.state::<AppState>().bump_auto_advance();
    let app_clone = app.clone();
    log(
        app,
        Level::Info,
        &format!("auto-advance: scheduled {live_id_owned} -> next in {secs}s (gen {gen})"),
    );
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        let state = app_clone.state::<AppState>();
        if state.current_auto_advance_gen() != gen {
            // Cancelled by a manual advance / clear / new timer.
            return;
        }
        // Still the same live slide? Re-check under lock.
        let still_live = { state.project.read().unwrap().live.clone() };
        if still_live.as_deref() != Some(live_id_owned.as_str()) {
            return;
        }
        // Verify the slide still has the same duration (in case it was edited to None mid-sleep).
        let current_secs = {
            let project = state.project.read().unwrap();
            project
                .find(&live_id_owned)
                .and_then(|s| s.auto_advance_secs)
        };
        if current_secs != Some(secs) {
            return;
        }
        // Find next playlist index and advance, or log at end.
        let next_id: Option<String> = {
            let project = state.project.read().unwrap();
            project
                .live
                .as_deref()
                .and_then(|id| project.next_slide(id))
                .map(|s| s.id.clone())
        };
        if let Some(next) = next_id {
            log(
                &app_clone,
                Level::Info,
                &format!("auto-advance: {live_id_owned} -> {next} after {secs}s"),
            );
            // Reuse make_live so the same window-reveal + broadcast + next-timer logic runs.
            let _ = make_live(&app_clone, &next);
        } else {
            log(
                &app_clone,
                Level::Info,
                &format!("auto-advance: at end of playlist (live {live_id_owned}), staying"),
            );
        }
    });
}

/// After any operation that changes what is live, (re)schedule or cancel the
/// auto-advance timer based on the current live slide's `auto_advance_secs`.
#[allow(dead_code)]
fn reschedule_auto_advance(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (live_id, secs) = {
        let project = state.project.read().unwrap();
        let live = project.live.clone();
        let secs = live
            .as_deref()
            .and_then(|id| project.find(id))
            .and_then(|s| s.auto_advance_secs);
        (live, secs)
    };
    match (live_id, secs) {
        (Some(id), Some(s)) if s > 0 => schedule_auto_advance(app, &id, s),
        _ => cancel_auto_advance(app),
    }
}

/// Apply a mutation to the single source of truth, schedule an autosave,
/// then broadcast the resulting state to every window.
fn mutate<R>(
    app: &AppHandle,
    f: impl FnOnce(&mut Project) -> Result<R, String>,
) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        f(&mut project)?;
        project.modified_at = now_iso();
    }
    state.request_save();
    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

fn replace_project(app: &AppHandle, project: Project) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    *state.project.write().unwrap() = project;
    state.set_notice(None);
    state.request_save();
    cancel_auto_advance(app);
    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> ClientState {
    snapshot(&app)
}

#[tauri::command]
pub fn get_library(app: AppHandle) -> Library {
    app.state::<AppState>().library.read().unwrap().clone()
}

/// Broadcast the current library to every window.
fn broadcast_library(app: &AppHandle) -> Library {
    let library = app.state::<AppState>().library.read().unwrap().clone();
    let _ = app.emit("library", &library);
    library
}

#[derive(Deserialize)]
pub struct LibrarySlideInput {
    title: String,
    body: String,
    #[serde(default)]
    positioning: Option<crate::project::SlidePositioning>,
    #[serde(default)]
    group_id: Option<String>,
    #[serde(default)]
    group_label: Option<String>,
}

#[tauri::command]
pub fn add_library_song(
    app: AppHandle,
    title: String,
    body: Option<String>,
    background: Option<Background>,
    slides: Option<Vec<LibrarySlideInput>>,
) -> Result<Library, String> {
    let state = app.state::<AppState>();
    let (blocks, arrangement) = if let Some(inputs) = slides {
        if inputs.is_empty() {
            return Err("no slides provided".to_string());
        }
        let mut blocks: std::collections::HashMap<String, LibrarySlide> = std::collections::HashMap::new();
        let mut arrangement: Vec<String> = Vec::new();
        for input in inputs {
            let base_key = if !input.title.trim().is_empty() {
                input.title.clone()
            } else {
                format!("Verse {}", arrangement.len() + 1)
            };
            let mut key = base_key.clone();
            if let Some(existing) = blocks.get(&key) {
                if existing.body != input.body {
                    let mut counter = 2;
                    let mut new_key = format!("{} ({})", key, counter);
                    while blocks.contains_key(&new_key) {
                        counter += 1;
                        new_key = format!("{} ({})", key, counter);
                    }
                    key = new_key;
                }
            }
            if !blocks.contains_key(&key) {
                blocks.insert(
                    key.clone(),
                    LibrarySlide {
                        id: Uuid::new_v4().to_string(),
                        title: key.clone(),
                        body: input.body.clone(),
                        positioning: input.positioning.clone(),
                        group_id: input.group_id.clone().or_else(|| Some(format!("block-{}", blocks.len() + 1))),
                        group_label: input.group_label.clone().or_else(|| Some(key.clone())),
                    },
                );
            }
            arrangement.push(key);
        }
        (blocks, arrangement)
    } else {
        let mut blocks: std::collections::HashMap<String, LibrarySlide> = std::collections::HashMap::new();
        let key = "Verse 1".to_string();
        let slide = LibrarySlide {
            id: Uuid::new_v4().to_string(),
            title: key.clone(),
            body: body.unwrap_or_default(),
            positioning: None,
            group_id: Some("verse-1".to_string()),
            group_label: Some(key.clone()),
        };
        blocks.insert(key.clone(), slide);
        (blocks, vec![key])
    };
    let song = LibrarySong {
        id: Uuid::new_v4().to_string(),
        title,
        default_background: background.unwrap_or_default(),
        blocks,
        arrangement,
        slides: None,
    };
    state.library.write().unwrap().songs.push(song);
    state.request_save();
    Ok(broadcast_library(&app))
}

#[tauri::command]
pub fn delete_library_song(app: AppHandle, song_id: String) -> Result<Library, String> {
    let state = app.state::<AppState>();
    {
        let mut library = state.library.write().unwrap();
        if !library.songs.iter().any(|s| s.id == song_id) {
            return Err(format!("library song {song_id} not found"));
        }
        library.songs.retain(|s| s.id != song_id);
    }
    state.request_save();
    Ok(broadcast_library(&app))
}

/// Update a song's default arrangement (e.g. ["Verse 1", "Chorus", "Verse 2", "Chorus"]).
/// The arrangement may repeat block keys (extra Chorus) and order matters — it is flattened at queue-time.
/// Validates that every key exists in `song.blocks`; returns the updated Library.
#[tauri::command]
pub fn set_song_arrangement(
    app: AppHandle,
    song_id: String,
    arrangement: Vec<String>,
) -> Result<Library, String> {
    if arrangement.is_empty() {
        return Err("arrangement must not be empty".to_string());
    }
    let state = app.state::<AppState>();
    {
        let mut library = state.library.write().unwrap();
        let song = library
            .songs
            .iter_mut()
            .find(|s| s.id == song_id)
            .ok_or_else(|| format!("library song {song_id} not found"))?;
        for key in &arrangement {
            if !song.blocks.contains_key(key) {
                return Err(format!("block \"{key}\" not found in song \"{}\"", song.title));
            }
        }
        song.arrangement = arrangement;
        // Keep deprecated slides in sync? No — slides stays None after migration.
    }
    state.request_save();
    Ok(broadcast_library(&app))
}

/// Local parsers for .pro (ProPresenter export via quick-xml), .cho/.chordpro (ChordPro text), and CCLI USR text.
/// Dragging a file onto the Library creates a new song with verses parsed locally — no cloud calls.
/// Conservatively extracts title + text into the existing library.json structure; does not preserve
/// ProPresenter styling/backgrounds. Malformed files return a clear Err rather than failing silently.
#[tauri::command]
pub async fn import_song_file(app: AppHandle, path: String) -> Result<Library, String> {
    let p = PathBuf::from(path.clone());
    let parsed = tauri::async_runtime::spawn_blocking(move || crate::song_import::import_song_file(&p))
        .await
        .map_err(|e| format!("import task failed: {e}"))??;
    let song = crate::song_import::parsed_to_library_song(parsed);
    let display_title = song.title.clone();
    let state = app.state::<AppState>();
    state.library.write().unwrap().songs.push(song);
    state.request_save();
    log(
        &app,
        Level::Info,
        &format!("library: imported \"{}\" from {}", display_title, path),
    );
    Ok(broadcast_library(&app))
}

/// Copy every verse/section of a library song into the playlist as linked
/// slides (one click from the editor). Flattens the song's `arrangement`
/// (master-block architecture) into a linear list — same end result as the
/// old flat `slides` list, but the underlying data is no longer duplicated.
#[tauri::command]
pub fn add_song_to_playlist(app: AppHandle, song_id: String) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let song = state
        .library
        .read()
        .unwrap()
        .songs
        .iter()
        .find(|s| s.id == song_id)
        .cloned()
        .ok_or_else(|| format!("library song {song_id} not found"))?;

    // Flatten arrangement -> blocks (v2) or fallback to deprecated slides (v1 pre-migration)
    let flattened: Vec<LibrarySlide> = song
        .flattened_slides()
        .into_iter()
        .cloned()
        .collect();

    mutate(&app, |project| {
        for slide in &flattened {
            project.slides.push(Slide {
                id: Uuid::new_v4().to_string(),
                library_id: Some(song.id.clone()),
                library_slide_id: Some(slide.id.clone()),
                title: slide.title.clone(),
                body: slide.body.clone(),
                background: song.default_background.clone(),
                auto_advance_secs: None,
            });
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_live_slide(app: AppHandle, slide_id: String) -> Result<ClientState, String> {
    make_live(&app, &slide_id)
}

/// Shared "put slide N live" path. This is *the* single slide-advance routine:
/// the UI commands (`set_live_slide`) and the external trigger system
/// (`execute_action`) both funnel through it so one piece of logic owns
/// mutation + window reveal + state broadcast.
fn make_live(app: &AppHandle, slide_id: &str) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let live_title = {
        let mut project = state.project.write().unwrap();
        let slide = project
            .find(slide_id)
            .cloned()
            .ok_or_else(|| format!("slide {slide_id} not found"))?;
        project.live = Some(slide_id.to_string());
        project.selected = Some(slide_id.to_string());
        project.show_text = true;
        project.show_background = true;
        project.modified_at = now_iso();
        slide.title
    };
    state.request_save();

    log(app, Level::Info, &format!("output: live slide \"{live_title}\""));

    // The output window appears on demand: the first slide going live is
    // what creates and shows it (never at startup).
    if let Err(e) = windows::show_output(app, &state) {
        log(app, Level::Error, &format!("output: could not show window: {e}"));
    }

    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    // Per-slide auto-advance: backend-driven timer (dumb-renderer principle).
    // Capture the live slide's duration after broadcast so the timer thread
    // can advance via the same `make_live` path any manual click would use.
    {
        let secs = {
            let project = state.project.read().unwrap();
            project.find(slide_id).and_then(|s| s.auto_advance_secs)
        };
        match secs {
            Some(s) if s > 0 => schedule_auto_advance(app, slide_id, s),
            _ => cancel_auto_advance(app),
        }
    }
    Ok(snap)
}

#[tauri::command]
pub fn clear_output(app: AppHandle) -> Result<ClientState, String> {
    do_clear_output(&app)
}

/// Shared "blank the output" path used by the UI and by trigger actions.
fn do_clear_output(app: &AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        project.live = None;
        project.show_text = true;
        project.show_background = true;
        project.modified_at = now_iso();
    }
    state.request_save();
    cancel_auto_advance(app);
    log(app, Level::Info, "output: cleared (black)");
    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn clear_text(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        project.show_text = false;
        project.modified_at = now_iso();
    }
    state.request_save();
    log(&app, Level::Info, "output: text cleared (background kept)");
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn clear_background(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        project.show_background = false;
        project.modified_at = now_iso();
    }
    state.request_save();
    log(&app, Level::Info, "output: background cleared (text on black)");
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

/// Resolve and run a trigger action through the shared command path.
/// This is what the MIDI and OSC listeners call so that hardware triggers
/// advance/clear slides identically to a manual click.
pub fn execute_action(app: &AppHandle, action: &TriggerAction) -> Result<ClientState, String> {
    match action {
        TriggerAction::NextSlide => advance(app, 1),
        TriggerAction::PrevSlide => advance(app, -1),
        TriggerAction::JumpTo { index } => {
            let state = app.state::<AppState>();
            let project = state.project.read().unwrap();
            let id = project
                .slides
                .get(*index as usize)
                .map(|s| s.id.clone())
                .ok_or_else(|| {
                    format!(
                        "trigger: jump to slide {} — only {} slides in the playlist",
                        index + 1,
                        project.slides.len()
                    )
                })?;
            drop(project);
            make_live(app, &id)
        }
        TriggerAction::ClearOutput => do_clear_output(app),
    }
}

/// Move the live slide forward (`+1`) or backward (`-1`) within the playlist,
/// reusing the exact same `make_live` path as clicking a slide. At the start
/// (nothing live) it cues the first slide in the requested direction. Stays
/// clamped at the ends and logs rather than erroring on a boundary.
fn advance(app: &AppHandle, delta: isize) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let live_slide_id = state.project.read().unwrap().live.clone();
    let slides = state.project.read().unwrap().slides.clone();
    if slides.is_empty() {
        log(
            app,
            Level::Warn,
            "trigger: no slides in the playlist — nothing to advance to",
        );
        return Ok(snapshot(app));
    }

    let target_id = match live_slide_id.as_deref() {
        None => {
            // Nothing live yet: go to the first (delta >= 0) or last (delta < 0).
            if delta >= 0 {
                slides.first().map(|s| s.id.clone())
            } else {
                slides.last().map(|s| s.id.clone())
            }
        }
        Some(live_id) => {
            let idx = slides.iter().position(|s| s.id == live_id).unwrap_or(0);
            let next = idx as isize + delta;
            if next < 0 || next >= slides.len() as isize {
                log(
                    app,
                    Level::Info,
                    &format!(
                        "trigger: already at the {} edge of the playlist — staying put",
                        if delta > 0 { "end" } else { "start" }
                    ),
                );
                return Ok(snapshot(app));
            }
            slides.get(next as usize).map(|s| s.id.clone())
        }
    };

    match target_id {
        Some(id) => make_live(app, &id),
        None => {
            log(app, Level::Warn, "trigger: could not resolve a slide to advance to");
            Ok(snapshot(app))
        }
    }
}

/// Per-project Output transition: "cut" (default) or "fade".
#[tauri::command]
pub fn set_transition(app: AppHandle, transition: Transition) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        project.transition = transition;
        project.modified_at = now_iso();
    }
    state.request_save();
    log(
        &app,
        Level::Info,
        &format!("project: transition set to {}", transition_value(transition)),
    );
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

fn transition_value(t: Transition) -> &'static str {
    match t {
        Transition::Cut => "cut",
        Transition::Fade => "fade",
    }
}

/// A thin patch for editing one Look. All fields optional; only the provided
/// ones are applied, matching how `update_slide` works for the editor's
/// optimistic input handling.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookPatch {
    pub name: Option<String>,
    pub title_size: Option<u32>,
    pub body_size: Option<u32>,
    pub title_font: Option<String>,
    pub body_font: Option<String>,
    pub text_color: Option<String>,
    pub show_background: Option<bool>,
    pub text_position: Option<TextPosition>,
    pub positioning: Option<Positioning>,
    pub title_box: Option<BoxGeometry>,
    pub body_box: Option<BoxGeometry>,
}

/// Create a new Look (when `look_id` is None) or update an existing one with
/// the given patch. Lives on the Project so it is saved with autosave.
#[tauri::command]
pub fn upsert_look(
    app: AppHandle,
    look_id: Option<String>,
    patch: LookPatch,
) -> Result<ClientState, String> {
    mutate(&app, |project| {
        let new_id = look_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        match project.looks.iter_mut().find(|l| l.id == new_id) {
            Some(look) => {
                apply_look_patch(look, patch);
            }
            None => {
                if look_id.is_none() {
                    let mut name = patch.name.clone().unwrap_or_else(|| "New Look".to_string());
                    // Ensure unique, non-empty names so the dropdowns read well.
                    if name.trim().is_empty() {
                        name = "New Look".to_string();
                    }
                    let mut look = Look::main_default();
                    look.id = new_id;
                    look.name = name;
                    apply_look_patch(&mut look, patch);
                    project.looks.push(look);
                } else {
                    return Err(format!("look {new_id} not found"));
                }
            }
        }
        Ok(())
    })
    .map(|s| {
        log(
            &app,
            Level::Info,
            &format!("look: upserted look \"{}\"", look_id.unwrap_or_else(|| "new".to_string())),
        );
        s
    })
}

fn apply_look_patch(look: &mut Look, patch: LookPatch) {
    if let Some(name) = patch.name {
        look.name = name;
    }
    if let Some(title_size) = patch.title_size {
        look.title_size = title_size.clamp(16, 300);
    }
    if let Some(body_size) = patch.body_size {
        look.body_size = body_size.clamp(16, 300);
    }
    if let Some(title_font) = patch.title_font {
        if !title_font.trim().is_empty() {
            look.title_font = title_font;
        }
    }
    if let Some(body_font) = patch.body_font {
        if !body_font.trim().is_empty() {
            look.body_font = body_font;
        }
    }
    if let Some(text_color) = patch.text_color {
        if !text_color.trim().is_empty() {
            look.text_color = text_color;
        }
    }
    if let Some(show_background) = patch.show_background {
        look.show_background = show_background;
    }
    if let Some(text_position) = patch.text_position {
        look.text_position = text_position;
    }
    if let Some(positioning) = patch.positioning {
        look.positioning = positioning;
    }
    if let Some(title_box) = patch.title_box {
        look.title_box = clamp_box(title_box);
    }
    if let Some(body_box) = patch.body_box {
        look.body_box = clamp_box(body_box);
    }
}

/// Clamp a bounding box's geometry to valid percent ranges so the renderer can
/// always translate it into sane absolute CSS.
fn clamp_box(mut b: BoxGeometry) -> BoxGeometry {
    b.x = b.x.clamp(0.0, 100.0);
    b.y = b.y.clamp(0.0, 100.0);
    b.width = b.width.clamp(5.0, 100.0);
    b.height = b.height.clamp(5.0, 100.0);
    b.z_index = b.z_index.min(100);
    b
}

/// Delete a Look. Outputs still mapped to it fall back to the first remaining
/// look rather than rendering un-styled.
#[tauri::command]
pub fn delete_look(app: AppHandle, look_id: String) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        if !project.looks.iter().any(|l| l.id == look_id) {
            return Err(format!("look {look_id} not found"));
        }
        project.looks.retain(|l| l.id != look_id);
        project.ensure_default_looks();
        project.modified_at = now_iso();
    }
    state.request_save();
    // Point any output that referenced the deleted look at the default.
    {
        let settings = state.current_settings();
        let looks = state.project.read().unwrap().looks.clone();
        let first_id = looks.first().map(|l| l.id.clone());
        let mut settings = settings;
        if settings.output_look_id.as_deref() == Some(look_id.as_str()) {
            settings.output_look_id = first_id.clone();
        }
        if settings.stage_look_id.as_deref() == Some(look_id.as_str()) {
            settings.stage_look_id = first_id.clone();
        }
        if settings.ndi_look_id.as_deref() == Some(look_id.as_str()) {
            settings.ndi_look_id = first_id;
        }
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    log(&app, Level::Info, &format!("look: deleted look \"{look_id}\""));
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

/// Assign a Look to the main Output window. Stored in per-machine settings, so
/// it is not part of the shared project file.
#[tauri::command]
pub fn set_output_look(app: AppHandle, look_id: Option<String>) -> Result<ClientState, String> {
    set_look_mapping(&app, "output", look_id)
}

/// Assign a Look to the Stage Display window. Stored in per-machine settings.
#[tauri::command]
pub fn set_stage_look(app: AppHandle, look_id: Option<String>) -> Result<ClientState, String> {
    set_look_mapping(&app, "stage", look_id)
}

/// Assign a Look to the NDI broadcast feed. Stored in per-machine settings.
#[tauri::command]
pub fn set_ndi_look(app: AppHandle, look_id: Option<String>) -> Result<ClientState, String> {
    set_look_mapping(&app, "ndi", look_id)
}

fn set_look_mapping(
    app: &AppHandle,
    target: &str,
    look_id: Option<String>,
) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    if let Some(id) = &look_id {
        let exists = state
            .project
            .read()
            .unwrap()
            .find_look(id)
            .is_some();
        if !exists {
            return Err(format!("look {id} not found"));
        }
    }
    {
        let mut settings = state.current_settings();
        match target {
            "output" => settings.output_look_id = look_id.clone(),
            "stage" => settings.stage_look_id = look_id.clone(),
            "ndi" => settings.ndi_look_id = look_id.clone(),
            _ => unreachable!("unknown look target"),
        }
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    log(
        app,
        Level::Info,
        &format!("look: {target} mapped to look {}", look_id.unwrap_or_else(|| "none".to_string())),
    );
    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

/// Turn NDI broadcast on or off. Enabling starts the runtime-loaded NDI sender
/// on its own thread (never the render loop); disabling tears it down.
#[tauri::command]
pub fn set_ndi_enabled(app: AppHandle, enabled: bool) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut settings = state.current_settings();
        settings.ndi_enabled = enabled;
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }

    if enabled {
        match state.broadcaster.start(crate::broadcast::NDI_SOURCE_NAME) {
            Ok(()) => log(
                &app,
                Level::Info,
                &format!("ndi: broadcast enabled — source \"{}\"", crate::broadcast::NDI_SOURCE_NAME),
            ),
            Err(e) => {
                log(&app, Level::Error, &format!("ndi: could not enable broadcast: {e}"));
                return Err(format!("could not enable NDI broadcast: {e}"));
            }
        }
    } else {
        state.broadcaster.stop();
        log(&app, Level::Info, "ndi: broadcast disabled");
    }

    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn new_project(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let default_transition = state.current_settings().default_transition;
    let mut project = Project::new("First Service");
    project.transition = default_transition;
    if let Some(first) = project.slides.first() {
        project.selected = Some(first.id.clone());
    }
    log(&app, Level::Info, "project: created new project");
    replace_project(&app, project)
}

#[tauri::command]
pub fn list_presets() -> Vec<crate::project::ServicePreset> {
    crate::project::default_presets()
}

#[tauri::command]
pub fn new_project_from_preset(
    app: AppHandle,
    preset_id: String,
    title: String,
    aspect: Option<String>,
    theme: Option<String>,
    transition: Option<String>,
) -> Result<ClientState, String> {
    let presets = crate::project::default_presets();
    let preset = presets.iter().find(|p| p.id == preset_id).cloned().unwrap_or(crate::project::ServicePreset {
        id: "blank".to_string(),
        name: "Custom".to_string(),
        category: "Custom".to_string(),
        description: "".to_string(),
        default_aspect: "16:9".to_string(),
        playlist_items: vec![],
    });
    let name = if title.trim().is_empty() { preset.name.clone() } else { title.trim().to_string() };
    let aspect_val = aspect.unwrap_or(preset.default_aspect.clone());
    let trans: crate::project::Transition = match transition.as_deref() {
        Some("fade") | Some("Fade 300ms") | Some("Dissolve") => crate::project::Transition::Fade,
        _ => crate::project::Transition::Cut,
    };
    let mut project = crate::project::Project::from_preset(&name, &aspect_val, trans, &preset);
    // Theme maps to look selection for now — keep default looks, optionally tweak
    if let Some(t) = theme {
        if t.to_lowercase().contains("lower third") {
            // Move text to bottom for lower third
            for look in &mut project.looks {
                look.text_position = crate::project::TextPosition::Bottom;
            }
        } else if t.to_lowercase().contains("gradient") {
            for look in &mut project.looks {
                look.show_background = true;
            }
        }
    }
    if let Some(first) = project.slides.first() {
        project.selected = Some(first.id.clone());
    }
    log(&app, Level::Info, &format!("project: created from preset '{}' as '{}'", preset_id, name));
    replace_project(&app, project)
}

// ---------------------------------------------------------------------------
// Playlist templates — save current playlist as reusable template and load
// a template into the current project. Templates store slide references
// (title/body/background/library refs) not duplicated bytes. Persisted in
// templates.json with atomic writes (temp + rename + sync_all) mirroring
// project.json / library.json.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_templates(app: AppHandle) -> Vec<PlaylistTemplate> {
    let data_dir = app.state::<AppState>().app_data_dir();
    crate::project::read_templates(&data_dir).templates
}

#[tauri::command]
pub fn save_template(app: AppHandle, name: String) -> Result<Vec<PlaylistTemplate>, String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("template name must not be empty".to_string());
    }
    if trimmed.len() > 80 {
        return Err("template name must be 80 characters or fewer".to_string());
    }
    let state = app.state::<AppState>();
    let data_dir = state.app_data_dir();
    let mut store = crate::project::read_templates(&data_dir);
    // Avoid duplicate names — replace existing with same name case-insensitively
    if let Some(existing) = store.templates.iter_mut().find(|t| t.name.to_lowercase() == trimmed.to_lowercase()) {
        existing.items = state
            .project
            .read()
            .unwrap()
            .slides
            .iter()
            .map(|s| TemplateItem {
                title: s.title.clone(),
                body: s.body.clone(),
                background: s.background.clone(),
                library_id: s.library_id.clone(),
                library_slide_id: s.library_slide_id.clone(),
                auto_advance_secs: s.auto_advance_secs,
            })
            .collect();
        existing.created_at = now_iso();
        log(&app, Level::Info, &format!("template: updated \"{trimmed}\" ({} slides)", existing.items.len()));
    } else {
        let items: Vec<TemplateItem> = state
            .project
            .read()
            .unwrap()
            .slides
            .iter()
            .map(|s| TemplateItem {
                title: s.title.clone(),
                body: s.body.clone(),
                background: s.background.clone(),
                library_id: s.library_id.clone(),
                library_slide_id: s.library_slide_id.clone(),
                auto_advance_secs: s.auto_advance_secs,
            })
            .collect();
        let tmpl = PlaylistTemplate {
            id: Uuid::new_v4().to_string(),
            name: trimmed.clone(),
            created_at: now_iso(),
            items,
        };
        let count = tmpl.items.len();
        store.templates.push(tmpl);
        log(&app, Level::Info, &format!("template: saved \"{trimmed}\" ({count} slides)"));
    }
    crate::project::write_templates(&data_dir, &store).map_err(|e| e.to_string())?;
    Ok(store.templates)
}

#[tauri::command]
pub fn load_template(app: AppHandle, template_id: String) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let data_dir = state.app_data_dir();
    let store = crate::project::read_templates(&data_dir);
    let tmpl = store
        .templates
        .iter()
        .find(|t| t.id == template_id)
        .cloned()
        .ok_or_else(|| format!("template {template_id} not found"))?;
    let new_slides: Vec<Slide> = tmpl
        .items
        .iter()
        .map(|it| Slide {
            id: Uuid::new_v4().to_string(),
            library_id: it.library_id.clone(),
            library_slide_id: it.library_slide_id.clone(),
            title: it.title.clone(),
            body: it.body.clone(),
            background: it.background.clone(),
            auto_advance_secs: it.auto_advance_secs,
        })
        .collect();
    let count = new_slides.len();
    let name = tmpl.name.clone();
    // Loading a template clears the live slide, so cancel any auto-advance.
    cancel_auto_advance(&app);
    mutate(&app, |project| {
        project.slides = new_slides;
        project.live = None;
        project.selected = project.slides.first().map(|s| s.id.clone());
        project.show_text = true;
        project.show_background = true;
        Ok(())
    })
    .map(|s| {
        log(&app, Level::Info, &format!("template: loaded \"{name}\" ({count} slides) into playlist"));
        s
    })
}

#[tauri::command]
pub fn delete_template(app: AppHandle, template_id: String) -> Result<Vec<PlaylistTemplate>, String> {
    let data_dir = app.state::<AppState>().app_data_dir();
    let mut store = crate::project::read_templates(&data_dir);
    let before = store.templates.len();
    store.templates.retain(|t| t.id != template_id);
    if store.templates.len() == before {
        return Err(format!("template {template_id} not found"));
    }
    crate::project::write_templates(&data_dir, &store).map_err(|e| e.to_string())?;
    log(&app, Level::Info, &format!("template: deleted {template_id}"));
    Ok(store.templates)
}

#[tauri::command]
pub fn add_slide(
    app: AppHandle,
    title: Option<String>,
    body: Option<String>,
) -> Result<ClientState, String> {
    let slide = Slide {
        id: Uuid::new_v4().to_string(),
        library_id: None,
        library_slide_id: None,
        title: title.unwrap_or_else(|| "New Slide".to_string()),
        body: body.unwrap_or_default(),
        background: Background::default(),
        auto_advance_secs: None,
    };
    let slide_title = slide.title.clone();
    mutate(&app, |project| {
        project.slides.push(slide);
        if let Some(created) = project.slides.last() {
            project.selected = Some(created.id.clone());
        }
        Ok(())
    })
    .map(|s| {
        log(&app, Level::Info, &format!("playlist: added slide \"{slide_title}\""));
        s
    })
}

#[tauri::command]
pub fn update_slide(
    app: AppHandle,
    slide_id: String,
    title: Option<String>,
    body: Option<String>,
    background: Option<Background>,
    auto_advance_secs: Option<Option<u64>>,
) -> Result<ClientState, String> {
    // Validate duration before mutating so we can give a clear error.
    if let Some(Some(secs)) = auto_advance_secs {
        if secs == 0 || secs > 86400 {
            return Err("auto-advance must be between 1 and 86400 seconds".to_string());
        }
    }
    let was_live = {
        let state = app.state::<AppState>();
        let project = state.project.read().unwrap();
        project.live.as_deref() == Some(slide_id.as_str())
    };
    let snap = mutate(&app, |project| {
        let slide = project
            .slides
            .iter_mut()
            .find(|s| s.id == slide_id)
            .ok_or_else(|| format!("slide {slide_id} not found"))?;
        if let Some(title) = title {
            slide.title = title;
        }
        if let Some(body) = body {
            slide.body = body;
        }
        if let Some(background) = background {
            if matches!(&background, Background::Solid { color } if color.trim().is_empty()) {
                return Err("background color must not be empty".to_string());
            }
            slide.background = background;
        }
        if let Some(inner) = auto_advance_secs {
            match inner {
                Some(secs) if secs > 0 => slide.auto_advance_secs = Some(secs),
                _ => slide.auto_advance_secs = None,
            }
        }
        Ok(())
    })
    .map(|s| {
        log(
            &app,
            Level::Info,
            &format!("playlist: updated slide \"{slide_id}\""),
        );
        s
    })?;
    // If the live slide's timer was edited, reschedule (backend-driven).
    if was_live {
        if let Some(inner) = auto_advance_secs {
            match inner {
                Some(secs) if secs > 0 => schedule_auto_advance(&app, &slide_id, secs),
                _ => cancel_auto_advance(&app),
            }
        }
    }
    Ok(snap)
}

#[tauri::command]
pub fn delete_slide(app: AppHandle, slide_id: String) -> Result<ClientState, String> {
    let was_live = {
        let state = app.state::<AppState>();
        let project = state.project.read().unwrap();
        project.live.as_deref() == Some(slide_id.as_str())
    };
    let snap = mutate(&app, |project| {
        if !project.slides.iter().any(|s| s.id == slide_id) {
            return Err(format!("slide {slide_id} not found"));
        }
        project.slides.retain(|s| s.id != slide_id);
        if project.live.as_deref() == Some(slide_id.as_str()) {
            project.live = None;
        }
        Ok(())
    })
    .map(|s| {
        log(&app, Level::Info, &format!("playlist: deleted slide \"{slide_id}\""));
        s
    })?;
    if was_live {
        cancel_auto_advance(&app);
    }
    Ok(snap)
}

#[tauri::command]
pub fn reorder_slide(app: AppHandle, slide_id: String, new_index: usize) -> Result<ClientState, String> {
    mutate(&app, |project| {
        let old_pos = project
            .slides
            .iter()
            .position(|s| s.id == slide_id)
            .ok_or_else(|| format!("slide {slide_id} not found"))?;
        let slide = project.slides.remove(old_pos);
        let clamped = new_index.min(project.slides.len());
        project.slides.insert(clamped, slide);
        Ok(())
    })
    .map(|s| {
        log(&app, Level::Info, &format!("playlist: reordered slide \"{slide_id}\" to {new_index}"));
        s
    })
}

#[tauri::command]
pub fn reorder_slides(app: AppHandle, ordered_ids: Vec<String>) -> Result<ClientState, String> {
    mutate(&app, |project| {
        if ordered_ids.len() != project.slides.len() {
            return Err(format!(
                "ordered_ids length {} does not match slides length {}",
                ordered_ids.len(),
                project.slides.len()
            ));
        }
        let mut id_to_slide: std::collections::HashMap<String, Slide> = project
            .slides
            .drain(..)
            .map(|s| (s.id.clone(), s))
            .collect();
        let mut reordered = Vec::with_capacity(ordered_ids.len());
        for id in ordered_ids {
            let slide = id_to_slide
                .remove(&id)
                .ok_or_else(|| format!("slide {id} not found"))?;
            reordered.push(slide);
        }
        if !id_to_slide.is_empty() {
            return Err("ordered_ids missing some slides".to_string());
        }
        project.slides = reordered;
        Ok(())
    })
    .map(|s| {
        log(&app, Level::Info, "playlist: reordered slides");
        s
    })
}

#[tauri::command]
pub fn list_displays(app: AppHandle) -> Result<Vec<DisplayInfo>, String> {
    windows::list_displays(&app)
}

#[tauri::command]
pub fn set_output_display(app: AppHandle, index: usize) -> Result<Vec<DisplayInfo>, String> {
    let window = windows::move_output_to(&app, index)?;

    let state = app.state::<AppState>();
    let mut settings = state.current_settings();
    settings.output_display_index = Some(index);
    settings.output_display_name = windows::list_displays(&app).ok().and_then(|displays| {
        displays
            .into_iter()
            .find(|d| d.index == index)
            .and_then(|d| (!d.name.is_empty()).then_some(d.name))
    });
    state.apply_settings(settings);
    let _ = write_settings(&state.app_data_dir(), &state.current_settings());

    if state.current_settings().output_fullscreen {
        window.set_fullscreen(true).map_err(|e| e.to_string())?;
    }

    log(&app, Level::Info, &format!("output: display set to monitor {index}"));
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    windows::list_displays(&app)
}

#[tauri::command]
pub fn toggle_output_fullscreen(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    // The output may not exist yet; toggling fullscreen also reveals it.
    if !windows::output_visible(&app) {
        if let Err(e) = windows::show_output(&app, &state) {
            log(&app, Level::Error, &format!("output: could not show window: {e}"));
        }
    }
    let window = windows::ensure_output(&app)?;
    let next = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window.set_decorations(!next).map_err(|e| e.to_string())?;
    window.set_fullscreen(next).map_err(|e| e.to_string())?;

    let mut settings = state.current_settings();
    settings.output_fullscreen = next;
    state.apply_settings(settings);
    let _ = write_settings(&state.app_data_dir(), &state.current_settings());

    log(
        &app,
        Level::Info,
        if next {
            "output: fullscreen on"
        } else {
            "output: fullscreen off"
        },
    );
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(next)
}

fn display_name(app: &AppHandle, index: usize) -> Option<String> {
    windows::list_displays(app).ok().and_then(|displays| {
        displays
            .into_iter()
            .find(|d| d.index == index)
            .and_then(|d| (!d.name.is_empty()).then_some(d.name))
    })
}

/// Move (and thereby show) the stage window onto the given display.
#[tauri::command]
pub fn set_stage_display(app: AppHandle, index: usize) -> Result<Vec<DisplayInfo>, String> {
    let window = windows::move_stage_to(&app, index)?;

    let state = app.state::<AppState>();
    let mut settings = state.current_settings();
    settings.stage_display_index = Some(index);
    settings.stage_display_name = display_name(&app, index);
    settings.stage_visible = true;
    state.apply_settings(settings);
    let _ = write_settings(&state.app_data_dir(), &state.current_settings());

    let focus_res = window.set_focus();
    log(&app, Level::Info, &format!("commands: set_stage_display: set_focus result: {:?}", focus_res));
    log(&app, Level::Info, &format!("stage: display set to monitor {index}"));
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    windows::list_displays(&app)
}

/// Turn the stage display on/off. When turning on, it picks the configured
/// display (or the best second monitor) and opens the window there.
#[tauri::command]
pub fn toggle_stage(app: AppHandle) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let settings = state.current_settings();
    let next = !settings.stage_visible;

    if next {
        let index = settings
            .stage_display_index
            .or_else(|| windows::default_output_display(&app).ok())
            .ok_or_else(|| "no display available".to_string())?;
        let mut updated = settings;
        updated.stage_visible = true;
        if updated.stage_display_index.is_none() {
            updated.stage_display_index = Some(index);
            updated.stage_display_name = display_name(&app, index);
        }
        let window = windows::move_stage_to(&app, index)?;
        let focus_res = window.set_focus();
        log(&app, Level::Info, &format!("commands: toggle_stage (on): set_focus result: {:?}", focus_res));
        state.apply_settings(updated);
    } else {
        if let Some(window) = windows::get_stage(&app) {
            window.hide().map_err(|e| e.to_string())?;
        }
        let mut updated = settings;
        updated.stage_visible = false;
        state.apply_settings(updated);
    }

    let _ = write_settings(&state.app_data_dir(), &state.current_settings());

    log(
        &app,
        Level::Info,
        if next {
            "stage: shown"
        } else {
            "stage: hidden"
        },
    );

    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(next)
}

/// Targeted stage-only message (nursery alerts, countdowns, operator notes) — never appears on Output.
/// Separate from `Project.live`; changing it never touches Output or the main live slide.
/// Manual clear is baseline; optional `duration_secs` auto-clears after N seconds (straightforward, cancellable).
#[tauri::command]
pub fn set_stage_message(
    app: AppHandle,
    message: String,
    duration_secs: Option<u64>,
) -> Result<ClientState, String> {
    let trimmed = message.trim().to_string();
    if trimmed.is_empty() {
        return Err("stage message must not be empty".to_string());
    }
    if trimmed.len() > 500 {
        return Err("stage message must be 500 characters or fewer".to_string());
    }
    let dur = duration_secs.filter(|s| *s > 0);
    if let Some(secs) = dur {
        if secs > 3600 {
            return Err("duration must be between 1 and 3600 seconds".to_string());
        }
    }
    let gen = {
        let state = app.state::<AppState>();
        {
            let mut msg = state.stage_message.write().unwrap();
            *msg = Some(trimmed.clone());
        }
        state.bump_stage_message()
    };
    log(
        &app,
        Level::Info,
        &format!(
            "stage_message: set \"{}\"{}",
            trimmed,
            dur.map(|s| format!(" (auto-clear in {s}s, gen {gen})"))
                .unwrap_or_default()
        ),
    );
    let snap = snapshot_and_emit(&app);
    if let Some(secs) = dur {
        let app_clone = app.clone();
        let msg_clone = trimmed.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            let state = app_clone.state::<AppState>();
            if state.current_stage_message_gen() != gen {
                return;
            }
            let still = { state.stage_message.read().unwrap().clone() };
            if still.as_deref() == Some(msg_clone.as_str()) {
                {
                    let mut m = state.stage_message.write().unwrap();
                    *m = None;
                }
                // Bump to invalidate any other pending auto-clear timers
                state.bump_stage_message();
                log(&app_clone, Level::Info, &format!("stage_message: auto-cleared after {secs}s"));
                let _ = snapshot_and_emit(&app_clone);
            }
        });
    }
    Ok(snap)
}

#[tauri::command]
pub fn clear_stage_message(app: AppHandle) -> Result<ClientState, String> {
    let had = {
        let state = app.state::<AppState>();
        let msg = state.stage_message.read().unwrap();
        msg.is_some()
    };
    {
        let state = app.state::<AppState>();
        let mut m = state.stage_message.write().unwrap();
        *m = None;
    }
    app.state::<AppState>().bump_stage_message();
    if had {
        log(&app, Level::Info, "stage_message: cleared");
    }
    Ok(snapshot_and_emit(&app))
}

/// Overlay layer for Output — independent of main slide/background, lower-third / logo.
/// Background at bottom, main slide in middle, overlay on top via z-index. Each layer independently toggleable.
#[tauri::command]
pub fn set_overlay(
    app: AppHandle,
    text: String,
    background: Option<Background>,
) -> Result<ClientState, String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() && background.is_none() {
        return Err("overlay must have text or image".to_string());
    }
    if trimmed.len() > 500 {
        return Err("overlay text must be 500 characters or fewer".to_string());
    }
    let overlay = Overlay {
        id: Uuid::new_v4().to_string(),
        text: trimmed.clone(),
        background,
        visible: true,
    };
    {
        let state = app.state::<AppState>();
        let mut o = state.overlay.write().unwrap();
        *o = Some(overlay);
    }
    log(&app, Level::Info, &format!("overlay: set \"{}\"", trimmed));
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn set_overlay_visible(app: AppHandle, visible: bool) -> Result<ClientState, String> {
    let has_overlay = {
        let state = app.state::<AppState>();
        let o = state.overlay.read().unwrap();
        o.is_some()
    };
    if !has_overlay {
        return Err("no overlay to show/hide — set one first".to_string());
    }
    {
        let state = app.state::<AppState>();
        let mut o = state.overlay.write().unwrap();
        if let Some(ref mut ov) = *o {
            ov.visible = visible;
        }
    }
    log(
        &app,
        Level::Info,
        &format!("overlay: {}", if visible { "shown" } else { "hidden" }),
    );
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn clear_overlay(app: AppHandle) -> Result<ClientState, String> {
    let had = {
        let state = app.state::<AppState>();
        let o = state.overlay.read().unwrap();
        o.is_some()
    };
    {
        let state = app.state::<AppState>();
        let mut o = state.overlay.write().unwrap();
        *o = None;
    }
    if had {
        log(&app, Level::Info, "overlay: cleared");
    }
    Ok(snapshot_and_emit(&app))
}

// ---------------------------------------------------------------------------
// Backing audio — single track, routable to specific device, not tied to slides
// Dedicated thread (same isolation as MIDI/OSC), never blocks main, no deadlock
// ---------------------------------------------------------------------------

/// List cpal output devices (independent of system default) for the audio player.
/// Uses `cpal::default_host().output_devices()` — same pattern as `list_displays` for video.
#[tauri::command]
pub fn list_audio_devices() -> Vec<crate::audio::AudioDeviceInfo> {
    crate::audio::list_output_devices()
}

/// Load a local audio file (MP3/WAV/FLAC via rodio) into the single backing track.
/// Does not auto-play; call `play_audio` to start. Replaces any previously loaded track.
#[tauri::command]
pub fn load_audio(app: AppHandle, path: String) -> Result<ClientState, String> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(format!("audio file not found: {}", path));
    }
    let state = app.state::<AppState>();
    state
        .audio
        .load(&p)
        .map_err(|e| format!("failed to load audio: {e}"))?;
    log(&app, Level::Info, &format!("audio: loaded \"{}\"", path));
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn play_audio(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    state
        .audio
        .play()
        .map_err(|e| format!("audio play failed: {e}"))?;
    log(&app, Level::Info, "audio: play");
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn pause_audio(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    state
        .audio
        .pause()
        .map_err(|e| format!("audio pause failed: {e}"))?;
    log(&app, Level::Info, "audio: pause");
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn stop_audio(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    state.audio.stop().map_err(|e| format!("audio stop failed: {e}"))?;
    log(&app, Level::Info, "audio: stop");
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn set_audio_volume(app: AppHandle, volume: f32) -> Result<ClientState, String> {
    if !volume.is_finite() || volume < 0.0 || volume > 1.5 {
        return Err("volume must be between 0.0 and 1.5".to_string());
    }
    let state = app.state::<AppState>();
    state
        .audio
        .set_volume(volume)
        .map_err(|e| format!("audio volume failed: {e}"))?;
    // Persist volume in Settings (same pattern as display/output device)
    {
        let mut settings = state.current_settings();
        settings.audio_volume = volume;
        state.apply_settings(settings.clone());
        let _ = write_settings(&state.app_data_dir(), &settings);
    }
    log(&app, Level::Info, &format!("audio: volume {volume:.2}"));
    Ok(snapshot_and_emit(&app))
}

#[tauri::command]
pub fn seek_audio(app: AppHandle, secs: u64) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    state
        .audio
        .seek(secs)
        .map_err(|e| format!("audio seek failed: {e}"))?;
    log(&app, Level::Info, &format!("audio: seek {secs}s"));
    Ok(snapshot_and_emit(&app))
}

/// Select which cpal output device the backing track plays through, independent of system default.
/// Stored in Settings (`audio_output_device_id`), same pattern as `output_display_index`.
#[tauri::command]
pub fn set_audio_device(app: AppHandle, device_id: Option<String>) -> Result<ClientState, String> {
    // Validate device exists if Some (allow None = system default)
    if let Some(ref id) = device_id {
        let devices = crate::audio::list_output_devices();
        if !devices.iter().any(|d| &d.id == id) {
            return Err(format!("audio device \"{id}\" not found"));
        }
    }
    let state = app.state::<AppState>();
    state
        .audio
        .set_device(device_id.clone())
        .map_err(|e| format!("audio device switch failed: {e}"))?;
    {
        let mut settings = state.current_settings();
        settings.audio_output_device_id = device_id.clone();
        state.apply_settings(settings.clone());
        let _ = write_settings(&state.app_data_dir(), &settings);
    }
    log(
        &app,
        Level::Info,
        &format!(
            "audio: output device set to {}",
            device_id.unwrap_or_else(|| "system default".to_string())
        ),
    );
    Ok(snapshot_and_emit(&app))
}

/// Explicit "Show Output" action: create and show the output window on its
/// configured (or auto-picked) display, without changing the live slide.
#[tauri::command]
pub fn show_output(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    windows::show_output(&app, &state)?;
    log(&app, Level::Info, "output: shown on demand");
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

/// Log an intentional output window close so the self-healing handler skips
/// auto-recreation. Called by the frontend when the user deliberately hides
/// the output window (e.g. end of service).
#[tauri::command]
pub fn log_output_intentionally_closed(app: AppHandle) {
    log(&app, Level::Info, "output: intentionally closed by user — self-healing disabled");
    let _ = app.emit("output-intentionally-closed", ());
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

/// Copy a picked media file into the managed cache and generate its thumbnail
/// (via ffmpeg), returning the background to assign to a slide. Runs the disk
/// work off-thread so the UI stays responsive while a large video is
/// imported/hashed.
#[tauri::command]
pub async fn import_media(app: AppHandle, path: String) -> Result<crate::media::MediaAsset, String> {
    let data_dir = app.state::<AppState>().app_data_dir();
    let source = std::path::PathBuf::from(&path);
    let copy_source = source.clone();
    let background = tauri::async_runtime::spawn_blocking(move || {
        crate::media::import(&copy_source, &data_dir)
    })
    .await
    .map_err(|e| format!("import task failed: {e}"))?
    .map_err(|e| e.to_string())?;

    let asset = crate::media::to_asset(background, &source);
    log(
        &app,
        Level::Info,
        &format!(
            "media: imported \"{}\" ({}) — {}",
            asset.file_name, asset.kind, asset.hash
        ),
    );
    Ok(asset)
}

/// Search the managed media cache (reuses `media::search_media_assets`).
/// Empty query returns up to 100 most recent; non-empty filters
/// case-insensitively by file name / hash / kind. Used by the global
/// search overlay alongside `search_scripture` and the library.
#[tauri::command]
pub fn search_media(app: AppHandle, query: String) -> Vec<crate::media::MediaAsset> {
    let data_dir = app.state::<AppState>().app_data_dir();
    crate::media::search_media_assets(&data_dir, &query)
}

/// List all cached media assets (no filter). Convenience for the overlay's
/// empty state; capped at 100.
#[tauri::command]
pub fn list_media(app: AppHandle) -> Vec<crate::media::MediaAsset> {
    let data_dir = app.state::<AppState>().app_data_dir();
    crate::media::list_media_assets(&data_dir)
        .into_iter()
        .take(100)
        .collect()
}

// ---------------------------------------------------------------------------
// Settings import/export
// ---------------------------------------------------------------------------

const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub path: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub changed_fields: Vec<String>,
    pub message: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsSnapshot {
    app: String,
    settings_schema_version: u32,
    settings: Settings,
    exported_at: String,
}

#[derive(Clone, Copy)]
enum SettingsField {
    OutputDisplay,
    OutputFullscreen,
    StageDisplay,
    StageVisible,
    DefaultTransition,
    OutputLook,
    StageLook,
    NdiLook,
    NdiEnabled,
    MidiEnabled,
    MidiDevice,
    OscEnabled,
    OscPort,
    Triggers,
    StageNetworkEnabled,
    StageNetworkPort,
    StageNetworkPin,
    AudioDevice,
    AudioVolume,
}

impl SettingsField {
    fn label(self) -> &'static str {
        match self {
            SettingsField::OutputDisplay => "Output display",
            SettingsField::OutputFullscreen => "Output fullscreen",
            SettingsField::StageDisplay => "Stage display",
            SettingsField::StageVisible => "Stage visibility",
            SettingsField::DefaultTransition => "Default transition",
            SettingsField::OutputLook => "Output look",
            SettingsField::StageLook => "Stage look",
            SettingsField::NdiLook => "NDI look",
            SettingsField::NdiEnabled => "NDI broadcast",
            SettingsField::MidiEnabled => "MIDI input",
            SettingsField::MidiDevice => "MIDI device",
            SettingsField::OscEnabled => "OSC listener",
            SettingsField::OscPort => "OSC port",
            SettingsField::Triggers => "Trigger mappings",
            SettingsField::StageNetworkEnabled => "Stage display on network",
            SettingsField::StageNetworkPort => "Stage display port",
            SettingsField::StageNetworkPin => "Stage display PIN",
            SettingsField::AudioDevice => "Audio output device",
            SettingsField::AudioVolume => "Audio volume",
        }
    }
}

fn settings_fields() -> [SettingsField; 19] {
    [
        SettingsField::OutputDisplay,
        SettingsField::OutputFullscreen,
        SettingsField::StageDisplay,
        SettingsField::StageVisible,
        SettingsField::DefaultTransition,
        SettingsField::OutputLook,
        SettingsField::StageLook,
        SettingsField::NdiLook,
        SettingsField::NdiEnabled,
        SettingsField::MidiEnabled,
        SettingsField::MidiDevice,
        SettingsField::OscEnabled,
        SettingsField::OscPort,
        SettingsField::Triggers,
        SettingsField::StageNetworkEnabled,
        SettingsField::StageNetworkPort,
        SettingsField::StageNetworkPin,
        SettingsField::AudioDevice,
        SettingsField::AudioVolume,
    ]
}

fn changed_settings(old: &Settings, new: &Settings) -> Vec<String> {
    let mut changed = Vec::new();
    for field in settings_fields() {
        let differs = match field {
            SettingsField::OutputDisplay => {
                new.output_display_index != old.output_display_index
            }
            SettingsField::OutputFullscreen => new.output_fullscreen != old.output_fullscreen,
            SettingsField::StageDisplay => new.stage_display_index != old.stage_display_index,
            SettingsField::StageVisible => new.stage_visible != old.stage_visible,
            SettingsField::DefaultTransition => {
                new.default_transition != old.default_transition
            }
            SettingsField::OutputLook => new.output_look_id != old.output_look_id,
            SettingsField::StageLook => new.stage_look_id != old.stage_look_id,
            SettingsField::NdiLook => new.ndi_look_id != old.ndi_look_id,
            SettingsField::NdiEnabled => new.ndi_enabled != old.ndi_enabled,
            SettingsField::MidiEnabled => new.midi_enabled != old.midi_enabled,
            SettingsField::MidiDevice => new.midi_device_id != old.midi_device_id,
            SettingsField::OscEnabled => new.osc_enabled != old.osc_enabled,
            SettingsField::OscPort => new.osc_port != old.osc_port,
            SettingsField::Triggers => {
                new.triggers.len() != old.triggers.len()
                    || new.triggers.iter().zip(old.triggers.iter()).any(|(a, b)| {
                        a.id != b.id
                            || a.trigger != b.trigger
                            || a.action != b.action
                            || a.enabled != b.enabled
                            || a.label != b.label
                    })
            }
            SettingsField::StageNetworkEnabled => {
                new.stage_network_enabled != old.stage_network_enabled
            }
            SettingsField::StageNetworkPort => {
                new.stage_network_port != old.stage_network_port
            }
            SettingsField::StageNetworkPin => {
                new.stage_network_pin != old.stage_network_pin
            }
            SettingsField::AudioDevice => {
                new.audio_output_device_id != old.audio_output_device_id
            }
            SettingsField::AudioVolume => {
                (new.audio_volume - old.audio_volume).abs() > f32::EPSILON
            }
        };
        if differs {
            changed.push(field.label().to_string());
        }
    }
    changed
}

/// Write the current settings (not the project or library) to the given JSON
/// path so they can be moved between machines.
///
/// Async + spawn_blocking so the write never blocks the Tauri main thread or
/// the command worker that the WebView2 message pump depends on (Windows 11 freeze guard).
#[tauri::command]
pub async fn export_settings(app: AppHandle, path: String) -> Result<ExportReport, String> {
    let state = app.state::<AppState>();
    let snapshot = SettingsSnapshot {
        app: "makepresent".to_string(),
        settings_schema_version: SETTINGS_SCHEMA_VERSION,
        settings: state.current_settings(),
        exported_at: now_iso(),
    };
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("could not serialize settings: {e}"))?;
    let path_for_write = path.clone();
    tauri::async_runtime::spawn_blocking(move || std::fs::write(&path_for_write, json))
        .await
        .map_err(|e| format!("export task failed: {e}"))?
        .map_err(|e| format!("could not write settings file: {e}"))?;
    log(&app, Level::Info, &format!("settings: exported to {path}"));
    Ok(ExportReport {
        path: path.clone(),
        fields: settings_fields()
            .iter()
            .map(|f| f.label().to_string())
            .collect(),
    })
}

/// Read a previously exported settings JSON file, validate it, apply it, and
/// report exactly what changed. Rejects mismatched versions and corrupt files
/// with a clear error instead of failing silently.
///
/// Async file read via spawn_blocking keeps the main thread / WebView2 pump responsive on Windows 11.
#[tauri::command]
pub async fn import_settings(app: AppHandle, path: String) -> Result<ImportReport, String> {
    let raw = {
        let p = path.clone();
        tauri::async_runtime::spawn_blocking(move || std::fs::read_to_string(&p))
            .await
            .map_err(|e| format!("import task failed: {e}"))?
            .map_err(|e| format!("could not read settings file: {e}"))?
    };
    let file_data: SettingsSnapshot = serde_json::from_str(&raw)
        .map_err(|e| format!("not a valid MakePresent settings file: {e}"))?;

    if file_data.app != "makepresent" {
        return Err(format!(
            "not a MakePresent settings file (app is \"{}\")",
            file_data.app
        ));
    }
    if file_data.settings_schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(format!(
            "unsupported settings schema version {} (this app supports {})",
            file_data.settings_schema_version, SETTINGS_SCHEMA_VERSION
        ));
    }

    let state = app.state::<AppState>();
    let old = state.current_settings();
    let new = file_data.settings;
    let changed = changed_settings(&old, &new);

    state.apply_settings(new);
    let _ = write_settings(&state.app_data_dir(), &state.current_settings());

    // Apply the display side effects that are safe to do immediately.
    if windows::output_visible(&app) {
        if let Some(index) = state.current_settings().output_display_index {
            if let Err(e) = windows::move_output_to(&app, index) {
                log(&app, Level::Warn, &format!("settings: moving output failed: {e}"));
            }
        }
    }
    if state.current_settings().stage_visible {
        let latest = state.current_settings();
        let index = latest
            .stage_display_index
            .or_else(|| windows::default_output_display(&app).ok());
        if let Some(index) = index {
            if let Err(e) = windows::move_stage_to(&app, index) {
                log(&app, Level::Warn, &format!("settings: moving stage failed: {e}"));
            }
        }
    }

    // Apply the imported NDI enablement (start/stop the runtime broadcaster).
    let latest = state.current_settings();
    if latest.ndi_enabled != state.broadcaster.is_active() {
        if latest.ndi_enabled {
            match state.broadcaster.start(crate::broadcast::NDI_SOURCE_NAME) {
                Ok(()) => log(&app, Level::Info, "settings: NDI broadcast started on import"),
                Err(e) => log(&app, Level::Warn, &format!("settings: NDI start on import failed: {e}")),
            }
        } else {
            state.broadcaster.stop();
            log(&app, Level::Info, "settings: NDI broadcast stopped on import");
        }
    }

    // Apply imported MIDI listener enablement (start/stop the connection).
    let latest = state.current_settings();
    if latest.midi_enabled != state.midi.is_active() {
        if latest.midi_enabled {
            if let Some(device_id) = latest.midi_device_id.clone() {
                match state.midi.start(app.clone(), &device_id) {
                    Ok(()) => log(&app, Level::Info, "settings: MIDI input started on import"),
                    Err(e) => log(&app, Level::Warn, &format!("settings: MIDI start on import failed: {e}")),
                }
            } else {
                log(&app, Level::Warn, "settings: MIDI enabled on import but no device selected");
            }
        } else {
            state.midi.stop();
            log(&app, Level::Info, "settings: MIDI input stopped on import");
        }
    }

    // Apply imported OSC enablement / port (start/stop the UDP listener).
    let latest = state.current_settings();
    if latest.osc_enabled != state.osc.is_active() || (latest.osc_enabled && latest.osc_port != 0) {
        if latest.osc_enabled {
            match state.osc.start(app.clone(), latest.osc_port) {
                Ok(()) => log(&app, Level::Info, &format!("settings: OSC listener started on UDP :{}", latest.osc_port)),
                Err(e) => log(&app, Level::Warn, &format!("settings: OSC start on import failed: {e}")),
            }
        } else {
            state.osc.stop();
            log(&app, Level::Info, "settings: OSC listener stopped on import");
        }
    }

    // Apply imported Stage Network enablement / port / PIN (start/stop the HTTP
    // + WebSocket server).
    let latest = state.current_settings();
    if latest.stage_network_enabled != state.network.is_active() {
        if latest.stage_network_enabled {
            let addr: SocketAddr = format!("0.0.0.0:{}", latest.stage_network_port)
                .parse()
                .map_err(|e| format!("invalid stage network address: {e}"))?;
            match state
                .network
                .start(app.clone(), addr, latest.stage_network_pin.clone())
            {
                Ok(()) => log(
                    &app,
                    Level::Info,
                    &format!("settings: Stage Network started on :{}", latest.stage_network_port),
                ),
                Err(e) => log(
                    &app,
                    Level::Warn,
                    &format!("settings: Stage Network start on import failed: {e}"),
                ),
            }
        } else {
            state.network.stop();
            log(&app, Level::Info, "settings: Stage Network stopped on import");
        }
    } else if latest.stage_network_enabled {
        // Already running — push the fresh PIN/state through live.
        state.network.set_pin_live(&latest.stage_network_pin);
    }

    log(
        &app,
        Level::Info,
        &format!("settings: imported from {path} ({} changed)", changed.len()),
    );

    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);

    let message = if changed.is_empty() {
        "Settings imported — nothing changed.".to_string()
    } else {
        format!(
            "Imported settings: {} changed ({}).",
            changed.len(),
            changed.join(", ")
        )
    };
    Ok(ImportReport {
        changed_fields: changed,
        message,
    })
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

/// The most recent log lines, newest first, for the Settings > Logs panel.
/// Async offload avoids blocking the WebView2 pump when the log is large (8000 lines, rotation).
#[tauri::command]
pub async fn get_logs(app: AppHandle, limit: Option<usize>) -> Vec<LogEntry> {
    let app_clone = app.clone();
    let lim = limit.unwrap_or(300);
    tauri::async_runtime::spawn_blocking(move || app_clone.state::<AppState>().logger.recent(lim))
        .await
        .unwrap_or_default()
}

/// Copy the current log file to the chosen path so it can be shared without
/// digging through the filesystem. Offloaded to blocking pool to keep UI responsive.
#[tauri::command]
pub async fn export_logs_to(app: AppHandle, path: String) -> Result<String, String> {
    let dest = std::path::PathBuf::from(path.clone());
    let app_for_log = app.clone();
    let p = dest.clone();
    tauri::async_runtime::spawn_blocking(move || app_for_log.state::<AppState>().logger.export_to(&p))
        .await
        .map_err(|e| format!("export task failed: {e}"))?
        .map_err(|e| format!("could not export log file: {e}"))?;
    log(&app, Level::Info, &format!("logs: exported to {path}"));
    Ok(path)
}

/// Search the in-memory KJV scripture index as the user types a reference,
/// returning up to 10 matches with the verse text for autocomplete.
#[tauri::command]
pub fn search_scripture(
    app: AppHandle,
    query: String,
) -> Result<Vec<ScriptureMatch>, String> {
    let state = app.state::<AppState>();
    let scripture = state.scripture.read().unwrap();
    let index = scripture
        .as_ref()
        .ok_or_else(|| "scripture index not loaded".to_string())?;
    Ok(index.search(&query, 10))
}

/// The outcome of folding imported scripture into the search index.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptureImportResult {
    /// Number of distinct books that were (re)added by this import.
    pub books: usize,
    /// Total verses folded in by this import (across all its books).
    pub verses: usize,
    /// Total distinct books now in the search index (bundled + imported).
    pub total_books: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleInfo {
    pub id: String,
    pub name: String,
    pub book_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterVerse {
    pub verse: u32,
    pub text: String,
}

/// Merge imported books into the live scripture index and persist them to the
/// re-importable cache so imports survive an app restart.
fn apply_scripture_import(
    state: &AppState,
    books: &[crate::scripture::RawBook],
) -> Result<ScriptureImportResult, String> {
    let verses;
    let total_books;
    {
        let mut guard = state.scripture.write().unwrap();
        let index = guard
            .as_mut()
            .ok_or_else(|| "scripture index not loaded yet — try again in a moment".to_string())?;
        verses = index.merge_books(books.to_vec());
        total_books = index.book_count();
    }
    let data_dir = state.app_data_dir();
    let mut imported = crate::scripture::load_imported_books(&data_dir);
    crate::scripture::merge_persisted(&mut imported, books.to_vec());
    crate::scripture::save_imported_books(&data_dir, &imported)?;
    Ok(ScriptureImportResult {
        books: books.len(),
        verses,
        total_books,
    })
}

/// Import a custom Bible from an OpenLP / Zefania XML file (the format OpenLP
/// ships and imports). The frontend picks the file; parsing runs off-thread so
/// a large XML dump cannot stall the UI. Books merge into the scripture search
/// index and then use the same add-slide path as the bundled KJV.
#[tauri::command]
pub async fn import_openlp_bible(
    app: AppHandle,
    path: String,
) -> Result<ScriptureImportResult, String> {
    let path_buf = std::path::PathBuf::from(path.clone());
    let books = tauri::async_runtime::spawn_blocking(move || {
        let xml = std::fs::read_to_string(&path_buf)
            .map_err(|e| format!("failed to read {}: {e}", path_buf.display()))?;
        crate::scripture::parse_openlp_xml(&xml)
    })
    .await
    .map_err(|e| format!("import task failed: {e}"))??;
    let state = app.state::<AppState>();
    let result = apply_scripture_import(&state, &books)?;
    log(
        &app,
        Level::Info,
        &format!(
            "scripture: imported {} books / {} verses from {path}",
            result.books, result.verses
        ),
    );
    Ok(result)
}

/// Import a book (or a specific passage) from the bible-api.com REST service
/// as a fallback source, mapping the JSON response into the same scripture
/// index and slide-generation workflow used by the bundled Bible. `reference`
/// is a human-readable string like "John 3:16" or "rom 8:28"; `translation`
/// is optional (defaults to WEB on the service).
#[tauri::command]
pub async fn import_api_bible(
    app: AppHandle,
    reference: String,
    translation: Option<String>,
) -> Result<ScriptureImportResult, String> {
    let books = crate::scripture::fetch_api_bible(&reference, translation.as_deref()).await?;
    let state = app.state::<AppState>();
    let result = apply_scripture_import(&state, &books)?;
    log(
        &app,
        Level::Info,
        &format!(
            "scripture: imported {} books / {} verses from bible-api.com ({})",
            result.books, result.verses, reference
        ),
    );
    Ok(result)
}

/// Fetch a passage from bible-api.com, fold it into the search index, and
/// return `ScriptureMatch` records ready for the same `add_slide(reference,
/// text)` workflow used by bundled KJV autocomplete.
#[tauri::command]
pub async fn lookup_api_scripture(
    app: AppHandle,
    reference: String,
    translation: Option<String>,
) -> Result<Vec<ScriptureMatch>, String> {
    let books = crate::scripture::fetch_api_bible(&reference, translation.as_deref()).await?;
    let matches = crate::scripture::matches_from_books(&books, 20);
    let state = app.state::<AppState>();
    apply_scripture_import(&state, &books)?;
    log(
        &app,
        Level::Info,
        &format!(
            "scripture: bible-api.com fallback for \"{}\" — {} verses",
            reference,
            matches.len()
        ),
    );
    Ok(matches)
}

#[tauri::command]
pub fn list_bibles(app: AppHandle) -> Vec<BibleInfo> {
    let state = app.state::<AppState>();
    let data_dir = state.app_data_dir();
    // Ensure bibles folder exists and scan for dropped XML files (refresh without restart)
    let bibles_dir = crate::scripture::bibles_folder(&data_dir);
    if !bibles_dir.exists() {
        let _ = std::fs::create_dir_all(&bibles_dir);
        state.logger.log(
            Level::Info,
            &format!(
                "scripture: bibles folder not found — created at {} (place OpenLP XML files there or use Import button)",
                bibles_dir.display()
            ),
        );
    }
    let (scanned, errs) = crate::scripture::scan_bibles_folder(&data_dir);
    for (fname, err) in errs {
        state.logger.log(
            Level::Warn,
            &format!(
                "scripture: malformed Bible file \"{fname}\" in {} — {err} (expected OpenLP/Zefania XML)",
                bibles_dir.display()
            ),
        );
    }
    // Auto-merge any newly dropped XML files so they appear without restart
    if !scanned.is_empty() {
        let mut persisted = crate::scripture::load_imported_books(&data_dir);
        let before = persisted.len();
        crate::scripture::merge_persisted(&mut persisted, scanned.clone());
        if persisted.len() != before {
            let _ = crate::scripture::save_imported_books(&data_dir, &persisted);
            state.logger.log(
                Level::Info,
                &format!(
                    "scripture: auto-imported {} new books from dropped XML files in {}",
                    persisted.len() - before,
                    bibles_dir.display()
                ),
            );
            // Also merge into live index so browse works immediately
            if let Some(idx) = state.scripture.write().unwrap().as_mut() {
                let verses = idx.merge_books(scanned.clone());
                state.logger.log(
                    Level::Info,
                    &format!(
                        "scripture: live index updated with {} verses from dropped files (now {} books)",
                        verses,
                        idx.book_count()
                    ),
                );
            }
        }
    }

    let mut out = Vec::new();
    if let Some(idx) = state.scripture.read().unwrap().as_ref() {
        out.push(BibleInfo {
            id: "kjv".to_string(),
            name: "King James Version".to_string(),
            book_count: idx.book_count(),
        });
    }
    let imported = crate::scripture::load_imported_books(&data_dir);
    // Count distinct books across persisted + scanned (in case scanned not yet persisted due to error)
    let mut all_books = imported;
    all_books.extend(scanned);
    if !all_books.is_empty() {
        let distinct: std::collections::HashSet<String> = all_books.iter().map(|b| b.book.clone()).collect();
        out.push(BibleInfo {
            id: "imported".to_string(),
            name: "Imported Bibles".to_string(),
            book_count: distinct.len(),
        });
    }
    if out.is_empty() {
        out.push(BibleInfo {
            id: "kjv".to_string(),
            name: "King James Version".to_string(),
            book_count: 66,
        });
    }
    out
}

#[tauri::command]
pub fn get_bibles_folder(app: AppHandle) -> String {
    let data_dir = app.state::<AppState>().app_data_dir();
    crate::scripture::bibles_folder(&data_dir).display().to_string()
}

#[tauri::command]
pub fn get_book_list(app: AppHandle, bible_id: String) -> Result<Vec<String>, String> {
    let state = app.state::<AppState>();
    match bible_id.as_str() {
        "kjv" => {
            let guard = state.scripture.read().unwrap();
            let idx = guard.as_ref().ok_or_else(|| "scripture index not loaded yet".to_string())?;
            Ok(idx.ordered_book_names())
        }
        "imported" => {
            let mut imported = crate::scripture::load_imported_books(&state.app_data_dir());
            let (scanned, _) = crate::scripture::scan_bibles_folder(&state.app_data_dir());
            imported.extend(scanned);
            if imported.is_empty() {
                return Err("no imported Bibles found — place OpenLP XML files in bibles folder or use Import button".to_string());
            }
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for b in imported {
                if seen.insert(b.book.clone()) {
                    out.push(b.book);
                }
            }
            Ok(out)
        }
        _ => Err(format!("unknown bible id: {bible_id}")),
    }
}

#[tauri::command]
pub fn get_chapter(
    app: AppHandle,
    bible_id: String,
    book: String,
    chapter: u32,
) -> Result<Vec<ChapterVerse>, String> {
    let state = app.state::<AppState>();
    match bible_id.as_str() {
        "kjv" => {
            let guard = state.scripture.read().unwrap();
            let idx = guard.as_ref().ok_or_else(|| "scripture index not loaded yet".to_string())?;
            let verses = idx
                .get_chapter_verses(&book, chapter)
                .ok_or_else(|| format!("{book} {chapter} not found in KJV"))?;
            Ok(verses.into_iter().map(|(verse, text)| ChapterVerse { verse, text }).collect())
        }
        "imported" => {
            let mut imported = crate::scripture::load_imported_books(&state.app_data_dir());
            let (scanned, _) = crate::scripture::scan_bibles_folder(&state.app_data_dir());
            imported.extend(scanned);
            if imported.is_empty() {
                return Err("no imported Bibles found — place OpenLP XML files in bibles folder or use Import button".to_string());
            }
            for b in imported {
                if b.book.to_lowercase() == book.to_lowercase() {
                    for ch in b.chapters {
                        if ch.chapter.parse::<u32>().ok() == Some(chapter) {
                            return Ok(ch
                                .verses
                                .into_iter()
                                .filter_map(|v| {
                                    let n = v.verse.parse::<u32>().ok()?;
                                    Some(ChapterVerse { verse: n, text: v.text })
                                })
                                .collect());
                        }
                    }
                    return Err(format!("{book} {chapter} not found in imported Bibles"));
                }
            }
            Err(format!("book {book} not found in imported Bibles"))
        }
        _ => Err(format!("unknown bible id: {bible_id}")),
    }
}

#[tauri::command]
pub fn list_chapters(app: AppHandle, bible_id: String, book: String) -> Result<Vec<u32>, String> {
    let state = app.state::<AppState>();
    match bible_id.as_str() {
        "kjv" => {
            let guard = state.scripture.read().unwrap();
            let idx = guard
                .as_ref()
                .ok_or_else(|| "scripture index not loaded yet".to_string())?;
            idx.chapter_numbers(&book)
                .ok_or_else(|| format!("book {book} not found in KJV"))
        }
        "imported" => {
            let mut imported = crate::scripture::load_imported_books(&state.app_data_dir());
            let (scanned, _) = crate::scripture::scan_bibles_folder(&state.app_data_dir());
            imported.extend(scanned);
            if imported.is_empty() {
                return Err("no imported Bibles found — place OpenLP XML files in bibles folder or use Import button".to_string());
            }
            for b in imported {
                if b.book.to_lowercase() == book.to_lowercase() {
                    let mut nums: Vec<u32> = b
                        .chapters
                        .iter()
                        .filter_map(|c| c.chapter.parse::<u32>().ok())
                        .collect();
                    nums.sort_unstable();
                    return Ok(nums);
                }
            }
            Err(format!("book {book} not found in imported Bibles"))
        }
        _ => Err(format!("unknown bible id: {bible_id}")),
    }
}

// ---------------------------------------------------------------------------
// External triggers (MIDI + OSC)
// ---------------------------------------------------------------------------

/// Enumerate every MIDI input device currently available. The visible set
/// differs by OS (WinMM on Windows, ALSA on Ubuntu/Linux, CoreMIDI on macOS).
#[tauri::command]
pub fn list_midi_devices() -> Result<Vec<crate::midi::MidiDeviceInfo>, String> {
    crate::midi::list_devices()
}

/// Set whether the MIDI input listener is enabled. Start/stop the native
/// connection (using the saved device) rather than faking the state.
#[tauri::command]
pub fn set_midi_enabled(app: AppHandle, enabled: bool) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    if enabled {
        let device_id = state
            .current_settings()
            .midi_device_id
            .clone()
            .ok_or_else(|| "select a MIDI device first".to_string())?;
        {
            let mut settings = state.current_settings();
            settings.midi_enabled = true;
            state.apply_settings(settings);
            let _ = write_settings(&state.app_data_dir(), &state.current_settings());
        }
        match state.midi.start(app.clone(), &device_id) {
            Ok(()) => log(&app, Level::Info, "midi: input enabled"),
            Err(e) => {
                log(&app, Level::Error, &format!("midi: could not enable input: {e}"));
                return Err(e);
            }
        }
    } else {
        state.midi.stop();
        {
            let mut settings = state.current_settings();
            settings.midi_enabled = false;
            state.apply_settings(settings);
            let _ = write_settings(&state.app_data_dir(), &state.current_settings());
        }
        log(&app, Level::Info, "midi: input disabled");
    }
    Ok(snapshot_and_emit(&app))
}

/// Choose the MIDI input device and start listening on it immediately.
#[tauri::command]
pub fn set_midi_device(app: AppHandle, device_id: String) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut settings = state.current_settings();
        settings.midi_device_id = Some(device_id.clone());
        settings.midi_enabled = true;
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    match state.midi.start(app.clone(), &device_id) {
        Ok(()) => log(&app, Level::Info, &format!("midi: device set + listening ({device_id})")),
        Err(e) => log(&app, Level::Error, &format!("midi: device set but could not open: {e}")),
    }
    Ok(snapshot_and_emit(&app))
}

/// Set whether the OSC UDP listener is enabled (on the saved port).
#[tauri::command]
pub fn set_osc_enabled(app: AppHandle, enabled: bool) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    if enabled {
        let port = state.current_settings().osc_port;
        {
            let mut settings = state.current_settings();
            settings.osc_enabled = true;
            state.apply_settings(settings);
            let _ = write_settings(&state.app_data_dir(), &state.current_settings());
        }
        match state.osc.start(app.clone(), port) {
            Ok(()) => log(&app, Level::Info, &format!("osc: listener enabled on UDP :{port}")),
            Err(e) => {
                log(&app, Level::Error, &format!("osc: could not enable listener: {e}"));
                return Err(e);
            }
        }
    } else {
        state.osc.stop();
        {
            let mut settings = state.current_settings();
            settings.osc_enabled = false;
            state.apply_settings(settings);
            let _ = write_settings(&state.app_data_dir(), &state.current_settings());
        }
        log(&app, Level::Info, "osc: listener disabled");
    }
    Ok(snapshot_and_emit(&app))
}

/// Change the OSC UDP port. Restarts the listener if it is currently enabled.
#[tauri::command]
pub fn set_osc_port(app: AppHandle, port: u16) -> Result<ClientState, String> {
    if port == 0 {
        return Err("OSC port must be between 1 and 65535".to_string());
    }
    let state = app.state::<AppState>();
    let was_enabled = state.current_settings().osc_enabled;
    {
        let mut settings = state.current_settings();
        settings.osc_port = port;
        if !was_enabled {
            settings.osc_enabled = false;
        }
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    if was_enabled {
        state.osc.stop();
        match state.osc.start(app.clone(), port) {
            Ok(()) => log(&app, Level::Info, &format!("osc: restarted on UDP :{port}")),
            Err(e) => log(&app, Level::Error, &format!("osc: restart failed: {e}")),
        }
    }
    log(&app, Level::Info, &format!("osc: port set to {port}"));
    Ok(snapshot_and_emit(&app))
}

/// Add a trigger-to-action mapping and persist it.
#[tauri::command]
pub fn add_trigger(
    app: AppHandle,
    trigger: crate::triggers::Trigger,
    action: crate::triggers::TriggerAction,
    label: Option<String>,
) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut settings = state.current_settings();
        settings.triggers.push(crate::triggers::TriggerMapping::new(
            trigger.clone(),
            action.clone(),
        ));
        if let Some(last) = settings.triggers.last_mut() {
            last.label = label;
        }
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    log(
        &app,
        Level::Info,
        &format!("trigger: added {} → {}", trigger.describe(), action.label()),
    );
    Ok(snapshot_and_emit(&app))
}

/// Remove a mapping by id and persist.
#[tauri::command]
pub fn delete_trigger(app: AppHandle, trigger_id: String) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut settings = state.current_settings();
        if !settings.triggers.iter().any(|m| m.id == trigger_id) {
            return Err(format!("trigger {trigger_id} not found"));
        }
        settings.triggers.retain(|m| m.id != trigger_id);
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    log(&app, Level::Info, &format!("trigger: deleted {trigger_id}"));
    Ok(snapshot_and_emit(&app))
}

/// Enable/disable a mapping without deleting it.
#[tauri::command]
pub fn set_trigger_enabled(
    app: AppHandle,
    trigger_id: String,
    enabled: bool,
) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut settings = state.current_settings();
        let mapping = settings
            .triggers
            .iter_mut()
            .find(|m| m.id == trigger_id)
            .ok_or_else(|| format!("trigger {trigger_id} not found"))?;
        mapping.enabled = enabled;
        state.apply_settings(settings);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    log(
        &app,
        Level::Info,
        &format!("trigger: {} {}",
            if enabled { "enabled" } else { "disabled" },
            trigger_id),
    );
    Ok(snapshot_and_emit(&app))
}

// ---------------------------------------------------------------------------
// Local-network Stage Display
// ---------------------------------------------------------------------------

/// The access point a phone uses to reach the stage display — its bind URL and
/// each viable LAN IP so Settings can show a QR/URL without a manual IP lookup.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageNetworkInfo {
    /// The machine-level URL form, e.g. "0.0.0.0:1426" (bind form, not directly
    /// reachable but the canonical server address).
    pub bind_host: String,
    /// Actual reachable URLs, one per non-loopback IPv4 address: the string a
    /// phone should be pointed at.
    pub urls: Vec<String>,
    pub port: u16,
    /// Whether the server is currently enabled/running.
    pub enabled: bool,
    /// The current PIN (shown in Settings so the operator can share it).
    pub pin: String,
}

/// Runtime status + access details for the Stage Network server: the reachable
/// LAN URL(s) a phone can be pointed at, plus the port. Enumerates non-loopback
/// IPv4 addresses so Settings can show a QR/URL without a manual IP lookup.
#[tauri::command]
pub fn get_stage_network_info(app: AppHandle) -> Result<StageNetworkInfo, String> {
    let state = app.state::<AppState>();
    let settings = state.current_settings();
    let port = settings.stage_network_port;
    let mut urls = Vec::new();
    if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
        for iface in ifaces.into_iter().filter(|i| !i.is_loopback()) {
            if let get_if_addrs::IfAddr::V4(v4) = iface.addr {
                urls.push(format!("http://{}:{}", v4.ip, port));
            }
        }
    }
    if urls.is_empty() {
        urls.push(format!("http://127.0.0.1:{port}"));
    }
    Ok(StageNetworkInfo {
        bind_host: format!("0.0.0.0:{port}"),
        urls,
        port,
        enabled: settings.stage_network_enabled,
        pin: settings.stage_network_pin.clone(),
    })
}

/// Turn the local-network Stage Display server on or off.
#[tauri::command]
pub fn set_stage_network_enabled(app: AppHandle, enabled: bool) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let settings = state.current_settings();
    if enabled {
        let addr: SocketAddr = format!("0.0.0.0:{}", settings.stage_network_port)
            .parse()
            .map_err(|e| format!("invalid stage network address: {e}"))?;
        {
            let mut s = state.current_settings();
            s.stage_network_enabled = true;
            state.apply_settings(s);
            let _ = write_settings(&state.app_data_dir(), &state.current_settings());
        }
        match state.network.start(app.clone(), addr, settings.stage_network_pin.clone()) {
            Ok(()) => log(
                &app,
                Level::Info,
                &format!("stage-network: enabled on :{}", settings.stage_network_port),
            ),
            Err(e) => {
                log(&app, Level::Error, &format!("stage-network: could not enable: {e}"));
                return Err(e);
            }
        }
    } else {
        state.network.stop();
        {
            let mut s = state.current_settings();
            s.stage_network_enabled = false;
            state.apply_settings(s);
            let _ = write_settings(&state.app_data_dir(), &state.current_settings());
        }
        log(&app, Level::Info, "stage-network: disabled");
    }
    Ok(snapshot_and_emit(&app))
}

/// Change the Stage Network port. Restarts the server if it is running.
#[tauri::command]
pub fn set_stage_network_port(app: AppHandle, port: u16) -> Result<ClientState, String> {
    if port == 0 {
        return Err("Stage Network port must be between 1 and 65535".to_string());
    }
    let state = app.state::<AppState>();
    let was_enabled = state.current_settings().stage_network_enabled;
    {
        let mut s = state.current_settings();
        s.stage_network_port = port;
        if !was_enabled {
            s.stage_network_enabled = false;
        }
        state.apply_settings(s);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    if was_enabled {
        state.network.stop();
        let pin = state.current_settings().stage_network_pin.clone();
        let addr: SocketAddr = format!("0.0.0.0:{port}")
            .parse()
            .map_err(|e| format!("invalid address: {e}"))?;
        match state.network.start(app.clone(), addr, pin) {
            Ok(()) => log(&app, Level::Info, &format!("stage-network: restarted on :{port}")),
            Err(e) => log(&app, Level::Error, &format!("stage-network: restart failed: {e}")),
        }
    }
    log(&app, Level::Info, &format!("stage-network: port set to {port}"));
    Ok(snapshot_and_emit(&app))
}

/// Set (or clear) the PIN required to view the Stage Display page.
#[tauri::command]
pub fn set_stage_network_pin(app: AppHandle, pin: String) -> Result<ClientState, String> {
    let trimmed = pin.trim().to_string();
    if trimmed.len() > 12 {
        return Err("PIN must be 12 characters or fewer".to_string());
    }
    let state = app.state::<AppState>();
    {
        let mut s = state.current_settings();
        s.stage_network_pin = trimmed.clone();
        state.apply_settings(s);
        let _ = write_settings(&state.app_data_dir(), &state.current_settings());
    }
    // Update the running server's PIN live (no need to restart).
    state.network.set_pin_live(&trimmed);
    log(&app, Level::Info, "stage-network: PIN updated");
    Ok(snapshot_and_emit(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            output_display_index: Some(1),
            output_display_name: Some("A".to_string()),
            output_fullscreen: true,
            stage_display_index: Some(2),
            stage_display_name: Some("B".to_string()),
            stage_visible: true,
            default_transition: Transition::Fade,
            output_look_id: Some("look-1".to_string()),
            stage_look_id: None,
            ndi_enabled: false,
            ndi_look_id: None,
            midi_enabled: false,
            midi_device_id: None,
            osc_enabled: false,
            osc_port: 9000,
            triggers: Vec::new(),
            stage_network_enabled: false,
            stage_network_port: 1426,
            stage_network_pin: String::new(),
            audio_output_device_id: None,
            audio_volume: 1.0,
        }
    }

    #[test]
    fn settings_snapshot_roundtrips() {
        let snapshot = SettingsSnapshot {
            app: "makepresent".to_string(),
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            settings: settings(),
            exported_at: now_iso(),
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let back: SettingsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.app, "makepresent");
        assert_eq!(back.settings_schema_version, SETTINGS_SCHEMA_VERSION);
        assert!(back.settings.output_fullscreen);
        assert_eq!(back.settings.default_transition, Transition::Fade);
        assert_eq!(back.settings.stage_display_index, Some(2));
    }

    #[test]
    fn changed_settings_reports_only_differences() {
        let old = settings();
        let mut new = settings();
        new.output_fullscreen = false;
        new.stage_visible = false;
        new.default_transition = Transition::Cut;
        let changed = changed_settings(&old, &new);
        assert_eq!(
            changed,
            vec![
                "Output fullscreen".to_string(),
                "Stage visibility".to_string(),
                "Default transition".to_string(),
            ]
        );
    }

    #[test]
    fn no_changes_reports_empty() {
        assert!(changed_settings(&settings(), &settings()).is_empty());
    }
}