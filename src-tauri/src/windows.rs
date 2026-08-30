use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Manager, Monitor, Position, WebviewUrl, WebviewWindow};

pub const EDITOR_WINDOW: &str = "main";
pub const OUTPUT_WINDOW: &str = "output";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub primary: bool,
    pub current: bool,
}

fn output_url() -> WebviewUrl {
    #[cfg(debug_assertions)]
    {
        WebviewUrl::External(
            "http://localhost:1420/output.html"
                .parse()
                .expect("valid dev URL"),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        WebviewUrl::App("output.html".into())
    }
}

pub fn editor_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(EDITOR_WINDOW)
        .ok_or_else(|| "Editor window not found".to_string())
}

/// The output window is a dumb renderer. Create it once, keep it around.
pub fn ensure_output(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(OUTPUT_WINDOW) {
        return Ok(window);
    }
    WebviewWindow::builder(app, OUTPUT_WINDOW, output_url())
        .title("MakePresent - Output")
        .decorations(false)
        .resizable(false)
        .fullscreen(false)
        .visible(false)
        .build()
        .map_err(|e| format!("failed to create output window: {e}"))
}

fn same_monitor(a: &Monitor, b: &Monitor) -> bool {
    a.position() == b.position() && a.size() == b.size()
}

pub fn list_displays(app: &AppHandle) -> Result<Vec<DisplayInfo>, String> {
    let editor = editor_window(app)?;
    let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
    let primary = editor.primary_monitor().ok().flatten();
    let current = editor.current_monitor().ok().flatten();
    Ok(monitors
        .iter()
        .enumerate()
        .map(|(i, m)| DisplayInfo {
            index: i,
            name: m.name().unwrap_or_default(),
            width: m.size().width,
            height: m.size().height,
            x: m.position().x,
            y: m.position().y,
            primary: primary.as_ref().is_some_and(|p| same_monitor(p, m)),
            current: current.as_ref().is_some_and(|c| same_monitor(c, m)),
        })
        .collect())
}

/// Default assignment: the largest display that is not the editor's own
/// monitor (external/projector), falling back to the largest overall so the
/// output is never lost.
pub fn default_output_display(app: &AppHandle) -> Result<usize, String> {
    let editor = editor_window(app)?;
    let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
    let current = editor.current_monitor().ok().flatten();
    let area = |m: &Monitor| m.size().width as u64 * m.size().height as u64;

    let external = monitors
        .iter()
        .enumerate()
        .filter(|(_, m)| !current.as_ref().is_some_and(|c| same_monitor(c, m)))
        .max_by_key(|(_, m)| area(m))
        .map(|(i, _)| i);
    let any = monitors
        .iter()
        .enumerate()
        .max_by_key(|(_, m)| area(m))
        .map(|(i, _)| i);
    Ok(external.or(any).unwrap_or(0))
}

/// Move (or create) the output window onto the given display. Always exits
/// fullscreen first so the reposition is reliable on every platform.
pub fn move_output_to(app: &AppHandle, monitor_index: usize) -> Result<WebviewWindow, String> {
    let editor = editor_window(app)?;
    let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors
        .get(monitor_index)
        .ok_or_else(|| format!("invalid display index {monitor_index}"))?;

    let window = ensure_output(app)?;
    if window.is_fullscreen().unwrap_or(false) {
        let _ = window.set_fullscreen(false);
    }
    let _ = window.set_position(Position::Physical(monitor.position()));
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

/// Startup placement: honor the stored assignment, else auto-pick a display.
pub fn place_default_output(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let settings = state.current_settings();
    let index = match settings.output_display_index {
        Some(index) => index,
        None => default_output_display(app)?,
    };
    let window = move_output_to(app, index)?;
    if settings.output_fullscreen {
        window.set_fullscreen(true).map_err(|e| e.to_string())?;
    }
    let _ = window.set_focus();

    // Persist the resolved assignment so restarts keep the same display.
    let name = list_displays(app)
        .ok()
        .and_then(|displays| displays.into_iter().find(|d| d.index == index))
        .and_then(|d| (!d.name.is_empty()).then_some(d.name));
    let mut resolved = settings;
    resolved.output_display_index = Some(index);
    resolved.output_display_name = name;
    state.apply_settings(resolved);
    let _ = crate::project::write_settings(&state.app_data_dir(), &state.current_settings());
    Ok(())
}