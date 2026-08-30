use crate::logging::{Level, LogEntry};
use crate::project::{
    is_first_run, now_iso, Background, ClientState, Library, LibrarySlide, LibrarySong,
    OutputView, Project, Settings, Slide, StageView, Transition, write_settings,
};
use crate::state::AppState;
use crate::windows::{self, DisplayInfo};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

fn snapshot(app: &AppHandle) -> ClientState {
    let state = app.state::<AppState>();
    let settings = state.current_settings();

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
    drop(project);

    let snap = ClientState {
        project: state.project.read().unwrap().clone(),
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
        first_run: is_first_run(&state.app_data_dir()),
        default_transition: settings.default_transition,
        current,
        next,
        on_deck,
    };
    snap
}

fn log(app: &AppHandle, level: Level, message: &str) {
    app.state::<AppState>().logger.log(level, message);
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

    let _ = window.set_focus();
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
        let _ = window.set_focus();
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
}

impl SettingsField {
    fn label(self) -> &'static str {
        match self {
            SettingsField::OutputDisplay => "Output display",
            SettingsField::OutputFullscreen => "Output fullscreen",
            SettingsField::StageDisplay => "Stage display",
            SettingsField::StageVisible => "Stage visibility",
            SettingsField::DefaultTransition => "Default transition",
        }
    }
}

fn settings_fields() -> [SettingsField; 5] {
    [
        SettingsField::OutputDisplay,
        SettingsField::OutputFullscreen,
        SettingsField::StageDisplay,
        SettingsField::StageVisible,
        SettingsField::DefaultTransition,
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
        };
        if differs {
            changed.push(field.label().to_string());
        }
    }
    changed
}

/// Write the current settings (not the project or library) to the given JSON
/// path so they can be moved between machines.
#[tauri::command]
pub fn export_settings(app: AppHandle, path: String) -> Result<ExportReport, String> {
    let state = app.state::<AppState>();
    let snapshot = SettingsSnapshot {
        app: "makepresent".to_string(),
        settings_schema_version: SETTINGS_SCHEMA_VERSION,
        settings: state.current_settings(),
        exported_at: now_iso(),
    };
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("could not serialize settings: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("could not write settings file: {e}"))?;
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
#[tauri::command]
pub fn import_settings(app: AppHandle, path: String) -> Result<ImportReport, String> {
    let state = app.state::<AppState>();
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read settings file: {e}"))?;
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
#[tauri::command]
pub fn get_logs(app: AppHandle, limit: Option<usize>) -> Vec<LogEntry> {
    app.state::<AppState>().logger.recent(limit.unwrap_or(300))
}

/// Copy the current log file to the chosen path so it can be shared without
/// digging through the filesystem.
#[tauri::command]
pub fn export_logs_to(app: AppHandle, path: String) -> Result<String, String> {
    let state = app.state::<AppState>();
    state
        .logger
        .export_to(std::path::Path::new(&path))
        .map_err(|e| format!("could not export log file: {e}"))?;
    log(&app, Level::Info, &format!("logs: exported to {path}"));
    Ok(path)
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