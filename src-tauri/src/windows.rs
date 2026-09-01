use crate::state::AppState;
use serde::Serialize;
use std::cell::Cell;
use std::time::Duration;
use tauri::{
    AppHandle, Manager, Monitor, PhysicalSize, Position, Size, WebviewUrl, WebviewWindow,
};

thread_local! {
    static IS_MAIN_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// Mark the current thread as the GUI main thread. Call once at the start of
/// `setup()` and inside every `run_on_main_thread` closure.
pub fn mark_as_main_thread() {
    IS_MAIN_THREAD.with(|c| c.set(true));
}

fn is_main_thread() -> bool {
    IS_MAIN_THREAD.with(|c| c.get())
}

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

/// Run the given window-creation/placement closure on the GUI main thread and
/// block until it completes.
///
/// Window creation on Windows (wry/WebView2) must run on the main thread; doing
/// it from Tauri's command worker threads wedges the event loop. Tauri's own
/// `builder.build()` posts a `Message::CreateWindow` to the loop asynchronously,
/// so we force it to run synchronously on the main thread instead.
///
/// Correctness: `setup()` and any `run_on_main_thread` closure both run on the
/// main thread. If this helper unconditionally queued a new closure and blocked,
/// a call from the main thread (stage restore during setup, or the nested
/// `ensure_output` inside `move_output_to` when that outer closure is already
/// on the main thread) would self-deadlock — the main thread would be blocked
/// waiting for a closure it can never service. We detect that case via a
/// thread-local flag and run inline instead. Tauri does not expose
/// `is_main_thread()`, so we maintain the flag ourselves (set at the start of
/// `setup()` and inside every dispatched closure).
///
/// A 5s timeout is used as a diagnostic safety net: instead of freezing
/// silently forever, we log `main thread dispatch timed out — likely deadlock`.
///
/// Bounds each dispatch with a log line before and after, so any remaining hang
/// is still clearly located between those two lines in `logs/app.log`.
fn run_on_main<F, T>(app: &AppHandle, state: &AppState, op: &str, f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    // Fast path: already on the main thread → run inline, no queue, no block.
    if is_main_thread() {
        state.logger.log(
            crate::logging::Level::Info,
            &format!("windows: {op}: already on main thread, running inline"),
        );
        let res = f();
        state.logger.log(
            crate::logging::Level::Info,
            &format!("windows: {op}: inline execution completed"),
        );
        return res;
    }

    state.logger.log(
        crate::logging::Level::Info,
        &format!("windows: {op}: dispatching to main thread"),
    );
    let (tx, rx) = std::sync::mpsc::channel::<Result<T, String>>();
    if let Err(e) = app.run_on_main_thread(move || {
        // This closure runs on the main thread — mark it so any nested
        // `run_on_main` call inside `f()` takes the inline fast path.
        mark_as_main_thread();
        let _ = tx.send(f());
    }) {
        let msg = format!("{op}: run_on_main_thread dispatch failed: {e}");
        state
            .logger
            .log(crate::logging::Level::Error, &format!("windows: {msg}"));
        return Err(msg);
    }
    let result = rx.recv_timeout(Duration::from_secs(5)).map_err(|e| {
        let msg = match e {
            std::sync::mpsc::RecvTimeoutError::Timeout => {
                format!("{op}: main thread dispatch timed out after 5s — likely deadlock")
            }
            std::sync::mpsc::RecvTimeoutError::Disconnected => {
                format!("{op}: main thread never responded (channel closed, possible hang/panic)")
            }
        };
        state
            .logger
            .log(crate::logging::Level::Error, &format!("windows: {msg}"));
        msg
    })?;
    state.logger.log(
        crate::logging::Level::Info,
        &format!("windows: {op}: main thread dispatch completed"),
    );
    result
}

/// Fire-and-forget variant of `run_on_main`. Queues `f` on the main thread
/// and returns immediately without waiting for a result. Used for
/// `show_output` triggered by `set_live_slide` — the slide should go live
/// and `snapshot` should be emitted without blocking the command worker on
/// WebView2 creation, which on Windows can leave the message pump degraded
/// for several hundred ms. Errors are logged inside the closure.
fn run_on_main_async<F>(app: &AppHandle, op: &str, f: F) -> Result<(), String>
where
    F: FnOnce() + Send + 'static,
{
    if is_main_thread() {
        // Already on main thread — run inline fire-and-forget.
        f();
        return Ok(());
    }
    app.run_on_main_thread(move || {
        mark_as_main_thread();
        f();
    })
    .map_err(|e| format!("{op}: run_on_main_thread dispatch failed: {e}"))
}

/// The output window is a dumb renderer. Create it once, keep it around.
/// Pre-created hidden after setup so live handlers never call builder().build().
pub fn ensure_output(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(OUTPUT_WINDOW) {
        return Ok(window);
    }
    let state = app.state::<AppState>();
    // Fallback — should have been pre-created. On Windows, never build inline
    // from a live command handler (inline Build blocks WebView2 IPC); schedule deferred.
    #[cfg(windows)]
    {
        state.logger.log(
            crate::logging::Level::Error,
            "windows: ensure_output FALLBACK triggered — Output window was not pre-created! Scheduling deferred build (Windows inline deadlock avoidance).",
        );
        let app_clone = app.clone();
        let _ = app.run_on_main_thread(move || {
            mark_as_main_thread();
            let r = WebviewWindow::builder(&app_clone, OUTPUT_WINDOW, output_url())
                .title("MakePresent - Output")
                .decorations(false)
                .resizable(false)
                .fullscreen(false)
                .visible(false)
                .build();
            match r {
                Ok(w) => app_clone.state::<AppState>().logger.log(
                    crate::logging::Level::Info,
                    &format!("windows: deferred fallback Output created ({})", describe_window(&w)),
                ),
                Err(e) => app_clone.state::<AppState>().logger.log(
                    crate::logging::Level::Error,
                    &format!("windows: deferred fallback Output FAILED: {e}"),
                ),
            }
        });
        return Err("output window not yet pre-created — deferred build scheduled (retry shortly)".to_string());
    }
    #[cfg(not(windows))]
    {
        state.logger.log(crate::logging::Level::Info, "windows: creating output window");
        let app_main = app.clone();
        return run_on_main(app, &state, "ensure_output", move || {
            match WebviewWindow::builder(&app_main, OUTPUT_WINDOW, output_url())
                .title("MakePresent - Output")
                .decorations(false)
                .resizable(false)
                .fullscreen(false)
                .visible(false)
                .build()
            {
                Ok(window) => {
                    app_main.state::<AppState>().logger.log(
                        crate::logging::Level::Info,
                        &format!(
                            "windows: output window created successfully ({})",
                            describe_window(&window)
                        ),
                    );
                    Ok(window)
                }
                Err(e) => {
                    app_main.state::<AppState>().logger.log(
                        crate::logging::Level::Error,
                        &format!("windows: FAILED to create output window: {e}"),
                    );
                    Err(format!("failed to create output window: {e}"))
                }
            }
        });
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
    // Fallback: should have been pre-created hidden after setup. On Windows, never build inline
    // from a live command handler — that inline Build blocks WebView2 IPC and deadlocks the app.
    // Log loudly and schedule a genuinely deferred next-tick build instead.
    #[cfg(windows)]
    {
        state.logger.log(
            crate::logging::Level::Error,
            "windows: ensure_stage FALLBACK triggered — Stage window was not pre-created! Scheduling deferred build (Windows inline deadlock avoidance, not inline even if is_main_thread).",
        );
        let app_clone = app.clone();
        let _ = app.run_on_main_thread(move || {
            mark_as_main_thread();
            let r = WebviewWindow::builder(&app_clone, STAGE_WINDOW, stage_url())
                .title("MakePresent - Stage Display")
                .resizable(true)
                .visible(false)
                .build();
            match r {
                Ok(w) => app_clone.state::<AppState>().logger.log(
                    crate::logging::Level::Info,
                    &format!("windows: deferred fallback Stage created ({})", describe_window(&w)),
                ),
                Err(e) => app_clone.state::<AppState>().logger.log(
                    crate::logging::Level::Error,
                    &format!("windows: deferred fallback Stage FAILED: {e}"),
                ),
            }
        });
        return Err("stage window not yet pre-created — deferred build scheduled (retry shortly)".to_string());
    }
    #[cfg(not(windows))]
    {
        state.logger.log(crate::logging::Level::Info, "windows: creating stage window");
        let app_main = app.clone();
        return run_on_main(app, &state, "ensure_stage", move || {
            match WebviewWindow::builder(&app_main, STAGE_WINDOW, stage_url())
                .title("MakePresent - Stage Display")
                .resizable(true)
                .visible(false)
                .build()
            {
                Ok(window) => {
                    app_main.state::<AppState>().logger.log(
                        crate::logging::Level::Info,
                        &format!(
                            "windows: stage window created successfully ({})",
                            describe_window(&window)
                        ),
                    );
                    Ok(window)
                }
                Err(e) => {
                    app_main.state::<AppState>().logger.log(
                        crate::logging::Level::Error,
                        &format!("windows: FAILED to create stage window: {e}"),
                    );
                    Err(format!("failed to create stage window: {e}"))
                }
            }
        });
    }
}

/// Pre-create hidden Output+Stage windows once, unconditionally, after setup
/// via a deferred next-tick dispatch. This ensures live command handlers
/// (set_live_slide, show_output, toggle_stage) never call builder().build()
/// and therefore never block WebView2 IPC on Windows.
pub fn precreate_hidden_windows(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.logger.log(
        crate::logging::Level::Info,
        "windows: pre-create hidden Output+Stage (deferred, not blocking setup)",
    );
    if app.get_webview_window(OUTPUT_WINDOW).is_none() {
        match WebviewWindow::builder(app, OUTPUT_WINDOW, output_url())
            .title("MakePresent - Output")
            .decorations(false)
            .resizable(false)
            .fullscreen(false)
            .visible(false)
            .build()
        {
            Ok(w) => state.logger.log(
                crate::logging::Level::Info,
                &format!("windows: pre-created Output hidden ({})", describe_window(&w)),
            ),
            Err(e) => state.logger.log(
                crate::logging::Level::Error,
                &format!("windows: pre-create Output FAILED: {e}"),
            ),
        }
    } else {
        state.logger.log(
            crate::logging::Level::Info,
            "windows: Output already exists, skip pre-create",
        );
    }
    if app.get_webview_window(STAGE_WINDOW).is_none() {
        match WebviewWindow::builder(app, STAGE_WINDOW, stage_url())
            .title("MakePresent - Stage Display")
            .resizable(true)
            .visible(false)
            .build()
        {
            Ok(w) => state.logger.log(
                crate::logging::Level::Info,
                &format!("windows: pre-created Stage hidden ({})", describe_window(&w)),
            ),
            Err(e) => state.logger.log(
                crate::logging::Level::Error,
                &format!("windows: pre-create Stage FAILED: {e}"),
            ),
        }
    } else {
        state.logger.log(
            crate::logging::Level::Info,
            "windows: Stage already exists, skip pre-create",
        );
    }
    let count = app.webview_windows().len();
    state.logger.log(
        crate::logging::Level::Info,
        &format!("windows: pre-create complete — window count = {count}"),
    );
}

/// Place the stage window on the given display, windowed at ~70% of the
/// monitor, and show it. Same display-picker pattern as the output window.
/// All window work happens on the main thread via `run_on_main`.
pub fn move_stage_to(app: &AppHandle, monitor_index: usize) -> Result<WebviewWindow, String> {
    let state = app.state::<AppState>();
    let app_main = app.clone();
    run_on_main(app, &state, "move_stage_to", move || {
        let editor = editor_window(&app_main)?;
        let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
        let monitor = monitors.get(monitor_index).ok_or_else(|| {
            let msg = format!(
                "invalid display index {monitor_index} (only {} monitors)",
                monitors.len()
            );
            app_main.state::<AppState>().logger.log(
                crate::logging::Level::Error,
                &format!("windows: move_stage_to: {msg}"),
            );
            msg
        })?;
        let logger = &app_main.state::<AppState>().logger;
        let name = monitor.name().cloned().unwrap_or_else(|| "(unnamed)".to_string());
        let target_pos = *monitor.position();
        let target_size = monitor.size();
        logger.log(
            crate::logging::Level::Info,
            &format!(
                "windows: move_stage_to: monitor #{monitor_index} \"{name}\" is {}x{} at ({}, {})",
                target_size.width, target_size.height, target_pos.x, target_pos.y
            ),
        );

        // Fast HashMap lookup — never calls builder().build() from a live handler (Windows deadlock avoidance).
        // Window should have been pre-created hidden after setup; fallback is deferred, not inline.
        let window = match app_main.get_webview_window(STAGE_WINDOW) {
            Some(w) => w,
            None => {
                logger.log(
                    crate::logging::Level::Error,
                    "windows: move_stage_to — Stage window not pre-created! Scheduling deferred build (fallback, should not happen after pre-create)",
                );
                #[cfg(windows)]
                {
                    let ac_for_run = app_main.clone();
                    let ac_for_build = app_main.clone();
                    let _ = ac_for_run.run_on_main_thread(move || {
                        mark_as_main_thread();
                        let r = WebviewWindow::builder(&ac_for_build, STAGE_WINDOW, stage_url())
                            .title("MakePresent - Stage Display")
                            .resizable(true)
                            .visible(false)
                            .build();
                        if let Err(e) = r {
                            ac_for_build.state::<AppState>().logger.log(
                                crate::logging::Level::Error,
                                &format!("windows: deferred Stage fallback FAILED: {e}"),
                            );
                        }
                    });
                    return Err("stage window not pre-created — deferred build scheduled".to_string());
                }
                #[cfg(not(windows))]
                {
                    ensure_stage(&app_main)?
                }
            }
        };
        let w = (target_size.width as f64 * 0.7).round() as u32;
        let h = (target_size.height as f64 * 0.7).round() as u32;
        let size_res = window.set_size(Size::Physical(PhysicalSize::new(w, h)));
        let pos_res = window.set_position(Position::Physical(target_pos));
        let show_res = window.show();
        logger.log(
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
    })
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
/// All window work happens on the main thread via `run_on_main`.
pub fn move_output_to(app: &AppHandle, monitor_index: usize) -> Result<WebviewWindow, String> {
    let state = app.state::<AppState>();
    let app_main = app.clone();
    run_on_main(app, &state, "move_output_to", move || {
        let editor = editor_window(&app_main)?;
        let monitors = editor.available_monitors().map_err(|e| e.to_string())?;
        let monitor = monitors.get(monitor_index).ok_or_else(|| {
            let msg = format!(
                "invalid display index {monitor_index} (only {} monitors)",
                monitors.len()
            );
            app_main.state::<AppState>().logger.log(
                crate::logging::Level::Error,
                &format!("windows: move_output_to: {msg}"),
            );
            msg
        })?;
        let logger = &app_main.state::<AppState>().logger;
        let name = monitor.name().cloned().unwrap_or_else(|| "(unnamed)".to_string());
        let target_pos = *monitor.position();
        let target_size = monitor.size();
        logger.log(
            crate::logging::Level::Info,
            &format!(
                "windows: move_output_to: monitor #{monitor_index} \"{name}\" is {}x{} at ({}, {})",
                target_size.width, target_size.height, target_pos.x, target_pos.y
            ),
        );

        // Fast HashMap lookup — never calls builder().build() from a live handler (Windows deadlock avoidance).
        let window = match app_main.get_webview_window(OUTPUT_WINDOW) {
            Some(w) => w,
            None => {
                logger.log(
                    crate::logging::Level::Error,
                    "windows: move_output_to — Output window not pre-created! Scheduling deferred build (fallback)",
                );
                #[cfg(windows)]
                {
                    let ac_for_run = app_main.clone();
                    let ac_for_build = app_main.clone();
                    let _ = ac_for_run.run_on_main_thread(move || {
                        mark_as_main_thread();
                        let r = WebviewWindow::builder(&ac_for_build, OUTPUT_WINDOW, output_url())
                            .title("MakePresent - Output")
                            .decorations(false)
                            .resizable(false)
                            .fullscreen(false)
                            .visible(false)
                            .build();
                        if let Err(e) = r {
                            ac_for_build.state::<AppState>().logger.log(
                                crate::logging::Level::Error,
                                &format!("windows: deferred Output fallback FAILED: {e}"),
                            );
                        }
                    });
                    return Err("output window not pre-created — deferred build scheduled".to_string());
                }
                #[cfg(not(windows))]
                {
                    ensure_output(&app_main)?
                }
            }
        };
        let exit_fs = if window.is_fullscreen().unwrap_or(false) {
            Some(window.set_fullscreen(false))
        } else {
            None
        };
        // Single-monitor / same-monitor mitigation: when the output target is
        // the same screen the editor is on, a full-monitor borderless window
        // would completely cover the editor and make it appear frozen. In that
        // case create a centered windowed preview instead so the editor stays
        // reachable. Multi-monitor keeps the true full-monitor placement.
        let same_as_editor = editor
            .current_monitor()
            .ok()
            .flatten()
            .is_some_and(|c| same_monitor(&c, monitor));
        let single = monitors.len() <= 1;
        let (place_w, place_h, place_pos) = if same_as_editor || single {
            let w = (target_size.width as f64 * 0.72).round() as u32;
            let h = (target_size.height as f64 * 0.72).round() as u32;
            let x = target_pos.x + ((target_size.width as i32 - w as i32) / 2);
            let y = target_pos.y + ((target_size.height as i32 - h as i32) / 2);
            (w, h, tauri::PhysicalPosition::new(x, y))
        } else {
            (
                target_size.width,
                target_size.height,
                tauri::PhysicalPosition::new(target_pos.x, target_pos.y),
            )
        };
        if same_as_editor || single {
            let _ = window.set_decorations(true);
            let _ = window.set_resizable(true);
        } else {
            let _ = window.set_decorations(false);
            let _ = window.set_resizable(false);
        }
        // Explicitly size the window to the target dimensions
        // *before* any subsequent set_fullscreen. On Linux/GTK, relying on
        // set_fullscreen alone with a stale/default window size can leave the
        // window smaller than the monitor depending on the window manager
        // (X11 vs Wayland). Sizing up-front gives the WM correct bounds to
        // work from when the fullscreen toggle lands.
        let size_res = window.set_size(Size::Physical(PhysicalSize::new(place_w, place_h)));
        let pos_res = window.set_position(Position::Physical(place_pos));
        let show_res = window.show();
        logger.log(
            crate::logging::Level::Info,
            &format!(
                "windows: move_output_to: exit_fullscreen -> {:?}, set_size({}x{}) -> {:?}, set_position({}, {}) -> {:?}, show() -> {:?}; after: {}",
                exit_fs,
                place_w,
                place_h,
                size_res,
                place_pos.x,
                place_pos.y,
                pos_res,
                show_res,
                describe_window(&window)
            ),
        );
        show_res.map_err(|e| e.to_string())?;
        Ok(window)
    })
}

/// Whether the on-demand output window currently exists and is showing.
///
/// On Windows (WebView2) `WebviewWindow::is_visible()` dispatches a message to
/// the main thread and blocks the caller waiting for a reply. After the first
/// WebView2 window is created the main thread's message pump can be degraded
/// (known Tauri/wry issue on Windows), so a worker thread calling
/// `is_visible()` would block forever and freeze every subsequent backend
/// command that touches `snapshot()` (which is every `mutate`). Existence in the
/// window manager (`get_webview_window`) is a HashMap lookup under a local lock
/// and never touches the main thread, so we use that instead. The window is
/// kept `visible(false)` until `show()` and never hidden again except via
/// explicit hide, so existence is a reliable proxy for "should be visible".
pub fn output_visible(app: &AppHandle) -> bool {
    app.get_webview_window(OUTPUT_WINDOW).is_some()
}

/// Show (and create if needed) the output window on the configured or
/// auto-picked display. This is the "Show Output" action used both by the
/// explicit button and by the first slide going live.
///
/// On Windows, `move_output_to`/`ensure_output` must run on the main thread
/// and WebView2 creation can temporarily degrade the event loop. To keep
/// backend commands (`add_slide`, `set_live_slide`, etc.) responsive, this
/// function queues all window work fire-and-forget on the main thread and
/// returns immediately. The caller (`set_live_slide`) can then `snapshot` +
/// `emit` without waiting for WebView2, so subsequent `invoke`s are not
/// blocked behind window creation.
pub fn show_output(app: &AppHandle, _state: &AppState) -> Result<(), String> {
    // All window/monitor work must happen on the main thread. Queue it
    // fire-and-forget so the command worker (set_live_slide) can snapshot +
    // emit without waiting for WebView2 creation, which on Windows can
    // temporarily degrade the event loop and would otherwise block every
    // subsequent `invoke`.
    let app_for_queue = app.clone();

    run_on_main_async(app, "show_output", move || {
        let st = app_for_queue.state::<AppState>();
        st.logger.log(
            crate::logging::Level::Info,
            "windows: show_output: queued work starting on main thread",
        );

        // Diagnostic: full monitor topology as reported on the main thread.
        match list_displays(&app_for_queue) {
            Ok(displays) => {
                st.logger.log(
                    crate::logging::Level::Info,
                    &format!(
                        "windows: show_output: {} monitor(s) via list_displays:",
                        displays.len()
                    ),
                );
                for d in &displays {
                    st.logger.log(
                        crate::logging::Level::Info,
                        &format!(
                            "windows: show_output:   #{} \"{}\" {}x{} at ({}, {}) primary={} current={}",
                            d.index, d.name, d.width, d.height, d.x, d.y, d.primary, d.current
                        ),
                    );
                }
            }
            Err(e) => st.logger.log(
                crate::logging::Level::Error,
                &format!("windows: show_output: list_displays failed: {e}"),
            ),
        }

        let settings = st.current_settings();
        let index = match settings.output_display_index {
            Some(idx) => idx,
            None => match default_output_display(&app_for_queue) {
                Ok(i) => i,
                Err(e) => {
                    st.logger.log(
                        crate::logging::Level::Error,
                        &format!("windows: show_output: default_output_display failed: {e}"),
                    );
                    return;
                }
            },
        };

        // All window work happens on the main thread — `move_output_to` will
        // take the `already on main thread` inline path, no extra dispatch.
        let window = match move_output_to(&app_for_queue, index) {
            Ok(w) => w,
            Err(e) => {
                st.logger.log(
                    crate::logging::Level::Error,
                    &format!("windows: show_output: move_output_to failed: {e}"),
                );
                return;
            }
        };
        let focus_res = window.set_focus();
        st.logger.log(
            crate::logging::Level::Info,
            &format!("windows: show_output: set_focus result: {:?}", focus_res),
        );

        // On Linux/GTK, requesting fullscreen on the *same* synchronous
        // main-thread pass that just created an unshown window can leave the
        // window smaller than the monitor. Defer the fullscreen request so the
        // GTK event loop processes the geometry first.
        if settings.output_fullscreen {
            let target = list_displays(&app_for_queue)
                .ok()
                .and_then(|d| d.into_iter().find(|d| d.index == index));
            let (tw, th, tname) = match target {
                Some(d) => (d.width, d.height, d.name),
                None => (0, 0, "(unknown)".to_string()),
            };
            let app_clone2 = app_for_queue.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(120));
                let app_borrowed = app_clone2.clone();
                let st2 = app_borrowed.state::<AppState>();
                let _ = run_on_main(
                    &app_borrowed,
                    &st2,
                    "show_output_deferred_fullscreen",
                    move || {
                        let w = app_clone2
                            .get_webview_window(OUTPUT_WINDOW)
                            .ok_or_else(|| "output window missing".to_string())?;
                        let fs_res = w.set_fullscreen(true);
                        let inner = w.inner_size().ok();
                        let outer = w.outer_size().ok();
                        let logger = &app_clone2.state::<AppState>().logger;
                        logger.log(
                            crate::logging::Level::Info,
                            &format!(
                                "windows: show_output: deferred set_fullscreen(true) -> {:?}, inner_size={inner:?} outer_size={outer:?}, monitor \"{tname}\" is {tw}x{th}; fullscreen={:?}; {}",
                                fs_res,
                                w.is_fullscreen().ok(),
                                describe_window(&w)
                            ),
                        );
                        let mismatch = inner
                            .map(|s| (s.width != tw, s.height != th))
                            .unwrap_or((false, false));
                        if mismatch.0 || mismatch.1 {
                            logger.log(
                                crate::logging::Level::Warn,
                                &format!(
                                    "windows: show_output: SIZE MISMATCH — window inner is {:?} but monitor \"{tname}\" is {tw}x{th}; fullscreen did not fill the display",
                                    inner
                                ),
                            );
                        }
                        Ok(())
                    },
                );
            });
        }

        // Persist the resolved assignment so restarts keep the same display.
        let name = list_displays(&app_for_queue)
            .ok()
            .and_then(|displays| displays.into_iter().find(|d| d.index == index))
            .and_then(|d| (!d.name.is_empty()).then_some(d.name));
        let mut resolved = settings;
        resolved.output_display_index = Some(index);
        resolved.output_display_name = name;
        st.apply_settings(resolved);
        let _ = crate::project::write_settings(
            &st.app_data_dir(),
            &st.current_settings(),
        );
        st.logger.log(
            crate::logging::Level::Info,
            "windows: show_output: queued work completed on main thread",
        );
    })
    .map_err(|e| e.to_string())?;

    Ok(())
}