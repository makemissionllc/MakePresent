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
| **1** | `windows.rs:ensure_editor` `src-tauri/src/windows.rs:86` | `WebviewWindow::builder().build()` for Editor recreation still uses `run_on_main` inline fast-path. Called only from tray `show_editor` (`src-tauri/src/lib.rs:92`, `on_tray_icon_event`/`on_menu_event`) — *not* from a `#[tauri::command]` handler, but still a live event on the main thread. Rare (only if Editor was destroyed). Inline build could briefly stall WebView2 pump even from tray. | **MEDIUM** — real `build()` on Windows main thread, but not triggered by `invoke()`, only by tray/menu event; frequency very low. Should be deferred like Output/Stage if it ever fires during a live service. | `windows.rs:96`, `lib.rs:92` |
| **2** | `windows.rs:describe_window` / `log_window_state` `windows.rs:129` / `lib.rs:31` | `is_visible`/`is_focused`/`inner_position`/`inner_size`/`current_monitor` are WebView2 IPCs that dispatch to main thread. `log_window_state` spawns a worker thread that calls these after 2.5s. If that worker runs while main is blocked building a window, it would deadlock (same class). Currently only at startup + 2.5s diagnostic, not during live handlers. `snapshot()` no longer uses `is_visible` — fixed to HashMap. | **LOW** — same IPC class, but only diagnostic at startup, not in hot `snapshot()` path anymore. Safe after pre-create. | `windows.rs:129`, `lib.rs:219` |
| **3** | `windows.rs:list_displays` `windows.rs:632` | `editor.available_monitors()`/`primary_monitor()`/`current_monitor()` called directly from `#[tauri::command] list_displays` *without* `run_on_main` dispatch. These use Win32 `EnumDisplayMonitors` (synchronous, not WebView2 IPC) — they do not go through the WebView2 message loop. Quick (<1ms) and on handler thread, not main. | **FALSE POSITIVE** — synchronous Win32, not WebView2 IPC; does not block message loop. No defer needed, but could be wrapped in `run_on_main` for consistency if paranoia. | `windows.rs:632` |
| **4** | `windows.rs:move_stage_to`/`move_output_to` remaining `set_size`/`set_position`/`show`/`set_fullscreen`/`set_decorations` `windows.rs:605`/`741` | These `WebviewWindow` setters dispatch to main thread via `run_on_main` (now deferred pre-create ensures the window already exists, so only `set_*`/`show` run, not `build`). `set_*` are Win32 `SetWindowPos`/`ShowWindow` — synchronous but short, and now run on main thread inside `run_on_main`'s inline fast-path. After pre-create, no `build()` in handler. | **LOW** — now safe; remaining `set_*` are not `build()` and are short Win32 calls. Already inside `run_on_main` which is correct. | `windows.rs:605`, `741` |
| **5** | `midi.rs:76` `MidiListener::start` `src-tauri/src/midi.rs:76` | `MidiInput::new` + `midiInOpen` (`connect`) via `midir` (WinMM `midiInOpen`/`midiInGetNumDevs`) called *directly* from `set_midi_enabled`/`set_midi_device` handlers `commands.rs:1420` (`MidiInput::new` + `find_port_by_id` + `connect`). These are WinMM synchronous calls, but on the **handler worker thread**, not the main Win32 message loop. Handler blocks while opening, but other handlers run on other pool threads; main loop not blocked. | **LOW / FALSE POSITIVE** for WebView2 class — correct threading (worker, not main). Could still make that one `invoke()` feel slow (device open <100ms), but does not freeze *entire* app. Already not on main loop. No defer needed for deadlock, but could be `spawn_blocking` to keep handler responsive if desired. | `midi.rs:76`, `commands.rs:1420` |
| **6** | `osc.rs:55` `OscListener::start` `src-tauri/src/osc.rs:55` | `UdpSocket::bind` and thread spawn happen *inside* a newly spawned `osc-listener` thread, not in handler. Handler just does `self.stop()` (which `join()`s old thread ≤1s) then `thread::Builder::spawn`. `bind` not on handler nor main. `stop()` `join()` blocks handler up to 1s (read-timeout loop) but not main. | **LOW** — already off-thread. `stop()` join is handler-blocking but not message-loop blocking. Safe. | `osc.rs:55`, `135` |
| **7** | `network.rs:126` `NetworkServer::start` `src-tauri/src/network.rs:126` | Same pattern as OSC: `TcpListener::bind` inside spawned `network-stage` thread with its own tokio runtime, not handler. Handler just spawns. `stop()` `join()` blocks handler ≤200ms. Not main loop. | **LOW** — already off-thread. Safe. | `network.rs:126` |
| **8** | `broadcast.rs:222` `BroadcastCore::start` / `load_ndi` `src-tauri/src/broadcast.rs:222`, `147` | `Library::new("Processing.NDI.Lib.x64.dll")` → `LoadLibraryExW` synchronous Win32 loader, plus `NDIlib_initialize`/`send_create`. Called directly from `set_ndi_enabled` handler `commands.rs:591` on worker thread. On success, spawns `ndi-send` thread; on missing DLL, logs graceful `NDI SDK not found` and returns error (no block). `LoadLibraryExW` is synchronous but on worker, not main loop. At startup, `lib.rs:296` calls `broadcaster.start` *on setup main thread* — could briefly block main at startup (before loop), but not live. Once DLL present, success path still has synchronous load, but not WebView2. | **LOW for live**, **MEDIUM for startup** (setup main thread). Already graceful on missing DLL. For live, not WebView2 deadlock, but handler will be slow while loading DLL. Could be moved to `spawn_blocking` to keep handler snappy, but not required for deadlock fix. | `broadcast.rs:147`, `commands.rs:591`, `lib.rs:296` |
| **9** | `tray` `src-tauri/src/lib.rs:130` `setup_tray` / `on_tray_icon_event` / `on_menu_event` | `setup_tray` builds `MenuItem`/`Menu` + `tray.set_menu` only in `setup` (main thread, before live). No runtime tray updates from any `#[tauri::command]` handler — menu is static. Tray events (`Click`, `MenuEvent`) call `show_editor`/`quit_app` on main thread event loop, not via `invoke`. `show_editor` → `ensure_editor` still uses inline `build()` (see Rank 1). No `invoke()`-originated tray blocking. | **FALSE POSITIVE** — no command-handler → tray blocking. Only rare `ensure_editor` inline from tray event (Rank 1). | `lib.rs:130`, `180`, `198` |
| **10** | `dialog` `src-tauri/src/commands.rs:892` `import_media` / `src-tauri/src/commands.rs:1065` `export_settings` + frontend `open()` `src/components/Editor.svelte:251` | Tauri `dialog` plugin shows native `IFileDialog`/`GetOpenFileName` modally on main thread. While open, main loop is in modal loop (expected, user is picking file). After close, no lingering block. `open()` is invoked from frontend JS `await open(...)` — async, not `builder().build()`. `import_media`'s heavy `hash_file`/`ffmpeg` is already `spawn_blocking` `commands.rs:896`. `export_settings`/`import_settings` already `spawn_blocking` for file I/O. No lock held across `await` that a window op needs. | **FALSE POSITIVE** — modal is expected, not post-close deadlock. Already async/threaded. | `commands.rs:892`, `1065`, `Editor.svelte:251` |

**Verdict:** No remaining **HIGH** same-class `builder().build()` in live handlers after pre-create. Highest residual is Rank 1 `ensure_editor` rare tray path (MEDIUM) and diagnostic `describe_window` IPC (LOW). MIDI/OSC/Network/NDI/dialog are all already off-thread or worker-thread, not main-loop blocking (LOW/false-positive).

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