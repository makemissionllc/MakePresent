//! NDI receive confidence monitor — low-rate preview of a network source.
//!
//! This is the *receiving* counterpart to [`crate::broadcast`] (which only
//! sends). It answers one narrow question at ~2 fps: "is this camera alive
//! and pointed at the right thing?" It is explicitly NOT live video.
//!
//! # Why the same runtime-load pattern as send
//!
//! The NDI SDK is closed-source, not vendored, and absent from CI machines.
//! Like `broadcast.rs`, this module hand-binds the C ABI with `libloading`
//! and loads `Processing.NDI.Lib.x64.dll` / `libndi.so.5` at runtime. If the
//! SDK is missing, starting the monitor returns a clear error and everything
//! else keeps working. Fully independent from the sender: separate toggle,
//! separate thread, separate SDK handles. (Both sides call
//! `NDIlib_initialize`/`NDIlib_destroy`; the SDK reference-counts those, the
//! same way ffmpeg's simultaneous NDI muxer+demuxer relies on.)
//!
//! # FFI values — verified, not recalled
//!
//! All enum integers and struct layouts below were verified against the SDK
//! headers (via the DistroAV header mirror, cross-checked with the
//! `gst-plugin-ndi` Rust bindings and the official NDI SDK docs):
//! - `NDIlib_recv_color_format_BGRX_BGRA = 0`,
//!   `NDIlib_recv_bandwidth_highest = 100` (`Processing.NDI.Recv.h`)
//! - `NDIlib_recv_create_v3_t` field order: source-by-value, color_format,
//!   bandwidth, allow_video_fields, name (`Processing.NDI.Recv.h`)
//! - `NDIlib_find_create_t` field order: show_local_sources, groups,
//!   extra_ips (`Processing.NDI.Find.h`)
//! - Frame types: none=0, video=1, audio=2, metadata=3, error=4,
//!   status_change=100 (`Processing.NDI.structs.h`)
//!
//! # Threading (Windows-freeze-safe by construction)
//!
//! One dedicated `ndi-monitor` thread owns the loaded library, the finder,
//! and the receiver. `NDIlib_find_wait_for_sources` and
//! `NDIlib_recv_capture_v2` both block with timeouts, so they never run on a
//! Tauri command handler or the main thread — same rule as the WebView2
//! freeze fixes. Commands only push [`Control`] messages or read the shared
//! snapshot; `stop()` joins the thread (bounded by the 250 ms capture
//! timeout, same precedent as `osc.rs`'s join-on-stop).
//!
//! # Frame delivery — the MJPEG swap seam
//!
//! The capture loop produces small RGB [`PreviewFrame`]s and hands them to
//! [`deliver_preview_frame`]. That function is the *only* place that knows
//! how frames reach the frontend (today: JPEG+base64 over the dedicated
//! `ndi-preview-frame` event). To swap in MJPEG-over-localhost later, replace
//! that one function with an HTTP multipart writer — the capture loop,
//! ownership discipline, and stale detection are untouched. Frames NEVER go
//! through `snapshot_and_emit`: pushing base64 through the `state` broadcast
//! would serialize it to every window per frame (the IPC-saturation freeze
//! class), so delivery uses dedicated events only.

use libloading::{Library, Symbol};
use base64::Engine as _;
use serde::Serialize;
use std::ffi::{c_char, c_void, CStr, CString};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Behavioural constants
// ---------------------------------------------------------------------------

/// Preview width in pixels (spec: 480–640). Height follows aspect ratio.
/// Small on purpose: keeps JPEG encode + base64 + IPC cheap at 2 fps.
pub const PREVIEW_WIDTH: u32 = 560;
/// Delivery cadence regardless of actual capture rate (~2 fps).
const EMIT_PERIOD: Duration = Duration::from_millis(500);
/// No captured frame for this long while "connected" => STALE. Covers both
/// a dead source and a source the SDK auto-reconnect is silently holding.
const STALE_AFTER: Duration = Duration::from_secs(4);
/// Blocking timeout for each capture call: bounds loop tick and stop-join.
const CAPTURE_TIMEOUT_MS: u32 = 250;
/// Blocking timeout for discovery polling while not connected.
const FIND_WAIT_MS: u32 = 500;
/// JPEG quality for preview frames (small events over IPC matter more than
/// pixel perfection at 560 px).
const JPEG_QUALITY: u8 = 60;
/// Sanity cap on a received frame buffer (256 MB — far above 4K BGRA's
/// ~33 MB; protects the raw-pointer copy from insane stride values).
const MAX_FRAME_BYTES: u64 = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// NDI C ABI — hand-written mirrors (see module docs for verification)
// ---------------------------------------------------------------------------

/// `NDIlib_source_t` — `{ p_ndi_name, p_url_address }`.
#[repr(C)]
struct NdiSourceRaw {
    p_ndi_name: *const c_char,
    p_url_address: *const c_char,
}

/// `NDIlib_find_create_t` — `{ show_local_sources, p_groups, p_extra_ips }`.
#[repr(C)]
struct FindCreate {
    show_local_sources: u8,
    p_groups: *const c_char,
    p_extra_ips: *const c_char,
}

/// `NDIlib_recv_create_v3_t` — source by value, color_format, bandwidth,
/// allow_video_fields, name. We request BGRX_BGRA (0) explicitly: the SDK
/// then hands us BGRX/BGRA directly and no YUV conversion path exists in
/// this module. (The C++ default constructor would give UYVY_BGRA — passing
/// NULL is NOT what we want, hence the explicit struct.)
#[repr(C)]
struct RecvCreateV3 {
    source: NdiSourceRaw,
    color_format: c_int,
    bandwidth: c_int,
    allow_video_fields: u8,
    p_ndi_recv_name: *const c_char,
}

/// `NDIlib_recv_color_format_BGRX_BGRA` — no-alpha gives BGRX, alpha gives
/// BGRA. Both are 4 bytes/pixel, BGR byte order.
const COLOR_BGRX_BGRA: c_int = 0;
/// `NDIlib_recv_bandwidth_highest` — full resolution for the preview.
const BANDWIDTH_HIGHEST: c_int = 100;

/// `NDIlib_video_frame_v2_t` — same layout as the send-side mirror in
/// `broadcast.rs` (two int, two int, float, int, i64, pointer, int,
/// pointer, i64). Read-only use here.
#[repr(C)]
struct VideoFrame {
    xres: c_int,
    yres: c_int,
    four_cc: i32,
    frame_rate_n: c_int,
    frame_rate_d: c_int,
    picture_aspect_ratio: f32,
    frame_format_type: u32,
    timecode: i64,
    p_data: *const u8,
    line_stride_in_bytes: c_int,
    p_metadata: *const c_char,
    timestamp: i64,
}

/// FourCCs this module can arrive in (BGRX_BGRA mode only). Values match
/// `broadcast.rs`'s `FourCC` and the SDK header.
const FOURCC_BGRA: i32 = 0x4152_4742;
const FOURCC_BGRX: i32 = 0x5852_4742;

/// `NDIlib_frame_type_e` return values of `recv_capture_v2`.
const FRAME_NONE: i32 = 0;
const FRAME_VIDEO: i32 = 1;
const FRAME_ERROR: i32 = 4;
const FRAME_STATUS_CHANGE: i32 = 100;

type FindInstance = *mut c_void;
type RecvInstance = *mut c_void;

fn is_null(p: *mut c_void) -> bool {
    p.is_null()
}

/// Dynamically loaded receive-side SDK handles. The `Library` is kept alive
/// for the monitor thread's lifetime; fn-pointers are `Copy` for handoff.
struct NdiRecvLib {
    _lib: Library,
    initialize: unsafe extern "C" fn() -> c_int,
    destroy: unsafe extern "C" fn(),
    find_create: unsafe extern "C" fn(*const FindCreate) -> FindInstance,
    find_destroy: unsafe extern "C" fn(FindInstance),
    find_wait_for_sources: unsafe extern "C" fn(FindInstance, u32) -> u8,
    find_get_sources: unsafe extern "C" fn(FindInstance, *mut u32) -> *const NdiSourceRaw,
    recv_create: unsafe extern "C" fn(*const RecvCreateV3) -> RecvInstance,
    recv_destroy: unsafe extern "C" fn(RecvInstance),
    recv_connect: unsafe extern "C" fn(RecvInstance, *const NdiSourceRaw),
    recv_capture: unsafe extern "C" fn(RecvInstance, *mut VideoFrame, *const c_void, *const c_void, u32) -> i32,
    recv_free_video: unsafe extern "C" fn(RecvInstance, *const VideoFrame),
}

/// Load the NDI SDK and resolve the receive-side entry points. Same DLL
/// filename as the sender (`broadcast::lib_filename`); missing SDK is a
/// graceful `Err`, never a crash.
unsafe fn load_recv_lib() -> Result<NdiRecvLib, String> {
    let file = crate::broadcast::lib_filename();
    let lib = Library::new(file)
        .map_err(|e| format!("NDI SDK not found (looked for \"{file}\"): {e}"))?;

    let err_of = |n: &str, e: libloading::Error| format!("failed to resolve NDI symbol \"{n}\": {e}");
    let (initialize, destroy, find_create, find_destroy, find_wait_for_sources, find_get_sources, recv_create, recv_destroy, recv_connect, recv_capture, recv_free_video) = {
        let initialize: Symbol<unsafe extern "C" fn() -> c_int> =
            lib.get(b"NDIlib_initialize").map_err(|e| err_of("NDIlib_initialize", e))?;
        let destroy: Symbol<unsafe extern "C" fn()> =
            lib.get(b"NDIlib_destroy").map_err(|e| err_of("NDIlib_destroy", e))?;
        let find_create: Symbol<unsafe extern "C" fn(*const FindCreate) -> FindInstance> =
            lib.get(b"NDIlib_find_create_v2").map_err(|e| err_of("NDIlib_find_create_v2", e))?;
        let find_destroy: Symbol<unsafe extern "C" fn(FindInstance)> =
            lib.get(b"NDIlib_find_destroy").map_err(|e| err_of("NDIlib_find_destroy", e))?;
        let find_wait_for_sources: Symbol<unsafe extern "C" fn(FindInstance, u32) -> u8> =
            lib.get(b"NDIlib_find_wait_for_sources").map_err(|e| err_of("NDIlib_find_wait_for_sources", e))?;
        let find_get_sources: Symbol<unsafe extern "C" fn(FindInstance, *mut u32) -> *const NdiSourceRaw> =
            lib.get(b"NDIlib_find_get_current_sources").map_err(|e| err_of("NDIlib_find_get_current_sources", e))?;
        let recv_create: Symbol<unsafe extern "C" fn(*const RecvCreateV3) -> RecvInstance> =
            lib.get(b"NDIlib_recv_create_v3").map_err(|e| err_of("NDIlib_recv_create_v3", e))?;
        let recv_destroy: Symbol<unsafe extern "C" fn(RecvInstance)> =
            lib.get(b"NDIlib_recv_destroy").map_err(|e| err_of("NDIlib_recv_destroy", e))?;
        let recv_connect: Symbol<unsafe extern "C" fn(RecvInstance, *const NdiSourceRaw)> =
            lib.get(b"NDIlib_recv_connect").map_err(|e| err_of("NDIlib_recv_connect", e))?;
        let recv_capture: Symbol<unsafe extern "C" fn(RecvInstance, *mut VideoFrame, *const c_void, *const c_void, u32) -> i32> =
            lib.get(b"NDIlib_recv_capture_v2").map_err(|e| err_of("NDIlib_recv_capture_v2", e))?;
        let recv_free_video: Symbol<unsafe extern "C" fn(RecvInstance, *const VideoFrame)> =
            lib.get(b"NDIlib_recv_free_video_v2").map_err(|e| err_of("NDIlib_recv_free_video_v2", e))?;
        (
            *initialize, *destroy, *find_create, *find_destroy, *find_wait_for_sources,
            *find_get_sources, *recv_create, *recv_destroy, *recv_connect, *recv_capture,
            *recv_free_video,
        )
    };

    Ok(NdiRecvLib {
        _lib: lib,
        initialize,
        destroy,
        find_create,
        find_destroy,
        find_wait_for_sources,
        find_get_sources,
        recv_create,
        recv_destroy,
        recv_connect,
        recv_capture,
        recv_free_video,
    })
}

// ---------------------------------------------------------------------------
// Frontend contract (dedicated events — never the `state` broadcast)
// ---------------------------------------------------------------------------

/// One discovered NDI source, as the source picker shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NdiSourceInfo {
    pub name: String,
    pub url: String,
}

/// Monitor liveness. `Live` = frames arriving; `Stale` = connected but no
/// fresh frame (the last preview shown must NOT be presented as current).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NdiMonitorState {
    Off,
    Scanning,
    Connecting,
    Live,
    Stale,
    Error,
}

/// Full monitor status pushed over the dedicated `ndi-monitor-status` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NdiMonitorStatus {
    pub state: NdiMonitorState,
    pub source: Option<String>,
    pub message: String,
}

/// One delivered preview frame over the dedicated `ndi-preview-frame` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NdiPreviewEvent {
    pub jpeg_base64: String,
    pub width: u32,
    pub height: u32,
    pub at: String,
}

/// Sources list pushed over the dedicated `ndi-sources` event.
#[derive(Debug, Clone, Serialize)]
pub struct NdiSourcesEvent {
    pub sources: Vec<NdiSourceInfo>,
}

// ---------------------------------------------------------------------------
// Delivery seam — capture produces RGB, this fn alone delivers it
// ---------------------------------------------------------------------------

/// A decoded preview frame ready for delivery: small packed RGB pixels.
/// Produced by the capture loop, consumed by [`deliver_preview_frame`].
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    /// Packed RGB, `width*height*3` bytes.
    pub rgb: Vec<u8>,
}

/// Deliver one preview frame to the frontend (current transport: JPEG+base64
/// over the dedicated `ndi-preview-frame` event, throttled by the caller).
///
/// SWAP POINT for MJPEG-over-localhost: replace this function body with an
/// HTTP `multipart/x-mixed-replace` writer fed by the same `PreviewFrame`s;
/// the capture loop, buffer ownership, and stale detection stay untouched.
fn deliver_preview_frame(app: &AppHandle, frame: &PreviewFrame) {
    let mut jpeg = Vec::new();
    {
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY);
        if encoder
            .encode(&frame.rgb, frame.width, frame.height, image::ExtendedColorType::Rgb8)
            .is_err()
        {
            return;
        }
    }
    // Encoder borrow ended; the buffer is ours again for base64 + emit.
    let payload = NdiPreviewEvent {
        jpeg_base64: base64::engine::general_purpose::STANDARD.encode(&jpeg),
        width: frame.width,
        height: frame.height,
        at: crate::project::now_iso(),
    };
    let _ = app.emit("ndi-preview-frame", &payload);
}

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested, SDK-independent)
// ---------------------------------------------------------------------------

/// Downscale packed BGR(A) (`stride`-pitched, 4 bytes/pixel) to `PREVIEW_WIDTH`
/// packed RGB via nearest-neighbor + BGR→RGB swizzle. Returns `None` for
/// degenerate geometry or short buffers. Native size is kept when the source
/// is already at/below preview width (still re-packed to RGB).
fn downscale_bgr_to_rgb(src: &[u8], sw: u32, sh: u32, stride: usize) -> Option<PreviewFrame> {
    if sw == 0 || sh == 0 {
        return None;
    }
    let sw_us = sw as usize;
    let sh_us = sh as usize;
    if stride < sw_us.saturating_mul(4) {
        return None;
    }
    if (stride as u64).saturating_mul(sh as u64) > src.len() as u64 {
        return None;
    }
    let (dw, dh) = if sw <= PREVIEW_WIDTH {
        (sw, sh)
    } else {
        let dw = PREVIEW_WIDTH;
        let dh = ((sh as u64 * dw as u64) / sw as u64).max(1).min(u32::MAX as u64) as u32;
        (dw, dh)
    };
    let (dw_us, dh_us) = (dw as usize, dh as usize);
    let mut rgb = vec![0u8; dw_us.saturating_mul(dh_us).saturating_mul(3)];
    for y in 0..dh_us {
        let sy = (y * sh_us) / dh_us;
        for x in 0..dw_us {
            let sx = (x * sw_us) / dw_us;
            let s_off = sy * stride + sx * 4;
            let d_off = (y * dw_us + x) * 3;
            // Bounds are guaranteed by the checks above, but index with
            // `get` anyway — a corrupt stride must never panic the thread.
            let b = *src.get(s_off)?;
            let g = *src.get(s_off + 1)?;
            let r = *src.get(s_off + 2)?;
            if let Some(slot) = rgb.get_mut(d_off..d_off + 3) {
                slot[0] = r;
                slot[1] = g;
                slot[2] = b;
            }
        }
    }
    Some(PreviewFrame { width: dw, height: dh, rgb })
}

/// Stale predicate: no frame (or connect) activity for longer than
/// `STALE_AFTER`. Pure for testability; the thread passes `Instant::now()`.
fn is_stale(last_activity: Instant, now: Instant) -> bool {
    now.duration_since(last_activity) > STALE_AFTER
}

// ---------------------------------------------------------------------------
// Monitor — owned by `AppState`, mirrors `Broadcaster`'s shape
// ---------------------------------------------------------------------------

enum Control {
    /// Connect the receiver to the named source (replaces any current one).
    Connect(String),
    /// Tear down the receiver, keep scanning.
    Disconnect,
}

struct Shared {
    sources: Vec<NdiSourceInfo>,
    status: NdiMonitorStatus,
}

impl Shared {
    fn idle() -> Self {
        Self {
            sources: Vec::new(),
            status: NdiMonitorStatus {
                state: NdiMonitorState::Off,
                source: None,
                message: "Monitor off.".to_string(),
            },
        }
    }
}

pub struct NdiReceiveMonitor {
    shared: Arc<Mutex<Shared>>,
    tx: Mutex<Option<Sender<Control>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
    stop_flag: Arc<AtomicBool>,
}

// SAFETY: raw SDK pointers never leave the monitor thread; the shared state
// is plain owned data behind a mutex.
unsafe impl Send for NdiReceiveMonitor {}
unsafe impl Sync for NdiReceiveMonitor {}

impl Default for NdiReceiveMonitor {
    fn default() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared::idle())),
            tx: Mutex::new(None),
            thread: Mutex::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl NdiReceiveMonitor {
    fn set_status(&self, app: &AppHandle, state: NdiMonitorState, source: Option<String>, message: String) -> NdiMonitorStatus {
        let status = NdiMonitorStatus { state, source, message };
        self.shared.lock().unwrap().status = status.clone();
        let _ = app.emit("ndi-monitor-status", &status);
        status
    }

    /// Current source list snapshot (cheap clone; empty while converging).
    pub fn list_sources(&self) -> Vec<NdiSourceInfo> {
        self.shared.lock().unwrap().sources.clone()
    }

    /// Current monitor status (cheap clone; for panel init).
    pub fn monitor_status(&self) -> NdiMonitorStatus {
        self.shared.lock().unwrap().status.clone()
    }

    fn running(&self) -> bool {
        self.thread.lock().unwrap().is_some()
    }

    /// Start the finder thread (no-op when already running). Fails fast with
    /// a clear message when the NDI SDK is absent — the app keeps working.
    pub fn start_scan(&self, app: &AppHandle) -> Result<Vec<NdiSourceInfo>, String> {
        if self.running() {
            return Ok(self.list_sources());
        }
        let lib = unsafe { load_recv_lib()? };
        unsafe {
            if (lib.initialize)() == 0 {
                return Err("NDIlib_initialize returned false".to_string());
            }
        }
        self.stop_flag.store(false, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<Control>();
        let shared = Arc::clone(&self.shared);
        let stop_flag = Arc::clone(&self.stop_flag);
        let app_handle = app.clone();
        let thread = std::thread::Builder::new()
            .name("ndi-monitor".to_string())
            .spawn(move || monitor_thread(app_handle, lib, shared, rx, stop_flag))
            .map_err(|e| format!("could not spawn ndi-monitor thread: {e}"))?;
        *self.tx.lock().unwrap() = Some(tx);
        *self.thread.lock().unwrap() = Some(thread);
        self.set_status(app, NdiMonitorState::Scanning, None, "Scanning for NDI sources…".to_string());
        Ok(self.list_sources())
    }

    /// Connect the preview to a source by exact name. Starts the thread first
    /// when the monitor was never enabled, so connect works standalone.
    pub fn connect(&self, app: &AppHandle, name: &str) -> Result<NdiMonitorStatus, String> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("no NDI source selected".to_string());
        }
        if !self.running() {
            self.start_scan(app)?;
        }
        // Same-source reconnect is a no-op (avoids tearing down a live feed).
        {
            let shared = self.shared.lock().unwrap();
            if shared.status.state == NdiMonitorState::Live
                && shared.status.source.as_deref() == Some(name.as_str())
            {
                return Ok(shared.status.clone());
            }
        }
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            tx.send(Control::Connect(name.clone()))
                .map_err(|_| "ndi-monitor thread is gone".to_string())?;
        }
        Ok(self.set_status(app, NdiMonitorState::Connecting, Some(name.clone()), format!("Connecting to \"{name}\"…")))
    }

    /// Tear down the receiver (recv_connect(NULL) + recv_destroy on the
    /// monitor thread); scanning continues.
    pub fn disconnect(&self, app: &AppHandle) -> NdiMonitorStatus {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(Control::Disconnect);
        }
        self.set_status(app, NdiMonitorState::Scanning, None, "Disconnected — still scanning.".to_string())
    }

    /// Full teardown: signal stop, drop the control channel, join the thread
    /// (bounded by the capture timeout), which destroys receiver + finder +
    /// SDK in that order. No leaked connections or threads.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        // Take (drop) the sender first so the thread's `try_recv` can never
        // block on a dead channel peer, then join without holding the lock.
        let thread = {
            *self.tx.lock().unwrap() = None;
            self.thread.lock().unwrap().take()
        };
        if let Some(thread) = thread {
            let _ = thread.join();
        }
        let mut shared = self.shared.lock().unwrap();
        shared.sources.clear();
        shared.status = NdiMonitorStatus {
            state: NdiMonitorState::Off,
            source: None,
            message: "Monitor off.".to_string(),
        };
    }
}

// Compile-time check: managed state must be `Send`.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<NdiReceiveMonitor>();
};

// ---------------------------------------------------------------------------
// Monitor thread — owns lib + finder + receiver, runs both blocking loops
// ---------------------------------------------------------------------------

/// Read the current finder list into owned values. The SDK owns the returned
/// pointers only until the next finder call, so every string is copied here.
unsafe fn read_sources(lib: &NdiRecvLib, find: FindInstance) -> Vec<NdiSourceInfo> {
    let mut count: u32 = 0;
    let raw = (lib.find_get_sources)(find, &mut count);
    if raw.is_null() || count == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..(count as usize) {
        let entry = &*raw.add(i);
        if entry.p_ndi_name.is_null() {
            continue;
        }
        let name = CStr::from_ptr(entry.p_ndi_name).to_string_lossy().into_owned();
        let url = if entry.p_url_address.is_null() {
            String::new()
        } else {
            CStr::from_ptr(entry.p_url_address).to_string_lossy().into_owned()
        };
        out.push(NdiSourceInfo { name, url });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn monitor_thread(
    app: AppHandle,
    lib: NdiRecvLib,
    shared: Arc<Mutex<Shared>>,
    rx: mpsc::Receiver<Control>,
    stop_flag: Arc<AtomicBool>,
) {
    let set_status = |state: NdiMonitorState, source: Option<String>, message: String| {
        let status = NdiMonitorStatus { state, source, message };
        shared.lock().unwrap().status = status.clone();
        let _ = app.emit("ndi-monitor-status", &status);
    };
    let publish_sources = |sources: Vec<NdiSourceInfo>| {
        shared.lock().unwrap().sources = sources.clone();
        let _ = app.emit("ndi-sources", &NdiSourcesEvent { sources });
    };

    // Finder lives for the whole thread (created once, destroyed on exit).
    let find: FindInstance = unsafe {
        let create = FindCreate {
            show_local_sources: 1,
            p_groups: std::ptr::null(),
            p_extra_ips: std::ptr::null(),
        };
        (lib.find_create)(&create)
    };
    if is_null(find) {
        set_status(
            NdiMonitorState::Error,
            None,
            "NDI finder could not start.".to_string(),
        );
        unsafe { (lib.destroy)() };
        return;
    }
    // Immediate first read (catches already-known sources without waiting),
    // then the wait-loop below picks up changes.
    publish_sources(unsafe { read_sources(&lib, find) });

    let recv_name = CString::new("MakrStudio Camera Monitor").unwrap_or_default();
    let mut recv: RecvInstance = std::ptr::null_mut();
    let mut connected_source: Option<String> = None;
    let mut last_activity: Option<Instant> = None;
    let mut last_emit: Option<Instant> = None;
    let mut current_live = false;

    while !stop_flag.load(Ordering::SeqCst) {
        // Drain control messages first (connect/disconnect apply promptly).
        while let Ok(control) = rx.try_recv() {
            match control {
                Control::Connect(name) => {
                    // Tear down any previous receiver before creating the new
                    // one: recv_connect(NULL) + recv_destroy, no leaks.
                    if !is_null(recv) {
                        unsafe {
                            (lib.recv_connect)(recv, std::ptr::null());
                            (lib.recv_destroy)(recv);
                        }
                        recv = std::ptr::null_mut();
                    }
                    let c_name = match CString::new(name.clone()) {
                        Ok(c) => c,
                        Err(_) => {
                            set_status(NdiMonitorState::Error, None, "Invalid source name.".to_string());
                            continue;
                        }
                    };
                    let source = NdiSourceRaw {
                        p_ndi_name: c_name.as_ptr(),
                        p_url_address: std::ptr::null(),
                    };
                    let create = RecvCreateV3 {
                        source,
                        color_format: COLOR_BGRX_BGRA,
                        bandwidth: BANDWIDTH_HIGHEST,
                        allow_video_fields: 0,
                        p_ndi_recv_name: recv_name.as_ptr(),
                    };
                    // # Safety: `create` borrows `c_name`, both alive for the
                    // call; the SDK copies what it needs.
                    let new_recv = unsafe { (lib.recv_create)(&create) };
                    if is_null(new_recv) {
                        set_status(
                            NdiMonitorState::Error,
                            Some(name.clone()),
                            format!("Could not open receiver for \"{name}\"."),
                        );
                        continue;
                    }
                    recv = new_recv;
                    connected_source = Some(name.clone());
                    last_activity = Some(Instant::now());
                    last_emit = None;
                    current_live = false;
                    set_status(
                        NdiMonitorState::Connecting,
                        Some(name.clone()),
                        format!("Connecting to \"{name}\"…"),
                    );
                }
                Control::Disconnect => {
                    if !is_null(recv) {
                        unsafe {
                            (lib.recv_connect)(recv, std::ptr::null());
                            (lib.recv_destroy)(recv);
                        }
                        recv = std::ptr::null_mut();
                    }
                    connected_source = None;
                    last_activity = None;
                    current_live = false;
                    set_status(NdiMonitorState::Scanning, None, "Disconnected — still scanning.".to_string());
                }
            }
        }
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        if is_null(recv) {
            // Scan-only tick: block in the finder wait (never a hot spin),
            // refresh the list when the set changes.
            let changed = unsafe { (lib.find_wait_for_sources)(find, FIND_WAIT_MS) != 0 };
            if changed {
                publish_sources(unsafe { read_sources(&lib, find) });
            }
            continue;
        }

        // Connected tick: capture video only (audio/metadata NULL — we never
        // want them, and the call then can't return those frame types).
        let mut frame: VideoFrame = unsafe { std::mem::zeroed() };
        let frame_type = unsafe {
            (lib.recv_capture)(
                recv,
                &mut frame,
                std::ptr::null(),
                std::ptr::null(),
                CAPTURE_TIMEOUT_MS,
            )
        };
        match frame_type {
            FRAME_VIDEO => {
                // Copy OUT of the SDK-owned buffer first, then free
                // IMMEDIATELY — before any processing. The free happens even
                // when the copy/validation fails (see below).
                let copied: Option<(u32, u32, usize, Vec<u8>)> = unsafe {
                    let valid_fourcc = frame.four_cc == FOURCC_BGRA || frame.four_cc == FOURCC_BGRX;
                    let w = frame.xres;
                    let h = frame.yres;
                    let stride = frame.line_stride_in_bytes;
                    let bytes: Option<Vec<u8>> = if !valid_fourcc
                        || w <= 0
                        || h <= 0
                        || stride < 0
                        || frame.p_data.is_null()
                    {
                        None
                    } else {
                        let total = (stride as u64).saturating_mul(h as u64);
                        if total == 0 || total > MAX_FRAME_BYTES {
                            None
                        } else {
                            let slice = std::slice::from_raw_parts(frame.p_data, total as usize);
                            Some(slice.to_vec())
                        }
                    };
                    // MANDATORY free — every captured frame, no exceptions.
                    (lib.recv_free_video)(recv, &frame);
                    bytes.map(|b| (w as u32, h as u32, stride as usize, b))
                };
                last_activity = Some(Instant::now());
                if !current_live {
                    current_live = true;
                    set_status(
                        NdiMonitorState::Live,
                        connected_source.clone(),
                        connected_source
                            .as_deref()
                            .map(|s| format!("Receiving from \"{s}\" (preview ~2 fps)."))
                            .unwrap_or_default(),
                    );
                }
                // Throttled delivery: downscale + JPEG + emit at most 2 fps.
                let due = last_emit.is_none_or(|t| t.elapsed() >= EMIT_PERIOD);
                if due {
                    if let Some((w, h, stride, data)) = copied {
                        if let Some(preview) = downscale_bgr_to_rgb(&data, w, h, stride) {
                            deliver_preview_frame(&app, &preview);
                            last_emit = Some(Instant::now());
                        }
                    }
                }
            }
            FRAME_ERROR => {
                // Connection lost — the SDK keeps the receiver and retries on
                // its own, but the UI must NOT keep showing the last frame as
                // live. Flip to stale immediately; fresh frames flip it back.
                if current_live {
                    current_live = false;
                    set_status(
                        NdiMonitorState::Stale,
                        connected_source.clone(),
                        "Connection lost — retrying. Last frame shown is not live.".to_string(),
                    );
                }
            }
            FRAME_STATUS_CHANGE | FRAME_NONE => {
                // Heartbeat tick: check whether the feed has gone quiet.
            }
            _ => {}
        }

        // Stale watchdog: connected but nothing fresh within STALE_AFTER.
        // Covers silent source loss the SDK auto-reconnect would otherwise
        // hide behind a frozen last frame.
        if !is_null(recv) {
            if let Some(since) = last_activity {
                if is_stale(since, Instant::now()) && current_live {
                    current_live = false;
                    set_status(
                        NdiMonitorState::Stale,
                        connected_source.clone(),
                        "No fresh frames — last preview shown is stale, not live.".to_string(),
                    );
                }
            }
        }
    }

    // Clean teardown in reverse order: receiver, finder, SDK.
    if !is_null(recv) {
        unsafe {
            (lib.recv_connect)(recv, std::ptr::null());
            (lib.recv_destroy)(recv);
        }
    }
    unsafe {
        (lib.find_destroy)(find);
        (lib.destroy)();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_width_is_within_spec() {
        assert!((480..=640).contains(&PREVIEW_WIDTH));
    }

    #[test]
    fn emit_period_is_two_fps() {
        assert_eq!(EMIT_PERIOD, Duration::from_millis(500));
    }

    #[test]
    fn downscale_rejects_degenerate_input() {
        assert!(downscale_bgr_to_rgb(&[], 0, 10, 40).is_none());
        assert!(downscale_bgr_to_rgb(&[], 10, 0, 40).is_none());
        // Stride narrower than the row: corrupt, must not panic.
        assert!(downscale_bgr_to_rgb(&vec![0u8; 100], 10, 10, 20).is_none());
        // Buffer shorter than stride*height.
        assert!(downscale_bgr_to_rgb(&vec![0u8; 10], 10, 10, 40).is_none());
    }

    #[test]
    fn downscale_swizzles_bgr_to_rgb_and_scales() {
        // 4x2 BGRX: row 0 solid red-ish (B=10,G=20,R=30), row 1 solid
        // green-ish (B=40,G=50,R=60). Downscale to PREVIEW? No — source is
        // far below preview width, so native size is kept.
        let sw = 4u32;
        let sh = 2u32;
        let stride = (sw as usize) * 4;
        let mut src = vec![0u8; stride * sh as usize];
        for x in 0..sw as usize {
            src[x * 4] = 10;
            src[x * 4 + 1] = 20;
            src[x * 4 + 2] = 30;
            src[x * 4 + 3] = 255;
            let o = stride + x * 4;
            src[o] = 40;
            src[o + 1] = 50;
            src[o + 2] = 60;
            src[o + 3] = 255;
        }
        let out = downscale_bgr_to_rgb(&src, sw, sh, stride).expect("valid input");
        assert_eq!((out.width, out.height), (4, 2));
        assert_eq!(out.rgb.len(), 4 * 2 * 3);
        // BGR in, RGB out.
        assert_eq!(&out.rgb[0..3], &[30, 20, 10]);
        let row1 = &out.rgb[4 * 3..4 * 3 + 3];
        assert_eq!(row1, &[60, 50, 40]);
    }

    #[test]
    fn downscale_shrinks_large_frames_to_preview_width() {
        let sw = 1920u32;
        let sh = 1080u32;
        let stride = sw as usize * 4;
        let src = vec![0x80u8; stride * sh as usize];
        let out = downscale_bgr_to_rgb(&src, sw, sh, stride).expect("valid input");
        assert_eq!(out.width, PREVIEW_WIDTH);
        assert_eq!(out.height, 1080 * PREVIEW_WIDTH / 1920);
        assert_eq!(out.rgb.len(), out.width as usize * out.height as usize * 3);
    }

    #[test]
    fn stale_predicate_matches_threshold() {
        let now = Instant::now();
        let fresh = now - Duration::from_secs(2);
        let old = now - Duration::from_secs(10);
        assert!(!is_stale(fresh, now));
        assert!(is_stale(old, now));
    }

    #[test]
    fn status_serializes_for_frontend_contract() {
        let s = NdiMonitorStatus {
            state: NdiMonitorState::Stale,
            source: Some("CAM (Cam)".to_string()),
            message: "stale".to_string(),
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["state"], "stale");
        assert_eq!(v["source"], "CAM (Cam)");
    }

    #[test]
    fn ffi_constant_values_match_sdk_headers() {        assert_eq!(COLOR_BGRX_BGRA, 0);
        assert_eq!(BANDWIDTH_HIGHEST, 100);
        assert_eq!(FOURCC_BGRA, 0x4152_4742);
        assert_eq!(FOURCC_BGRX, 0x5852_4742);
        assert_eq!(FRAME_VIDEO, 1);
        assert_eq!(FRAME_STATUS_CHANGE, 100);
        // Struct sizes: source {2 ptr}, find-create {1 byte + pad + 2 ptr},
        // recv-create-v3 {2 ptr + int + int + 1 byte + pad + 1 ptr}.
        assert_eq!(std::mem::size_of::<NdiSourceRaw>(), 16);
        assert_eq!(std::mem::size_of::<FindCreate>(), 24);
        assert_eq!(std::mem::size_of::<RecvCreateV3>(), 40);
    }
}
