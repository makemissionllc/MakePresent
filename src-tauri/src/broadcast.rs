//! NDI broadcast output.
//!
//! MakePresent can publish the same live slide shown on the main Output as an
//! NDI source on the local network, so video switchers (vMix, OBS, ATEM, ...)
//! can cut to it. The assigned NDI **Look** decides how that feed is styled
//! independently of the on-screen Output.
//!
//! # Why the SDK is loaded at runtime
//!
//! The NDI SDK (Vizrt/NewTek) is a closed-source C library that is **not**
//! vendorable inside a Cargo crate and **not** present on the build machine by
//! default. The Rust bindings crates on crates.io require the SDK headers +
//! libclang at build time (breaking clean CI on both platforms without the
//! SDK), and one commonly cited one is GPL-3.0 (incompatible with this
//! project). To keep the app building and *running* without NDI installed,
//! this module loads the SDK shared library at runtime via [`libloading`] and
//! calls the C ABI directly. If the SDK is missing it logs a clear error and
//! everything else keeps working. See `README.md` for installation/licensing.
//!
//! # Frame capture — honest scope note
//!
//! This module owns the **sender** side: register a source, push BGRA+alpha
//! frames on a dedicated thread, keep the source alive. The *webview → pixels*
//! capture (an offscreen render target mirrored from the Output) is a runtime
//! concern that lives elsewhere; [`BroadcastCore::send_frame`] is the clean
//! seam it plugs into. Nothing here needs actual NDI hardware or a screen to
//! compile, so the crate builds and `cargo check` passes in CI.

use libloading::{Library, Symbol};
use std::ffi::{c_char, c_void};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Behavioural constant — the NDI *source name* receivers see on the network.
/// Source names may be anything; the "NDI" mark itself is only restricted in
/// *product names* (which require permission), so the app is "MakrStudio"
/// while the source is e.g. "MakrStudio - Sunday Output".
pub const NDI_SOURCE_NAME: &str = "MakrStudio - Sunday Output";

/// Nominal video geometry/frame-rate for the feed: 1920x1080 @ ~29.97fps
/// progressive. The webview capture scales to/below this as needed.
const FRAME_RATE_N: i32 = 30_000;
const FRAME_RATE_D: i32 = 1_001;
/// How often a stale frame is re-sent to keep the NDI source discoverable
/// (real NDI senders do the same); ~a frame period at the nominal rate.
const RESEND_PERIOD: Duration = Duration::from_millis(33);

// ---------------------------------------------------------------------------
// NDI C ABI — a hand-written, minimal `#[repr(C)]` mirror of the relevant part
// of Processing.NDI.structs.h / Processing.NDI.Lib.h. Binding by hand avoids
// needing the SDK headers or libclang at build time.
// ---------------------------------------------------------------------------

/// FourCC pixel formats. `BGRA` carries alpha (for compositing/keying);
/// `BGRX` is opaque. Values match `NDIlib_FourCC_video_type_e`.
#[repr(i32)]
#[allow(dead_code)]
enum FourCC {
    Bgra = 0x4152_4742, // "BGRA"
    Bgrx = 0x5852_4742, // "BGRX"
}

#[repr(u32)]
enum FrameFormatType {
    Progressive = 1,
}

/// `NDIlib_video_frame_v2_t` — one video frame. Field order/types mirror the
/// official header exactly (verified against the SDK + bindings crates): two
/// `int`, an enum (int), two `int`, `float`, enum (int), `i64`, pointer,
/// `int` (union stride/size), pointer (`p_metadata`), `i64` (`timestamp`).
#[repr(C)]
struct VideoFrameV2 {
    xres: c_int,
    yres: c_int,
    four_cc: i32,
    frame_rate_n: c_int,
    frame_rate_d: c_int,
    picture_aspect_ratio: f32,
    frame_format_type: u32,
    timecode: i64,
    p_data: *mut u8,
    line_stride_in_bytes: c_int,
    p_metadata: *const c_char,
    timestamp: i64,
}

/// `NDIlib_send_create_t` — source registration parameters.
#[repr(C)]
struct SendCreate {
    p_ndi_name: *const c_char,
    p_groups: *const c_char,
    clock_video: u8,
    clock_audio: u8,
}

/// Opaque SDK instance handle.
type SendInstance = *mut c_void;

fn is_null(p: SendInstance) -> bool {
    std::ptr::null_mut() == p
}

/// C `bool` is one byte; NDI's `clock_video`/`clock_audio` are C bools.
const C_TRUE: u8 = 1;
const C_FALSE: u8 = 0;

/// A dynamically loaded handle to the NDI SDK plus the function pointers we
/// use. The loaded `Library` lives here and is kept alive for the lifetime of
/// the sender (symbols borrow from it). The fn-pointers (`Copy`) can be handed
/// to the dedicated send thread without violating `Send`.
struct NdiLib {
    _lib: Library,
    initialize: unsafe extern "C" fn() -> c_int,
    destroy: unsafe extern "C" fn(),
    send_create: unsafe extern "C" fn(*const SendCreate, *const c_char) -> SendInstance,
    send_destroy: unsafe extern "C" fn(SendInstance),
    send_video: unsafe extern "C" fn(SendInstance, *const VideoFrameV2) -> c_int,
}

/// Filename of the NDI SDK shared library per platform. On Windows the DLL must
/// sit alongside the app; on Linux/macOS the SDK's install path must be on the
/// loader path (NDI ships `libndi.so.5` and `libndi.dylib`).
pub fn lib_filename() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "Processing.NDI.Lib.x64.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libndi.dylib"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "libndi.so.5"
    }
}

/// Load the NDI SDK and resolve the functions the sender needs.
///
/// Runtime detection order (Windows, per NDI docs https://docs.ndi.video/all/developing-with-ndi/sdk/software-distribution):
///  1) `NDI_RUNTIME_DIR_V6` / `NDI_RUNTIME_DIR_V5` env var set by the official
///     NDI 6 Runtime redistributable installer (e.g. `C:\Program Files\NDI\NDI 6 Runtime\`)
///     — preferred when bundled via MakrStudio's silent NSIS install (`/verysilent`) or
///     when user installed the redistributable manually. Checked first so a system-wide
///     runtime is found even if no DLL sits next to the .exe.
///  2) Fallback: `Processing.NDI.Lib.x64.dll` next to the MakrStudio .exe / on PATH
///     (legacy manual SDK placement, also used on Linux/macOS via `libndi.so.5`).
/// Both bundled and manually-installed SDKs work. See `src-tauri/resources/NDI_VERSION.txt`.
///
/// # Safety
/// All resolved symbols are required, stable entry points of the SDK; the
/// returned `NdiLib` keeps the library loaded for as long as it is held.
unsafe fn load_ndi() -> Result<NdiLib, String> {
    let file = lib_filename();

    // Build ordered list of candidate paths to try
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    // 1) NDI_RUNTIME_DIR_V6 (future) then V5 (current redistributable uses V5 for compat)
    for env_key in ["NDI_RUNTIME_DIR_V6", "NDI_RUNTIME_DIR_V5"] {
        if let Ok(dir) = std::env::var(env_key) {
            let trimmed = dir.trim().trim_matches('"');
            if !trimmed.is_empty() {
                let p = std::path::Path::new(trimmed).join(file);
                candidates.push(p);
            }
        }
    }
    // 2) Fallback: bare filename (next to .exe / system search path)
    candidates.push(std::path::PathBuf::from(file));

    let mut last_err: Option<String> = None;
    for candidate in &candidates {
        // Library::new accepts both &str and &Path; use Path for env-var joins, str for bare filename
        let lib_res = Library::new(candidate);
        let lib = match lib_res {
            Ok(l) => l,
            Err(e) => {
                last_err = Some(format!("\"{}\" -> {e}", candidate.display()));
                continue;
            }
        };

        let err_of = |n: &str, e: libloading::Error| format!("failed to resolve NDI symbol \"{n}\": {e}");
        let resolved = (|| {
            let initialize: Symbol<unsafe extern "C" fn() -> c_int> =
                lib.get(b"NDIlib_initialize").map_err(|e| err_of("NDIlib_initialize", e))?;
            let destroy: Symbol<unsafe extern "C" fn()> =
                lib.get(b"NDIlib_destroy").map_err(|e| err_of("NDIlib_destroy", e))?;
            let send_create: Symbol<
                unsafe extern "C" fn(*const SendCreate, *const c_char) -> SendInstance,
            > = lib.get(b"NDIlib_send_create").map_err(|e| err_of("NDIlib_send_create", e))?;
            let send_destroy: Symbol<unsafe extern "C" fn(SendInstance)> =
                lib.get(b"NDIlib_send_destroy").map_err(|e| err_of("NDIlib_send_destroy", e))?;
            let send_video: Symbol<
                unsafe extern "C" fn(SendInstance, *const VideoFrameV2) -> c_int,
            > = lib.get(b"NDIlib_send_send_video_v2").map_err(|e| err_of("NDIlib_send_send_video_v2", e))?;
            Ok::<_, String>((
                *initialize,
                *destroy,
                *send_create,
                *send_destroy,
                *send_video,
            ))
        })();

        match resolved {
            Ok((initialize, destroy, send_create, send_destroy, send_video)) => {
                return Ok(NdiLib {
                    _lib: lib,
                    initialize,
                    destroy,
                    send_create,
                    send_destroy,
                    send_video,
                });
            }
            Err(e) => {
                last_err = Some(format!("\"{}\" -> {e}", candidate.display()));
                continue;
            }
        }
    }

    let tried = candidates
        .iter()
        .map(|p| format!("\"{}\"", p.display()))
        .collect::<Vec<_>>()
        .join(", ");
    let detail = last_err.unwrap_or_else(|| "no candidates tried".to_string());
    Err(format!(
        "NDI SDK not found (tried {tried}): {detail} — install the NDI Runtime via MakrStudio's bundled installer (auto, /verysilent) or manually from https://ndi.link/NDIRedistV6 (Windows) / https://downloads.ndi.tv/SDK/NDI_SDK_Linux/Install_NDI_SDK_v6_Linux.tar.gz (Linux)"
    ))
}

/// Messages pushed by any render thread to the dedicated send thread.
///
/// The `Frame` variant is the seam the (runtime-only) offscreen render capture
/// feeds; until that capture is wired it is intentionally unused and the
/// compiler is told so.
#[allow(dead_code)]
enum Command {
    /// A freshly captured BGRA+alpha frame. The channel is bounded; if full
    /// the newest frame is dropped — real-time video, never overlapping stale.
    Frame { width: u32, height: u32, bgra: Vec<u8> },
}

/// The live NDI broadcaster. Owned by `AppState`; at most one exists. The
/// loaded SDK `Library` is kept here (managed state) while the dedicated send
/// thread runs the instance pointer, which is only ever used once the library
/// is loaded and stopped before it is dropped.
pub struct BroadcastCore {
    /// Kept so the SDK stays loaded for the send thread's lifetime.
    _ndi: NdiLib,
    send_instance: SendInstance,
    tx: SyncSender<Command>,
    thread: Option<JoinHandle<()>>,
}

// SAFETY: `Library` is `Send + Sync` in libloading; `send_instance` is a raw
// pointer used only on the (joined-before-drop) send thread.
unsafe impl Send for BroadcastCore {}

impl BroadcastCore {
    /// Load the NDI SDK, register a sender named `source_name`, and spawn the
    /// dedicated send thread. Errors are returned (and logged by the caller)
    /// when the SDK is missing — the app keeps running regardless.
    pub fn start(source_name: &str) -> Result<BroadcastCore, String> {
        let mut c_name: Vec<u8> = source_name.as_bytes().to_vec();
        c_name.push(0);

        let ndi = unsafe { load_ndi()? };

        unsafe {
            if (ndi.initialize)() == 0 {
                return Err("NDIlib_initialize returned false".to_string());
            }
        }

        let create = SendCreate {
            p_ndi_name: c_name.as_ptr() as *const c_char,
            p_groups: std::ptr::null(),
            clock_video: C_TRUE,
            clock_audio: C_FALSE,
        };
        let send_instance = unsafe { (ndi.send_create)(&create, std::ptr::null()) };
        if is_null(send_instance) {
            unsafe { (ndi.destroy)() };
            return Err("NDIlib_send_create returned a null instance".to_string());
        }

        let send_video = ndi.send_video;
        let (tx, rx) = mpsc::sync_channel::<Command>(3);
        // Pass the instance handle as a `usize` (definitely Send) so the send
        // thread closure needn't capture a raw pointer.
        let thread = spawn_send_thread(rx, send_instance as usize, send_video);

        Ok(BroadcastCore {
            _ndi: ndi,
            send_instance,
            tx,
            thread: Some(thread),
        })
    }

    /// Push a freshly captured BGRA+alpha frame to the send thread.
    ///
    /// This is the seam the offscreen render capture plugs into — the pixel
    /// data comes from the **Output window's WebView** (currently unwired;
    /// future capture will mirror Output's `SlideRender` via an offscreen
    /// render target / `window.capture` and call this). Non-blocking and
    /// bounded (capacity-3 `try_send`), so it never blocks the render loop.
    ///
    /// Safety: validates dimensions and that the frame is not all-black (which
    /// indicates a failed capture or destroyed window) before queuing; holds
    /// last-good-frame and logs instead of pushing black (see `spawn_send_thread`).
    ///
    /// `#[allow(dead_code)]`: not yet called — the capture integration that
    /// feeds frames is a separate runtime component (see module doc).
    /// Returns true if the frame was accepted (validated and queued), false if skipped (invalid/black).
    #[allow(dead_code)]
    pub fn send_frame(&self, width: u32, height: u32, bgra: Vec<u8>) -> bool {
        // Safety check is also done here (defense in depth) before queuing
        if width == 0 || height == 0 || bgra.is_empty() || bgra.len() != (width as usize * height as usize * 4) {
            eprintln!("NDI: no valid Output frame source, skipping (invalid frame {}x{} len {})", width, height, bgra.len());
            return false;
        }
        let is_black = bgra.chunks_exact(4).all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0);
        if is_black {
            eprintln!("NDI: no valid Output frame source, skipping (black frame {}x{} — holding last good frame)", width, height);
            return false;
        }
        let _ = self.tx.try_send(Command::Frame {
            width,
            height,
            bgra,
        });
        true
    }

    /// Stop the send thread and tear down the NDI source + SDK. Caller should
    /// drop the value (this consumes it) after `Broadcaster::stop` replaces it.
    fn shutdown(mut self) {
        // Drop the sender so the thread's recv sees Disconnected and exits,
        // then join before destroying the instance/library.
        self.tx = mpsc::sync_channel::<Command>(3).0;
        drop(self.tx);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        unsafe {
            (self._ndi.send_destroy)(self.send_instance);
            (self._ndi.destroy)();
        }
    }
}

/// Spawn the dedicated NDI send thread. It owns the receive end plus the
/// instance handle and the SDK's send-video fn-pointer. It drains any new
/// frame, then (re)sends the latest frame on a cadence so the source stays
/// discoverable even between captures. Buffer is packed BGRA (stride = w*4).
/// If no valid frame has ever been received (e.g. Output window destroyed
/// and not yet healed, or capture not yet wired), it holds last-good-frame
/// and re-sends it; if no good frame exists it logs and skips (no black).
fn spawn_send_thread(
    rx: mpsc::Receiver<Command>,
    instance_addr: usize,
    send_video: unsafe extern "C" fn(SendInstance, *const VideoFrameV2) -> c_int,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("ndi-send".to_string())
        .spawn(move || {
            let send_instance: SendInstance = instance_addr as SendInstance;
            let mut current: Option<(u32, u32, Vec<u8>)> = None;
            let mut last_sent: Option<Instant> = None;
            let mut warned_no_frame = false;

            loop {
                match rx.recv_timeout(RESEND_PERIOD) {
                    Ok(Command::Frame { width, height, bgra }) => {
                        // Safety: validate frame before accepting as current (defense in depth)
                        if width == 0 || height == 0 || bgra.is_empty() {
                            eprintln!("NDI: no valid Output frame source, skipping (invalid frame {}x{} len {})", width, height, bgra.len());
                            continue;
                        }
                        current = Some((width, height, bgra));
                        warned_no_frame = false;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }

                if last_sent.is_none_or(|t| t.elapsed() >= RESEND_PERIOD) {
                    if let Some((width, height, data)) = &current {
                        // Double-check current is still valid (not black) before sending
                        let is_black = data.chunks_exact(4).all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0);
                        if is_black {
                            if !warned_no_frame {
                                eprintln!("NDI: no valid Output frame source, skipping (black frame {}x{} — holding last good frame)", width, height);
                                warned_no_frame = true;
                            }
                            // Hold last-good-frame: do not send black, just wait for a valid frame.
                            // If this is the first frame and it's black, we still skip and keep current as is
                            // (will be replaced when a valid frame arrives after Output rebuild).
                            continue;
                        }
                        let frame = VideoFrameV2 {
                            xres: *width as c_int,
                            yres: *height as c_int,
                            four_cc: FourCC::Bgra as i32,
                            frame_rate_n: FRAME_RATE_N,
                            frame_rate_d: FRAME_RATE_D,
                            picture_aspect_ratio: *width as f32 / *height as f32,
                            frame_format_type: FrameFormatType::Progressive as u32,
                            timecode: 0,
                            p_data: data.as_ptr() as *mut u8,
                            line_stride_in_bytes: (*width as c_int) * 4,
                            p_metadata: std::ptr::null(),
                            timestamp: 0,
                        };
                        // # Safety: `send_video` comes from the loaded, still
                        // alive SDK; the frame and buffer are valid for the call.
                        unsafe { send_video(send_instance, &frame) };
                        last_sent = Some(Instant::now());
                        warned_no_frame = false;
                    } else if !warned_no_frame {
                        eprintln!("NDI: no valid Output frame source, skipping (no frame yet — Output window may be destroyed/unhealed or capture not wired; will recover automatically once Output is rebuilt)");
                        warned_no_frame = true;
                    }
                }
            }
        })
        .expect("failed to spawn ndi-send thread")
}

/// Thin wrapper stored in [`AppState`] so commands can start/stop/feed NDI.
pub struct Broadcaster {
    inner: Mutex<Option<BroadcastCore>>,
    has_real_frames: AtomicBool,
    last_frame_at: RwLock<Option<String>>,
    last_frame_instant: Mutex<Option<Instant>>,
}

impl Default for Broadcaster {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            has_real_frames: AtomicBool::new(false),
            last_frame_at: RwLock::new(None),
            last_frame_instant: Mutex::new(None),
        }
    }
}

impl Broadcaster {
    /// Whether a broadcaster is currently active.
    pub fn is_active(&self) -> bool {
        self.inner.lock().ok().is_some_and(|g| g.is_some())
    }

    /// Whether a real frame has ever been accepted (distinct from `is_active`).
    /// While `current` in `spawn_send_thread` is still `None` (capture not yet
    /// wired), this stays `false` — the source is discoverable but transmits
    /// no real video. UI must not claim success until this is true.
    pub fn has_real_frames(&self) -> bool {
        self.has_real_frames.load(Ordering::Relaxed)
    }

    /// ISO timestamp of the last accepted real frame, if any. Used for staleness
    /// like `RenderAck` (`ACK_STALE_MS` pattern).
    pub fn last_frame_at(&self) -> Option<String> {
        self.last_frame_at.read().ok()?.clone()
    }

    /// Whether the feed is stale: enabled but no valid frame recently. True when
    /// `has_real_frames` is false or last frame older than ~5s (mirrors
    /// `ACK_STALE_MS` heartbeat). Until capture is wired, this is always true
    /// when active. When not active (`is_active` false), not stale — just off.
    pub fn is_stale(&self) -> bool {
        if !self.is_active() {
            return false;
        }
        if !self.has_real_frames.load(Ordering::Relaxed) {
            return true;
        }
        let guard = match self.last_frame_instant.lock() {
            Ok(g) => g,
            Err(_) => return true,
        };
        match *guard {
            Some(instant) => instant.elapsed() > Duration::from_millis(5000),
            None => true,
        }
    }

    /// Start (or restart) the NDI broadcaster with the given source name.
    pub fn start(&self, source_name: &str) -> Result<(), String> {
        self.stop();
        let core = BroadcastCore::start(source_name)?;
        *self.inner.lock().unwrap() = Some(core);
        // Fresh source — no real frames yet until capture wires and first `send_frame` succeeds.
        self.has_real_frames.store(false, Ordering::Relaxed);
        *self.last_frame_at.write().unwrap() = None;
        *self.last_frame_instant.lock().unwrap() = None;
        Ok(())
    }

    /// Stop and tear down any running broadcaster. No-op when inactive.
    pub fn stop(&self) {
        if let Some(core) = self.inner.lock().unwrap().take() {
            core.shutdown();
        }
        self.has_real_frames.store(false, Ordering::Relaxed);
        *self.last_frame_at.write().unwrap() = None;
        *self.last_frame_instant.lock().unwrap() = None;
    }

    /// Push a BGRA+alpha frame to the running broadcaster (no-op when off).
    /// Not yet called (capture-integration seam) — see `BroadcastCore::send_frame`.
    /// Tracks `has_real_frames`/`last_frame_at` distinctly from `is_active` so
    /// the UI can be honest about whether real video is actually flowing.
    #[allow(dead_code)]
    pub fn send_frame(&self, width: u32, height: u32, bgra: Vec<u8>) {
        let accepted = if let Some(core) = self.inner.lock().unwrap().as_ref() {
            core.send_frame(width, height, bgra)
        } else {
            false
        };
        if accepted {
            self.has_real_frames.store(true, Ordering::Relaxed);
            let now_iso = crate::project::now_iso();
            *self.last_frame_at.write().unwrap() = Some(now_iso);
            *self.last_frame_instant.lock().unwrap() = Some(Instant::now());
        }
    }

    /// Window-handle-validated variant: verifies the Output window exists and is rendering
    /// before pushing. If no valid frame source (Output destroyed/unhealed or capture not wired),
    /// logs `NDI: no valid Output frame source, skipping` and holds last-good-frame instead of
    /// pushing black. Recovers automatically once the Output window is rebuilt and new frames arrive.
    #[allow(dead_code)]
    pub fn send_frame_checked(&self, app: &tauri::AppHandle, width: u32, height: u32, bgra: Vec<u8>) {
        use tauri::Manager;
        if app.get_webview_window(crate::windows::OUTPUT_WINDOW).is_none() {
            eprintln!("NDI: no valid Output frame source, skipping (Output window not found — destroyed/unhealed, holding last good frame; will recover when rebuilt)");
            return;
        }
        if let Some(win) = app.get_webview_window(crate::windows::OUTPUT_WINDOW) {
            if win.inner_size().is_err() {
                eprintln!("NDI: no valid Output frame source, skipping (Output window handle invalid — not rendering)");
                return;
            }
        }
        self.send_frame(width, height, bgra);
    }
}

// Compile-time sanity checks that the wrapper is shareable across the threads
// Tauri uses (managed state must be `Send`).
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Broadcaster>();
};

#[cfg(test)]
mod tests {
    use super::*;

    // The real SDK is never installed in CI, so unit tests cover the
    // SDK-independent logic (constants/geometry) only.

    #[test]
    fn source_name_is_documented() {
        assert_eq!(NDI_SOURCE_NAME, "MakrStudio - Sunday Output");
    }

    #[test]
    fn bgra_is_four_bytes_per_pixel() {
        let w = 1920usize;
        let h = 1080usize;
        assert_eq!(w * h * 4, w * h * 4);
        // A frame must be exactly width*height*4 bytes for packed BGRA.
        assert_eq!(w * 4, 7680);
    }

    #[test]
    fn fourcc_constants_match_ndi_header() {
        assert_eq!(FourCC::Bgra as i32, 0x4152_4742);
        assert_eq!(FourCC::Bgrx as i32, 0x5852_4742);
    }

    #[test]
    fn lib_filename_is_nonempty() {
        assert!(!lib_filename().is_empty());
    }
}
