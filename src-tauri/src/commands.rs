use crate::project::{
    now_iso, Background, ClientState, Library, LibrarySlide, LibrarySong, OutputView, Project,
    Slide, StageView, Transition, write_settings,
};
use crate::state::AppState;
use crate::windows::{self, DisplayInfo};
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
    drop(project);

    let snap = ClientState {
        project: state.project.read().unwrap().clone(),
        notice: state.notice.read().unwrap().clone(),
        output: OutputView {
            monitor_index: settings.output_display_index,
            monitor_name: settings.output_display_name,
            fullscreen: settings.output_fullscreen,
        },
        stage: StageView {
            visible: settings.stage_visible,
            monitor_index: settings.stage_display_index,
            monitor_name: settings.stage_display_name,
        },
        current,
        next,
    };
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
    mutate(&app, |project| {
        if project.find(&slide_id).is_none() {
            return Err(format!("slide {slide_id} not found"));
        }
        project.live = Some(slide_id);
        Ok(())
    })
}

#[tauri::command]
pub fn clear_output(app: AppHandle) -> Result<ClientState, String> {
    mutate(&app, |project| {
        project.live = None;
        Ok(())
    })
}

/// Per-project Output transition: "cut" (default) or "fade".
#[tauri::command]
pub fn set_transition(app: AppHandle, transition: Transition) -> Result<ClientState, String> {
    mutate(&app, |project| {
        project.transition = transition;
        Ok(())
    })
}

#[tauri::command]
pub fn new_project(app: AppHandle) -> Result<ClientState, String> {
    replace_project(&app, Project::new("First Service"))
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
    mutate(&app, |project| {
        project.slides.push(slide);
        Ok(())
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

    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    windows::list_displays(&app)
}

#[tauri::command]
pub fn toggle_output_fullscreen(app: AppHandle) -> Result<bool, String> {
    let window = windows::ensure_output(&app)?;
    let next = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window.set_decorations(!next).map_err(|e| e.to_string())?;
    window.set_fullscreen(next).map_err(|e| e.to_string())?;

    let state = app.state::<AppState>();
    let mut settings = state.current_settings();
    settings.output_fullscreen = next;
    state.apply_settings(settings);
    let _ = write_settings(&state.app_data_dir(), &state.current_settings());

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

    let snap = snapshot(&app);
    let _ = app.emit("state", &snap);
    Ok(next)
}