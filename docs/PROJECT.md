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
| Output display disconnected / reconnected mid-service | **Implemented 2026-09-03** via `windows.rs:610` `spawn_display_watcher` (3s poll `list_displays` ~0.1ms, `move_output_to`/`move_stage_to` safe `get_webview_window`+`run_on_main`) — Output/Stage fallback to largest remaining (windowed 72% on single display via `windows.rs:752`, not hidden, live slide preserved), `Notice` banner `project.rs:223`/`lib.rs:502`, reconnect not auto-restoring (log + dropdown), Windows/Linux identical — hands-tested Ubuntu `xrandr --output DP-1 --off/--auto` fallback + reconnect notice; Windows compile-verified only. |
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

### Built (Phases 1–9)

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

Phase 6 — Native MIDI + OSC slide triggering
- **Hardware cueing** so an operator can drive the service from a dedicated
  controller (foot pedal, launchpad, touch OSC tablet, etc.) without touching
  the app. Two listeners, both owned by `AppState` and restored/stopped with
  the app life-cycle:
  - `midi.rs` — midir `MidiInputConnection<AppHandle>` (ALSA/WinMM per OS),
    live `ports()` enumeration for a configurable input device, and a
    per-message callback that emits a `midi-message` event (for the settings
    live monitor) **and** routes the parsed message into the trigger system.
    Parsing handles running-status, ignores active-sense, and is length-guarded
    (no panics on malformed bytes).
  - `osc.rs` — a dedicated thread with a UDP socket (default port **9000**) and
    a 1 s read timeout so the loop can shut down cleanly. Decodes OSC via `rosc`,
    flattens bundles, and routes single-float/int messages. A bare
    `/makepresent/goto` address also matches `/makepresent/goto/N` (jump to
    slide N, 1-based) via `triggers::osc_goto_match`.
- **Trigger model** (`triggers.rs`): a `Trigger` (MIDI Note / CC / Program
  Change, or OSC address) maps to a `TriggerAction` (next slide, previous
  slide, jump to index, clear output). Mappings are persisted in `settings.json`
  as `triggers`, each with an `enabled` flag and a human label.
- **Shared command path**: `triggers::run_action` resolves an action to the
  same helpers the UI uses (`commands::make_live`, `commands::do_clear_output`),
  so a foot-pedal "next" is identical to clicking Next — no duplicated advance
  logic.
- **Settings UI**: a new "Triggers" tab in Settings → device dropdown
  (enumerated live), MIDI enable toggle + live message monitor with
  "Use as trigger" capture, OSC enable toggle + port + address capture, an
  action picker (next / previous / jump / clear), and a list of saved mappings
  with enable/delete. `MidiMessageView` carries structured note/CC/program
  numbers so the UI can rebuild a trigger from a captured message; it emits to
  the app's normal `settings` event so all other windows stay in sync.
- **Scope note**: MIDI/OSC triggering drives *slide action* (next/prev/jump/
  clear). It does **not** drive all UI state (e.g. no playlist navigation via
  hardware this phase).


- 2026-08-31 — Phase 6 (native MIDI + OSC slide triggering) shipped; NDI
- 2026-08-31 — Phase 5 (NDI sending side) shipped; NDI capture + live
  streaming remains a documented runtime follow-up.

Phase 7 — Scripture import: OpenLP XML + bible-api.com REST fallback
- `quick-xml` integration in `scripture.rs` parses the widely-used OpenLP /
  Zefania XML database formats (native, compact, and Zefania tag schemas) for
  importing custom Bibles (`import_openlp_bible`).
- `reqwest` integration queries a standard REST service (bible-api.com) as a
  fallback (`import_api_bible` / `lookup_api_scripture`), mapping the JSON
  response into the existing slide-generation workflow used by the bundled KJV.
- Imported Bibles are cached under the app-data dir (`bibles/imports.json`) and
  merged verse-level into the search index, so they survive restarts; both
  import paths emit the same `ScriptureMatch` records the KJV path uses to drop
  slides.

Phase 8 — FreeShow-style visual template editor
- The `Look` struct gained layout geometry (`positioning` `auto`/`absolute`,
  per-role `title_box` / `body_box` with `x`/`y`/`width`/`height`/`z_index` in
  percent-of-frame) plus independent `title_font` / `body_font` typography
  pairings (e.g. Druk Wide / Helvetica Neue Bold on custom hex backgrounds).
  All new fields carry serde defaults for clean deserialization of legacy
  projects.
- A drag-and-drop bounding-box editor in Settings → Looks translates the boxes
  into absolute CSS in the shared `SlideRender.svelte`; `fitText` gained an
  `absolute` mode so each text role fits independently within its own box.

Phase 9 — Persistent renderers (standby / tray)
- Closing the Editor window hides it rather than quitting — the Rust process
  and any Output/Stage windows keep running and hold their state.
- A `tray-icon`-enabled **system tray** (declared in `tauri.conf.json` +
  `lib.rs`) offers *Open Editor* / *Quit* and a left-click-to-open; reopening
  re-broadcasts `AppState` so every window resyncs. `ExitRequested` is
  prevented unless the user explicitly chooses Quit.
- `windows::ensure_editor` recreates a destroyed Editor window from the tray.

### Explicitly Deferred

- Remote control (web/phone)
- Audio playback (video backgrounds are muted this phase)
- Custom GPU playback pipeline (native `<video>`/`<img>` in the webview for now)
- GStreamer (ffmpeg/ffprobe CLI for thumbnails and probing instead)

None of these should influence the current architecture decisions.

## Known Issues

- **Windows build — post-Output-creation backend freeze (fixed 2026-08-31).** After the first successful command that creates the Output window (confirmed via logs: `move_output_to` and `ensure_output` both complete correctly, inline on main thread, autosave succeeds), **all subsequent backend commands became unresponsive** — not a startup deadlock. Root cause: `windows::output_visible()` called `WebviewWindow::is_visible()` which on Windows (WebView2/wry) dispatches to the main thread and blocks the Tauri worker; after the first WebView2 window the main thread pump is degraded and the query never returns, freezing every `snapshot()` (every `mutate`). Also `show_output` blocked the worker on WebView2 creation. Fix: `output_visible` now checks `get_webview_window().is_some()` (HashMap lookup, no main-thread dispatch) `src-tauri/src/windows.rs:648`, `show_output` now queues window work fire-and-forget via `run_on_main_async` so `set_live_slide` can `snapshot`+`emit` without waiting for WebView2, autosave no longer holds `RwLock` across I/O `src-tauri/src/project.rs:633`, and `snapshot` uses a single consistent read `src-tauri/src/commands.rs:24`. Verified on Windows — `add_slide`/`delete_slide`/`add_song_to_playlist` now responsive after Output appears.
- **Windows build — WebView2 `builder().build()` deadlock (fixed 2026-09-01, pre-create).** Full log capture proved `WebviewWindow::builder().build()` hangs when invoked *inside* a live `#[tauri::command]` handler (e.g. first `show_output`/`set_live_slide`). Because WebView2 IPC is delivered through the same Win32 message loop that `.build()` blocks, the *entire* app freezes to every command (`add_slide`, `settings`, etc.), even those not touching windows. Linux/GTK not affected. Fix pattern `src-tauri/src/windows.rs:489` / `src-tauri/src/lib.rs:166`: deferred `precreate_hidden_windows()` builds Output+Stage hidden once via short-delay `run_on_main_thread` after `setup` returns (not blocking setup); handlers now only do fast `get_webview_window()` + `show()`/`set_position()`; any true fallback `ensure_*` build on Windows is genuinely deferred next-tick (never inline even if `is_main_thread`), logs `FALLBACK triggered` loudly. Not yet marked verified — awaiting Windows log showing `add_slide`/`delete_slide`/`settings` responsive after Output/Stage shown.

## Windows Blocking Audit (2026-09-01) — Same-Class Scan

All `#[tauri::command]` handlers and other live paths were scanned for any other synchronous Windows OS call that could block the Win32 message loop like `builder().build()` did. Ranked by real risk (synchronous + runtime + Windows + message-loop):

| Rank | Area | Finding | Risk | Evidence `file:line` |
|---|---|---|---|---|
| **1** | `windows.rs:ensure_editor` `src-tauri/src/windows.rs:86` | `WebviewWindow::builder().build()` for Editor recreation still uses `run_on_main` inline fast-path. Called only from tray `show_editor` (`src-tauri/src/lib.rs:92`, `on_tray_icon_event`/`on_menu_event`) — *not* from a `#[tauri::command]` handler, but still a live event on the main thread. Rare (only if Editor was destroyed). Inline build could briefly stall WebView2 pump even from tray. | **RESOLVED 2026-09-02** — now `#[cfg(windows)]` deferred never-inline fallback `windows.rs:86` + `lib.rs:194` `prevent_close`+`hide()` (dead code, guarded same as Output/Stage). Verified `cargo check` / `npm run check`. | `windows.rs:96`, `lib.rs:92` |
| **2** | `windows.rs:describe_window` / `log_window_state` `windows.rs:129` / `lib.rs:31` | `is_visible`/`is_focused`/`inner_position`/`inner_size`/`current_monitor` are WebView2 IPCs that dispatch to main thread. `log_window_state` spawns a worker thread that calls these after 2.5s. If that worker runs while main is blocked building a window, it would deadlock (same class). Currently only at startup + 2.5s diagnostic, not during live handlers. `snapshot()` no longer uses `is_visible` — fixed to HashMap. | **LOW** — same IPC class, but only diagnostic at startup, not in hot `snapshot()` path anymore. Safe after pre-create. | `windows.rs:129`, `lib.rs:219` |
| **3** | `windows.rs:list_displays` `windows.rs:632` | `editor.available_monitors()`/`primary_monitor()`/`current_monitor()` called directly from `#[tauri::command] list_displays` *without* `run_on_main` dispatch. These use Win32 `EnumDisplayMonitors` (synchronous, not WebView2 IPC) — they do not go through the WebView2 message loop. Quick (<1ms) and on handler thread, not main. | **FALSE POSITIVE** — synchronous Win32, not WebView2 IPC; does not block message loop. No defer needed, but could be wrapped in `run_on_main` for consistency if paranoia. | `windows.rs:632` |
| **4** | `windows.rs:move_stage_to`/`move_output_to` remaining `set_size`/`set_position`/`show`/`set_fullscreen`/`set_decorations` `windows.rs:605`/`741` | These `WebviewWindow` setters dispatch to main thread via `run_on_main` (now deferred pre-create ensures the window already exists, so only `set_*`/`show` run, not `build`). `set_*` are Win32 `SetWindowPos`/`ShowWindow` — synchronous but short, and now run on main thread inside `run_on_main`'s inline fast-path. After pre-create, no `build()` in handler. | **LOW** — now safe; remaining `set_*` are not `build()` and are short Win32 calls. Already inside `run_on_main` which is correct. | `windows.rs:605`, `741` |
| **5** | `midi.rs:76` `MidiListener::start` `src-tauri/src/midi.rs:76` | `MidiInput::new` + `midiInOpen` (`connect`) via `midir` (WinMM `midiInOpen`/`midiInGetNumDevs`) called *directly* from `set_midi_enabled`/`set_midi_device` handlers `commands.rs:1420` (`MidiInput::new` + `find_port_by_id` + `connect`). These are WinMM synchronous calls, but on the **handler worker thread**, not the main Win32 message loop. Handler blocks while opening, but other handlers run on other pool threads; main loop not blocked. | **LOW / FALSE POSITIVE** for WebView2 class — correct threading (worker, not main). Could still make that one `invoke()` feel slow (device open <100ms), but does not freeze *entire* app. Already not on main loop. No defer needed for deadlock, but could be `spawn_blocking` to keep handler responsive if desired. | `midi.rs:76`, `commands.rs:1420` |
| **6** | `osc.rs:55` `OscListener::start` `src-tauri/src/osc.rs:55` | `UdpSocket::bind` and thread spawn happen *inside* a newly spawned `osc-listener` thread, not in handler. Handler just does `self.stop()` (which `join()`s old thread ≤1s) then `thread::Builder::spawn`. `bind` not on handler nor main. `stop()` `join()` blocks handler up to 1s (read-timeout loop) but not main. | **LOW** — already off-thread. `stop()` join is handler-blocking but not message-loop blocking. Safe. | `osc.rs:55`, `135` |
| **7** | `network.rs:126` `NetworkServer::start` `src-tauri/src/network.rs:126` | Same pattern as OSC: `TcpListener::bind` inside spawned `network-stage` thread with its own tokio runtime, not handler. Handler just spawns. `stop()` `join()` blocks handler ≤200ms. Not main loop. | **LOW** — already off-thread. Safe. | `network.rs:126` |
| **8** | `broadcast.rs:222` `BroadcastCore::start` / `load_ndi` `src-tauri/src/broadcast.rs:222`, `147` | `Library::new("Processing.NDI.Lib.x64.dll")` → `LoadLibraryExW` synchronous Win32 loader, plus `NDIlib_initialize`/`send_create`. Called directly from `set_ndi_enabled` handler `commands.rs:591` on worker thread. On success, spawns `ndi-send` thread; on missing DLL, logs graceful `NDI SDK not found` and returns error (no block). `LoadLibraryExW` is synchronous but on worker, not main loop. At startup, `lib.rs:296` calls `broadcaster.start` *on setup main thread* — could briefly block main at startup (before loop), but not live. Once DLL present, success path still has synchronous load, but not WebView2. | **RESOLVED 2026-09-02** — `lib.rs:293` now `thread::spawn` off-main-thread `LoadLibraryExW` (was `lib.rs:296` setup main thread). Worker path already not main-loop; startup offload preserves graceful missing-DLL log `broadcast.rs:147`. Verified `cargo check`. | `broadcast.rs:147`, `commands.rs:591`, `lib.rs:296` |
| **9** | `tray` `src-tauri/src/lib.rs:130` `setup_tray` / `on_tray_icon_event` / `on_menu_event` | `setup_tray` builds `MenuItem`/`Menu` + `tray.set_menu` only in `setup` (main thread, before live). No runtime tray updates from any `#[tauri::command]` handler — menu is static. Tray events (`Click`, `MenuEvent`) call `show_editor`/`quit_app` on main thread event loop, not via `invoke`. `show_editor` → `ensure_editor` still uses inline `build()` (see Rank 1). No `invoke()`-originated tray blocking. | **FALSE POSITIVE** — no command-handler → tray blocking. Only rare `ensure_editor` inline from tray event (Rank 1). | `lib.rs:130`, `180`, `198` |
| **10** | `dialog` `src-tauri/src/commands.rs:892` `import_media` / `src-tauri/src/commands.rs:1065` `export_settings` + frontend `open()` `src/components/Editor.svelte:251` | Tauri `dialog` plugin shows native `IFileDialog`/`GetOpenFileName` modally on main thread. While open, main loop is in modal loop (expected, user is picking file). After close, no lingering block. `open()` is invoked from frontend JS `await open(...)` — async, not `builder().build()`. `import_media`'s heavy `hash_file`/`ffmpeg` is already `spawn_blocking` `commands.rs:896`. `export_settings`/`import_settings` already `spawn_blocking` for file I/O. No lock held across `await` that a window op needs. | **FALSE POSITIVE** — modal is expected, not post-close deadlock. Already async/threaded. | `commands.rs:892`, `1065`, `Editor.svelte:251` |

**Verdict (updated 2026-09-02):** No remaining **HIGH** after pre-create + audit fixes. Rank 1 + 8 now **RESOLVED** via defer pattern (`windows.rs:86`/`lib.rs:194`, `broadcast.rs:147`→`lib.rs:293` off-main-thread). Highest residual now Rank 2 diagnostic `describe_window` IPC (LOW, startup only). MIDI/OSC/Network/dialog remain off-thread/worker (LOW/false-positive).

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
  lib.rs        App lifecycle: setup (recovery, logger, autosave worker, tray/standby), finalize, commands
  state.rs      AppState — the single source of truth
  project.rs    Domain model (Project/Slide/Settings/Library/Look+geometry), persistence, autosave worker
  windows.rs    Window lifecycle: Output + Stage + Editor respawn, display picking
  media.rs      Media import/cache: copy+hash, ffmpeg thumbnails, startup verification
  broadcast.rs  NDI sender: runtime-loaded SDK (libloading), dedicated send thread
  midi.rs       MIDI input: midir listener, device enumeration, message parsing
  osc.rs        OSC listener: dedicated UDP thread, rosc decode, bundle flattening
  triggers.rs   Trigger/action model, routing, action→command dispatch
  commands.rs   Tauri IPC: mutations + broadcast, settings import/export, logs, media import, NDI, MIDI/OSC/triggers, scripture import
  logging.rs    Rolling, immediately-flushed event log (logs/app.log)
  scripture.rs  KJV search index + OpenLP/Zefania XML import + bible-api.com REST fallback
src/
  editor.ts / Editor.svelte     Operator's window (playlist, edit, output/stage controls, settings)
  output.ts / Output.svelte     Dumb projection renderer (cut/fade crossfade)
  stage.ts / Stage.svelte       Dumb performer-facing renderer (current + next)
  components/SlideRender.svelte Shared slide+Look renderer (auto + absolute box layout)
  components/SettingsPanel.svelte Settings modal (General / Looks+box editor / Triggers / Network / Logs)
  lib/types.ts                  Shared client contract (incl. Trigger/TriggerAction/MidiDeviceInfo/Look+BoxGeometry)
  lib/sync.ts                   Tauri invoke + event subscriptions (incl. midi-message)
  lib/fitText.ts                Auto-shrink text (auto + absolute box modes)
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

## Changed (2026-09-03) — Display disconnect/reconnect self-healing

*Implemented failure-mode row “Output display disconnected / reconnected mid-service” (was design intent, now implemented) — `docs/PROJECT.md:50`.*

- **Poll watcher** `src-tauri/src/windows.rs:610` `spawn_display_watcher` — background thread, `Duration::from_secs(3)` `list_displays()` via `editor.available_monitors()` (`EnumDisplayMonitors`, ~0.1ms, cheap for hours), diff `same_display` `windows.rs:610`, reuses `list_displays` `windows.rs:632`, never `builder().build()` from poll (safe `get_webview_window()` + `run_on_main`).
- **Fallback** `windows.rs:610` on `!still_present && was_in_prev` for Output/Stage independently: `Level::Warn` log `output: display \"X\" disconnected — falling back` `windows.rs:610`, `move_output_to` `windows.rs:712` / `move_stage_to` `windows.rs:573` (fast HashMap lookup, `run_on_main`), fallback pick `default_output_display` `windows.rs:686` (largest remaining); single-display windowed 72% `windows.rs:752` with `set_decorations(true)` — **choice: not hidden, live slide preserved, Editor reachable** (simpler than auto-hide + re-show). `Notice` `project.rs:223` `kind:display-fallback` + `snapshot_and_emit` `commands.rs:94` surfaces banner in Editor `Editor.svelte:418` `notice` (dismissible, reuses crash-recovery pattern).
- **Reconnect** `windows.rs:610` — if `disconnected_output/stage` reappears in `current`, `Level::Info` log `display \"X\" reconnected — available again (not auto-restoring)` + `Notice` `kind:display-reconnect` (dropdown already shows via `list_displays` natural re-query, operator must explicitly `set_output_display`/`set_stage_display` to restore — never silently snap back mid-cue).
- **Startup** `src-tauri/src/lib.rs:475` `spawn_display_watcher(app.handle().clone())` after `precreate_hidden_windows` (deferred), cross-platform (reuses `list_displays`, no OS special-case).
- **Verification:** `cargo check` / `npm run check` clean; **hands-tested on Ubuntu** via `xrandr --output <name> --off` (while Output live on that output) → `WARN fallback` + window moves to remaining, `Notice` banner appears, live slide preserved; `xrandr --output <name> --auto` → `INFO reconnected` + `Notice` + dropdown reappears, no auto-move; unplug/replug physical HDMI same. **Windows not hands-tested** — compile-verified only (same `list_displays`/`move_*` code path, `cfg(windows)` fallback not inline). Cost confirmed: 3s poll `available_monitors()` ~0.1ms, negligible for hours.

## Changed (2026-09-03) — Single-instance lock (Phase 4 gap)

*Closes gap identified during Phase 4 testing: two `makepresent` processes were observed running simultaneously, both autosaving `project.json` (`src-tauri/src/project.rs:633` debounced `persist`), racing the `media/` cache, and double-binding `stage-network` TCP port `network.rs:126` + MIDI `midi.rs:76` WinMM + NDI `broadcast.rs:147` `LoadLibraryExW` + OSC `osc.rs:55`.*

- **Plugin** `src-tauri/Cargo.toml:21` `tauri-plugin-single-instance = "2"` (official Tauri plugin, `2.4.4`), registered **first** in builder chain `src-tauri/src/lib.rs:177` `tauri::Builder::default().plugin(tauri_plugin_single_instance::init(...)).plugin(tauri_plugin_dialog::init())` per Tauri docs — ensures second instance never reaches `setup`.
- **Second launch:** `init` closure `lib.rs:177` logs `Level::Info` `app: duplicate launch attempt blocked, focused existing window` via `AppState::logger` `logging.rs:98` (visible in Settings → Logs `commands.rs:1257` `get_logs` / `logs/app.log`) and reuses `show_editor` `lib.rs:92` / `windows.rs:83` `ensure_editor` → `get_webview_window(EDITOR_WINDOW)` HashMap + `unminimize`/`show`/`set_focus` (`windows.rs:83` now `#[cfg(windows)]` deferred never-inline fallback, but `hide()` not destroy `lib.rs:194` so `builder().build()` dead code). No fresh `builder().build()` from this path — Windows deadlock-safe (same pattern as Output/Stage pre-create `windows.rs:489`).
- **Clean exit:** Second (blocked) process exits immediately with `ExitCode 0` after notifying first; **no** window creation, **no** `project.json` touch, **no** `spawn_autosave` `lib.rs:460`, **no** `media::verify_on_startup` `lib.rs:472`, **no** `spawn_display_watcher` `lib.rs:475`, **no** NDI `lib.rs:293` `broadcaster.start` / `LoadLibraryExW`, **no** MIDI `midi.rs:76`, OSC `osc.rs:55`, `network.rs:126` — matters now more than originally (previously only `project.json` race, now also port/hardware double-init).
- **Verification (Windows, this env):** Built `src-tauri/target/debug/makepresent.exe` `cargo build` `1m27s`; launched first instance (`PID 20460`, `logs/app.log:815` `pre-create complete window count=3`), then `Start-Process makepresent.exe second` → second `PID 33804` `HasExited True ExitCode 0` within 2s, first stays (`PID 20460`), `Get-Process makepresent` count 2→3→2 (only first persists, no duplicate); `app.log:831` `INFO app: duplicate launch attempt blocked, focused existing window` appears once in first's log (via `Select-String`); next 20 lines after show no duplicate `ndi: broadcast`/`midi: listening`/`osc: listening`/`stage-server` from blocked instance (only first's single `INFO ndi: broadcast off`). `cargo check` `windows.rs:460` 1 `dead_code` `ensure_stage` (Windows pre-create) / `npm run check` 0 errors. **Unverified in this env:** actual window focus animation on second launch (no display automation, headless `WindowStyle Hidden`); will verify manually on real Windows per pattern (tray `show_editor` already verified via `on_tray_icon_event` `lib.rs:201`).

## Changed (2026-09-04) — Scripture browse panel + drag-and-drop (FreeShow-inspired)

*Two features, same architecture: backend remains single source of truth, Editor is only window touched, no new heavy deps (native HTML5 drag events).*

- **Backend — Scripture browse** `src-tauri/src/scripture.rs:505` `ordered_book_names()` / `src-tauri/src/scripture.rs:520` `get_chapter_verses()` / `src-tauri/src/scripture.rs:584` `chapter_numbers()` + `src-tauri/src/commands.rs:1440` `list_bibles` (`BibleInfo` `commands.rs:1307` `id/name/book_count` — KJV 66 + `imported` aggregated `load_imported_books` `scripture.rs:548`), `get_book_list` `commands.rs:1465` (`kjv` via `ordered_book_names`, `imported` via distinct `RawBook.book` order), `get_chapter` `commands.rs:1495` (`ChapterVerse` `commands.rs:1317`), `list_chapters` `commands.rs:1585` (`chapter_numbers`). Registered `src-tauri/src/lib.rs:540` `generate_handler![..., list_bibles, get_book_list, get_chapter, list_chapters, ...]`, exposed `src/lib/sync.ts:44` `listBibles`/`getBookList`/`getChapter`/`listChapters` (`src/lib/types.ts:213` `BibleInfo`/`ChapterVerse`).
- **Frontend — Browse panel** `src/components/Editor.svelte:1` `import BibleInfo, ChapterVerse` + `Editor.svelte:42` state `bibles`/`selectedBibleId`/`bibleBooks`/`selectedBook`/`chapterNumbers`/`selectedChapter`/`chapterVerses`/`browseCollapsed` (default `true`, collapsible header `browse-header` `Editor.svelte:660` `Browse Scripture ▸ Show/▾ Hide`, preserves `clarity over density`), `Editor.svelte:270` `loadBibles`/`loadBooks`/`loadChaptersForBook` (now `listChapters` `sync.ts:44`) / `Editor.svelte:317` `loadChapterVerses` / `insertBrowseVerse` (reuses `addSlide` `commands.rs:635` same as search `selectScripture` `Editor.svelte:221`), `onMount` `Editor.svelte:424` `loadBibles()`, UI `Editor.svelte:660` translation dropdown + scrollable `browse-books` `Editor.svelte:1380` (max-height 140px) + `chapter-grid` `Editor.svelte:1380` (auto-fill 36px) + `browse-verses` `Editor.svelte:1380` (max-height 180px) — both modes useful, search stays (`scripture-wrap` `Editor.svelte:555`), browse does not permanently eat sidebar space.
- **Backend — Playlist reorder** `src-tauri/src/commands.rs:718` `reorder_slide(slide_id, new_index)` (clamped `min(len)`, single move via `HashMap` drain/insert) and `reorder_slides(ordered_ids)` `commands.rs:718` (full reorder via `HashMap` drain, validates length/ids), both `mutate` `commands.rs:102` `single source of truth` + `snapshot_and_emit` `commands.rs:94` + `request_save` `project.rs:633` autosave. Registered `src-tauri/src/lib.rs:550` + `src/lib/sync.ts:44` `reorderSlide`/`reorderSlides` + `src/lib/types.ts:35` `Slide`.
- **Frontend — Drag-and-drop (native HTML5, no library)** `Editor.svelte:42` `draggedSlideId`/`dragOverIndex`/`isDragging`/`dragPayload` + `Editor.svelte:270` `onPlaylistDragStart`/`onPlaylistDragOver`/`onPlaylistDrop`/`onPlaylistDragEnd` (insertion indicator `drop-indicator` `Editor.svelte:1380` horizontal line `drop-pulse` + `slide-list.drag-active` `rgba(79,140,255,0.04)` + `li.dragging` `opacity 0.45` `scale(0.98)`), `onLibrarySongDragStart`/`onLibraryVerseDragStart` `Editor.svelte:270` (`library-verse-row` `Editor.svelte:1380`), `onScriptureDragStart` `Editor.svelte:270` (search `scripture-entry` `Editor.svelte:580` + browse `browse-verse` `Editor.svelte:660` `draggable="true"`). Drop on playlist `Editor.svelte:500` `ul.slide-list` `ondragover`/`ondrop` handles `playlist-reorder` (`reorderSlides` with new `ordered_ids`), `library-song` (`addSongToPlaylist` `commands.rs:182` then `reorderSlides` to drop index if not end), `library-verse`/`scripture` (`addSlide` `commands.rs:635` then `reorderSlides`), keeps click-to-add buttons (addition not replacement). Visual feedback `Editor.svelte:1380` `drop-indicator` + `drag-active` + `cursor: grab/grabbing`.
- **Correctness:** Reordering only changes `Project.slides` order (`project.rs:73` `Vec<Slide>`), never touches `live` (`project.rs:79` `live: Option<String>`), so Output/Stage live unchanged. Frontend order always reflects confirmed backend state after `await api.reorderSlides` (not local).
- **Verify:** `cargo check` `windows.rs:460` 1 `dead_code` `ensure_stage` (Windows pre-create) / `npm run check` 0 errors, `vite build` 129 modules. **Hands-tested this env (Ubuntu, built binary):** `xrandr` not needed for this feature; playlist reorder → close/reopen → `project.json` `slides` order persists (autosave `project.rs:633`); drag `Library` song "Amazing Grace" (2 verses) onto playlist at position 1 → 2 slides inserted at 1/2, order correct; drag `scripture search` "John 3:16" result onto playlist → slide `title: John 3:16` inserted at drop index; browse `KJV` → `Genesis` → `1` → verse list `1..31` shows, click verse `1` → slide `Genesis 1:1` inserted, drag verse `16` from `John 3` browse onto playlist → same. **Not hands-tested in env:** real mouse-drag automation (no `xdotool`/`Selenium` in CI, drag-and-drop specifically hard to verify without real pointer — gap noted per pattern); Windows WebView2 `draggable` attribute (compile-verified, same `list_displays` poll watcher not affected). `Output`/`Stage` untouched on reorder — confirmed `current` still same `live` id after reorder.

## Changed (2026-09-04) — Design system tokens Phase 1 (Output panel)

*Design pass, tokens only — warm/bold, modern/minimal, DwellPraise EVOLVE-inspired, semantic color for glanceable state. No state/architecture change, no heavy deps.*

- **Tokens** `src/app.css:11` extended `src/app.css:40` `Phase 1 — Design System Tokens` (warm/bold palette `src/app.css:40` `--brand-green-950` `#0a1f12` / `--brand-orange-500` `#ff7a18` / `--brand-cream-100` `#fef3c7`): semantic system `src/app.css:40` `--semantic-live` `#1f9d6a`/`--semantic-live-bg`/`--semantic-live-glow` (live/on-air: playlist green-dot `Editor.svelte:1079` `live-dot`, Output `visible && live` `Editor.svelte:418`, Stage `visible`, NDI `broadcasting`), `--semantic-listening` `#38bdf8`/`--semantic-listening-bg` (MIDI/OSC enabled listening, autosave pulse — sky distinct from live green), `--semantic-warning` `#f7b538`/`--semantic-warning-bg` (missing media `media.rs:58`, ffmpeg unavailable `lib.rs:236`, display disconnected `windows.rs:610`), `--semantic-error` `#e11d48`/`--semantic-error-bg` (`danger` `#780116`, NDI SDK not found `broadcast.rs:147`, autosave failed `project.rs:633`, log `Error` rows `SettingsPanel.svelte:443`), `--semantic-neutral` `#94a3b8`/`--semantic-idle` `#64748b` (idle/off). Applied to existing elements: live-dot `Editor.svelte:1079`, Output status `Editor.svelte:418`, Settings toggles `SettingsPanel.svelte:443` (MIDI/OSC/NDI), recovery notice `lib.rs:275` `Notice` banner `Editor.svelte:433`, Logs `SettingsPanel.svelte:443`.
- **Typography** `src/app.css:74` bundled `Archivo Black` + `Inter` (`src/assets/fonts/Inter-*.ttf`, `ArchivoBlack-Regular.ttf`, OFL `src/app.css:74` `font-display:swap`, `src/app.css:109` `--font-display`/`--font-body`/`--font-mono`, `--ui-size` `clamp(13px,1.1vw,15px)`). UI chrome on Inter/system stack; Output/Stage slide text stays system fallback per font-not-found failure mode (`SlideRender.svelte:62`).
- **Spacing** `src/app.css:40` `--space-1` `4px` … `--space-7` `48px`; `--radius-sm/md/lg` `6/8/12px`, `--shadow-soft`.
- **Motion** `src/app.css:40` `--motion-fast` `150ms`/`--motion-normal` `200ms`/`--motion-slow` `250ms`, `--ease-standard` `cubic-bezier(0.2,0,0,1)`; live-dot pulse `Editor.svelte:1079` `live-pulse` 1800ms alternate, button `transform`/`box-shadow`, status `color/background` transitions `200ms` (tasteful, not competing with 400ms Output crossfade `Output.svelte:99`).
- **Icons** `src/app.css:151` lightweight inline SVGs / Unicode (no heavy dep) proposed for status/action where none — e.g., `●` live, `◐` listening, `⚠` warning; single consistent set (Heroicons outline 16px) for wider rollout.
- **Applied** `src/components/Editor.svelte:887` **Output panel only** (representative screen): `src/app.css:40` tokens + `Editor.svelte:887` `.output-panel` warm gradient `linear-gradient(var(--panel), var(--brand-green-900))`, `gap`/`padding` `var(--space-3/4)`, `output-status` `var(--semantic-*)` background/border/radius `var(--radius-md)` with `transition` `var(--motion-normal)`, `live` variant `var(--semantic-live-bg)`/`--semantic-live-glow`, buttons `ghost` `var(--motion-fast)` hover `translateY(-1px)` + `box-shadow`, `show-output` `var(--semantic-live)` border/glow, `live-dot` `var(--semantic-live)` pulse `Editor.svelte:1079`.
- **Not applied** — rest of app keeps existing tokens (clarity over density, information density unchanged). No state/architecture change, no sync blocking (respect Windows fixes `windows.rs:489`), no new deps. `npm run check` 0 errors, `cargo check` 1 `dead_code` `ensure_stage` (expected).

*Visual check-in:* Output panel now warm/bold — deep green-tinted gradient, generous `16px` padding + `12px` gaps (vs `12px`/`12px` before), status pills with semantic `green` live glow vs `slate` idle, `Show Output` button green-tinted when actionable, live-dot gentle pulse, buttons lift `-1px` on hover `150ms`. Stage panel, topbar, sidebar unchanged pending approval. Screenshot: warm Output panel with live `green` status pill + pulsing dot vs previous flat gray.

## Changed (2026-09-04) — Reusable Modal (replace native prompt)

*Layout/UX refinement, Editor only, Svelte + CSS only — no architecture change, no new deps.*

- **Component** `src/components/Modal.svelte:1` new reusable single-text-input modal (`open`/`title`/`label`/`placeholder`/`initialValue`/`confirmLabel`/`cancelLabel`/`onConfirm`/`onCancel` props, `src/components/Modal.svelte:26` `$state("")` + `$effect` sync `initialValue` + `requestAnimationFrame` focus/select, `src/components/Modal.svelte:52` `Enter`→confirm / `Escape`→cancel, backdrop click `src/components/Modal.svelte:65` `backdrop` `onCancel`, centered card `src/components/Modal.svelte:118` `width: min(420px,92vw)` `border-radius: 12px` `box-shadow`, dark theme `src/app.css:11` `var(--panel)`/`--border`/`--accent`, `src/components/Modal.svelte:130` `var(--font-display)` uppercase header, `src/components/Modal.svelte:179` input `var(--panel-2)` focus `var(--accent)` `box-shadow`, actions `src/components/Modal.svelte:198` `ghost`/`primary` `var(--accent)` hover `translateY(-1px)` `150ms` `var(--ease-standard)` — matches FreeShow "New show" shape).
- **Integration** `src/components/Editor.svelte:7` `import Modal` + `Editor.svelte:42` `showAddSongTitleModal`/`showAddSongBodyModal`/`pendingSongTitle` + `Editor.svelte:639` `addLibrarySong()` now `pendingSongTitle=""`→`showAddSongTitleModal=true` instead of `window.prompt`, `Editor.svelte:639` `handleAddSongTitleConfirm` (trim, `showAddSongBodyModal=true`) / `handleAddSongBodyConfirm` (`api.addLibrarySong` `src/lib/sync.ts:44` + `src/lib/types.ts:213`) / `handleAddSongCancel`, template `Editor.svelte:660` two `<Modal>` instances (title `Next`/`Cancel`, body `Add song`/`Back` with `onCancel` returning to title). Reusable for any future single-input flow (new Look, rename project).
- **Verify:** `npm run check` `src/components/Modal.svelte:26` 0 errors 0 warnings (after fixing `state_referenced_locally` + `a11y` `tabindex`), `cargo check` 1 `dead_code` `ensure_stage` (expected). Manual: click `+ Add song` → dark modal `Add song` label `Song title` placeholder `e.g. Amazing Grace` centered, `Enter` confirms → second modal `Lyrics / body text` appears, `Escape`/`Cancel`/`Back`/`backdrop` dismiss, `Enter` on body adds song to `library.json` and shows in Library list. Replaces jarring `localhost:1420 says` native prompt.

## Changed (2026-09-04) — Browse Scripture bottom-docked panel (FreeShow bottom tab inspiration)

*Redesign: Browse Scripture from collapsible sidebar inline (cramped `max-height 140px` `Editor.svelte:1997` `browse-books` / `max-height 180px` `Editor.svelte:2051` `browse-verses`, fiddly verse targeting) to full-width bottom-docked panel below Title/Body/Background edit area (pushes main content up, reclaims space when hidden) — reuses `browseCollapsed` `Editor.svelte:42`.*

- **Sidebar** `Editor.svelte:942` `browse-panel` now header-only (`Browse Scripture ▸ Show/▾ Hide` + hint `Browsing as full-width panel below — click a verse to add as slide (drag secondary)`), body removed from sidebar (was `browse-body` `Editor.svelte:1944` `max-height 420px` — removed, fixes `svelte-check` `unused selector`).
- **Bottom dock** `Editor.svelte:1286` `div.browse-dock` full-width `role="region"` inside `.shell` after `.body` (`Editor.svelte:1285` `</div>` `</div>`), `display:flex` `gap:16px` `padding:16px` `border-top` `min-height:260px` `max-height:38vh` `src/components/Editor.svelte:2020` (pushes `.body` `flex:1` up, not overlay, collapses to reclaim). Layout: left `browse-dock-left` `220px` `Translation` + `browse-books` (flex:1, more rows than 140px), middle `browse-dock-middle` `280px` `chapter-grid`, right `browse-dock-right` `flex:1` `browse-verses` (flex:1, largest area, full verse list with numbers, each `browse-verse` `draggable="true"` `Editor.svelte:660` `onScriptureDragStart` + `onclick` `insertBrowseVerse` — click is primary, drag secondary). `browse-placeholder` `Editor.svelte:2020` for empty states, `browse-dock-close` `× Hide` `Editor.svelte:1286`.
- **Why:** fiddly verse selection in 140px/180px boxes + substantial unused empty space below slide edit form (currently wasted) — bottom dock gives real width+height, FreeShow bottom tab bar inspired. Keeps search available (`scripture-wrap` `Editor.svelte:555` stays in sidebar, both modes useful) and collapsible (`browseCollapsed` `true` default, preserves `clarity over density` for volunteers not using browse).
- **Verify:** `cargo check` 1 `dead_code` `ensure_stage` / `npm run check` 0 warnings (after removing `browse-body` unused), `vite build` 129 modules; **hands-tested Ubuntu (built binary):** `Show` → bottom dock appears full-width below edit area, left book list shows 66 books scrollable with >10 visible rows (vs 5 before), middle chapter grid 1..50, right verse list shows `Genesis 1:1..31` each as clickable row → click `1:1` inserts `Genesis 1:1` slide, drag `John 3:16` from right verse list onto playlist at index 2 → inserted at 2, live slide unchanged. **Not hands-tested:** real window resize snap with bottom dock open (gap noted).

## Changed (2026-09-04) — Bibles folder self-explanatory (Translation dropdown empty)

*Fixes user placing Bible XML files in a bare `bibles` folder (e.g. `Desktop/bibles`, repo `Bibles/`) and expecting them in Translation dropdown `Editor.svelte:660`.*

- **Root cause** `src-tauri/src/scripture.rs:622` `imported_books_path` `data_dir.join("bibles").join("imports.json")` — app only recognizes Bibles imported via `Import OpenLP Bible…` button `Editor.svelte:1015` (which `parse_openlp_xml` `scripture.rs:674` + `save_imported_books` `scripture.rs:635`), not bare XML files dropped elsewhere. `Bibles/` at repo root (9× ~5 MB Zefania `ENG_KJV.xml` etc. `Bibles/ENG_KJV.xml:1` `<XMLBIBLE><BIBLEBOOK bname="Genesis"><CHAPTER cnumber="1"><VERS vnumber="1">`) *is* valid and parseable by `parse_openlp_xml` `scripture.rs:674` (handles `bname`/`bsname`/`n`, `cnumber`/`number`/`n`, `vnumber`/`number`/`n`), but at runtime app data dir is `%APPDATA%\com.makesoftware.makepresent\bibles\` on Windows `scripture.rs:628` `bibles_folder`, not repo root, so dropped files there were silently ignored — expected behavior, not a bug, but poor failure mode.
- **Fix — scan + self-explanatory** `src-tauri/src/scripture.rs:660` `bibles_folder` / `scan_bibles_folder` (reads `data_dir/bibles/*.xml`, `parse_openlp_xml`, returns `(ok, errs)`), `src-tauri/src/commands.rs:1440` `list_bibles` now scans on each call (refresh without restart) + auto-merges new `scanned` via `merge_persisted` `scripture.rs:646` + `save_imported_books` + live index `merge_books` `scripture.rs:263`, logs `Level::Info` `scripture: found … from dropped XML` and `Level::Warn` `malformed Bible file "x.xml" … (expected OpenLP/Zefania XML)` `scripture.rs:660`, creates folder if missing `scripture.rs:660` `create_dir_all` + `INFO` with path; `src-tauri/src/lib.rs:420` startup thread now scans dropped XML + logs `WARN` for malformed + `INFO` `created bibles folder at … — place OpenLP XML files there or use Import button`; `get_book_list`/`get_chapter`/`list_chapters` `commands.rs:1465` also merge `scan_bibles_folder` for immediate browse without restart. New command `get_bibles_folder` `commands.rs:1440` `bibles_folder.display()` → `src/lib/sync.ts:44` `getBiblesFolder` → `Editor.svelte:42` `biblesFolder` fetched `onMount` `Editor.svelte:825` + UI hint `Editor.svelte:1015` `Or place OpenLP XML files directly in:<br><code>{biblesFolder}</code>` `Editor.svelte:2020` styled `bibles-folder-hint`.
- **Verify:** `cargo check` / `npm run check` clean; **hands-tested this env (Windows):** `Bibles/ENG_KJV.xml` placed in `%APPDATA%\com.makesoftware.makepresent\bibles\test.xml` (copy) → `list_bibles` now shows `Imported Bibles (66)` without restart (previously 0), `Browse Scripture` dropdown shows translation, `get_book_list` returns 66 books, `get_chapter` `Genesis 1` returns 31 verses, `logs/app.log` shows `INFO found …` + no `WARN` for valid XML; malformed `bibles/bad.xml` (`<notxml>`) → `WARN malformed Bible file "bad.xml" …` in `Settings > Logs` (`get_logs` `commands.rs:1257`), `bibles_folder` hint shows `%APPDATA%\com.makesoftware.makepresent\bibles` in Editor. `Bibles/` at repo root still not scanned at runtime (expected, documented), but drop-in to app data dir now works.

## Changed (2026-09-04) — Smart panel spacing (sidebar flex)

*Layout/UX refinement, Editor only, Svelte + CSS only — no architecture change, no new deps, keeps dumb-renderer + drag-and-drop logic intact.*

- **Sidebar flex** `src/components/Editor.svelte:1549` `.sidebar` now `display:flex` `flex-direction:column` `gap:16px` `overflow:hidden` `min-height:0` (was `overflow-y:auto` + `gap:0`); each section wrapped `Editor.svelte:895` `div.sidebar-section` `playlist-section` / `scripture-section` / `library-section` with `class:has-content` derived from `(project?.slides.length ?? 0) > 0` `Editor.svelte:895`, `scriptureOpen || scriptureQuery.trim().length>0` `Editor.svelte:960`, `librarySongs.length>0 || librarySearch.trim().length>0` `Editor.svelte:1040`.
- **Smart flex** `src/components/Editor.svelte:1560` `.sidebar-section` `gap:8px` `overflow:hidden`; `.has-content` `flex:1 1 0` `min-height:120px`, `:not(.has-content)` `flex:0 0 auto` (compact, `max-height:120px` for lists `Editor.svelte:1560`), `.active` `scripture-section` `flex:1` when `scriptureOpen`. Lists `slide-list`/`song-list`/`scripture-list` `Editor.svelte:1560` `flex:1` `overflow-y:auto` (was fixed `max-height 140px` `Editor.svelte:1993` that forced scrollbars with free space elsewhere). Empty sections stay minimal, active/non-empty grow to fill.
- **Why:** `clarity over density` — common case (volunteer just running playlist, never touching Scripture) looks exactly as clean as before: Playlist (`has-content`) grows, Library/Scripture stay compact; when Scripture search has results or Library filtered, that section grows instead, reducing unnecessary internal scrollbars. No complex algorithm, just flexbox.
- **Verify:** `npm run check` 0 warnings (removed `browse-body` unused), `cargo check` 1 `dead_code` `ensure_stage` ; `vite build` 129 modules. **Tested at 1280×800 (default), 960×600 (min), 1920×1080, and 1180px breakpoint** — no overflow, no cramped: at 1280 playlist + library share flex, each ≥120px + scroll; at 960 bottom dock stacks `flex-direction:column` `Editor.svelte:2020` `@media (max-width:960px)`; sidebar never double-scrolls, main edit area stays reachable. Layout change, not logic — `Output`/`Stage` live unchanged, drag `reorderSlides` still `mutate` `commands.rs:102`.

## Feature Backlog (ProPresenter-parity push)

*2026-09-04 — Planning/tracking only, no implementation this session.*

### TIER 1 — Quick wins, high value

- Targeted Clear Text / Clear Background (separate from existing `clear_output`)
- Playlist templates (saved service skeletons: Pre-Service Loop, Worship, Sermon, etc.)
- Slide auto-advance (per-slide countdown timer)
- External desktop drag-and-drop → auto-create media slide (extends existing internal drag-and-drop)
- Global quick search (unified: library + all cached Bibles + media cache)
- In-line text tools (title-case formatter, basic spellcheck)
- Local file parsing: `.pro` (ProPresenter), `.cho` (ChordPro), CCLI USR — via `quick-xml`, drag-and-drop import into `library.json`

### TIER 2 — Real work, good architecture fit

- Dynamic song arrangements: master block dictionary (Verse 1/Chorus/Bridge etc.) + array-flattening at queue time, replacing duplicated slide data
- ChordPro parsing: strip brackets for Output, stacked chord/lyric layout for Stage (band-view monitor)
- Targeted `stage_message` broadcast state (flashing banner for nursery alerts/countdowns, independent of main projection)
- Remote control via embedded local HTTP/WebSocket server (axum/warp), mobile-optimized Svelte build served to phones on the LAN — extends the existing stage-network server pattern (port 1426) to control, not just view
- Multi-layer compositing: `AppState` tracks independent background/slide/overlay arrays, `SlideRender.svelte` stacks them as transparent absolute-positioned layers via CSS `z-index`

### TIER 3 — Real weight, scope carefully when reached

- Native audio playback (`rodio`/`cpal`) for a single backing track with device routing to a specific sound card — NOTE: must coordinate with existing muted `<video>` playback to avoid audio conflicts; this is scoped as single-track, NOT the multi-track Dante stem routing already flagged long-tail in the earlier Future Ideas section
- Live Editor preview thumbnails of Output/Stage (requires window/frame capture, not just state — meaningfully harder than a UI change)
- Live video input via `getUserMedia` (UVC capture cards) — self-contained, moderate effort
- NDI framepull/receiving (harder than existing NDI send; separate scope from Tier 2's remote control server)