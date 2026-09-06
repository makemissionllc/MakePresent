//! Optional exit/outro animation on real Quit (tray "Quit MakrStudio").
//!
//! When the operator really quits — NOT a close-to-tray, which keeps hiding
//! the Editor as before — and an exit-animation video is configured in
//! per-machine settings, the Output and Stage windows each play that video
//! full-bleed for a moment before the app closes, so the service never ends
//! with an abrupt cut to desktop on the projector or confidence monitor.
//! The Editor window closes immediately; the outro is audience-facing only.
//!
//! Safety design (the video is cosmetic only, shutdown is never held hostage):
//! - The project + library are persisted *before* the outro starts, so state
//!   is safe even if the process died mid-outro. `finalize()` at real exit
//!   re-persists and remains the single writer of the `clean_shutdown` flag,
//!   so a mid-outro kill still (correctly) shows the recovery notice.
//! - A hard ceiling (`OUTRO_CAP`) bounds the whole interlude: shutdown always
//!   proceeds after it, with no click/Esc-to-skip interaction (the operator
//!   is not looking at those displays). A renderer finishing early
//!   (`outro-done` on video `ended`/`error`) only *shortens* the wait.
//! - A missing/corrupt/unsupported file fails silent: quit proceeds instantly
//!   on all windows, exactly as if no video were configured.
//! - NDI is deliberately NOT fed the outro in this pass: the send path has no
//!   wired capture yet (see the NDI honesty work), so there is no pixel seam
//!   to push outro frames through. The broadcaster keeps its existing
//!   last-frame keep-alive until `finalize()` stops it. Tracked as follow-up.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Hard ceiling for the outro interlude: `app.exit(0)` runs no later than
/// this after Quit, no matter what the video (or a missing renderer) does.
pub const OUTRO_CAP: Duration = Duration::from_secs(6);

/// Payload the Output/Stage renderers listen for (`exit-outro` event).
const OUTRO_EVENT: &str = "exit-outro";

/// Fired by each renderer when its outro video ends or fails to load.
pub const OUTRO_DONE_EVENT: &str = "outro-done";

/// Ensures `finish_quit` runs exactly once (cap thread vs. `outro-done`
/// races, and one event per visible window).
static OUTRO_EXITING: AtomicBool = AtomicBool::new(false);

fn log(app: &tauri::AppHandle, level: crate::logging::Level, message: &str) {
    app.state::<crate::state::AppState>()
        .logger
        .log(level, message);
}

/// Persist project + library right now, ahead of any outro delay. Mirrors the
/// persist half of `finalize()` without touching the session flag.
fn persist_now(app: &tauri::AppHandle) {
    let state = app.state::<crate::state::AppState>();
    let data_dir = state.app_data_dir();
    {
        let snapshot = state.project.read().unwrap().clone();
        let _ = crate::project::persist(&snapshot, &data_dir);
    }
    {
        let snapshot = state.library.read().unwrap().clone();
        let _ = crate::project::write_library(&data_dir, &snapshot);
    }
}

/// Whether a renderer window currently exists *and* is showing. Fail-open: if
/// the visibility query itself fails, assume visible so a real audience still
/// gets the outro (the cap still guarantees shutdown).
fn renderer_showing(app: &tauri::AppHandle, label: &str) -> bool {
    match app.get_webview_window(label) {
        Some(w) => w.is_visible().unwrap_or(true),
        None => false,
    }
}

fn finish_quit(app: &tauri::AppHandle) {
    if OUTRO_EXITING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        app.exit(0);
    }
}

/// Called from the `outro-done` frontend event: a renderer finished (video
/// ended or failed) — shorten the wait and exit now.
pub fn note_outro_done(app: &tauri::AppHandle) {
    log(
        app,
        crate::logging::Level::Info,
        "outro: renderer finished early — exiting",
    );
    finish_quit(app);
}

/// Real-Quit entry point (tray "Quit MakrStudio"). Close-to-tray never
/// reaches here — that path still just hides the Editor.
pub fn begin_quit(app: &tauri::AppHandle) {
    crate::windows::set_shutting_down();
    // State first: safe even if the process died mid-outro.
    persist_now(app);

    let path = app
        .state::<crate::state::AppState>()
        .current_settings()
        .exit_animation
        .unwrap_or_default()
        .trim()
        .to_string();
    let output_on = renderer_showing(app, crate::windows::OUTPUT_WINDOW);
    let stage_on = renderer_showing(app, crate::windows::STAGE_WINDOW);
    let file_ok = !path.is_empty() && std::path::Path::new(&path).is_file();

    if !file_ok || (!output_on && !stage_on) {
        if !path.is_empty() && !file_ok {
            // Cosmetic only: log at Info, never block quitting on a bad file.
            log(
                app,
                crate::logging::Level::Info,
                "outro: configured video missing/unreadable — quitting instantly",
            );
        }
        app.exit(0);
        return;
    }

    log(
        app,
        crate::logging::Level::Info,
        &format!(
            "outro: playing exit animation (output={}, stage={})",
            output_on, stage_on
        ),
    );
    // The Editor is operator-facing: close it immediately. (The
    // CloseRequested interceptor lets it really close once quit starts.)
    if let Some(editor) = app.get_webview_window(crate::windows::EDITOR_WINDOW) {
        let _ = editor.close();
    }
    // Tell every renderer to play; only Output/Stage act on it.
    let _ = app.emit(OUTRO_EVENT, serde_json::json!({ "path": path }));
    // Hard ceiling — shutdown ALWAYS proceeds after this.
    let cap_app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(OUTRO_CAP);
        finish_quit(&cap_app);
    });
}
