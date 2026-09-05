# MakrStudio (formerly MakePresent) — Living Project Doc

This document is the source of truth for **what MakrStudio is, how we build
it, and where we are**. Update it as the project evolves.

## What MakrStudio Is

MakrStudio is **DwellPraise Ministries' own church presentation software**.
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
  a sender (`"MakrStudio - Sunday Output"`), runs on its **own dedicated
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
   "Welcome to MakrStudio — add your first slide to get started". It
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
  project.rs    Domain model (Project/Slide/Settings/Library/Template/Look+geometry), persistence, autosave worker (templates.json atomic)
  windows.rs    Window lifecycle: Output + Stage + Editor respawn, display picking
  media.rs      Media import/cache: copy+hash, ffmpeg thumbnails, startup verification
  broadcast.rs  NDI sender: runtime-loaded SDK (libloading), dedicated send thread
  midi.rs       MIDI input: midir listener, device enumeration, message parsing
  osc.rs        OSC listener: dedicated UDP thread, rosc decode, bundle flattening
  triggers.rs   Trigger/action model, routing, action→command dispatch
  commands.rs   Tauri IPC: mutations + broadcast, settings import/export, logs, media import/search, NDI, MIDI/OSC/triggers, scripture import/search, templates (save/load), song import (.pro/.cho/.usr via quick-xml, no cloud), song arrangement (set_song_arrangement, flatten at queue-time), stage message (set/clear, auto-expire, stage-only), overlay (set/visible/clear, z0/z1/z2 independent layers for Output), audio (list/load/play/pause/stop/volume/seek/device, rodio/cpal single track, not tied to slides)
  logging.rs    Rolling, immediately-flushed event log (logs/app.log)
  scripture.rs  KJV search index + OpenLP/Zefania XML import + bible-api.com REST fallback
  song_import.rs Local parsers for .pro (ProPresenter via quick-xml), .cho (ChordPro), .usr (CCLI USR) — title+verses into library.json, no cloud
  audio.rs      Single backing track (rodio/cpal, dedicated thread, device routing, not tied to slides)
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

NDI® is a registered trademark of Vizrt. MakrStudio's broadcast build loads
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
(songs), `templates.json` (reusable playlist templates — slide refs, atomic writes),
`logs/app.log` (event log), `media/` (managed content-hashed media
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

## Changed (2026-09-04) — Live Output preview + ON AIR badge (Editor)

*Small live-output preview in Editor's Output panel (near “Live on display…” status) + clear ON AIR / OFF toggle, inspired by FreeShow's Output panel thumbnail. Preview of currently selected/live slide's content via `SlideRender` at small size, not window/frame capture Tier 3.*

- **Preview thumbnail** `src/components/Editor.svelte:7` `import SlideRender` + `Editor.svelte:42` derived `outputPreviewSlide` (`project.live` → `find` else `selected`) / `outputPreviewLook` (`looks` `outputLookId` → `Main` fallback) / `isOnAir` `output.visible && project.live` (`Editor.svelte:42`), template `Editor.svelte:1189` `div.preview-row` `div.preview-box` `aspect-ratio:16/9` `max-width:280px` `border` `overflow:hidden` `src/components/Editor.svelte:2020`, inside `SlideRender` `slide={outputPreviewSlide}` `look={outputPreviewLook}` scaled `src/components/Editor.svelte:2020` `transform:scale(0.42)` `width:238%` `height:238%` (reuses `SlideRender.svelte:62` absolute `inset:0` at small size), `preview-empty` fallback. Updates via existing `subscribeState` `Editor.svelte:787` — no new backend, frontend reuse (not `broadcast.rs:147` window capture Tier 3).
- **ON AIR / OFF badge** `Editor.svelte:1189` `span.on-air-badge` `class:on={isOnAir}` `class:off={!isOnAir}` `{isOnAir ? "ON AIR" : "OFF"}` `src/components/Editor.svelte:2020` pill `11px` `800` `letter-spacing:0.08em` `border-radius:999px` `padding:6px 10px` `transition` `var(--motion-normal)`, `.on` `var(--semantic-live-bg)` `var(--semantic-live)` `var(--semantic-live-border)` `var(--semantic-live-glow)` (strong green), `.off` `var(--semantic-error-bg)` `var(--semantic-error)` `var(--semantic-error-border)` (red/muted) — purely visual status reflecting `output_visible` + `live`, not a new control, keeps `output-status` `Editor.svelte:1242` text as supplement. Uses `src/app.css:40` semantic tokens (warm/bold `EVOLVE`), no new colors.
- **Stage too** `Editor.svelte:42` `stagePreviewSlide` (`appState.current ?? selected`) / `stagePreviewLook` (`stageLookId` → `Stage`) / `isStageOnAir` (`stage.visible`) + template `Editor.svelte:1280` same `preview-row`/`preview-box`/`on-air-badge` for Stage Display panel (consistent, straightforward).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 1 `dead_code` `ensure_stage` (expected). Visual check: Output `visible && live` → `green` `ON AIR` + `SlideRender` thumbnail 280px 16:9 scaled 0.42 shows title/body with correct Look; `visible false` or no `live` → `red` `OFF` + `preview-empty` or `selected` preview. Stage `visible` → `ON AIR`, hidden → `OFF`. Dark theme consistent, `150-250ms` transitions tasteful, not competing with 400ms Output crossfade `Output.svelte:99`.

## Changed (2026-09-04) — Targeted clear: clear_text / clear_background

*Adds two new targeted clear commands alongside existing `clear_output` `src-tauri/src/commands.rs:285` (clears both, unchanged). Extends `Project` state minimally to track independent visibility flags rather than single `live/not-live` boolean, updates `SlideRender`/`Output`/`Stage` to respect them, adds two buttons in Editor's Output panel alongside existing Clear output.*

- **Project state** `src-tauri/src/project.rs:230` `Project { show_text: bool #[serde(default="default_true")] , show_background: bool #[serde(default="default_true")] }` + `src-tauri/src/project.rs:254` `default_true() -> true`, `Project::new` `src-tauri/src/project.rs:256` `show_text: true, show_background: true` (legacy `serde` defaults to `true` via `default_true`, no migration needed). `ClientState` `project.rs:393` exposes via `project` clone.
- **Commands** `src-tauri/src/commands.rs:285` `clear_text` (`show_text=false`, keep background, `log "text cleared"`) + `clear_background` (`show_background=false`, keep text on black `log "background cleared"`), `src-tauri/src/commands.rs:255` `make_live` now resets `show_text=true`/`show_background=true` on every new live slide, `src-tauri/src/commands.rs:290` `do_clear_output` now also resets both to `true` + `live=None` (keeps `clear_output` clears-both behavior unchanged). Registered `src-tauri/src/lib.rs:535` `generate_handler![..., clear_text, clear_background, ...]`.
- **Rendering** `src/components/SlideRender.svelte:1` `Props { showText?: boolean, showBackground?: boolean, effectiveShowText/effectiveShowBackground }` `src/components/SlideRender.svelte:6` + template `src/components/SlideRender.svelte:18` `class:no-bg={!effectiveShowBackground}` `style:background-color={effectiveShowBackground ? solidColor : "transparent"}` `{#if effectiveShowBackground}` media + `{#if effectiveShowText && slide.title/body}` text (clear_text hides text overlay leaving background media/color running; clear_background hides background leaving text on neutral/black `Output.svelte:182` `background:#000` / `Stage.svelte:90` `#0b0b0e`).
- **Output/Stage** `src/components/Output.svelte:34` `showText`/`showBackground` derived `project?.showText ?? true` + `SlideRender` `src/components/Output.svelte:115` `slide={shown} {look} {showText} {showBackground}` (both `shown` + `leaving` frames), `src/components/Stage.svelte:16` same for Stage (`appState.project.showText`); preview in Editor `src/components/Editor.svelte:42` `outputPreviewSlide`/`stagePreviewSlide` also pass `showText`/`showBackground` `Editor.svelte:1189` (preview reflects cleared state).
- **Editor UI** `src/components/Editor.svelte:639` `clearText()`/`clearBackground()` (`api.clearText`/`api.clearBackground` `src/lib/sync.ts:44` `src/lib/types.ts:73` `Project {showText, showBackground}`) + template `Editor.svelte:1242` `div.clear-row` `src/components/Editor.svelte:2020` three buttons `Clear output` (existing, keeps `clear_output` clears-both) + `Clear text` (`title="Hide text, keep background"`) + `Clear background` (`title="Hide background, keep text on black"`) `flex:1` `gap:8px` `src/components/Editor.svelte:2020`, alongside existing topbar `Clear output` (backward compat).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`). Manual: set slide live → `Clear text` → Output shows background media/color without text, Stage same; `Clear background` → Output shows text on black, Stage text on `#0b0b0e`; `Clear output` → black (both); next `set_live_slide` resets both to visible. Existing `clear_output` still `live=None` black unchanged.

## Changed (2026-09-05) — Merge Project Hub ↔ Playlist Templates into one unified View/Playlist flow

*Implements the TIER 2 backlog item formerly at `docs/PROJECT.md:467` — collapses the two separate systems (Project Hub's hardcoded preset cards vs. the separate save/load-template flow) into one unified entry point. Display terminology renamed only: **"Project" → "View"** (the working document an operator runs a service from — title/date, live playlist, output settings) and **"Template" → "Playlist"** (a saved, reusable slide sequence, not tied to a date). Backend and data model untouched — fully backward compatible with existing `project.json` and `templates.json` on disk.*

- **Unified entry point (View Hub)** `src/lib/components/ProjectHub.svelte:106-209` — dialog `aria-label="View Hub"` + `<h1>View Hub</h1>` (was "Project Hub", `ProjectHub.svelte:64-71`), gallery heading **"Starting Playlist"** (was "New from Template", `ProjectHub.svelte:122`), inspector `aria-label="View Configuration"` (was "Preset Configuration"), field **"View Title / Date"** (was "Service Title / Date"), button **"Create View — N slides"** (was "Create Service …"), **"Recent View"** (was "Recent"). The gallery is now a single `$derived` list `ProjectHub.svelte:46` that merges the 4 hardcoded presets (`src/lib/presets.ts:3-56`) **plus** user-saved playlists from `list_templates` (prop `playlists` `ProjectHub.svelte:10`), each saved playlist badged `♻ Saved` (`ProjectHub.svelte:139-140`) with accent chip `card-badge[data-kind="playlist"]` `ProjectHub.svelte:244`. No separate load-template flow remains anywhere else in the app.
- **Create from a saved Playlist** `src/components/Editor.svelte:1298-1314` `handleHubCreateFromPlaylist` — reuses the existing backend unchanged: `api.newProjectFromPreset("blank", …)` builds the View shell with title/aspect/theme/transition (`commands.rs:934-976`, theme→look tweaks at `commands.rs:959-970`), then `api.loadTemplate(playlistId)` (`commands.rs:1055-1096`) populates the playlist with fresh ids (clears `live`, selects first slide, preserves library/background refs). New `refreshPlaylists()` `Editor.svelte:1316-1323` (`api.listTemplates`) feeds the hub at boot `Editor.svelte:1562-1563` and on `openHub` `Editor.svelte:1273-1274`.
- **Save as Playlist (keep-for-next-service loop)** `Editor.svelte:1326-1338` `openSavePlaylist`/`handleSavePlaylistConfirm` — renamed UI from "Save as template"; reuses `api.saveTemplate` (`commands.rs:992-1053`, upsert by case-insensitive name into `templates.json`). Playlist panel button `Editor.svelte:1737` title "Save this View's playlist as a reusable Playlist"; modal `Editor.svelte:2562-2571` title "Save as Playlist". The separate **Load template** picker modal + button were removed (redundant with the View Hub); `delete_template` stays in the backend/sync layer but is no longer surfaced in the UI.
- **"New project" → "New View"** topbar button `Editor.svelte:1620`; hub still opens at boot `Editor.svelte:1562`. Stale duplicate **`src/components/ProjectHub.svelte` deleted** (was unimported; only `src/lib/components/ProjectHub.svelte` is used, imported at `Editor.svelte:13`). Dead CSS removed: `template-picker`/`template-list`/`template-row`/`template-meta`/`template-name`/`template-count`/`template-at`/`template-actions-row`/`modal-backdrop`/`modal-card`/`modal-header`/`modal-close`/`modal-actions` from `Editor.svelte` (kept `.template-actions`/`.template-btn`).
- **Naming-mismatch flags (intentional — display-only rename, same caution as the MakrStudio rename `PROJECT.md:611`)** — internal names deliberately *not* renamed: Rust structs `Project`/`ServicePreset`/`TemplateItem`/`PlaylistTemplate`/`TemplateStore` (`src-tauri/src/project.rs:262/674/765/781/790`), commands `new_project`/`new_project_from_preset`/`list_templates`/`save_template`/`load_template`/`delete_template` (`commands.rs:916/934/987/993/1056/1099`, registered `lib.rs:621-685`) and their `src/lib/sync.ts:52-54/216-225` + `src/lib/types.ts:225-266` wrappers; TS `ProjectHub.svelte` component filename; `Editor.svelte` `templates` state/Vars and `.template-actions`/`.template-btn` CSS; `project.json`/`templates.json` filenames. Visible mismatches worth remembering: the "New view" button still calls internal fn `newProject()` `Editor.svelte:1293` → `openHub()`; "Save as Playlist" calls command `save_template`; a saved Playlist's picker card (`list_templates`) returns `PlaylistTemplate` items. Also note `OutputView`/`StageView` structs (`project.rs:407/417`) already use "View" in an unrelated sense (output/stage window render state) — distinct from the new View-as-working-document display term. Any future rename of the *data model* is a separate, riskier migration: `read_templates` `project.rs:808` returns an empty store on parse failure, so a breaking struct rename would silently drop a user's saved Playlists unless a read-migration is added.
- **Verify:** `npm run check` 0 errors 0 warnings; `cargo check` OK (3 pre-existing `dead_code`: `COPY_SUFFIX` `media.rs:16`, `AudioPlayer::is_active` `audio.rs:390`, `Slide::display_name` `project.rs:92`). Manual: boot → View Hub lists the 4 built-in starting Playlists plus any saved ones; create from a saved Playlist → View created with given title/theme and playlist populated (fresh ids, `live` cleared, first slide selected); Save as Playlist → upserts into `templates.json`; restart → saved Playlists persist and appear in the hub; an existing `project.json` and `templates.json` load unchanged.

## Feature Backlog (ProPresenter-parity push)

*2026-09-04 — Planning/tracking only, no implementation this session.*

### TIER 1 — Quick wins, high value

- Targeted Clear Text / Clear Background (separate from existing `clear_output`)
- ~~Playlist templates (saved service skeletons: Pre-Service Loop, Worship, Sermon, etc.)~~ — shipped 2026-09-02; merged into the View Hub as the unified **Playlist** flow 2026-09-05 (see the "Merge Project Hub ↔ Playlist Templates" changelog entry). Note: the TIER 2 cross-ref at line 467 called this "item 4" — it is position 2 in this list.
- Slide auto-advance (per-slide countdown timer)
- External desktop drag-and-drop → auto-create media slide (extends existing internal drag-and-drop)
- Global quick search (unified: library + all cached Bibles + media cache)
- In-line text tools (title-case formatter, basic spellcheck)
- Local file parsing: `.pro` (ProPresenter), `.cho` (ChordPro), CCLI USR — via `quick-xml`, drag-and-drop import into `library.json`
- Separate slide internal name/label from the on-screen Title text — currently the Title field doubles as both the playlist list-item label and the rendered on-screen text. Add a distinct "Slide name" field (shown in playlist/grid views) independent from the Title/Body content actually rendered on Output.
- Arrow-key slide navigation — Left/Right arrow keys (while Editor is focused) advance/reverse the live slide, reusing existing next/previous command logic. Scope as in-app-focused only for now (not a global system-wide shortcut — that's the separate, already-documented 'Global Keyboard Shortcuts' backlog item using Tauri's global shortcut plugin, which remains a distinct future item).
- Responsive/adaptive Editor layout audit — beyond the existing DPI fixes documented in WINDOWS.md (125%/150% scaling, viewport-relative grid), do a broader pass ensuring the Editor layout adapts cleanly across window sizes/zoom levels without hiding or overlapping components, not just the specific DPI cases already fixed.

### TIER 2 — Real work, good architecture fit

- Dynamic song arrangements: master block dictionary (Verse 1/Chorus/Bridge etc.) + array-flattening at queue time, replacing duplicated slide data
- ChordPro parsing: strip brackets for Output, stacked chord/lyric layout for Stage (band-view monitor)
- Targeted `stage_message` broadcast state (flashing banner for nursery alerts/countdowns, independent of main projection)
- **DEPRIORITIZED** — Remote control via embedded local HTTP/WebSocket server (axum/warp), mobile-optimized Svelte build served to phones on the LAN — extends the existing stage-network server pattern (port 1426) to control, not just view — *explicitly parked until the core presentation platform is further along. Not removed from backlog, just sequenced last.*
- Multi-layer compositing: `AppState` tracks independent background/slide/overlay arrays, `SlideRender.svelte` stacks them as transparent absolute-positioned layers via CSS `z-index`
- ~~Merge Project Hub and Playlist Templates into one concept~~ — **[DONE 2026-09-05]** (see the "Merge Project Hub ↔ Playlist Templates into one unified View/Playlist flow" changelog entry above). Note: the original text below called the Playlist-templates Tier 1 item "item 4"; it is position 2 in the Tier 1 list. — rename "Project" to "View" and "Template" to "Playlist" throughout the UI and data model terminology. The original intent: a View has a Playlist (either freshly configured or the last one used); after a service, the operator saves that Playlist for reuse next time — Project Hub and the existing Playlist Templates feature (Tier 1 item 4) should become one unified flow rather than two separate systems. This is a meaningful terminology + data-model change — **flag as its own dedicated prompt/session when reached, not a quick edit.**

### TIER 3 — Real weight, scope carefully when reached

- Native audio playback (`rodio`/`cpal`) for a single backing track with device routing to a specific sound card — NOTE: must coordinate with existing muted `<video>` playback to avoid audio conflicts; this is scoped as single-track, NOT the multi-track Dante stem routing already flagged long-tail in the earlier Future Ideas section
- Live Editor preview thumbnails of Output/Stage (requires window/frame capture, not just state — meaningfully harder than a UI change)
- Live video input via `getUserMedia` (UVC capture cards) — self-contained, moderate effort
- NDI framepull/receiving (harder than existing NDI send; separate scope from Tier 2's remote control server)
## Changed (2026-09-02) - Playlist templates (save/load reusable structures)

*Implements TIER 1 backlog item `Playlist templates`. Adds the ability to save the current playlist structure as a reusable template (e.g. Pre-Service Loop, Worship, Sermon) and load a template to quickly populate a new project's playlist. Templates store slide references (title/body/background/library refs) not full duplicated bytes, persisted in their own `templates.json` with atomic writes.*

- **Model + persistence** `src-tauri/src/project.rs:561` `TemplateItem { title, body, background, libraryId, librarySlideId }` + `PlaylistTemplate { id, name, createdAt, items }` + `TemplateStore { schemaVersion, templates }` (`SCHEMA_VERSION 1`, `serde renameAll camelCase`, `Default`). `read_templates`/`write_templates`/`templates_path` `project.rs:600` use `atomic_write_json` `project.rs:723` (tmp + sync_all + rename). Missing file -> empty store. Data lives `templates.json` alongside `project.json`/`library.json`. Same atomic-write pattern as project/library (prompt requirement).
- **Commands** `src-tauri/src/commands.rs:752` `list_templates` -> `Vec<PlaylistTemplate>`, `save_template(name)` (trim 1..80, upsert by case-insensitive name; items from `state.project.read().slides` mapping to `TemplateItem` preserving `Background` hash refs and `libraryId` links), `load_template(templateId)` (clone, map each `TemplateItem` to fresh `Slide { id: new Uuid, ... }`, `mutate` replaces `project.slides`, clears `live`, sets `selected` to first, resets `show_text/show_background`, `request_save` + `snapshot_and_emit`), `delete_template(templateId)`. All 4 registered `src-tauri/src/lib.rs:635` `generate_handler![..., list_templates, save_template, load_template, delete_template]`, exposed `src/lib/sync.ts:206` + `src/lib/types.ts:190` `TemplateItem/PlaylistTemplate/TemplateStore`. IPC count 46->50 (README).
- **Editor UI** `src/components/Editor.svelte:1` import + `Editor.svelte:87` state `templates`/`showSaveTemplateModal`/`showTemplatePicker`/`templatePickerLoading` + handlers `openSaveTemplate`/`handleSaveTemplateConfirm`/`openTemplatePicker` (`api.listTemplates`)/`handleLoadTemplate` (`api.loadTemplate` -> `appState`)/`handleDeleteTemplate`. Playlist panel `Editor.svelte:1006` `div.template-actions` two ghost buttons `Save as template` + `Load template` below Add slide. Save uses reusable `Modal.svelte`; picker is custom modal `Editor.svelte:1575` backdrop + card (560px, 80vh), hint, loading/empty/list rows with name/count/date + Load/Delete, Close. Styles `Editor.svelte:2680`.
- **Backlog:** Moves `Playlist templates` from TIER 1 Feature Backlog (was planned) to shipped; remaining TIER 1 items keep priority.
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX`, `ensure_stage`). Manual: 3-slide playlist -> Save as template Worship -> `templates.json` appears with 3 TemplateItems; New project blank -> Load template Worship -> 3 slides appear with fresh ids; Save again Worship overwrites; Delete -> removed; restart -> templates persist; media hash refs intact, library links preserved.

## Changed (2026-09-02) - Per-slide auto-advance timer (backend-driven)

*Implements TIER 1 backlog item `Slide auto-advance (per-slide countdown timer)`. Optional per-slide timer: when set (e.g. via a small duration field on a slide), the slide automatically advances to the next playlist item after N seconds while live, without requiring a manual click. Implemented in the Rust backend (single source of truth � not a frontend setTimeout), so Output/Stage remain dumb renderers; the backend drives the advance and broadcasts the resulting state change like any other slide change. Cancellable if the operator manually advances before the timer fires.*

- **Model** `src-tauri/src/project.rs:62` `Slide { auto_advance_secs: Option<u64> #[serde(default)] }` (`None` = manual). `Project::new` / `from_preset` seed `None`; legacy `project.json` loads via `default`. `TemplateItem` `project.rs:584` also stores `auto_advance_secs` so templates preserve timers. `src/lib/types.ts:35` `Slide { autoAdvanceSecs: number | null }` mirrored.
- **Backend timer � generation-cancellation, dumb-renderer** `src-tauri/src/state.rs:33` `AppState { auto_advance_gen: AtomicU64 }` (`bump_auto_advance()` / `current_auto_advance_gen()`). Helpers `src-tauri/src/commands.rs:105` `cancel_auto_advance` (bump gen) and `schedule_auto_advance(live_id, secs)` (bump gen, capture `gen`, `std::thread::spawn` `sleep(secs)` then check `gen` + `still live == live_id` + `current_secs == secs` before `make_live(next)`). Logging `auto-advance: scheduled ...` / `auto-advance: X -> Y after Ns` / `at end, staying`.
- **Wiring** `commands.rs:352` `make_live` after `snapshot_and_emit` reads `slide.auto_advance_secs` and schedules/cancels; `do_clear_output` `commands.rs:388` bumps gen; `replace_project` `commands.rs:119` bumps gen; `update_slide` `commands.rs:1026` extended `auto_advance_secs: Option<Option<u64>>` (`None` = not touching, `Some(None)` = clear, `Some(Some(n))` = set; validated `1..86400`) and when `was_live` reschedules; `delete_slide` `commands.rs:1092` cancels if `was_live`; `add_slide` / `add_song_to_playlist` set `None`; `load_template`/`save_template` preserve `auto_advance_secs`.
- **Editor UI** `src/components/Editor.svelte:94` `draftAutoAdvance` / `autoAdvanceTimer` / `\` sync `selected.autoAdvanceSecs`; `commitAutoAdvance` (`\"\" -> null` else validate `1..86400` -> `api.updateSlide({ autoAdvanceSecs })`), `onAutoAdvanceInput` (350ms debounce), `flushAutoAdvance`. Edit form `Editor.svelte:1303` field `Auto-advance` `<input type=\"number\" min=1 max=86400>` + hint. Playlist `Editor.svelte:1083` `auto-badge` `? Ns`. `src/lib/sync.ts:68` `updateSlide` patch extended.
- **Backlog:** Moves `Slide auto-advance` from TIER 1 Feature Backlog (was planned) to shipped; remaining TIER 1 items (external drag-drop -> media, global quick search, text tools, local file parsing) keep priority.
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 3 warnings (`COPY_SUFFIX`, `ensure_stage`, `reschedule_auto_advance` allow). Manual: slide 1 `auto-advance 3` live -> after 3s auto advances to slide 2 (same as click); manual click before 3s cancels; editing live slide blank cancels; last slide with timer stays at end; template save/load preserves value.

## Changed (2026-09-02) - External OS file drag-and-drop onto playlist (media import)

*Extends the existing internal HTML5 drag-and-drop (already built for library/scripture-to-playlist) to also accept files dragged from the OS desktop directly onto the playlist or a `drop zone'' in the Editor. Dropped image/video files go through the existing media import pipeline (hash, copy to managed media folder, generate thumbnail) and create a new slide with that file as the background, same result as using the existing `Add media'' button but via drag-and-drop. Rejects unsupported file types with a clear inline message, not a silent failure.*

- **Frontend � reuse existing pipeline** `src/components/Editor.svelte:40` `ALLOWED_EXTS` from `MEDIA_FILTERS` ? `src-tauri/src/media.rs:46` `MediaKind::from_extension` (`png jpg ... avif` / `mp4 m4v ... ogv`). State `externalDragActive`/`externalDragError` `Editor.svelte:95`. Helpers `isExternalFileDrag`/`getFileExt`/`handleExternalFiles` (validates via `ALLOWED_EXTS`, unsupported ? `errorMsg`+`externalDragError` (6s), supported extracts `(file as any).path` or string path from `tauri://drag-drop`; no path ? `no filesystem path � use Add media button`). For each supported path: `importMedia(path)` (`media.rs:270` hash+copy+thumb) ? `addSlide(baseName)` ? `updateSlide({ background })` ? `reorderSlides` if needed (sequential `insertIdx++` preserves order). `importingMedia` flag.
- **Playlist integration** `Editor.svelte:440` `onPlaylistDragOver` branches to `handleExternalDragOver` when `isExternalFileDrag` (sets `externalDragActive`, `dragOverIndex`, `dropEffect=copy`). `onPlaylistDrop` `Editor.svelte:479` early checks `files.length>0` ? `handleExternalFiles`. `ul.slide-list` `Editor.svelte:1195` `class:external-drag` + `ondragover` both-branch. `li` `ondragover` benefits via same helper. Drop zone `Editor.svelte:1256` `div.external-drop-zone` (`role=region`) below template actions: `ondragover`/`ondragleave`/`ondrop` ? `handleExternalFiles`; label + `drop-error` inline. CSS `Editor.svelte:3090` dashed border ? `drag-active` accent+shadow; `external-drag` outline.
- **Tauri fallback** `Editor.svelte:1` `import { listen } from \"@tauri-apps/api/event\"`, `Editor.svelte:1084` `onMount` `listen(\"tauri://drag-drop\")` + `listen(\"tauri://file-drop\")` (payload `paths: string[]`) ? `handleExternalFiles`; unlisten on destroy. Ensures OS drops work even when HTML5 `File.path` not exposed � same pipeline, same inline error.
- **Error handling** Unsupported (e.g. `.txt`) ? `Unsupported file type: foo.txt. Supported: ...` inline + top error; backend `media.rs:272` `unsupported media type` also surfaced. Mixed drop still imports supported files.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code`. Manual: drag `photo.jpg` onto playlist ? new slide with hashed background+thumb; `movie.mp4` onto drop zone ? video slide; `notes.txt` ? inline red error, no slide; mixed ? one slide + error.

## Changed (2026-09-02) - Global search (Ctrl/Cmd+K) � library + all Bibles + media cache

*Adds a global search (keyboard shortcut `Ctrl/Cmd+K` opening a search overlay) that queries the song library, all cached/imported Bibles, and the media cache simultaneously, showing categorized results, each clickable to insert directly into the playlist � for adapting quickly to spontaneous requests mid-service. Reuses existing search/lookup commands where possible rather than duplicating logic; primarily a new frontend overlay aggregating existing backend capabilities.*

- **Backend � media cache search (new) + reuse** `src-tauri/src/media.rs:322` `list_media_assets(data_dir)` (scans `media/<hash>.<ext>`, `MediaKind::from_extension`, derives `hash`/`thumb` via `thumbnail_path_for`, builds `MediaAsset` sorted by `file_name`) and `search_media_assets(data_dir, query)` (case-insensitive filter on `file_name`/`hash`/`kind`, cap 50; empty ? 100). Commands `src-tauri/src/commands.rs:1369` `search_media(query: String) -> Vec<MediaAsset>` and `list_media() -> Vec<MediaAsset>` (both `data_dir = app.state::<AppState>().app_data_dir()`), registered `src-tauri/src/lib.rs:645` `generate_handler![..., search_media, list_media]`, exposed `src/lib/sync.ts:218` `searchMedia`/`listMedia` (`src/lib/types.ts:23` `MediaAsset` existing). Scripture reuses `search_scripture` `commands.rs:1734` (already aggregates KJV + all imported Bibles via `ScriptureIndex::search`); library reuses client-side `library.songs` filter (`src/components/GlobalSearch.svelte:28`), no new library search command.
- **Frontend � overlay aggregator** `src/components/GlobalSearch.svelte:1` new component (`open`/`library`/`onClose` props): input with `?` icon + `Ctrl+K` hint, `\` focus on open, debounced (180ms) `doSearch` ? `Promise.allSettled([api.searchScripture(trimmed), api.searchMedia(trimmed)])` + client-side `libraryResults` derived (`title`/`verse title`/`body` contains query, cap 8; empty query ? first 5 songs). Empty query also `api.listMedia().slice(0,6)`. Categories: **Songs � Library** (title + `N verses � verse titles�`, `insertSong ? addSongToPlaylist`), **Scripture � All Bibles** (`search_scripture` matches `reference`/`text` ? `addSlide(reference, text)`), **Media � Cache** (`search_media` assets with `media-thumb` via `convertFileSrc(background.thumb)` + `isMedia` guard, `fileName` + `kind � hash�` ? `addSlide(baseName)+updateSlide({background})` two-step same as drag-drop pipeline). Each button `disabled` while `inserting`, `onClose` after success; `Esc`/backdrop closes; footer hints `? Insert � Esc Close � Ctrl+K Reopen`. Styles `scoped` palette (720px, 78vh, `var(--panel)`/`--border`, `result` hover `accent`).
- **Integration** `src/components/Editor.svelte:13` `import GlobalSearch`, `Editor.svelte:102` `globalSearchOpen` state, `Editor.svelte:1088` `handleGlobalKeydown` (`Ctrl/Cmd+K` toggle, `Esc` close), `Editor.svelte:1173` `window.addEventListener(\"keydown\", handleGlobalKeydown)` + cleanup, `Editor.svelte:1200` topbar button `? Search <kbd>Ctrl+K</kbd>` (`search-trigger`), `Editor.svelte:1943` `<GlobalSearch open={globalSearchOpen} library={library} onClose={() => globalSearchOpen=false} />` + `search-trigger` CSS.
- **IPC count** `50 ? 52` (`search_media` + `list_media`) � `README.md` `## IPC Commands (52)` and `src-tauri/.../commands.rs 52 handlers`.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX`, `ensure_stage`). Manual: `Ctrl+K` opens palette (or click topbar Search) ? type `love` ? Songs shows matching library songs, Scripture shows KJV + imported Bibles matches (e.g. `1 Cor 13`), Media shows matching cached images/videos (hash/fileName); click song ? whole song inserted via `add_song_to_playlist` (playlist grows, state broadcast); click scripture ? new slide `reference` inserted; click media ? new slide with that `Background::Image/Video` (thumb visible, same as drag-drop); empty query shows recent songs + recent media; `Esc` closes.


## Changed (2026-09-02) - Title-case formatter + native spellcheck (lightweight)

*Implements lightweight title-case (button, not auto-mangle) for the Title field and basic spellcheck for the Body textarea by leveraging the browser/webview's native `spellcheck` attribute � confirmed that `spellcheck=\"true\"` already gives adequate underline-based spellcheck via the OS/webview before building anything custom. Keeps this simple; goal is catching obvious typos before they reach the live screen, not full grammar.*

- **Title � Title Case button** `src/components/Editor.svelte:796` `toTitleCase(s)` (`trim ? split /\s+/ ? small words set a/an/and/.../via lowercased unless first word; hyphen/apostrophe aware`) + `applyTitleCase()` (`draftTitle = toTitleCase(draftTitle)` + clear `titleTimer` + `commitTitle`). UI `Editor.svelte:1503` `<div class=\"title-row\"><input spellcheck lang> + <button class=\"title-case-btn\>Aa</button></div>` + hint. CSS `Editor.svelte:3194` `.title-row`/`.title-case-btn`. Chosen button over auto-format-on-blur to avoid surprising caps; `flushTitle` on blur does *not* auto-mangle � only explicit `Aa` click formats.
- **Body � native spellcheck** `Editor.svelte:1524` `<textarea spellcheck=\"true\" lang=\"en\">` (also Title `<input spellcheck>`). Verified: WebView2/WebKitGTK honour `spellcheck` via OS dictionary � red underline for `helo`/`testt` without custom JS. No `autocorrect`/`autocapitalize` (rejected by Svelte `HTMLProps`; `lang` suffices). No backend change � `project.rs`/`commands.rs` unchanged.
- **Why native first** Checked that enabling `spellcheck="true"` on the textarea already gives adequate underline-based spellcheck via the webview before building anything custom � confirmed manual (type `helo wrld` ? red underlines). Deferred full grammar-checking as out of scope.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code`. Manual: Title `amazing grace` ? `Aa` ? `Amazing Grace`; Body `helo` underlined; live screen shows corrected.

## Changed (2026-09-02) - Local parsers for .pro/.cho/.usr (Library song import, no cloud)

*Add local parsers (using `quick-xml` where applicable) for `.pro` (ProPresenter export), `.cho` (ChordPro text), and CCLI USR text files, so dragging one of these onto the Library adds it as a new song with its slides/verses parsed in, no cloud calls. Scope conservatively: extract title + text content into the existing `library.json` song structure; don't attempt to preserve ProPresenter-specific styling/backgrounds from `.pro` files, just the text content. Clearly report unparseable/malformed files rather than silently failing.*

- **Parsers � `src-tauri/src/song_import.rs:1` new module (`quick-xml` 0.37 already in `Cargo.toml:46`)**: `ParsedSlide { title, body }` + `ParsedSong { title, slides }`; `import_song_file(path)` dispatches by ext: `.pro` ? `parse_pro` (`quick-xml`), `.cho/.chopro/.chord` ? `parse_cho`, `.usr` ? `parse_usr`, `.txt` heuristic. `parsed_to_library_song` ? `LibrarySong` (default background, text only, styling ignored). Unsupported ext ? clear `unsupported file type` Err.
- **.pro `song_import.rs:80` `parse_pro`** `Reader::from_str` tracking `RVSlideGrouping`/`RVDisplaySlide` boundaries, `name` attributes as titles, `NSString` text nodes; fallback `strip_xml_tags` if no slides; title from first group name or file stem; malformed/empty ? clear Err.
- **.cho `song_import.rs:200` `parse_cho`** `{title:}/{t:}` ? title, `{soc}/{eoc}` as separators, `strip_chords` `[C]` removal, split by blank lines ? `Verse N` slides; empty ? Err.
- **USR `song_import.rs:260` `parse_usr`** `Title:` header until blank/`---`, split by blank lines + labels `Verse/Chorus/Bridge` ? slides; fallback `parse_plain` for generic `.txt`.
- **Backend IPC** `src-tauri/src/commands.rs:315` `import_song_file(path)` (`spawn_blocking`), `src-tauri/src/lib.rs:8` `mod song_import` + `lib.rs:650` `53 handlers`, `src/lib/sync.ts:226` `importSongFile`.
- **Frontend � Library drop zone** `src/components/Editor.svelte:86` `libraryDragActive`/`libraryDragError`/`SONG_EXTS` + helpers `handleLibraryDragOver`/`handleLibraryFiles` (validates `SONG_EXTS`, calls `api.importSongFile` per path, inline `Unsupported`/`malformed` errors). Library sidebar `Editor.svelte:1605` `ondragover/ondrop` + inner `library-drop-zone` (`Drop .pro / .cho / .usr here`). Tauri `tauri://drag-drop` now partitions `songPaths` vs `mediaPaths`. Existing playlist media drop unchanged.
- **Tests** `song_import.rs:380` 5 tests (cho strips chords, usr title/verses, pro simple xml, malformed handling).
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code`. Manual: drag `.pro`/`.cho`/`.usr` onto Library ? Library song with verses, no styling; malformed ? inline `malformed XML` / `empty` error, not silent.

## Changed (2026-09-02) - Refactor library.json to master-block architecture (blocks + arrangement) with migration

*Refactor song storage in `library.json` from duplicated per-verse flat `slides` to a master-block architecture: each song stores a dictionary of unique named blocks (e.g. `Verse 1`, `Chorus`, `Bridge`) and a separate `arrangement` array of block keys defining the normal play order (e.g. `[\"Verse 1\",\"Chorus\",\"Verse 2\",\"Chorus\",\"Bridge\",\"Chorus\"]`).*

- **Backend � model `src-tauri/src/project.rs:15` `LIBRARY_SCHEMA_VERSION=2` + `LibrarySong { blocks: HashMap<String,LibrarySlide>, arrangement: Vec<String>, slides: Option<Vec<LibrarySlide>> }` + `impl LibrarySong { migrate_if_needed(), flattened_slides() }` (deduplicates by title, handles duplicate titles via `\" (2)\"` suffix). `Library` default now `2`.**
- **Migration � one-time, logged** `project.rs:896` `read_library` ? `read_library_with_migration_info` counts `migrated`, bumps `schema_version` to `2`, `eprintln!` + `write_library` persist, `lib.rs:267` logs via `state.logger.log(Level::Info, \"library: migrated {} song(s) ...\")`. Existing `library.json` with flat `slides` auto-converted on first load, no manual rebuild. Tested with `Amazing Grace`/`Great Is Thy Faithfulness`.**
- **Seed** `project.rs:932` `seed_library` now builds `HashMap` blocks + `arrangement` (`[\"Verse 1\",\"Chorus\"]`) for both samples.**
- **Queue-time flattening** `commands.rs:374` `add_song_to_playlist` now `song.flattened_slides()` ? `project.slides` (same linear result, data deduplicated).**
- **Song creation** `commands.rs:259` `add_library_song` + `song_import.rs:56` `parsed_to_library_song` now build `blocks`+`arrangement` (handling duplicate titles).**
- **Arrangement editing � backend** `commands.rs:345` `set_song_arrangement` validates keys, updates `arrangement`, `broadcast_library`. Registered `lib.rs:650` + `sync.ts:228`.**
- **Editor UI � chips** `Editor.svelte:979` helpers `getSongBlockCount`/`getBlocksArray`/`move/duplicate/remove/addBlockToArrangement` (via `setSongArrangement`). Library sidebar `Editor.svelte:1605` shows `getBlocksArray` blocks + `arrangement-row` chip list (`�`/`�`/`?`/`�`) + `<select>+ Add block�</select>`. `onPlaylistDrop` library-verse lookup handles both `blocks` and deprecated `slides`. Counts show `arrangement.length` (queued) � `blocks.length` (unique).**
- **Global search** `GlobalSearch.svelte:24` updated to check `Object.values(s.blocks)` fallback.**
- **Types** `types.ts:184` `LibrarySong { blocks: Record<string,LibrarySlide>, arrangement: string[], slides? }`.**
- **Tests** `project.rs:998` 3 migration tests (preserve Amazing Grace, seed, duplicate). `cargo test` 53 passed.**
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code`, `cargo test` 53 passed.**

## Changed (2026-09-02) - ChordPro chord notation for Stage Display (band-view)

*Add ChordPro chord notation support for the Stage Display (band-view monitor), building on the existing `.cho` parser. Backend keeps raw `[G]` text; frontend strips per-view (Output clean, Stage stacked).*

- **Backend `song_import.rs:335` `parse_cho` now keeps raw `[G]` (was `strip_chords` at storage, now `current_block.push(line.trim())` + fallback keeps raw) � strip only at render time per-view. Test `song_import.rs:658` updated to assert raw contains `[G]` and `strip_chords` removes it. `strip_chords` now `#[allow(dead_code)]` (only used in tests). No new deps.**
- **Frontend `src/lib/chords.ts:1` new**: `hasChords`/`stripChords`/`parseChordLine`/`parseChordBody` (bracket state machine, handles `[G/B]`). Simple inline-flex per segment left-edge alignment � reuses `fitText` scrollHeight, no canvas needed; flagged that per-glyph `measureText` would only be needed for justified text.
- **SlideRender `SlideRender.svelte:1`** `isStage?:boolean` + `shouldShowChords = isStage && hasChords(slide.body)`; title `stripChords`; body: if `shouldShowChords` ? `<div chord-body>` with `split(\"\n\")` ? `parseChordLine` ? `chord-segment` (`chord` `0.52em` `#fbbf24` above `lyric`), else `<p>` with `isStage ? body : stripChords(body)`. CSS `.chord-body`/`.chord-line`/`.chord-segment`.
- **Stage `Stage.svelte:1`** `import stripChords` + `<SlideRender isStage={true}>` + next preview `stripChords`; `Editor.svelte:2032` stage preview `isStage={true}` (output preview remains clean). Automatic: plain slides without `[`/`]` render unchanged.
- **Verify:** `[G]Amazing [C]grace�` chorus ? Output clean, Stage stacked. `npm run check` 0/0, `cargo check` 2 `dead_code`.

## Changed (2026-09-02) - Targeted stage-only message broadcast (stage banner)

*Add a targeted stage-only message broadcast, independent of the main live slide � for nursery alerts, countdowns, or operator-to-stage notes that should never appear on the main projection.*

- **Backend `state.rs:35` `stage_message: RwLock<Option<String>>` + `stage_message_gen: AtomicU64` + `project.rs:446` `ClientState { stage_message }` + `commands.rs:18` `snapshot` includes `stage_message` (no `request_save`, no `project` touch). Commands `commands.rs:1425` `set_stage_message(message, duration_secs?)` (trim `1..500`, `duration 1..3600`, set + `bump` + `log` + `snapshot_and_emit` + optional `thread::spawn sleep` with gen check ? auto-clear + `bump`) / `clear_stage_message` (clear + `bump`). Registered `lib.rs:650` + `types.ts:148` + `sync.ts:232`. Auto-expire implemented (straightforward, cancellable via gen); manual clear is baseline.**
- **Editor `Editor.svelte:118` `stageMessageDraft`/`stageMessageDuration` + `sendStageMessage`/`clearStageMessage` (`api.setStageMessage`/`clearStageMessage`) + `Editor.svelte:2100` `stage-message-panel` in Stage Display section (input `Nursery alert�` + duration `30s` + `Send`/`Clear`, current `stageMessage` preview, hint `Red flashing banner on Stage only`).**
- **Stage `Stage.svelte:56` banner `{#if stageMessage}` `<div class=\"stage-banner\" role=\"alert\">` absolute top `#e11d48` pulsing `banner-pulse` + `text-flash`, `z-index:10` overlay without disrupting `.current`/`.side` (next/clock). Output `Output.svelte` unchanged � never shows stage_message.**
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code`. Manual: set `Test 123` ? Stage red banner, Output live unchanged; `Clear` ? banner gone; `duration 2s` ? auto-clear after 2s; new message before expiry cancels old timer.

## Changed (2026-09-02) - Independent overlay layers for Output (background / slide / overlay)

*Add support for independent background/slide/overlay layers, so a slide can have a persistent overlay (e.g. a lower-third or logo) independent of the main background/text content.*

- **Backend `state.rs:35` `overlay: RwLock<Option<Overlay>>` + `project.rs:584` `Overlay { id,text,background:Option<Background>,visible:bool }` + `project.rs:446` `ClientState { overlay }` + `commands.rs:70` `snapshot` includes `overlay` (no `request_save`). Commands `commands.rs:1500` `set_overlay(text,background?)` (`1..500` or image required, `visible:true`), `set_overlay_visible(visible)` (validate has overlay), `clear_overlay` (`None`). Registered `lib.rs:650` `59 handlers` + `types.ts:184` `Overlay` + `sync.ts:232`.**
- **Frontend `SlideRender.svelte:1` 3 layers: `background` `z-index:0` (`media-layer`), main `look-title`/`look-body`/`chord-body` `z-index:1`, overlay `overlay-layer` `z-index:2` (`position:absolute inset:0 flex column justify-content:flex-end`) with `overlay-media` `18vh` bottom + `overlay-text` `rgba(0,0,0,0.72)`. `Output.svelte` window-level `overlay-layer` outside `frame` crossfade so video keeps playing; `Stage` never shows overlay (Output-only). Existing single-layer slides (`overlay` `None`) render exactly as before.**
- **Editor `Editor.svelte:118` `overlayTextDraft`/`overlayBackgroundDraft` + `setOverlay`/`showOverlay`/`hideOverlay`/`clearOverlay`/`pickOverlayImage` (`importMedia`) + `Editor.svelte:2186` `Overlays � Output only` panel (input `Lower-third text�` + image preview + `Image�`/`Set`/`Show`/`Hide`/`Clear`) + preview `SlideRender overlay={appState?.overlay}` (stage preview without).**
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code`. Manual: video slide live ? set overlay `Welcome` ? Output shows video (playing) + lower-third bar on top (`z2`), Stage shows no overlay; `Hide` ? overlay gone, video uninterrupted; `Clear` ? removed. Existing slides with no overlay render identically.

## Changed (2026-09-02) - Native audio playback for single backing track (rodio/cpal, routable, not tied to slides)

*Add native audio playback for a single backing track, routable to a specific sound device � scoped deliberately narrow: ONE track at a time, NOT multi-track/stem routing.*

- **Verification � muted video never produces audio** `Output.svelte:159` / `SlideRender.svelte:57` / `Output.svelte:175` / `SlideRender.svelte:138` all `<video autoplay loop muted playsinline preload=\"auto\">` with `muted` unconditionally; no JS ever sets `video.muted=false` or `volume` (grep `\.muted|\.volume` only finds CSS `.muted` class). Phase 4 doc already `Audio playback (video backgrounds are muted this phase)` � new rodio `Sink` is independent `OutputStream` per selected cpal device, never the `<video>` element, so no conflict.
- **Backend `Cargo.toml:47` `cpal 0.15` + `rodio 0.17 {mp3,wav,flac,vorbis}`** Dedicated thread `audio.rs:1` `AudioPlayer { state: Arc<Mutex<AudioStateView>>, tx, handle }` � `OutputStream`/`Sink` live only on audio thread (never in `AppState` `Send/Sync`), `Inner` not in `AppState`. `list_output_devices` via `cpal::default_host().output_devices()` ? `AudioDeviceInfo`. `Command` `Load/Play/Pause/Stop/SetVolume/Seek/SetDevice` via `mpsc`. `load` `File::open` ? `Decoder` ? `sink.append` ? `pause`; `play`/`pause`/`stop`/`set_volume` clamped. `AppState` `state.rs:35` `audio: AudioPlayer` + `project.rs:446` `ClientState { audio }` + `snapshot` `commands.rs:70` `audio: state.audio.get_status()`. `Settings` `project.rs:784` `audio_output_device_id`/`audio_volume` (`1.0`) persisted, restored in `lib.rs:335` + `finalize` `state.audio.shutdown()`. No main-thread block � `load` just `send`.
- **Device routing & Settings** `audio.rs:20` `list_output_devices` (cpal), `commands.rs:1574` `list_audio_devices`/`load_audio`/`play_audio`/`pause_audio`/`stop_audio`/`set_audio_volume`/`seek_audio`/`set_audio_device` (each `log Info` + `snapshot_and_emit`, device/volume persist via `write_settings`). `changed_settings` `19` fields now includes `AudioDevice`/`AudioVolume`. Registered `lib.rs:650` `67 handlers`.
- **Editor UI `SettingsPanel.svelte:1`** new `Audio` tab (`tab audio`) with `audioDevices`/`audioVolumeDraft` + `refreshAudioDevices`/`loadAudioFile` (`openDialog` `mp3/wav/flac/ogg/m4a`) + `play`/`pause`/`stop`/`volume`/`device`. Panel shows device `<select>` (`System default` + enumerated), `Refresh`, track `currentPath` basename + `status`, `Play`/`Pause`/`Stop` (disabled by status), volume `range 0..1.5`. Independent utility, not tied to slides/playlist.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (was `*mut ()` error fixed by moving `OutputStream` to thread), `cargo test` 53 passed. Hand test: `list_audio_devices` returns fallback default in headless; `load`/`play`/`pause`/`stop`/`volume` do not stall UI (audio thread), `Output` `<video muted>` remains silent while rodio plays � real-device audible routing needs manual verification on your machine (Settings ? Audio ? select device ? Load MP3 ? Play ? confirm sound on chosen device).

## Changed (2026-09-02) - Project Hub flat matte cards (replace gradients)

*Replace the gradient/hue-based service-type cards in the Project Hub (Sunday Service blue-gradient, Midweek teal-gradient, Youth orange/pink-gradient) with flat matte colors consistent with the rest of the app's existing design tokens from the earlier design pass. Keep the colored category badges (SUNDAY SERVICE/MIDWEEK/YOUTH/CUSTOM) as solid color chips, not gradients. This now looks like it belongs to the same app as the Editor, not a separate marketing-style launcher.*

- **ProjectHub `ProjectHub.svelte:1`** Remove `presetGradient` import, cards use `data-category` + `card-badge[data-category]` solid fills instead of `style:background={presetGradient}` gradients.
- **Flat matte `ProjectHub.svelte:187`** `.card` `var(--panel)`/`--border`/`var(--text)` (was gradient white), `.card-badge` per-category solid: Sunday `var(--color-green)`, Midweek `#1e3a4d`, Youth `var(--brand-orange-500)`, Custom `var(--panel-2)`/`--border`. `.hub-overlay` `rgba(0,0,0,0.55)` (was `5,10,20,0.72`), `.hub-head` `var(--panel-2)` (was gradient), `.close` `var(--panel-2)`/`--border` (was translucent white). All tokens from `src/app.css:11` earlier pass � no new palette, now unified with Editor.
- **Verify:** `npm run check` 0/0, `cargo check` 3 `dead_code`. Manual: Hub cards matte dark with solid badges vs previous gradients; selected state `--accent` border consistent with Editor.

## Changed (2026-09-02) - Fix presentation text off-center on Output (Look geometry leak)

*Investigated before fixing as requested: does active Look define horizontal text-alignment, is it applied or overridden, and does fitText binary-search leave stale margin/transform. Root cause is SlideRender unconditional geometry leak.*

- **Look** `src/lib/types.ts:57` `Look { textPosition, positioning, titleBox/bodyBox }` — only vertical `textPosition`; horizontal always centered via `SlideRender.svelte:157` `align-items:center` + `text-align:center` and `max-width:80%`. Applied via `SlideRender.svelte:30` `pos-*` classes (`justify-content`), not overridden in `Output.svelte:34` (`outputLookId -> Main`). No horizontal field to honor.
- **fitText** `src/lib/fitText.ts:77` `neutralise`/`restoreNatural` + `fitText.ts:116` binary search + `fitText.ts:240` auto loop — only `width/fontSize/display/overflow/webkitClamp/lineHeight` (now also `margin/transform` cleared). Pinned `width=allowedW` is flex-centered, not `margin`/`transform`; `restoreNatural` clears when `size===base` so short slides not stale. No stale margin/transform from fitText; hardened defensively.
- **Root cause** `SlideRender.svelte:69`/`85`/`112` `style:left/top/width/height/z-index` always set from `look.titleBox/bodyBox` (`x:5%` default `src-tauri/src/project.rs:123`) even when `positioning==='auto'` where CSS is `position:relative` (`SlideRender.svelte:203`). `left:5%` shifts flex child right, causing off-center in default Main `Center` + `Auto`.
- **Fix** `SlideRender.svelte:74`/`89`/`116` conditional `style:left={look.positioning==='absolute' ? `${x}%` : undefined}` (same for top/width/height/z-index); `use:fitText` `deps` already includes `positioning`/`textPosition`; `fitText.ts:77` hardened to clear `margin`/`transform`.
- **Verify:** `npm run check` 0/0, `cargo check` 3 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`). Manual: `Center`+`Auto` now horizontally centered (no 5% bias), `top`/`center`/`bottom` still correct, `Absolute` boxes still positioned, resize re-centers.

## Changed (2026-09-02) — MakrStudio (formerly MakePresent)

*Display-name rename: MakePresent → MakrStudio. First mention `MakrStudio (formerly MakePresent)` `docs/PROJECT.md:1` / `README.md:1` / `docs/WINDOWS.md:1`, thereafter `MakrStudio`. Only UI-visible text changed; internal crate `makepresent` `src-tauri/Cargo.toml:2`, `makepresent_lib` `Cargo.toml:12`, `identifier com.makesoftware.makepresent` `tauri.conf.json:5`, window labels `main`/`output`/`stage` `windows.rs:35` / `tauri.conf.json:15`, file paths, git repo name unchanged — display-name only.*

- **Window titles & bundle** `index.html:6` `MakrStudio - Editor`, `output.html:6` `MakrStudio - Output`, `stage.html:6` `MakrStudio - Stage Display`; `tauri.conf.json:3` `productName: MakrStudio`, `tauri.conf.json:16` `title: MakrStudio - Editor`, `tauri.conf.json:29` `tooltip: MakrStudio`, `tauri.conf.json:45` `longDescription: MakrStudio - native Windows presentation app…` (identifier `com.makesoftware.makepresent` kept).
- **Editor & backend titles** `src/components/Editor.svelte:1525` `Starting MakrStudio…`, `Editor.svelte:1533` `<h1>MakrStudio</h1>`, `Editor.svelte:1572` `Welcome to MakrStudio`; `windows.rs:107` `title("MakrStudio - Editor")` (3 Editor, 5 Output, 4 Stage sites) — `title` only, labels `main`/`output`/`stage` unchanged.
- **Tray, NDI, Stage page** `lib.rs:145` `Quit MakrStudio`, `broadcast.rs:41` `NDI_SOURCE_NAME: MakrStudio - Sunday Output` (test `broadcast.rs:406`), `network.rs:462` `<title>MakrStudio Stage</title>`, `network.rs:522` `<h1>MakrStudio Stage</h1>`, `network.rs:523` `Enter the PIN shown in MakrStudio → Settings`; `project.rs:279` `Welcome to MakrStudio`, `commands.rs:1974`/`1978` `MakrStudio settings file`, `midi.rs:83,176` `MidiInput::new("MakrStudio")`, `scripture.rs:956` `user_agent "MakrStudio/0.1"`, `package.json:4` / `Cargo.toml:4` descriptions `MakrStudio - live…` (package names `makepresent` kept), `.github/workflows/build.yml:99` `MakrStudio-windows-installers`, `docs/WINDOWS.md:1` `MakrStudio (formerly MakePresent)`.
- **Docs prose** Title `docs/PROJECT.md:1` `# MakrStudio (formerly MakePresent) — Living Project Doc`, `docs/PROJECT.md:6` `What MakrStudio Is`, etc. — `MakePresent/` `README.md:346` and `MakePresentIcons.zip` `README.md:351` kept as repo/assets paths.
- **Not renamed** Rust crate names, file paths, git repo, internal `makepresent-advance` `midi.rs:108`, capability `label` — avoids churn/risk.
- **Verify:** `npm run check` 0/0, `cargo check` 3 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`) — window `label` unchanged, so Tauri capabilities and routing unaffected; manual titles/tray/NDI/stage page show `MakrStudio`.

---

## Changed (2026-09-02) — Major Editor UI restructuring (Parts A–E)

*Left-to-right flow, grid-first workspace and dedicated Look editor — preserves "clarity over density", keeps search/drag, reports visual result before final.*

- **PART A — Slide name vs Title `src-tauri/src/project.rs:63` `Slide { name: Option<String> }` + `project.rs:89` `display_name()` + `project.rs:761` `TemplateItem { name }` + `project.rs:293` `new() name: Some("Welcome…")` + `commands.rs:1098` `add_slide(..., name)` + `commands.rs:1127` `update_slide(..., name)` + `commands.rs:411` `add_song_to_playlist` `name` + `commands.rs:982` `save/load_template` `name` + `src/lib/types.ts:35` `Slide { name? }` + `sync.ts:67` `addSlide`/`updateSlide` + `Editor.svelte:130` `draftName`/`slideDisplayName` + `Editor.svelte:1933` `Slide name` field `blank = follows Title` + `Editor.svelte:1662` playlist label `slideDisplayName`; legacy slides `name=None` fallback to `title` so no blank labels.**
- **PART B — Grid `Editor.svelte:195` `showDetail`/`openDetail`/`closeDetail` + `Editor.svelte:2060` center `{#if centralView==="looks"} LookEditorView {:else if showDetail} detail {:else} grid` + `grid-toolbar` + `slide-grid` `repeat(auto-fill,minmax(180px,1fr))` `gap:14px` + `grid-cell` draggable reusing `onPlaylistDrag*` + `grid-thumb` `SlideRender` scale 0.32 `look=outputPreviewLook` + `grid-label` `slideDisplayName` + `grid-actions` `Go Live`/`Delete`; detail via click, same fields.**
- **PART C — Left-to-right `Editor.svelte:2734` `.body` 3-col grid `clamp(220px,18vw,300px) 1fr clamp(220px,18vw,300px)` + left `sidebar` `workspace-switch` `Slides`/`Looks` + center `slide-grid`/`LookEditorView` + right `output-panel` `output-sticky-top` `position: sticky; top:-16px` `overflow-y:auto` — preview/status fixed top-right always visible, center scrolls independently.**
- **PART D — Arrow keys `commands.rs:580` `advance` + `commands.rs:633` `next_slide`/`prev_slide` + `lib.rs:628` handler + `sync.ts:59` `nextSlide`/`prevSlide` + `Editor.svelte:1345` `isTextInputFocused` + `handleGlobalKeydown` `ArrowRight/Left` → `next/prev` when not in input, reuses `make_live` path.**
- **PART E — Look editor `LookEditorView.svelte:1` dedicated view `activeLookId`/`draft`/`scheduleCommit` + `looks-sidebar` `look-pill`/`badge` + `look-main` `look-preview-box` `SlideRender` sample + `look-form` `Name`/`titleSize`/`bodySize`/`titleFont`/`bodyFont`/`textColor`/`showBackground`/`textPosition`/`positioning` + `box-canvas` draggable `title`/`body` boxes + `assign-block` Output/Stage/NDI + `Delete`; `Editor.svelte:203` `centralView` + `LookEditorView` in center `slides` default; not just dropdown.**
- **Visual:** Left `Slides`/`Looks` switch + Playlist/Library, Center `Slides — N` grid 180px cards 16:9 thumbs `LIVE` badge + centered name, `Selected` accent ring `Live` green, hover lift; click → `← Grid` detail `Slide name` + `Title — on-screen` + `Body` + `Background` + `Auto-advance`. `Looks` center shows Looks list + 16:9 preview + form + bounding boxes canvas 16:9. Right sticky Output `Display`/`Fullscreen`/`Transition`/`Look`/`preview`/`ON AIR`/`status`/`Clear` always top-right, then Stage. Not denser than list — generous gaps, not overwhelming.**
- **Verify:** `npm run check` 0/0, `cargo check` 4 `dead_code` (`COPY_SUFFIX`, `ensure_stage`, `AudioPlayer::is_active`, `display_name`).**

---

## Changed (2026-09-02) — Fix fullscreen Fade clips out/in (containment + GPU promotion)

*Investigated before fixing — OS fullscreen collides with fade, plus size containment and transient promotion.*

- **OS toggle vs fade `Output.svelte:62` vs `windows.rs:1332` `toggle_output_fullscreen` + `windows.rs:1242` deferred 120ms** — fade fires immediately on `live` (`FADE_MS 400`), `set_fullscreen` recreates swap-chain at monitor size; concurrent dispatch caused stale-size clip. `show_output` already deferred; `toggle` did not.
- **Clipping `Output.svelte:226` `.frame` `contain: size layout style paint` + `Output.svelte:206` `.stage` `overflow:hidden`** — `size` froze box to old `100vw` during resize, clips to old box then snaps.
- **GPU lost `Output.svelte:232` `.frame.gpu` only while `crossfading` — fullscreen discarded layer mid-fade.**
- **Fix `Output.svelte:226` `contain: layout style paint` + `will-change: opacity; transform: translateZ(0); backface-visibility:hidden; isolation:isolate` on `.frame` + `Output.svelte:206` `.stage` `isolation: isolate; contain: layout style` — size can follow viewport, promotion persists, heavy `will-change` stays on `.gpu`.**
- **Verify:** `npm run check` 0/0, `cargo check` 4 `dead_code`. Manual fullscreen Fade now seamless.**

---

## Changed (2026-09-05) — Contextual onboarding: empty-state hints, guided tour, Help + shortcuts

*Priority #3 from the original project goals (design principle: prefer contextual hints and empty-state guidance over a separate manual). Extends the Phase 3 first-run welcome banner. Frontend only — Svelte + CSS + localStorage, no new deps, no backend change.*

**Audit — features added since the welcome banner with no visible discovery path for a first-time volunteer:**

1. View Hub / View + Playlist flow (`New view` topbar, `Save as Playlist`, starting-Playlist gallery)
2. Browse Scripture panel (collapsed by default behind `▸ Show`)
3. Drag-and-drop (playlist reorder, library song/verse → playlist, scripture → playlist, OS image/video files → new slide, `.pro`/`.cho`/`.usr` → Library)
4. Arrow-key slide navigation (`←`/`→` via `next_slide`/`prev_slide`) and global search (`Ctrl/Cmd+K`)
5. Looks (Slides/Looks workspace switch, dedicated Look editor, per-output Output/Stage/NDI look mapping, absolute bounding-box canvas)
6. Stage Display (Show/Hide toggle, stage-only message banner, Output-only overlays)
7. Targeted clear (`Clear text` / `Clear background` beside `Clear output`)
8. Slide grid + detail editing (click thumbnail to edit, slide name vs on-screen title, `Aa` Title Case, spellcheck, auto-advance `↻`)
9. Library song editor + arrangement chips + song-file import
10. Media backgrounds (`Add media` image/video import, managed hash cache)
11. Scripture autocomplete + `bible-api.com` fallback + OpenLP import + drop-XML `bibles/` folder
12. Settings sections: General (display/fullscreen/transition), Looks, Triggers (MIDI/OSC), Network (stage web + PIN), Audio (backing track), Logs — plus NDI broadcast toggle and system-tray standby

**Implemented `src/lib/onboarding.ts:1` (localStorage store `makrstudio.onboarding.v1`):** `tourDismissed` + per-feature `used` + per-hint `dismissed` (`loadOnboarding`/`markUsed`/`dismissHint`/`dismissTour`/`resetTourDismissal`/`showHint`). Try/catch throughout — blocked storage never breaks the app.

- **Empty-state hints `Editor.svelte:1764`/`1842`/`1845`/`1961`/`2065`/`2413` (`hint-line` `Editor.svelte:3650`, muted 11px, one line, × each):** playlist drag + reorder (`dragdrop`, `use` on any recognised drop `Editor.svelte:656` / media-file success `Editor.svelte:864`), playlist `←/→` + `Ctrl+K` (`shortcuts`, `use` on arrow/Ctrl+K `Editor.svelte:1448`/`1465`/`1469`), Looks tab (`looks`, `use` on switch `Editor.svelte:1762`), Browse Scripture collapsed (`browse`, `use` on chapter load/verse insert `Editor.svelte:550`/`578`), Library empty (`songs`, `use` on add/import `Editor.svelte:983`/`1215`/`1245`/`1259`/`1220`), Stage hidden (`stage`, `use` on toggle/send `Editor.svelte:1215`/`1493`). Each hides forever once its feature is used or dismissed — returning operators see at most quiet lines, never popups.
- **Guided tour `src/components/GuidedTour.svelte:1` (non-blocking bottom card, `pointer-events:none` layer `GuidedTour.svelte:34`):** 4 steps `Editor.svelte:139` (Playlist → Output → Songs & Scripture → "Ignore all of this on Sunday morning"), Back/Next + always-visible Skip/×, `Esc` ends it `Editor.svelte:1456`. Auto-starts only when `firstRun` (no `project.json`/`settings.json`, `project.rs:934`) + tour never dismissed + View Hub closed so targets are visible (`$effect` `Editor.svelte:166`, `showTour` `Editor.svelte:179`); `endTour` persists `tourDismissed` (`Editor.svelte:189`) so it never re-shows. Current step's panel gets an accent outline (`tour-highlight` `Editor.svelte:3688`, bound `Editor.svelte:1768`/`1954`/`1972`/`2308`). Nothing blocks input — a slide can go live mid-tour.
- **Help entry point `src/components/HelpModal.svelte:1` + topbar `? Help` `Editor.svelte:1722`:** replays the tour on demand (`replayTour` `Editor.svelte:194`) and lists every shortcut (`←/→`, `Ctrl+K`, `Esc`, `↑/↓/Enter` in Scripture, `Enter/Esc` in dialogs) plus the drag-alternative note.
- **Verify:** `npm run check` 0 errors 0 warnings, `vite build` clean (editor bundle 153.62 kB). No Rust touched — `cargo check` unaffected. Tone check-in pending with user (this entry records the audit + implementation for that review).

---

## Changed (2026-09-05) — View/Playlist audit follow-ups (display-string sweep)

*Audit follow-up to the View/Playlist merge: the merge itself was verified already-shipped (unified View Hub gallery, `Save as Playlist`, display rename, internals kept, backward compat confirmed). Four small issues found and fixed — display text only, no data-model or command changes.*

- **Saved-playlist date never interpolated `src/lib/components/ProjectHub.svelte:62`** — description was a plain string containing literal `{new Date(pt.createdAt).toLocaleDateString()}`; saved cards showed code text. Fixed to a template literal with `${...}` so cards read e.g. "Saved Sept 5, 2026".
- **Gallery order now matches its note `ProjectHub.svelte:46`** — the `:128` note says saved Playlists "appear at the top of the list" but presets were spread first. Saved playlists now spread first, presets after; note text unchanged and now true.
- **Leftover "project" display strings → view language** `Editor.svelte:1706` `No project` → `No view`, `Editor.svelte:636`/`833` `Project not loaded yet` → `View not loaded yet`, `SettingsPanel.svelte:697` export hint `The project and library` → `The current view and library`, `SettingsPanel.svelte:751` Looks hint `stored with the project` → `stored with the current view`, tour step `Editor.svelte:147` `Project when you're ready` → `Show it when you're ready` (was verb, reworded to avoid the noun entirely), stale comment `Editor.svelte:120` `Project Hub` → `View Hub`.
- **Deliberately untouched:** all internal names (`new_project_from_preset`, `load_template`/`save_template`, `PlaylistTemplate`, `templates.json`, `project.json`, `TemplateItem`, CSS classes `template-actions`/`project-name`) — display-only rename per the MakrStudio-rename caution. Future-confusion note: the `New view` button calls `newProject()` → `api.newProjectFromPreset`, and `Save as Playlist` calls `api.saveTemplate` — command names still say project/template; see README Changed entry for the mapping.
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` clean (3 pre-existing `dead_code`: `COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`).
