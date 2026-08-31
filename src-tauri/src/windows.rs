use crate::state::AppState;
use serde::Serialize;
use tauri::{
    AppHandle, Manager, Monitor, PhysicalSize, Position, Size, WebviewUrl, WebviewWindow,
};

pub const EDITOR_WINDOW: &str = "main";
pub const OUTPUT_WINDOW: &str = "output";
pub const STAGE_WINDOW: &str = "stage";

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

/// Diagnostic: compact one-line description of a window's visibility, focus,
/// inner position/size and current monitor. Used after every create/placement
/// so a Windows app.log shows exactly where each window ended up.
fn describe_window(w: &WebviewWindow) -> String {
    let pos = w
        .inner_position()
        .map(|p| format!("({}, {})", p.x, p.y))
        .unwrap_or_else(|_| "(?, ?)".to_string());
    let size = w
        .inner_size()
        .map(|s| format!("{}x{}", s.width, s.height))
        .unwrap_or_else(|_| "?x?".to_string());
    let visible = w.is_visible().unwrap_or(false);
    let focused = w.is_focused().unwrap_or(false);
    let monitor = w
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|m| m.name().cloned())
        .unwrap_or_else(|| "(none)".to_string());
    format!(
        "visible={visible}, focused={focused}, inner_pos={pos}, inner_size={size}, monitor=\"{monitor}\""
    )
}

/// The output window is a dumb renderer. Create it once, keep it around.
pub fn ensure_output(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(OUTPUT_WINDOW) {
        return Ok(window);
    }
    let state = app.state::<AppState>();
    state.logger.log(crate::logging::Level::Info, "windows: creating output window");
    match WebviewWindow::builder(app, OUTPUT_WINDOW, output_url())
        .title("MakePresent - Output")
        .decorations(false)
        .resizable(false)
        .fullscreen(false)
        .visible(false)
        .build()
    {
        Ok(window) => {
            state.logger.log(
                crate::logging::Level::Info,
                &format!(
                    "windows: output window created successfully ({})",
                    describe_window(&window)
                ),
            );
            Ok(window)
        }
        Err(e) => {
            state.logger.log(
                crate::logging::Level::Error,
                &format!("windows: FAILED to create output window: {e}"),
            );
            Err(format!("failed to create output window: {e}"))
        }
    }
}

/// The stage display is a dumb renderer aimed at the performers/presenters.
/// Created on demand so it only exists while the user has it switched on.
pub fn get_stage(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(STAGE_WINDOW)
}

fn stage_url() -> WebviewUrl {
    #[cfg(debug_assertions)]
    {
        WebviewUrl::External(
            "http://localhost:1420/stage.html"
                .parse()
                .expect("valid dev URL"),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        WebviewUrl::App("stage.html".into())
    }
}

pub fn ensure_stage(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = get_stage(app) {
        return Ok(window);
    }
    let state = app.state::<AppState>();
    state.logger.log(crate::logging::Level::Info, "windows: creating stage window");
    match WebviewWindow::builder(app, STAGE_WINDOW, stage_url())
        .title("MakePresent - Stage Display")
        .resizable(true)
        .visible(false)
        .build()
    {
        Ok(window) => {
            state.logger.log(
                crate::logging::Level::Info,
                &format!(
                    "windows: stage window created successfully ({})",
                    describe_window(&window)
                ),
            );
            Ok(window)
        }
        Err(e) => {
            state.logger.log(
                crate::logging::Level::Error,
                &format!("windows: FAILED to create stage window: {e}"),
            );
            Err(format!("failed to create stage window: {e}"))
        }
    }
}

/// Place the stage window on the given display, windowed at ~70% of the
/// monitor, and show it. Same display-picker pattern as the output window.
pub fn move_stage_to(app: &AppHandle, monitor_index: usize) -> Result<WebviewWindow, String> {
    let state = app.state::<AppState>();
    let editor = editor_window(app)?;
    let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors.get(monitor_index).ok_or_else(|| {
        let msg = format!("invalid display index {monitor_index} (only {} monitors)", monitors.len());
        state
            .logger
            .log(crate::logging::Level::Error, &format!("windows: move_stage_to: {msg}"));
        msg
    })?;
    let name = monitor.name().cloned().unwrap_or_else(|| "(unnamed)".to_string());
    let target_pos = *monitor.position();
    let target_size = monitor.size();
    state.logger.log(
        crate::logging::Level::Info,
        &format!(
            "windows: move_stage_to: monitor #{monitor_index} \"{name}\" is {}x{} at ({}, {})",
            target_size.width, target_size.height, target_pos.x, target_pos.y
        ),
    );

    let window = ensure_stage(app)?;
    let w = (target_size.width as f64 * 0.7).round() as u32;
    let h = (target_size.height as f64 * 0.7).round() as u32;
    let size_res = window.set_size(Size::Physical(PhysicalSize::new(w, h)));
    let pos_res = window.set_position(Position::Physical(target_pos));
    let show_res = window.show();
    state.logger.log(
        crate::logging::Level::Info,
        &format!(
            "windows: move_stage_to: set_size({w}x{h}) -> {:?}, set_position({}, {}) -> {:?}, show() -> {:?}; after: {}",
            size_res,
            target_pos.x,
            target_pos.y,
            pos_res,
            show_res,
            describe_window(&window)
        ),
    );
    show_res.map_err(|e| e.to_string())?;
    Ok(window)
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
            name: m.name().cloned().unwrap_or_default(),
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
    let state = app.state::<AppState>();
    let editor = editor_window(app)?;
    let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
    let monitor = monitors.get(monitor_index).ok_or_else(|| {
        let msg = format!("invalid display index {monitor_index} (only {} monitors)", monitors.len());
        state
            .logger
            .log(crate::logging::Level::Error, &format!("windows: move_output_to: {msg}"));
        msg
    })?;
    let name = monitor.name().cloned().unwrap_or_else(|| "(unnamed)".to_string());
    let target_pos = *monitor.position();
    let target_size = monitor.size();
    state.logger.log(
        crate::logging::Level::Info,
        &format!(
            "windows: move_output_to: monitor #{monitor_index} \"{name}\" is {}x{} at ({}, {})",
            target_size.width, target_size.height, target_pos.x, target_pos.y
        ),
    );

    let window = ensure_output(app)?;
    let exit_fs = if window.is_fullscreen().unwrap_or(false) {
        Some(window.set_fullscreen(false))
    } else {
        None
    };
    let pos_res = window.set_position(Position::Physical(target_pos));
    let show_res = window.show();
    state.logger.log(
        crate::logging::Level::Info,
        &format!(
            "windows: move_output_to: exit_fullscreen -> {:?}, set_position({}, {}) -> {:?}, show() -> {:?}; after: {}",
            exit_fs,
            target_pos.x,
            target_pos.y,
            pos_res,
            show_res,
            describe_window(&window)
        ),
    );
    show_res.map_err(|e| e.to_string())?;
    Ok(window)
}

/// Whether the on-demand output window currently exists and is showing.
pub fn output_visible(app: &AppHandle) -> bool {
    app.get_webview_window(OUTPUT_WINDOW)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Show (and create if needed) the output window on the configured or
/// auto-picked display. This is the "Show Output" action used both by the
/// explicit button and by the first slide going live.
pub fn show_output(app: &AppHandle, state: &AppState) -> Result<(), String> {
    // Diagnostic: full monitor topology as reported by Tauri/Windows right
    // before placement, so an off-screen/invalid target shows up as nonsense
    // coordinates here (per-monitor DPI virtualization) rather than a mystery.
    match list_displays(app) {
        Ok(displays) => {
            state.logger.log(
                crate::logging::Level::Info,
                &format!("windows: show_output: {} monitor(s) via list_displays:", displays.len()),
            );
            for d in &displays {
                state.logger.log(
                    crate::logging::Level::Info,
                    &format!(
                        "windows: show_output:   #{} \"{}\" {}x{} at ({}, {}) primary={} current={}",
                        d.index, d.name, d.width, d.height, d.x, d.y, d.primary, d.current
                    ),
                );
            }
        }
        Err(e) => state.logger.log(
            crate::logging::Level::Error,
            &format!("windows: show_output: list_displays failed: {e}"),
        ),
    }

    let settings = state.current_settings();
    let index = match settings.output_display_index {
        Some(index) => index,
        None => default_output_display(app)?,
    };
    let window = move_output_to(app, index)?;
    if settings.output_fullscreen {
        window.set_fullscreen(true).map_err(|e| e.to_string())?;
    }
    let focus_res = window.set_focus();
    state.logger.log(
        crate::logging::Level::Info,
        &format!("windows: show_output: set_focus result: {:?}", focus_res),
    );

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