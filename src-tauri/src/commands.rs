use crate::logging::{Level, LogEntry};
use crate::project::{
    is_first_run, now_iso, Background, BroadcastView, ClientState, Library, LibrarySlide,
    LibrarySong, Look, OutputView, Project, Settings, Slide, StageView, TextPosition, Transition,
    write_settings,
};
use crate::scripture::ScriptureMatch;
use crate::state::AppState;
use crate::windows::{self, DisplayInfo};
use serde::{Deserialize, Serialize};
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
    };
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

#[tauri::command]
pub fn add_library_song(
    app: AppHandle,
    title: String,
    body: Option<String>,
    background: Option<Background>,
) -> Result<Library, String> {
    let state = app.state::<AppState>();
    let song = LibrarySong {
        id: Uuid::new_v4().to_string(),
        title,
        default_background: background.unwrap_or_default(),
        slides: vec![LibrarySlide {
            id: Uuid::new_v4().to_string(),
            title: "".to_string(),
            body: body.unwrap_or_default(),
        }],
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

/// Copy every verse/section of a library song into the playlist as linked
/// slides (one click from the editor).
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

    mutate(&app, |project| {
        for slide in &song.slides {
            project.slides.push(Slide {
                id: Uuid::new_v4().to_string(),
                library_id: Some(song.id.clone()),
                library_slide_id: Some(slide.id.clone()),
                title: slide.title.clone(),
                body: slide.body.clone(),
                background: song.default_background.clone(),
            });
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_live_slide(app: AppHandle, slide_id: String) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    let live_title = {
        let mut project = state.project.write().unwrap();
        let slide = project
            .find(&slide_id)
            .cloned()
            .ok_or_else(|| format!("slide {slide_id} not found"))?;
        project.live = Some(slide_id.clone());
        project.selected = Some(slide_id);
        project.modified_at = now_iso();
        slide.title
    };
    state.request_save();

    log(&app, Level::Info, &format!("output: live slide \"{live_title}\""));

    // The output window appears on demand: the first slide going live is
    // what creates and shows it (never at startup).
    if let Err(e) = windows::show_output(&app, &state) {
        log(&app, Level::Error, &format!("output: could not show window: {e}"));
    }

    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn clear_output(app: AppHandle) -> Result<ClientState, String> {
    let state = app.state::<AppState>();
    {
        let mut project = state.project.write().unwrap();
        project.live = None;
        project.modified_at = now_iso();
    }
    state.request_save();
    log(&app, Level::Info, "output: cleared (black)");
    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(snap)
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
    pub text_color: Option<String>,
    pub show_background: Option<bool>,
    pub text_position: Option<TextPosition>,
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
) -> Result<ClientState, String> {
    mutate(&app, |project| {
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
        Ok(())
    })
    .map(|s| {
        log(
            &app,
            Level::Info,
            &format!("playlist: updated slide \"{slide_id}\""),
        );
        s
    })
}

#[tauri::command]
pub fn delete_slide(app: AppHandle, slide_id: String) -> Result<ClientState, String> {
    mutate(&app, |project| {
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
        }
    }
}

fn settings_fields() -> [SettingsField; 9] {
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