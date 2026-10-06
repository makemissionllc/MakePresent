//! Windows-only WGC supervisor and GPU readback callback. No renderer IPC.
use super::{prepare_bgra, CaptureWorker};
use crate::{logging::Level, state::AppState, windows::OutputCaptureTarget};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

const PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 30);
const POLL: Duration = Duration::from_millis(250);

// Only instantaneous Win32 handle predicates: no message dispatch or window
// mutation. This also guards a hide/minimize between supervisor snapshots.
#[link(name = "user32")]
extern "system" {
    fn IsWindow(hwnd: *mut std::ffi::c_void) -> i32;
    fn IsWindowVisible(hwnd: *mut std::ffi::c_void) -> i32;
    fn IsIconic(hwnd: *mut std::ffi::c_void) -> i32;
}

#[link(name = "runtimeobject")]
extern "system" {
    fn RoInitialize(apartment_type: u32) -> i32;
    fn RoUninitialize();
}

struct WinRtApartment;
impl WinRtApartment {
    fn initialize() -> Result<Self, String> {
        // SAFETY: this fresh supervisor thread owns its MTA initialization.
        let result = unsafe { RoInitialize(1) };
        if result < 0 {
            Err(format!(
                "WinRT initialization failed (HRESULT 0x{:08x})",
                result as u32
            ))
        } else {
            Ok(Self)
        }
    }
}
impl Drop for WinRtApartment {
    fn drop(&mut self) {
        // SAFETY: balances successful RoInitialize on the same thread.
        unsafe { RoUninitialize() };
    }
}

#[derive(Default)]
struct Progress {
    captured: AtomicU64,
    dropped: AtomicU64,
    errors: AtomicU64,
    last: Mutex<Option<Instant>>,
}

#[derive(Clone)]
struct Flags {
    app: AppHandle,
    hwnd: usize,
    stop: Arc<AtomicBool>,
    allowed: Arc<AtomicBool>,
    progress: Arc<Progress>,
}

struct OutputCapture {
    flags: Flags,
    last_processed: Option<Instant>,
    first: bool,
}

impl GraphicsCaptureApiHandler for OutputCapture {
    type Flags = Flags;
    type Error = String;

    fn new(context: Context<Flags>) -> Result<Self, String> {
        Ok(Self {
            flags: context.flags,
            last_processed: None,
            first: true,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame<'_>,
        control: InternalCaptureControl,
    ) -> Result<(), String> {
        if self.flags.stop.load(Ordering::Acquire) || !self.flags.allowed.load(Ordering::Acquire) {
            control.stop();
            return Ok(());
        }
        let hwnd = self.flags.hwnd as *mut std::ffi::c_void;
        // SAFETY: Win32 predicates accept stale HWNDs and return false; the
        // numeric handle came exclusively from Tauri's Output window.
        if unsafe { IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 } {
            return Ok(());
        }
        if self.last_processed.is_some_and(|at| at.elapsed() < PERIOD) {
            self.flags.progress.dropped.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        self.last_processed = Some(Instant::now());
        let result = (|| {
            // Bgra8 is requested from WGC, so keep channel order and strip
            // D3D row padding before submitting a packed, <=1080p frame.
            let mut buffer = frame.buffer().map_err(|e| e.to_string())?;
            let width = buffer.width();
            let height = buffer.height();
            let stride = buffer.row_pitch() as usize;
            let prepared = prepare_bgra(width, height, stride, buffer.as_raw_buffer())?;
            if self.first {
                self.first = false;
                log(
                    &self.flags.app,
                    Level::Info,
                    &format!(
                        "first frame hwnd=0x{:x} size={}x{} stride={stride} BGRA={}x{}",
                        self.flags.hwnd, width, height, prepared.0, prepared.1
                    ),
                );
            }
            Ok::<_, String>(prepared)
        })();
        let state = self.flags.app.state::<AppState>();
        match result {
            Ok((width, height, pixels)) => {
                if self.flags.stop.load(Ordering::Acquire)
                    || !self.flags.allowed.load(Ordering::Acquire)
                {
                    return Ok(());
                }
                self.flags.progress.captured.fetch_add(1, Ordering::Relaxed);
                *self.flags.progress.last.lock().unwrap() = Some(Instant::now());
                if !state.broadcaster.send_frame(width, height, pixels) {
                    self.flags.progress.dropped.fetch_add(1, Ordering::Relaxed);
                } else {
                    state.broadcaster.set_capture_issue(None);
                }
                Ok(())
            }
            Err(reason) => {
                self.flags.progress.errors.fetch_add(1, Ordering::Relaxed);
                state
                    .broadcaster
                    .set_capture_issue(Some(format!("Capture error: {reason}")));
                // Returning an error ends this WGC session. Supervisor joins
                // it, logs the reason and retries; sender holds its last image.
                Err(reason)
            }
        }
    }

    fn on_closed(&mut self) -> Result<(), String> {
        self.flags.allowed.store(false, Ordering::Release);
        Ok(())
    }
}

fn log(app: &AppHandle, level: Level, message: &str) {
    app.state::<AppState>()
        .logger
        .log(level, &format!("ndi-capture: method=WGC {message}"));
}

fn start_session(flags: Flags) -> Result<CaptureControl<OutputCapture, String>, String> {
    let window = Window::from_raw_hwnd(flags.hwnd as *mut std::ffi::c_void);
    let border = if GraphicsCaptureApi::is_border_settings_supported().map_err(|e| e.to_string())? {
        DrawBorderSettings::WithoutBorder
    } else {
        log(
            &flags.app,
            Level::Info,
            "border suppression unavailable on this Windows version",
        );
        DrawBorderSettings::Default
    };
    let secondary =
        if GraphicsCaptureApi::is_secondary_windows_supported().map_err(|e| e.to_string())? {
            SecondaryWindowSettings::Exclude
        } else {
            SecondaryWindowSettings::Default
        };
    let interval =
        if GraphicsCaptureApi::is_minimum_update_interval_supported().map_err(|e| e.to_string())? {
            MinimumUpdateIntervalSettings::Custom(PERIOD)
        } else {
            MinimumUpdateIntervalSettings::Default
        };
    // The crate owns a dedicated WGC/D3D thread; this is called only from the
    // dedicated supervisor. Callback also limits processing to <=30 fps.
    OutputCapture::start_free_threaded(Settings::new(
        window,
        CursorCaptureSettings::WithoutCursor,
        border,
        secondary,
        interval,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        flags,
    ))
    .map_err(|e| e.to_string())
}

fn target(app: &AppHandle, stop: &AtomicBool) -> Result<Option<OutputCaptureTarget>, String> {
    let rx = crate::windows::request_output_capture_target(app)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(None);
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => return result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
            Err(e) => return Err(format!("Output window lookup failed: {e}")),
        }
    }
}

fn stop_session(
    app: &AppHandle,
    session: &mut Option<CaptureControl<OutputCapture, String>>,
) -> bool {
    if let Some(control) = session.take() {
        if let Err(e) = control.stop() {
            log(
                app,
                Level::Warn,
                &format!("session stopped with capture error: {e}"),
            );
            app.state::<AppState>()
                .broadcaster
                .set_capture_issue(Some(format!("Capture error: {e}")));
            return false;
        }
    }
    true
}

pub(super) fn spawn(app: AppHandle) -> Result<CaptureWorker, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let thread = std::thread::Builder::new().name("ndi-wgc-supervisor".into()).spawn(move || {
        log(&app, Level::Info, "started; Output only, no cursor, <=30fps, real black accepted");
        // Broadcaster publishes the core after spawning us. Wait so an early
        // initialization error cannot be overwritten by start's status reset.
        while !app.state::<AppState>().broadcaster.is_active() {
            if thread_stop.load(Ordering::Acquire) { return; }
            std::thread::park_timeout(Duration::from_millis(20));
        }
        let _apartment = match WinRtApartment::initialize() {
            Ok(apartment) => apartment,
            Err(e) => {
                app.state::<AppState>().broadcaster.set_capture_issue(Some(format!("Capture error: {e}")));
                log(&app, Level::Warn, &e);
                let _ = crate::commands::snapshot_and_emit(&app);
                return;
            }
        };
        let progress = Arc::new(Progress::default());
        let mut session = None;
        let mut current = None;
        let mut allowed = Arc::new(AtomicBool::new(false));
        let mut started = Instant::now();
        let mut last_log = Instant::now();
        let mut last_status = String::new();
        let mut last_observed = None;
        let mut last_error = String::new();
        let mut retry_at = Instant::now();
        while !thread_stop.load(Ordering::Acquire) {
            if !app.state::<AppState>().broadcaster.is_active() {
                std::thread::park_timeout(Duration::from_millis(20));
                continue;
            }
            let observed = target(&app, &thread_stop);
            if thread_stop.load(Ordering::Acquire) { break; }
            let (next, issue) = match observed {
                Ok(Some(window)) => {
                    if last_observed.as_ref() != Some(&window) {
                        log(&app, Level::Info, &format!("Output found title={:?} hwnd=0x{:x} visible={} minimized={} display={}",
                            window.title, window.hwnd, window.visible, window.minimized, window.display_signature));
                        last_observed = Some(window.clone());
                    }
                    if !window.visible {
                        (None, Some("Output window is hidden. Click Show Output.".into()))
                    } else if window.minimized {
                        (None, Some("Output window is minimized. Restore Output.".into()))
                    } else { (Some(window), None) }
                }
                Ok(None) => {
                    if last_observed.take().is_some() || last_status.is_empty() {
                        log(&app, Level::Info, "Output not found; title/HWND unavailable");
                    }
                    (None, Some("Output window not found. Click Show Output.".into()))
                }
                Err(e) => (None, Some(format!("Capture error: {e}"))),
            };
            let finished = session.as_ref().is_some_and(|s: &CaptureControl<OutputCapture, String>| s.is_finished());
            let stalled = session.is_some() && started.elapsed() > Duration::from_secs(5)
                && progress.last.lock().unwrap().is_none_or(|at| at.elapsed() > Duration::from_secs(5));
            if current != next || finished || stalled || (!allowed.load(Ordering::Acquire) && Instant::now() >= retry_at) {
                allowed.store(false, Ordering::Release);
                let stopped_cleanly = stop_session(&app, &mut session);
                current = next.clone();
                if stalled { log(&app, Level::Warn, "no recent WGC frame; restarting session"); }
                if !stopped_cleanly {
                    retry_at = Instant::now() + Duration::from_secs(1);
                } else if let Some(window) = next {
                    allowed = Arc::new(AtomicBool::new(true));
                    let flags = Flags { app: app.clone(), hwnd: window.hwnd, stop: thread_stop.clone(), allowed: allowed.clone(), progress: progress.clone() };
                    app.state::<AppState>().broadcaster.set_capture_issue(Some("Waiting for Output capture".into()));
                    match start_session(flags) {
                        Ok(control) => {
                            session = Some(control);
                            started = Instant::now();
                        }
                        Err(e) => {
                            progress.errors.fetch_add(1, Ordering::Relaxed);
                            retry_at = Instant::now() + Duration::from_secs(1);
                            allowed.store(false, Ordering::Release);
                            app.state::<AppState>().broadcaster.set_capture_issue(Some(format!("Capture error: {e}")));
                            if e != last_error || last_log.elapsed() >= Duration::from_secs(5) {
                                log(&app, Level::Warn, &format!("Capture error: {e}"));
                                last_error = e;
                                last_log = Instant::now();
                            }
                        }
                    }
                }
            }
            if let Some(issue) = issue {
                app.state::<AppState>().broadcaster.set_capture_issue(Some(issue));
            }
            let message = app.state::<AppState>().broadcaster.status_message();
            if message != last_status {
                log(&app, Level::Info, &format!("status={message}"));
                last_status = message;
                let _ = crate::commands::snapshot_and_emit(&app);
            }
            if last_log.elapsed() >= Duration::from_secs(5) {
                log(&app, Level::Info, &format!("captured={} dropped={} capture_errors={} rejected_black=0",
                    progress.captured.load(Ordering::Relaxed), progress.dropped.load(Ordering::Relaxed), progress.errors.load(Ordering::Relaxed)));
                last_log = Instant::now();
            }
            std::thread::park_timeout(POLL);
        }
        allowed.store(false, Ordering::Release);
        stop_session(&app, &mut session);
        log(&app, Level::Info, &format!("stopped; captured={} dropped={} capture_errors={} rejected_black=0",
            progress.captured.load(Ordering::Relaxed), progress.dropped.load(Ordering::Relaxed), progress.errors.load(Ordering::Relaxed)));
    }).map_err(|e| format!("could not start WGC supervisor: {e}"))?;
    Ok(CaptureWorker { stop, thread })
}
