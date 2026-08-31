# MakePresent — Living Project Doc

This document is the source of truth for **what MakePresent is, how we build
it, and where we are**. Update it as the project evolves.

## What MakePresent Is

MakePresent is **DwellPraise Ministries' own church presentation software**.
It is **not a ProPresenter clone** — it is built to fit our specific service
flow, it is free forever, and it is **ours to control**. That last point
matters: no license fees, no feature roadmaps dictated by a vendor, no data
leaving our building, and the freedom to shape every detail to how our
volunteers actually run a service.

The audience is a **first-time volunteer**: someone who may have fifteen
minutes of training. The app has to feel obvious.

## Design Principles

1. **Simple first, one obvious path.** A first-time volunteer should see a
   single clear route from "open the app" to "slide is live" with no dead
   ends and no unexplained states. Every screen state must answer "what am I
   looking at and what do I do next?"
2. **Clarity over density.** Prefer fewer, clearer controls over packed,
   powerful ones. If a feature makes the common path harder to read, it does
   not ship yet.
3. **Polish is deferred.** Visual polish and animation wait until the
   underlying flow is already simple without them. A boring-but-clear
   interface beats a pretty-but-confusing one.
4. **Single source of truth.** All application state lives in the Rust
   backend; every window (Editor, Output, Stage) is a *dumb renderer* pushed
   fresh state. No window computes its own copy of "what should be live".

## Reliability Priorities (carried forward from Phase 1)

- **No crashes.** The app must survive anything the operator throws at it
  mid-service.
- **Predictable output.** Once a slide is live it stays live; no surprise
  window changes, no spurious re-layouts on the projection display.
- **Autosave + crash recovery.** Every edit is persisted automatically and
  an interrupted session recovers the last good state on the next launch.
- **Resource management under multi-hour, multi-screen use.** No unbounded
  memory or log growth; windows are created on demand and released when not
  needed.

## Anticipated Failure Modes (defend these in future features)

| Failure mode | Desired behavior |
| --- | --- |
| Output display disconnected / reconnected mid-service | Output falls back to the next available display (or stays until reconnected), never crashes or silently goes to the wrong screen. |
| Output display sleeps / screensaver activates independently of the editor | Output continues to show the live slide; re-presenting on wake must "just work". |
| Font not found on a different machine | Falls back silently to a sensible system font stack — never a blank or broken output. |
| Project files reference missing media when opened elsewhere | Missing media is detected, reported clearly, and degrades gracefully (placeholder) instead of crashing or silently showing nothing. |
| Unsaved work lost on abrupt close | Autosave + session bookkeeping means the next launch recovers the work and *tells the user* it did. |
| No visibility into pre-crash app state | A persistent, immediately-flushed event log (`logs/app.log`) plus versioned project snapshots let us reconstruct what happened. |

## Environment Adaptability (design intent for later phases, not built yet)

The app must behave sensibly across:

- **Single-monitor laptop** — everything on one screen, the Output window
  tiling or toggling sensibly alongside the Editor.
- **Dual-monitor setup** — Editor on one, Output on the other, Stage on
  whichever the operator chooses.
- **Full multi-display stage rig** — Output on the projection, Stage facing
  the platform, Editor at the operator desk.

**Default assumptions must degrade gracefully rather than error when a
display disappears.** Any configured display that no longer exists should be
skipped with a log entry, falling back to the largest remaining display —
never a dead end or a crash.

## Current Status

### Built (Phases 1–5)

Phase 1 — Foundation
- Two-window Tauri app (Editor + Output), single Rust `AppState` source of
  truth, dumb-renderer windows fed by a `state` broadcast event.
- Autosave (debounced worker, atomic writes), versioned snapshots, session
  bookkeeping, crash recovery with a visible notice.
- CI builds on Ubuntu 22.04 and Windows 2022.

Phase 2 — Stage display, icons, library, transitions
- Third window: **Stage Display** (large live slide, next-slide preview,
  running clock), created on demand, restored on launch if left on.
- App icon set generated and bundled.
- Persistent **song/slide library** (`library.json`): songs with multiple
  verses, client-side search, one-click add-to-playlist that links slides
  back to their source verse.
- Per-project **Cut / Fade transition**; Fade crossfades the output over
  ~400 ms via CSS.

Phase 3 — Onboarding, settings, logging
- Welcome message on first-ever launch; loading animation while the editor
  initialises.
- **Single-window onboarding**: only the Editor exists at launch; the Output
  window appears on demand (first live slide or "Show Output"), the Stage on
  its own toggle.
- **Settings import/export** (native dialogs): per-machine settings
  (display assignments, fullscreen, stage, default transition) — never the
  project or library — with schema validation and clear errors.
- **Logs**: persistent, immediately-flushed event log with rotation, a
  "Settings → Logs" panel (monospace, newest first), copy-to-clipboard and
  export-log-file buttons.

Phase 4 — Media (image/video) slide backgrounds
- `Background::Image` and `Background::Video` slide backgrounds, rendered by
  native `<img>` / `<video>` elements in the Output (muted, looping,
  `object-fit: cover`). A **custom GPU pipeline is explicitly deferred**.
- **Managed import**: picking a file copies it into `media/<hash>.<ext>`
  inside the app data dir (never references the original), dedupes identical
  content by **SHA-256 content hash**, and generates a thumbnail into
  `thumbnails/<hash>.jpg` with ffmpeg (`-ss <duration-aware>` for videos).
  Metadata (path, hash, thumb, video duration) is stored on the slide.
- ffmpeg availability is checked at startup and surfaced loudly if missing —
  thumbnails are never skipped silently.
- **Startup cache verification**: every media asset referenced by the project
  or library must still have its source file and thumbnail; missing/corrupt
  thumbnails are rebuilt automatically and missing sources are logged loudly.
  The UI shows a dark placeholder (never a broken-image glyph) when a thumb
  fails to load.
- Editor: "Add media" in the Background picker opens the import flow; the
  chosen slide shows a thumbnail swatch (with remove); playlist swatches show
  each slide's thumbnail.
- **On-deck preload (resource management)**: the backend names one "on-deck"
  slide per state (selected-but-not-live, else the next playlist slide). The
  Output keeps exactly one hidden preloader for it; media is preloaded while
  a slide is on deck so a cut never decodes on demand mid-service. Only the
  live + leaving (during a 400 ms fade) + the single on-deck element exist at
  any time — no unbounded accumulation over a service.

Phase 5 — NDI broadcast output (sending side)
- **NDI sending infrastructure** (`broadcast.rs`): publishes the live slide
  as an NDI source on the LAN so a video switcher can cut to it. It registers
  a sender (`"MakePresent - Sunday Output"`), runs on its **own dedicated
  thread** independent of the Output render loop, keeps the source alive by
  re-sending the last frame on a cadence, and pushes BGRA+alpha frames through
  a bounded, non-blocking channel (no render-loop stalls, no unbounded memory).
- **Runtime-loaded SDK**: the NDI SDK (Vizrt) is **not** vendored and **not**
  required to build. `broadcast.rs` hand-binds the C ABI with `libloading` and
  loads `Processing.NDI.Lib.x64.dll` / `libndi.so.5` / `libndi.dylib` at
  runtime. If the SDK is missing, enabling NDI logs a clear error and
  everything else keeps working — the crate builds and `cargo check` passes in
  CI without the SDK. (The other crates.io binding crates were rejected: they
  need the SDK headers + libclang at build time, or are GPL-3.0.)
- **NDI Look**: a per-machine `ndiLookId`, settable from Settings → Looks →
  "NDI Feed", styles the broadcast feed independently of the on-screen Output
  (see `set_ndi_look`). Settings export/import round-trip `ndi_enabled` +
  `ndiLookId`, and NDI is started/stopped on import to match.
- **Settings + IPC**: `set_ndi_enabled` / `set_ndi_look` commands; a
  broadcast enable toggle (with source-name + ndi.video link) and Look
  assignment in the settings UI; runtime `BroadcastView` in `ClientState`.
- **Honest scope**: the webview → pixel **capture** (an offscreen render target
  mirrored from Output) is a runtime concern that this phase does **not**
  wire. `BroadcastCore::send_frame` is the clean seam a later capture step
  feeds. So NDI *streaming* requires: install the NDI SDK, then hook the
  render capture into `send_frame` and drive it in a live app — neither of
  which can be exercised headless/CI.

### Date
- 2026-08-31 — Phase 5 (NDI sending side) shipped; NDI capture + live
  streaming remains a documented runtime follow-up.

### Explicitly Deferred

- Remote control (web/phone)
- MIDI / OSC
- Audio playback (video backgrounds are muted this phase)
- Custom GPU playback pipeline (native `<video>`/`<img>` in the webview for now)
- GStreamer (ffmpeg/ffprobe CLI for thumbnails and probing instead)

None of these should influence the current architecture decisions.

## Known Issues

- **Windows build — post-Output-creation backend freeze (fixed 2026-08-31).** After the first successful command that creates the Output window (confirmed via logs: `move_output_to` and `ensure_output` both complete correctly, inline on main thread, autosave succeeds), **all subsequent backend commands became unresponsive** — not a startup deadlock. Root cause: `windows::output_visible()` called `WebviewWindow::is_visible()` which on Windows (WebView2/wry) dispatches to the main thread and blocks the Tauri worker; after the first WebView2 window the main thread pump is degraded and the query never returns, freezing every `snapshot()` (every `mutate`). Also `show_output` blocked the worker on WebView2 creation. Fix: `output_visible` now checks `get_webview_window().is_some()` (HashMap lookup, no main-thread dispatch) `src-tauri/src/windows.rs:435`, `show_output` now queues window work fire-and-forget via `run_on_main_async` so `set_live_slide` can `snapshot`+`emit` without waiting for WebView2, autosave no longer holds `RwLock` across I/O `src-tauri/src/project.rs:622`, and `snapshot` uses a single consistent read `src-tauri/src/commands.rs:13`. Verified on Windows — `add_slide`/`delete_slide`/`add_song_to_playlist` now responsive after Output appears.

## Onboarding Flow

Startup creates **only the Editor window**.

1. The Editor shows a brief loading animation while the backend runs the
   recovery check and sends the initial state.
2. On the **very first launch** the playlist area shows an inline welcome:
   "Welcome to MakePresent — add your first slide to get started". It
   dismisses permanently once the first slide is added or a project is
   created.
3. The Output panel reads **"Not shown yet"** with a clear **Show Output**
   button. The Output window is not created at startup — a black fullscreen
   window with nothing on it is exactly the dead end we are avoiding.
4. The Output window is created and placed (configured display, or the best
   second monitor) the first time:
   - the operator sets a slide live, **or**
   - the operator clicks **Show Output**.
   Whichever happens first.
5. The Stage Display is created purely on demand through its toggle and
   remains hidden unless the operator switches it on.
6. On later launches a previously-live project is restored from autosave
   (with a recovery notice when the last exit was unclean), but windows are
   still created on demand — the operator decides what is on screen, not
   the app.

## Layout of the Code

```
src-tauri/src/
  lib.rs        App lifecycle: setup (recovery, logger, autosave worker), finalize, commands
  state.rs      AppState — the single source of truth
  project.rs    Domain model (Project/Slide/Settings/Library), persistence, autosave worker
  windows.rs    Window lifecycle: Output + Stage, display picking
  media.rs      Media import/cache: copy+hash, ffmpeg thumbnails, startup verification
  broadcast.rs  NDI sender: runtime-loaded SDK (libloading), dedicated send thread
  commands.rs   Tauri IPC: mutations + broadcast, settings import/export, logs, media import, NDI
  logging.rs    Rolling, immediately-flushed event log (logs/app.log)
src/
  editor.ts / Editor.svelte     Operator's window (playlist, edit, output/stage controls, settings)
  output.ts / Output.svelte     Dumb projection renderer (cut/fade crossfade)
  stage.ts / Stage.svelte       Dumb performer-facing renderer (current + next)
  lib/types.ts                  Shared client contract
  lib/sync.ts                   Tauri invoke + event subscriptions
```

## NDI licensing & installation

NDI® is a registered trademark of Vizrt. MakePresent's broadcast build loads
the **free standard NDI SDK** at runtime and does **not** vendor it, so no NDI
code or headers ship with the app and the app builds, tests, and CI-run without
it. To actually broadcast:

1. Download the free NDI SDK from <https://ndi.video> and install it (on
   Windows place `Processing.NDI.Lib.x64.dll` alongside the app; on Linux/macOS
   put `libndi.so.5` / `libndi.dylib` on the loader path).
2. Keep the ndi.video link near any NDI usage and the trademark attribution
   "NDI® is a registered trademark of Vizrt NDI AB".
3. The NDI SDK is closed-source and royalty-free for the standard SDK; its own
   license terms (in the SDK download) govern distribution of its DLLs.

The `libloading` approach avoids the GPL-3.0 `ndi-sdk-sys` binding crate and
the build-time SDK requirement of the other crates.io binding crates.

Data lives under the app data dir (`~/.local/share/com.makesoftware.makepresent`):
`project.json` (autosaved), `versions/` (snapshots), `session.json`
(recovery bookkeeping), `settings.json` (per-machine), `library.json`
(songs), `logs/app.log` (event log), `media/` (managed content-hashed media
copies), `thumbnails/` (hash-keyed thumbnails).