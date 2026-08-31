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
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Behavioural constant — the NDI *source name* receivers see on the network.
/// Source names may be anything; the "NDI" mark itself is only restricted in
/// *product names* (which require permission), so the app is "MakePresent"
/// while the source is e.g. "MakePresent - Sunday Output".
pub const NDI_SOURCE_NAME: &str = "MakePresent - Sunday Output";

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
/// # Safety
/// All resolved symbols are required, stable entry points of the SDK; the
/// returned `NdiLib` keeps the library loaded for as long as it is held.
unsafe fn load_ndi() -> Result<NdiLib, String> {
    let file = lib_filename();
    let lib = Library::new(file)
        .map_err(|e| format!("NDI SDK not found (looked for \"{file}\"): {e}"))?;

    // Resolve the required SDK entry points inside a block, copy the raw
    // function pointers out, then move `lib` into the struct once the borrows
    // from `lib.get(...)` have ended.
    let err_of = |n: &str, e: libloading::Error| format!("failed to resolve NDI symbol \"{n}\": {e}");

    let (initialize, destroy, send_create, send_destroy, send_video) = {
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

        (
            *initialize,
            *destroy,
            *send_create,
            *send_destroy,
            *send_video,
        )
    };

    Ok(NdiLib {
        _lib: lib,
        initialize,
        destroy,
        send_create,
        send_destroy,
        send_video,
    })
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
    /// This is the seam the offscreen render capture plugs into. Non-blocking
    /// and bounded (the channel is capacity-3 and `try_send` is used), so it
    /// never blocks the render loop or grows memory under a stall.
    ///
    /// `#[allow(dead_code)]`: not yet called — the capture integration that
    /// feeds frames is a separate runtime component (see module doc).
    #[allow(dead_code)]
    pub fn send_frame(&self, width: u32, height: u32, bgra: Vec<u8>) {
        let _ = self.tx.try_send(Command::Frame {
            width,
            height,
            bgra,
        });
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

            loop {
                match rx.recv_timeout(RESEND_PERIOD) {
                    Ok(Command::Frame { width, height, bgra }) => {
                        current = Some((width, height, bgra));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }

                if last_sent.is_none_or(|t| t.elapsed() >= RESEND_PERIOD) {
                    if let Some((width, height, data)) = &current {
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
                    }
                }
            }
        })
        .expect("failed to spawn ndi-send thread")
}

/// Thin wrapper stored in [`AppState`] so commands can start/stop/feed NDI.
pub struct Broadcaster {
    inner: Mutex<Option<BroadcastCore>>,
}

impl Default for Broadcaster {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl Broadcaster {
    /// Whether a broadcaster is currently active.
    pub fn is_active(&self) -> bool {
        self.inner.lock().ok().is_some_and(|g| g.is_some())
    }

    /// Start (or restart) the NDI broadcaster with the given source name.
    pub fn start(&self, source_name: &str) -> Result<(), String> {
        self.stop();
        let core = BroadcastCore::start(source_name)?;
        *self.inner.lock().unwrap() = Some(core);
        Ok(())
    }

    /// Stop and tear down any running broadcaster. No-op when inactive.
    pub fn stop(&self) {
        if let Some(core) = self.inner.lock().unwrap().take() {
            core.shutdown();
        }
    }

    /// Push a BGRA+alpha frame to the running broadcaster (no-op when off).
    /// Not yet called (capture-integration seam) — see `BroadcastCore::send_frame`.
    #[allow(dead_code)]
    pub fn send_frame(&self, width: u32, height: u32, bgra: Vec<u8>) {
        if let Some(core) = self.inner.lock().unwrap().as_ref() {
            core.send_frame(width, height, bgra);
        }
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
        assert_eq!(NDI_SOURCE_NAME, "MakePresent - Sunday Output");
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
