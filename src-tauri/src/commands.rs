use crate::project::{
    now_iso, Background, ClientState, OutputView, Project, Slide, write_settings,
};
use crate::state::AppState;
use crate::windows::{self, DisplayInfo};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

fn snapshot(app: &AppHandle) -> ClientState {
    let state = app.state::<AppState>();
    let settings = state.current_settings();
    ClientState {
        project: state.project.read().unwrap().clone(),
        notice: state.notice.read().unwrap().clone(),
        output: OutputView {
            monitor_index: settings.output_display_index,
            monitor_name: settings.output_display_name,
            fullscreen: settings.output_fullscreen,
        },
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
    let snap = snapshot(app);
    let _ = app.emit("state", &snap);
    Ok(snap)
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> ClientState {
    snapshot(&app)
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