# MakePresent

**Live presentation software for churches — by DwellPraise Ministries / MakeSoftware.**

MakePresent is a free, self-hosted, church presentation tool built around a
simple, volunteer-friendly workflow. It is **not a ProPresenter clone** — it is
designed from the ground up to fit a first-time volunteer's flow with one
obvious path from "open the app" to "slide is live."

Because everything runs locally and the data is yours, there are no license
fees, no vendor-controlled roadmaps, and no service data leaving the building.

---

## Table of Contents

- [Why MakePresent](#why-makepresent)
- [Design Principles](#design-principles)
- [What It Does](#what-it-does)
- [Architecture](#architecture)
- [The Three Windows](#the-three-windows)
- [Data & Persistence](#data--persistence)
- [Key Features](#key-features)
- [Per-Output "Looks"](#per-output-looks)
- [Technology Stack](#technology-stack)
- [Project Structure](#project-structure)
- [IPC Commands](#ipc-commands-38)
- [Getting Started (Development)](#getting-started-development)
- [Testing & Verification](#testing--verification)
- [CI / CD](#ci--cd)
- [Deferred Work](#deferred-work)
- [Known Issues](#known-issues)
- [Documentation](#documentation)

---

## Why MakePresent

DwellPraise Ministries needed its own presentation software that:

- fits the exact way **their** volunteers run a service,
- is **free forever** and fully under their control,
- keeps all data **on-premises** — nothing leaves the building,
- is simple enough for a **first-time volunteer** with ~15 minutes of training.

The audience is a first-time volunteer: the app has to feel obvious. Every
screen state must answer *"what am I looking at and what do I do next?"*

---

## Design Principles

1. **Simple first, one obvious path.** A clear single route from "open the
   app" to "slide is live," with no dead ends and no unexplained states.
2. **Clarity over density.** Fewer, clearer controls beat packed, powerful
   ones. If a feature makes the common path harder to read, it doesn't ship yet.
3. **Polish is deferred.** Visual polish waits until the underlying flow is
   simple without it. A boring-but-clear interface beats a pretty-but-confusing
   one.
4. **Single source of truth.** All application state lives in the Rust
   backend; every window (Editor, Output, Stage) is a *dumb renderer* pushed
   fresh state. No window computes its own copy of "what should be live."

### Reliability priorities

- **No crashes.** Survives anything the operator throws at it mid-service.
- **Predictable output.** Once a slide is live it stays live — no surprise
  window changes or re-layouts on the projection display.
- **Autosave + crash recovery.** Every edit persists automatically; an
  interrupted session recovers the last good state and *tells the user* it did.
- **Resource management under multi-hour, multi-screen use.** No unbounded
  memory or log growth; windows are created on demand and released when not
  needed.

---

## What It Does

MakePresent renders song/scripture slides to external screens in a classic
live-presentation workflow:

- Build a **playlist** of slides (typed manually or pulled from a library).
- Set a slide **live** — it appears immediately on the projected **Output**
  screen.
- Show a separate **Stage Display** so performers/presenters can read the
  current *and* next slide at a glance.
- Give each screen its own **Look** (font size, text colour, position,
  background visibility) so the same slide renders differently per output.
- Automatically **autosaves** everything and recovers on the next launch.

---

## Architecture

MakePresent is a **Tauri 2** app: a **Rust** backend owns all state and
persistence, and a **Svelte 5** frontend renders it. Vite builds **three
separate HTML/JS bundles** (editor, output, stage), each loaded by its own
native window.

```
┌─────────────────────────────────────────────────────────────────┐
│                   Rust backend (single source of truth)          │
│   lib.rs · state.rs · project.rs · windows.rs · media.rs         │
│   commands.rs · logging.rs · scripture.rs · broadcast.rs         │
│                                                                   │
│   AppState ──► broadcasts "state" event ──► every window          │
└───────────────┬───────────────┬───────────────┬───────────────────┘
                │               │               │
        Editor window      Output window    Stage window
        (Editor.svelte)    (Output.svelte)  (Stage.svelte)
```

**State flow:** commands mutate `AppState` → schedule an autosave → build a
`ClientState` snapshot → emit a `"state"` event to **all** windows → return the
snapshot. Every window renders whatever the backend most recently broadcast;
none of them keep their own "what is live" state.

---

## The Three Windows

| Window | Label | Page / Component | Purpose |
|---|---|---|---|
| **Editor** | `main` | `index.html` → `Editor.svelte` | The operator's control surface: playlist, slide editing, output/stage controls, settings. Always exists at launch. |
| **Output** | `output` | `output.html` → `Output.svelte` | The dumb projection renderer (full background + graphics). Created on demand, placed on the chosen display, optionally fullscreen. |
| **Stage Display** | `stage` | `stage.html` → `Stage.svelte` | Performer-facing screen: current slide + `NEXT` + a live clock. Created/toggled on demand. |

**On-demand windows:** only the Editor exists at launch. The Output window is
created the first time a slide goes live **or** the operator clicks *Show
Output*. The Stage window is created purely through its toggle. This avoids a
black/empty dead-end window at startup.

**Persistent standby:** the app runs in the **system tray**. Closing the Editor
hides it (the process and any Output/Stage windows keep running and holding
their state); the tray's *Open Editor* (or a left-click on the icon) instantly
respawns it and re-broadcasts state so every view resyncs. The app only exits
via the tray's *Quit*.

---

## Data & Persistence

All data lives under the app data directory
(`~/.local/share/com.makesoftware.makepresent` on Linux):

| File / dir | Contents |
|---|---|
| `project.json` | The autosaved project (slides, looks, live/selected, transition). Atomic writes. |
| `versions/` | Versioned snapshots of prior project state (capped at 50). |
| `session.json` | Crash-recovery bookkeeping (`clean_shutdown` flag). |
| `settings.json` | Per-machine settings (displays, fullscreen, stage, default transition, look mappings). |
| `library.json` | Reusable song/slide library. |
| `logs/app.log` | Immediately-flushed, rotating event log (capped ~8000 lines). |
| `media/<hash>.<ext>` | Managed copies of imported media, deduped by **SHA-256** content hash. |
| `thumbnails/<hash>.jpg` | ffmpeg-generated thumbnails keyed by content hash. |

**Autosave + recovery:** a background worker wakes on every mutation, debounces
~1.2s, then persists atomically (temp file + rename). On launch the app prefers
the session's project file, then the current autosave, then the newest snapshot,
and shows a recovery notice when the prior exit was unclean.

---

## Key Features

### Slide backgrounds
- **Solid** colour (with a palette and custom colour picker).
- **Image / Video** full-bleed backgrounds via native `<img>` / `<video>`
  (`object-fit: cover`, muted + looping for video).
- **Managed import:** picking a file copies it into `media/<hash>.<ext>`,
  dedupes identical content by content hash, and generates a thumbnail. The
  project never references the user's original file.
- **Startup cache verification:** every referenced asset's source *and*
  thumbnail are checked; missing thumbnails are regenerated automatically and
  missing sources are logged loudly.

### Transitions
- Per-project **Cut** (default) or **Fade**; Fade crossfades the output over
  ~400ms via CSS (`Output.svelte`).

### Library
- Persistent songs with multiple verses/sections, client-side search, and
  one-click **Add to playlist** that links each slide back to its source
  verse/section.

### Scripture autocomplete
- A bundled **KJV** index (`kjv.json`, all 66 books) with abbreviation support
  (`jn`, `1 cor`, `psalm`, etc.). Search as you type and insert a verse as a
  new slide.
- **Import custom Bibles** from the widely-used **OpenLP / Zefania XML** format
  (`quick-xml`) — native, compact, and Zefania tag schemas — via
  `import_openlp_bible`.
- **REST fallback** via **bible-api.com** (`reqwest`): query any reference or
  translation and fold the JSON response into the same slide-generation
  workflow (`import_api_bible` / `lookup_api_scripture`). Imports are cached in
  the app-data dir and merged into the search index so they survive restarts.

### Persistent renderers (standby / tray)
- Closing the Editor window **hides it instead of quitting** — the Rust process
  (and any Output / Stage windows) keep running and hold their state.
- A **system tray icon** (Open Editor / Quit) plus left-click-on-icon respawns
  the Editor instantly; reopening re-broadcasts `AppState` so every view resyncs.
- The process only exits when the user explicitly chooses **Quit** from the tray.

### On-deck media preloading
- The backend names one **on-deck** slide per state (the selected-but-not-live
  slide, else the next playlist item). The Output keeps exactly one hidden
  preloader for it so a cut starts instantly without decoding on demand — no
  unbounded media accumulation over a service.

### Auto text fitting
- The `fitText` action binary-searches the largest font size at which title and
  body both fit their container, shrinking on overflow (flooring to title
  24px / body 16px before ellipsis). Recomputes on resize/content change and on
  Look changes.

### Settings
- Native **Import/Export** of per-machine settings (never the project/library)
  with schema validation and clear error messages.
- **Logs** panel: newest-first monospace view, copy-to-clipboard, and
  export-to-file.

### Display management
- Enumerates monitors; picks the **largest external display** (not the editor's)
  for the Output by default, falling back to the largest overall so the output
  is never lost.
- Fullscreen with a Linux/GTK-aware deferred toggle and size-mismatch
  diagnostics.

### NDI broadcast
- Publishes the live slide as an **NDI source** (`MakePresent - Sunday Output`)
  on the local network so a video switcher can cut to it.
- Runs on its **own thread** — never blocks the Output render loop — with a
  bounded, non-blocking frame channel and live-source keep-alive.
- The **NDI SDK is loaded at runtime** (`libloading`), not linked, so the app
  builds, tests, and CI-runs without it. Installing the free SDK (see below)
  is only needed to actually stream.
- Assign a **NDI Look** independently of the on-screen Output; enable/disable
  the feed and pick the Look from **Settings**.
- *Scope note:* the sender side is implemented; the webview→pixel **capture**
  that feeds it is a runtime follow-up (not exercisable headless/CI).

### MIDI & OSC slide triggering
- Drive the service from hardware: map a MIDI **Note / CC / Program Change**
  (or an **OSC address**) to **next / previous / jump / clear output**.
- Two always-ready listeners owned by the backend: a **midir** MIDI input
  (configurable device) and a **UDP OSC** socket (default port **9000**), both
  restored on launch and shut down cleanly on exit.
- A trigger maps to the **same slide-advance path the UI uses**, so a foot
  pedal "next" is identical to clicking Next.
- Configure it all in **Settings → Triggers**: pick the MIDI device, watch a
  live message monitor and capture a note as a trigger, or type an OSC address
  (bare `/makepresent/goto` also matches `/makepresent/goto/N` for jumps),
  choose the action, and enable/delete saved mappings.
- *Scope note:* triggers drive slide *actions*; they don't navigate the whole
  UI (e.g. no playlist navigation via hardware yet).

---

## Per-Output "Looks"

The same live slide can render **differently on each screen** via named style
profiles called **Looks**.

A **Look** contains:

- `name`
- `titleSize` / `bodySize` (base font sizes in px — `fitText` still shrinks on
  overflow)
- `titleFont` / `bodyFont` (font-family strings, e.g. `Druk Wide`,
  `Helvetica Neue Bold`) for typography pairings on any background
- `textColor` (colour override)
- `showBackground` (on/off — off yields plain text, useful for stage/stream
  compositing)
- `textPosition` (`top` / `center` / `bottom`)
- `positioning` — `auto` (text flows naturally, filled to the frame) or
  `absolute` (FreeShow-style template editor: each text role lives in an
  explicit draggable box)
- `titleBox` / `bodyBox` — bounding-box geometry in % of the frame
  (`x`, `y`, `width`, `height`, `zIndex`) used in `absolute` mode

**Mapping:** each output window is assigned a Look id. The mapping lives in
per-machine **settings** (`outputLookId` / `stageLookId` / `ndiLookId`), *not*
hardcoded — so the main Output, the Stage Display, and the NDI broadcast feed
each get their own Look. Unmapped outputs fall back to the look named
`Main` / `Stage` respectively (then the first); the NDI feed falls back to the
first look.

**Storage:** Looks are stored on the **Project** (`project.looks`), so they save
and load with autosave. New (and legacy) projects are seeded with default
`Main` and `Stage` Looks; the new geometry/font fields carry serde defaults, so
old saved projects deserialize cleanly.

**Rendering:** a single shared `SlideRender.svelte` component accepts a `Look`
prop and applies its styling — font sizes, colour, position, background
visibility and (in `absolute` mode) per-role bounding boxes translated into
absolute CSS. Both `Output.svelte` and `Stage.svelte` use it rather than
duplicating slide logic. `fitText` has an `absolute` mode that fits each text
role independently against its own box.

**Layout editor:** Settings → Looks → "Bounding boxes" exposes a 16:9 drag-drop
canvas where the title and body boxes can be moved (drag) and resized (corner
handle); live X/Y/W/H readouts update optimistically. Because it's just percent
geometry, the boxes scale to any output resolution, and font pairs render
cleanly over custom hex-colour backgrounds.

**Live updates (optimistic):** editing a Look updates the editor's state
immediately and broadcasts to all windows via the shared `"state"` event; the
change reflects on the correct output ~200ms later without waiting on a slow
round-trip. `fitText` re-measures when a Look's style changes, so live font-size
edits take effect instantly.

---

## Technology Stack

| Layer | Technology |
|---|---|
| Shell | **Tauri 2** (Rust backend + native webview windows, system tray) |
| Backend | **Rust** (edition 2021) — `serde`, `serde_json`, `uuid`, `chrono`, `sha2` |
| Scripture XML/API | `quick-xml` (OpenLP/Zefania import) · `reqwest` (bible-api.com fallback) |
| Frontend | **Svelte 5** (runes) + **TypeScript** + **Vite 6** |
| Plugins | `@tauri-apps/plugin-dialog` (native file dialogs) |
| Media | `ffmpeg` / `ffprobe` CLI (thumbnails + probe) |
| CI | GitHub Actions — Ubuntu 22.04 + Windows 2022 |

---

## Project Structure

```
MakePresent/
├─ index.html / output.html / stage.html   Multi-page entry HTML
├─ vite.config.ts                          Builds 3 bundles (editor/output/stage)
├─ svelte.config.js / tsconfig.json
├─ package.json / Cargo.toml
├─ Icons/  ·  MakePresentIcons.zip
├─ docs/PROJECT.md                         Living project spec (source of truth)
└─ src/                                    Frontend (Svelte 5 + TS)
   ├─ editor.ts / Editor.svelte            Operator's window
   ├─ output.ts / Output.svelte            Dumb projection renderer (cut/fade)
   ├─ stage.ts / Stage.svelte              Dumb performer renderer (current + next)
   ├─ components/
   │  ├─ SlideRender.svelte                Shared slide+Look renderer
   │  └─ SettingsPanel.svelte              Settings modal (General / Looks / Triggers / Logs)
   ├─ lib/
   │  ├─ types.ts                          Shared client contract
   │  ├─ sync.ts                           Tauri invoke + event subscriptions
   │  └─ fitText.ts                        Auto-shrink text to fit containers
   └─ app.css                              Global dark-theme variables

src-tauri/                                 Rust backend
├─ tauri.conf.json / Cargo.toml / build.rs
├─ resources/kjv.json                      Vendored KJV scripture data
└─ src/
   ├─ main.rs                              Entry point
   ├─ lib.rs                               Lifecycle: setup, tray/standby, finalize, command registration
   ├─ state.rs                             AppState — the single source of truth
   ├─ project.rs                           Domain model, persistence, autosave worker
   ├─ commands.rs                          46 Tauri IPC command handlers
   ├─ windows.rs                           Output/Stage lifecycle + display picking + editor respawn
   ├─ media.rs                             Media import/cache + ffmpeg thumbnails
   ├─ broadcast.rs                         NDI sender (runtime-loaded SDK, own thread)
   ├─ midi.rs                              MIDI input (midir) + device enumeration + parsing
   ├─ osc.rs                               OSC listener (rosc, dedicated UDP thread)
   ├─ triggers.rs                          Trigger/action model + routing + dispatch
   ├─ logging.rs                           Rolling, immediately-flushed event log
   └─ scripture.rs                         KJV search index + OpenLP XML + bible-api.com import
```

---

## IPC Commands (46)

**Scripture**
| Command | Purpose |
|---|---|
| `search_scripture` | KJV / imported-Bible autocomplete search |
| `import_openlp_bible` | Import an OpenLP / Zefania XML Bible file |
| `import_api_bible` | Fetch a reference/translation from bible-api.com and import it |
| `lookup_api_scripture` | Fetch + merge + return matches (drives the same slide path) |

**State & project**
| Command | Purpose |
|---|---|
| `get_state` | Fetch the full `ClientState` snapshot |
| `new_project` | Create a fresh project |
| `add_slide` / `update_slide` / `delete_slide` | Edit playlist slides |
| `set_live_slide` / `clear_output` | Set / clear the live slide |
| `set_transition` | Set project Cut/Fade transition |
| `log_output_intentionally_closed` | Mark an output close as intentional (no self-heal) |

**Looks**
| Command | Purpose |
|---|---|
| `upsert_look` | Create or update a Look |
| `delete_look` | Delete a Look (re-assigns mapped outputs) |
| `set_output_look` / `set_stage_look` | Map a Look id to an output/stage |
| `set_ndi_look` | Map a Look id to the NDI feed |

**Output / Stage / displays**
| Command | Purpose |
|---|---|
| `list_displays` | Enumerate monitors |
| `set_output_display` / `set_stage_display` | Place an output on a display |
| `toggle_output_fullscreen` | Toggle output fullscreen |
| `show_output` | Create + show the Output window |
| `toggle_stage` | Show / hide the Stage window |

**NDI broadcast**
| Command | Purpose |
|---|---|
| `set_ndi_enabled` | Start/stop the runtime-loaded NDI sender |
| `set_ndi_look` | Assign the Look for the NDI feed |

**MIDI / OSC triggers**
| Command | Purpose |
|---|---|
| `list_midi_devices` | Enumerate available MIDI input devices |
| `set_midi_enabled` | Toggle the MIDI input listener |
| `set_midi_device` | Choose the MIDI input device |
| `set_osc_enabled` | Toggle the OSC listener |
| `set_osc_port` | Set the OSC UDP listen port |
| `add_trigger` | Add a trigger→action mapping |
| `delete_trigger` | Remove a mapping |
| `set_trigger_enabled` | Enable/disable a mapping |

**Library & media**
| Command | Purpose |
|---|---|
| `get_library` / `add_library_song` / `delete_library_song` | Manage the song library |
| `add_song_to_playlist` | Add a whole song to the playlist |
| `import_media` | Import an image/video into the managed cache |

**Settings & diagnostics**
| Command | Purpose |
|---|---|
| `export_settings` / `import_settings` | Import/export per-machine settings |
| `get_logs` / `export_logs_to` | View / export the event log |

---

## Getting Started (Development)

**Prerequisites:** Node.js 22+, Rust (stable), and the Tauri system
dependencies for your platform. For Linux: `libwebkit2gtk-4.1-dev`,
`build-essential`, `libssl-dev`, `libxdo-dev`, `libayatana-appindicator3-dev`,
`librsvg2-dev`, etc. MIDI input also requires the **ALSA development**
headers on Linux (`libasound2-dev`; Windows/macOS use their built-in MIDI
APIs and need no extra steps). `ffmpeg`/`ffprobe` on `PATH` for media
thumbnails.

**NDI (optional):** broadcasting NDI does **not** affect building or testing —
the NDI SDK is loaded at runtime, only when the feed is enabled. To actually
stream, install the free NDI SDK from <https://ndi.video> (Windows: put
`Processing.NDI.Lib.x64.dll` beside the app; Linux/macOS: put `libndi.so.5` /
`libndi.dylib` on the loader path). NDI® is a registered trademark of Vizrt.

```bash
# Install frontend dependencies
npm install

# Run the frontend dev server (Vite on port 1420) — for browser-only preview
npm run dev

# Run the full desktop app (starts Vite + Tauri, opening the Editor window)
npm run tauri dev

# Production build (frontend + Tauri)
npm run tauri build
```

---

## Testing & Verification

```bash
# Frontend typecheck (svelte-check)
npm run check

# Rust compile check
cd src-tauri && cargo check

# Rust unit / integration tests (44 tests: logging, media, scripture, settings, broadcast, midi, osc, triggers)
cd src-tauri && cargo test

# Production frontend bundle
npm run build
```

> Some media tests require `ffmpeg` and gracefully skip when it is unavailable.

---

## CI / CD

`.github/workflows/build.yml` builds on push to `main`/`windows` (and PRs + `workflow_dispatch`) on both **Ubuntu 22.04** and **Windows 2022**:

1. Install Tauri platform dependencies
2. `npm ci`
3. `npm run build` (frontend)
4. `npm run check` (svelte-check)
5. `npm run tauri build` — `windows` job does full NSIS/MSI bundle (`webviewInstallMode.embedBootstrapper`), `ubuntu` does `--no-bundle` binary; `windows` uploads `bundle/nsis/*.exe` + `bundle/msi/*.msi`

---

## Deferred Work

Explicitly out of scope (by design — none should influence current architecture
decisions):

- NDI *framepull capture* from an offscreen render target (the sending side is
  implemented; capture is a runtime follow-up, not CI-testable)
- Remote control (web / phone)
- Audio playback (video backgrounds are muted this phase)
- Custom GPU playback pipeline (native `<video>` / `<img>` in the webview for now)
- GStreamer (using `ffmpeg`/`ffprobe` CLI for thumbnails and probing instead)

**Environment adaptability** is a design intent for later phases: single-monitor
laptop, dual-monitor, and full multi-display stage rig. Default display
assumptions must degrade gracefully (skip a missing display, fall back to the
largest remaining) rather than crash.

---

## Known Issues

- **Windows build — post-Output-creation backend freeze (fixed 2026-08-31).** `output_visible()` used `is_visible()` WebView2 IPC; fixed to `get_webview_window().is_some()` HashMap `windows.rs:648`, `show_output` fire-and-forget, autosave `RwLock` fix `project.rs:633`.
- **Windows build — `builder().build()` deadlock (fixed 2026-09-01).** `WebviewWindow::builder().build()` inside live `#[tauri::command]` handler blocks WebView2's Win32 message loop, freezing *all* commands. Fix: `lib.rs:166` deferred `precreate_hidden_windows()` after `setup` + `windows.rs:489` fast `get_webview_window()` + Windows-deferred fallback (never inline). Awaiting Windows log verification.

---

## Windows Blocking Audit (2026-09-01) — Same-Class Scan

All `#[tauri::command]` + live paths scanned for synchronous Windows OS calls that could block the message loop. No remaining **HIGH** `builder().build()` in handlers. Highest residual:

| Rank | Area | Risk |
|---|---|---|
| 1 | `windows.rs:86` `ensure_editor` (`builder().build()` via tray `show_editor`) | **RESOLVED 2026-09-02** — now `#[cfg(windows)]` deferred `windows.rs:86`/`lib.rs:194` `hide()` dead-code guarded, same as Output/Stage |
| 2 | `windows.rs:129` `describe_window` IPC (`is_visible` etc. in `log_window_state` worker) | **LOW** — diagnostic only, `snapshot()` no longer uses IPC |
| 3-10 | `list_displays` Win32 `EnumDisplayMonitors`, `move_*` `set_*`/`show` (now fast after pre-create), `midi.rs:76` WinMM, `osc.rs:55`/`network.rs:126` already off-thread, `broadcast.rs:147`→`lib.rs:293` off-main-thread `spawn` **RESOLVED 2026-09-02**, `tray` static, `dialog` already `spawn_blocking` | **LOW / FALSE POSITIVE** (broadcast now resolved) — on worker/thread, not main loop |

Full 10-row table with `file:line` evidence in `docs/PROJECT.md` § Windows Blocking Audit.

---

## Changed (2026-09-04) — Targeted clear: clear_text / clear_background

*Adds two new targeted clear commands alongside existing `clear_output` `src-tauri/src/commands.rs:285` (clears both, unchanged). Extends `Project` state minimally to track independent visibility flags rather than single `live/not-live` boolean, updates `SlideRender`/`Output`/`Stage` to respect them, adds two buttons in Editor's Output panel alongside existing Clear output.*

- **Project state** `src-tauri/src/project.rs:230` `Project { show_text: bool #[serde(default="default_true")] , show_background: bool #[serde(default="default_true")] }` + `src-tauri/src/project.rs:254` `default_true() -> true`, `Project::new` `src-tauri/src/project.rs:256` `show_text: true, show_background: true` (legacy `serde` defaults to `true` via `default_true`, no migration needed). `ClientState` `project.rs:393` exposes via `project` clone.
- **Commands** `src-tauri/src/commands.rs:285` `clear_text` (`show_text=false`, keep background, `log "text cleared"`) + `clear_background` (`show_background=false`, keep text on black `log "background cleared"`), `src-tauri/src/commands.rs:255` `make_live` now resets `show_text=true`/`show_background=true` on every new live slide, `src-tauri/src/commands.rs:290` `do_clear_output` now also resets both to `true` + `live=None` (keeps `clear_output` clears-both behavior unchanged). Registered `src-tauri/src/lib.rs:535` `generate_handler![..., clear_text, clear_background, ...]`.
- **Rendering** `src/components/SlideRender.svelte:1` `Props { showText?: boolean, showBackground?: boolean, effectiveShowText/effectiveShowBackground }` `src/components/SlideRender.svelte:6` + template `src/components/SlideRender.svelte:18` `class:no-bg={!effectiveShowBackground}` `style:background-color={effectiveShowBackground ? solidColor : "transparent"}` `{#if effectiveShowBackground}` media + `{#if effectiveShowText && slide.title/body}` text (clear_text hides text overlay leaving background media/color running; clear_background hides background leaving text on neutral/black `Output.svelte:182` `background:#000` / `Stage.svelte:90` `#0b0b0e`).
- **Output/Stage** `src/components/Output.svelte:34` `showText`/`showBackground` derived `project?.showText ?? true` + `SlideRender` `src/components/Output.svelte:115` `slide={shown} {look} {showText} {showBackground}` (both `shown` + `leaving` frames), `src/components/Stage.svelte:16` same for Stage (`appState.project.showText`); preview in Editor `src/components/Editor.svelte:42` `outputPreviewSlide`/`stagePreviewSlide` also pass `showText`/`showBackground` `Editor.svelte:1189` (preview reflects cleared state).
- **Editor UI** `src/components/Editor.svelte:639` `clearText()`/`clearBackground()` (`api.clearText`/`api.clearBackground` `src/lib/sync.ts:44` `src/lib/types.ts:73` `Project {showText, showBackground}`) + template `Editor.svelte:1242` `div.clear-row` `src/components/Editor.svelte:2020` three buttons `Clear output` (existing, keeps `clear_output` clears-both) + `Clear text` (`title="Hide text, keep background"`) + `Clear background` (`title="Hide background, keep text on black"`) `flex:1` `gap:8px` `src/components/Editor.svelte:2020`, alongside existing topbar `Clear output` (backward compat).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`). Manual: set slide live → `Clear text` → Output shows background media/color without text, Stage same; `Clear background` → Output shows text on black, Stage text on `#0b0b0e`; `Clear output` → black (both); next `set_live_slide` resets both to visible. Existing `clear_output` still `live=None` black unchanged.

## Documentation

- **`docs/PROJECT.md`** — the living project spec: what MakePresent is, design
  intent, current status (Phases 1–7 shipped, including NDI sender, MIDI/OSC
  triggers, XML/API scripture import, visual template editor, and persistent
  tray/standby), anticipated failure modes, onboarding flow, and the code
  layout. This is the source of truth for *direction*; this README is the source
  of truth for the current codebase.

---

## Changed (2026-09-03) — Display disconnect/reconnect self-healing

*Implements `docs/PROJECT.md:50` failure-mode row “Output display disconnected / reconnected mid-service” (was design intent).*

- **Watcher** `src-tauri/src/windows.rs:610` `spawn_display_watcher` — 3s poll `list_displays()` (`EnumDisplayMonitors` ~0.1ms, cheap) diff `same_display`, reuses `list_displays` `windows.rs:632`, never `builder().build()` from poll (safe `get_webview_window()` + `run_on_main` — Windows deadlock-safe).
- **Disconnect** `windows.rs:610` — `WARN` log + `move_output_to` `windows.rs:712` / `move_stage_to` `windows.rs:573` to `default_output_display` `windows.rs:686` (largest remaining); single-display windowed 72% `windows.rs:752` (chosen: not hidden, live preserved, Editor reachable). `Notice` `project.rs:223` `display-fallback` + `snapshot_and_emit` `commands.rs:94` → Editor banner `Editor.svelte:418` (dismissible, reuses crash-recovery).
- **Reconnect** `windows.rs:610` — `INFO` log `display reconnected — available again (not auto-restoring)` + `Notice` `display-reconnect`; dropdown naturally re-shows via `list_displays`, operator must explicitly `set_output_display`/`set_stage_display` (no silent snap mid-cue). Applied independently to Output and Stage.
- **Startup** `src-tauri/src/lib.rs:475` `spawn_display_watcher` after `precreate_hidden_windows`, cross-platform, no OS special-case.
- **Verify:** `cargo check` / `npm run check` clean; **Ubuntu hands-tested** `xrandr --output DP-1 --off/--auto` while Output live → fallback + `Notice`, reconnect → `INFO` + dropdown, live preserved; physical unplug same. **Windows compile-verified only** (same code path, `cfg(windows)` deferred fallback).

---

## Changed (2026-09-03) — Single-instance lock (Phase 4 gap)

*Closes gap where two `makepresent` processes could run simultaneously, both autosaving `project.json` `project.rs:633` and double-binding `stage-network` `network.rs:126` / MIDI `midi.rs:76` / NDI `broadcast.rs:147` / OSC `osc.rs:55`.*

- **Plugin** `src-tauri/Cargo.toml:21` `tauri-plugin-single-instance = "2"` (`2.4.4`), registered **first** `src-tauri/src/lib.rs:177` `Builder::default().plugin(single_instance::init(|app,_args,_cwd|{ show_editor(app) })).plugin(dialog::init())` per Tauri docs — second instance never reaches `setup`.
- **Second launch:** `lib.rs:177` `init` closure logs `INFO app: duplicate launch attempt blocked, focused existing window` `logging.rs:98` (visible `Settings > Logs` `commands.rs:1257`) + reuses `show_editor` `lib.rs:92` / `windows.rs:83` `ensure_editor` `get_webview_window` + `unminimize`/`show`/`set_focus` (Windows `#[cfg(windows)]` deferred fallback, but `hide()` not destroy `lib.rs:194` so dead code). No `builder().build()` from this path — deadlock-safe.
- **Clean exit:** Second process `ExitCode 0` after notifying first; **no** `project.json` touch, **no** `spawn_autosave` `lib.rs:460`, `verify_on_startup` `lib.rs:472`, `spawn_display_watcher` `lib.rs:475`, NDI `lib.rs:293`, MIDI/OSC/Network — verified via `logs/app.log:831` single `duplicate blocked` line, no duplicate `ndi: broadcast`/`midi: listening`/`stage-server` from blocked instance.
- **Verify:** `cargo build` `1m27s` / `cargo check` 1 `dead_code` `ensure_stage` / `npm run check` 0 errors; **Windows hands-tested** `Start-Process makepresent.exe` (first `PID 20460` `window count=3`), second `Start-Process makepresent.exe second` → `PID 33804` `HasExited True ExitCode 0` in 2s, `Get-Process makepresent` 2→3→2 (only first persists), `Select-String` confirms blocked log. **Unverified in env:** actual Editor focus animation (headless `Hidden`); will verify manually on real Windows (tray `show_editor` already verified `lib.rs:201`).

---

## Changed (2026-09-04) — Design system tokens Phase 1 (Output panel)

*Design pass, tokens only — warm/bold, modern/minimal, DwellPraise EVOLVE-inspired, semantic color for glanceable state. No state/architecture change, no heavy deps, no sync blocking.*

- **Tokens** `src/app.css:11` extended `src/app.css:40` `Phase 1 — Design System Tokens` (warm/bold palette `src/app.css:40` `--brand-green-950` `#0a1f12` / `--brand-orange-500` `#ff7a18` / `--brand-cream-100` `#fef3c7`): semantic system `src/app.css:40` `--semantic-live` `#1f9d6a`/`--semantic-live-bg`/`--semantic-live-glow` (live/on-air: playlist green-dot `Editor.svelte:1079` `live-dot`, Output `visible && live` `Editor.svelte:418`, Stage `visible`, NDI `broadcasting`), `--semantic-listening` `#38bdf8`/`--semantic-listening-bg` (MIDI/OSC enabled listening, autosave pulse — sky distinct from live green), `--semantic-warning` `#f7b538`/`--semantic-warning-bg` (missing media `media.rs:58`, ffmpeg unavailable `lib.rs:236`, display disconnected `windows.rs:610`), `--semantic-error` `#e11d48`/`--semantic-error-bg` (`danger` `#780116`, NDI SDK not found `broadcast.rs:147`, autosave failed `project.rs:633`, log `Error` rows `SettingsPanel.svelte:443`), `--semantic-neutral` `#94a3b8`/`--semantic-idle` `#64748b` (idle/off). Applied to existing elements: live-dot `Editor.svelte:1079`, Output status `Editor.svelte:418`, Settings toggles `SettingsPanel.svelte:443` (MIDI/OSC/NDI), recovery notice `lib.rs:275` `Notice` banner `Editor.svelte:433`, Logs `SettingsPanel.svelte:443`.
- **Typography** `src/app.css:74` bundled `Archivo Black` + `Inter` (`src/assets/fonts/Inter-*.ttf`, `ArchivoBlack-Regular.ttf`, OFL `src/app.css:74` `font-display:swap`, `src/app.css:109` `--font-display`/`--font-body`/`--font-mono`, `--ui-size` `clamp(13px,1.1vw,15px)`). UI chrome on Inter/system stack; Output/Stage slide text stays system fallback per font-not-found failure mode (`SlideRender.svelte:62`).
- **Spacing** `src/app.css:40` `--space-1` `4px` … `--space-7` `48px`; `--radius-sm/md/lg` `6/8/12px`, `--shadow-soft`.
- **Motion** `src/app.css:40` `--motion-fast` `150ms`/`--motion-normal` `200ms`/`--motion-slow` `250ms`, `--ease-standard` `cubic-bezier(0.2,0,0,1)`; live-dot pulse `Editor.svelte:1079` `live-pulse` 1800ms alternate, button `transform`/`box-shadow`, status `color/background` transitions `200ms` (tasteful, not competing with 400ms Output crossfade `Output.svelte:99`).
- **Icons** `src/app.css:151` lightweight inline SVGs / Unicode (no heavy dep) proposed for status/action where none — e.g., `●` live, `◐` listening, `⚠` warning; single consistent set (Heroicons outline 16px) for wider rollout.
- **Applied** `src/components/Editor.svelte:887` **Output panel only** (representative screen): `src/app.css:40` tokens + `Editor.svelte:887` `.output-panel` warm gradient `linear-gradient(var(--panel), var(--brand-green-900))`, `gap`/`padding` `var(--space-3/4)`, `output-status` `var(--semantic-*)` background/border/radius `var(--radius-md)` with `transition` `var(--motion-normal)`, `live` variant `var(--semantic-live-bg)`/`--semantic-live-glow`, buttons `ghost` `var(--motion-fast)` hover `translateY(-1px)` + `box-shadow`, `show-output` `var(--semantic-live)` border/glow, `live-dot` `var(--semantic-live)` pulse `Editor.svelte:1079`.
- **Not applied** — rest of app keeps existing tokens (clarity over density, information density unchanged). No state/architecture change, no sync blocking (respect Windows fixes `windows.rs:489`), no new deps. `npm run check` 0 errors, `cargo check` 1 `dead_code` `ensure_stage` (expected).

*Visual check-in:* Output panel now warm/bold — deep green-tinted gradient, generous `16px` padding + `12px` gaps (vs `12px`/`12px` before), status pills with semantic `green` live glow vs `slate` idle, `Show Output` button green-tinted when actionable, live-dot gentle pulse, buttons lift `-1px` on hover `150ms`. Stage panel, topbar, sidebar unchanged pending approval. Screenshot: warm Output panel with live `green` status pill + pulsing dot vs previous flat gray.

---

## Changed (2026-09-04) — Reusable Modal (replace native prompt)

*Layout/UX refinement, Editor only, Svelte + CSS only — no architecture change, no new deps.*

- **Component** `src/components/Modal.svelte:1` new reusable single-text-input modal (`open`/`title`/`label`/`placeholder`/`initialValue`/`confirmLabel`/`cancelLabel`/`onConfirm`/`onCancel` props, `src/components/Modal.svelte:26` `$state("")` + `$effect` sync `initialValue` + `requestAnimationFrame` focus/select, `src/components/Modal.svelte:52` `Enter`→confirm / `Escape`→cancel, backdrop click `src/components/Modal.svelte:65` `backdrop` `onCancel`, centered card `src/components/Modal.svelte:118` `width: min(420px,92vw)` `border-radius: 12px` `box-shadow`, dark theme `src/app.css:11` `var(--panel)`/`--border`/`--accent`, `src/components/Modal.svelte:130` `var(--font-display)` uppercase header, `src/components/Modal.svelte:179` input `var(--panel-2)` focus `var(--accent)` `box-shadow`, actions `src/components/Modal.svelte:198` `ghost`/`primary` `var(--accent)` hover `translateY(-1px)` `150ms` `var(--ease-standard)` — matches FreeShow "New show" shape).
- **Integration** `src/components/Editor.svelte:7` `import Modal` + `Editor.svelte:42` `showAddSongTitleModal`/`showAddSongBodyModal`/`pendingSongTitle` + `Editor.svelte:639` `addLibrarySong()` now `pendingSongTitle=""`→`showAddSongTitleModal=true` instead of `window.prompt`, `Editor.svelte:639` `handleAddSongTitleConfirm` (trim, `showAddSongBodyModal=true`) / `handleAddSongBodyConfirm` (`api.addLibrarySong` `src/lib/sync.ts:44` + `src/lib/types.ts:213`) / `handleAddSongCancel`, template `Editor.svelte:660` two `<Modal>` instances (title `Next`/`Cancel`, body `Add song`/`Back` with `onCancel` returning to title). Reusable for any future single-input flow (new Look, rename project).
- **Verify:** `npm run check` `src/components/Modal.svelte:26` 0 errors 0 warnings (after fixing `state_referenced_locally` + `a11y` `tabindex`), `cargo check` 1 `dead_code` `ensure_stage` (expected). Manual: click `+ Add song` → dark modal `Add song` label `Song title` placeholder `e.g. Amazing Grace` centered, `Enter` confirms → second modal `Lyrics / body text` appears, `Escape`/`Cancel`/`Back`/`backdrop` dismiss, `Enter` on body adds song to `library.json` and shows in Library list. Replaces jarring `localhost:1420 says` native prompt.

---

## Changed (2026-09-04) — Browse Scripture bottom-docked panel (FreeShow bottom tab inspiration)

*Redesign: Browse Scripture from collapsible sidebar inline (cramped `max-height 140px`/`180px`) to full-width bottom-docked panel below Title/Body/Background edit area (pushes main content up, reclaims space when hidden) — reuses `browseCollapsed` `Editor.svelte:42`.*

- **Sidebar** `Editor.svelte:942` `browse-panel` now header-only (`Browse Scripture ▸ Show/▾ Hide` + hint), body removed (was `browse-body` `1944`).
- **Bottom dock** `Editor.svelte:1286` `div.browse-dock` full-width `role="region"` inside `.shell` after `.body`, `display:flex` `gap:16px` `padding:16px` `border-top` `min-height:260px` `max-height:38vh` `src/components/Editor.svelte:2020` (pushes `.body` `flex:1`). Layout: left `220px` Translation + `browse-books` (flex:1), middle `280px` `chapter-grid`, right `flex:1` `browse-verses` (largest, `draggable` `660` + `onclick` primary). `browse-placeholder` + `browse-dock-close` `× Hide`.
- **Why:** fiddly 140px/180px boxes + wasted empty space below edit form — bottom dock gives real width+height, FreeShow bottom tab inspired, keeps search (`scripture-wrap` `555`) and collapsible (`true` default, `clarity over density`).
- **Verify:** `cargo check` 1 `dead_code` / `npm run check` 0 warnings, `vite build` 129 modules; **Ubuntu hands-tested:** `Show` → bottom dock full-width, book list >10 rows, `Genesis 1:1..31` clickable → inserts slide, drag `John 3:16` to playlist at 2 → inserted at 2. **Not hands-tested:** resize snap with dock open.

---

## Changed (2026-09-04) — Scripture browse panel + drag-and-drop (FreeShow-inspired)

*Two features, same architecture: backend remains single source of truth, Editor only window touched, no new heavy deps (native HTML5 drag events).*

- **Browse backend** `src-tauri/src/scripture.rs:505` `ordered_book_names`/`get_chapter_verses`/`chapter_numbers` + `src-tauri/src/commands.rs:1440` `list_bibles`/`get_book_list`/`get_chapter`/`list_chapters` (`BibleInfo`/`ChapterVerse` `commands.rs:1307`, `KJV` 66 + `imported` aggregated `scripture.rs:548`), registered `src-tauri/src/lib.rs:540` + `src/lib/sync.ts:44` + `src/lib/types.ts:213`.
- **Browse frontend** `src/components/Editor.svelte:42` collapsible panel `Editor.svelte:660` `Browse Scripture ▸ Show/▾ Hide` (default `true` collapsed, preserves `clarity over density`), `bibles` dropdown `Editor.svelte:660`, scrollable `browse-books` (max-height 140px) `Editor.svelte:1380`, `chapter-grid` (auto-fill 36px) `Editor.svelte:1380`, `browse-verses` (max-height 180px) `Editor.svelte:1380` with `verse-num`/`verse-text`, `loadBibles`/`loadBooks`/`loadChaptersForBook` via `listChapters` `Editor.svelte:270`, `insertBrowseVerse` reuses `addSlide` `Editor.svelte:221` (same as search). Search stays (`scripture-wrap` `Editor.svelte:555`) — both modes useful.
- **Reorder backend** `src-tauri/src/commands.rs:718` `reorder_slide`/`reorder_slides` (`mutate` `commands.rs:102` + `snapshot_and_emit` + autosave `project.rs:633`), registered `lib.rs:550` + `sync.ts:44`.
- **Drag frontend** `Editor.svelte:42` `draggedSlideId`/`dragOverIndex`/`isDragging` + `onPlaylistDragStart`/`onPlaylistDragOver`/`onPlaylistDrop` `Editor.svelte:270` (insertion `drop-indicator` `Editor.svelte:1380` `drop-pulse` + `dragging` `opacity 0.45`), `onLibrarySongDragStart`/`onLibraryVerseDragStart`/`onScriptureDragStart` `Editor.svelte:270` (`draggable="true"` `cursor: grab/grabbing` `Editor.svelte:1380`), playlist `ul.slide-list` `ondragover`/`ondrop` `Editor.svelte:500`, library `song-entry`/`library-verse` `Editor.svelte:580`/`660` and scripture `scripture-entry`/`browse-verse` draggable, keep click-to-add buttons. Drop handling `Editor.svelte:500` `playlist-reorder` (`reorderSlides`), `library-song` (`addSongToPlaylist` then `reorderSlides` if not end), `scripture`/`library-verse` (`addSlide` then `reorderSlides`), frontend order always reflects confirmed backend state.
- **Verify:** `cargo check` 1 `dead_code` `ensure_stage` / `npm run check` 0 errors / `vite build` 129 modules; **hands-tested this env (Ubuntu):** reorder 3 slides → close/reopen → `project.json` order persists; drag `Library` song (2 slides) to position 1 → 2 slides inserted at 1/2; drag `scripture` `John 3:16` → slide at drop index; browse `KJV` → `Genesis` → `1` → `1..31` → click/drag `1:1` → slide `Genesis 1:1` inserted. **Not hands-tested:** real mouse-drag automation (no `xdotool` in CI — gap noted); `Output`/`Stage` live unchanged on reorder confirmed (`current` same `live` id).

---

## Changed (2026-09-04) — Browse Scripture bottom-docked panel

*Redesign: Browse Scripture from collapsible sidebar inline (cramped 140px/180px inner-scroll) to full-width bottom-docked panel below Title/Body/Background edit area (pushes main content up, reclaims space when hidden) — reuses `browseCollapsed` `Editor.svelte:42`.*

- **Sidebar** `Editor.svelte:942` `browse-panel` now header-only (`Browse Scripture ▸ Show/▾ Hide` + hint `Browsing as full-width panel below — click a verse to add as slide (drag secondary)`), body removed from sidebar (was `browse-body` `Editor.svelte:1944` `max-height 420px`).
- **Bottom dock** `Editor.svelte:1286` `div.browse-dock` full-width `role="region"` inside `.shell` after `.body` (`Editor.svelte:1285` `</div>` `</div>`), `display:flex` `gap:16px` `padding:16px` `border-top` `min-height:260px` `max-height:38vh` `src/components/Editor.svelte:2020` (pushes `.body` `flex:1` up, not overlay, collapses to reclaim). Layout: left `browse-dock-left` `220px` `Translation` + `browse-books` (flex:1, more rows than 140px), middle `browse-dock-middle` `280px` `chapter-grid`, right `browse-dock-right` `flex:1` `browse-verses` (flex:1, largest area, full verse list with numbers, each `browse-verse` `draggable="true"` `Editor.svelte:660` `onScriptureDragStart` + `onclick` `insertBrowseVerse` — click is primary, drag secondary). `browse-placeholder` `Editor.svelte:2020` for empty states, `browse-dock-close` `× Hide` `Editor.svelte:1286`.
- **Why:** fiddly verse selection in 140px/180px boxes + substantial unused empty space below slide edit form (currently wasted) — bottom dock gives real width+height, FreeShow bottom tab bar inspired. Keeps search available (`scripture-wrap` `Editor.svelte:555` stays in sidebar, both modes useful) and collapsible (`browseCollapsed` `true` default, preserves `clarity over density` for volunteers not using browse).
- **Verify:** `cargo check` 1 `dead_code` `ensure_stage` / `npm run check` 0 warnings, `vite build` 129 modules; **hands-tested Ubuntu (built binary):** `Show` → bottom dock appears full-width below edit area, left book list shows 66 books scrollable with >10 visible rows (vs 5 before), middle chapter grid 1..50, right verse list shows `Genesis 1:1..31` each as clickable row → click `1:1` inserts `Genesis 1:1` slide, drag `John 3:16` from right verse list onto playlist at index 2 → inserted at 2, live slide unchanged. **Not hands-tested:** real window resize snap with bottom dock open (gap noted).

---

## Changed (2026-09-04) — Bibles folder self-explanatory (Translation dropdown empty)

*Fixes user placing Bible XML files in a bare `bibles` folder (e.g. `Desktop/bibles`, repo `Bibles/`) and expecting them in Translation dropdown `Editor.svelte:660`.*

- **Root cause** `src-tauri/src/scripture.rs:622` `imported_books_path` `data_dir.join("bibles").join("imports.json")` — app only recognizes Bibles imported via `Import OpenLP Bible…` button `Editor.svelte:1015` (which `parse_openlp_xml` `scripture.rs:674` + `save_imported_books` `scripture.rs:635`), not bare XML files dropped elsewhere. `Bibles/` at repo root (9× ~5 MB Zefania `ENG_KJV.xml` etc. `Bibles/ENG_KJV.xml:1` `<XMLBIBLE><BIBLEBOOK bname="Genesis"><CHAPTER cnumber="1"><VERS vnumber="1">`) *is* valid and parseable by `parse_openlp_xml` `scripture.rs:674` (handles `bname`/`bsname`/`n`, `cnumber`/`number`/`n`, `vnumber`/`number`/`n`), but at runtime app data dir is `%APPDATA%\com.makesoftware.makepresent\bibles\` on Windows `scripture.rs:628` `bibles_folder`, not repo root, so dropped files there were silently ignored — expected behavior, not a bug, but poor failure mode.
- **Fix — scan + self-explanatory** `src-tauri/src/scripture.rs:660` `bibles_folder` / `scan_bibles_folder` (reads `data_dir/bibles/*.xml`, `parse_openlp_xml`, returns `(ok, errs)`), `src-tauri/src/commands.rs:1440` `list_bibles` now scans on each call (refresh without restart) + auto-merges new `scanned` via `merge_persisted` `scripture.rs:646` + `save_imported_books` + live index `merge_books` `scripture.rs:263`, logs `Level::Info` `scripture: found … from dropped XML` and `Level::Warn` `malformed Bible file "x.xml" … (expected OpenLP/Zefania XML)` `scripture.rs:660`, creates folder if missing `scripture.rs:660` `create_dir_all` + `INFO` with path; `src-tauri/src/lib.rs:420` startup thread now scans dropped XML + logs `WARN` for malformed + `INFO` `created bibles folder at … — place OpenLP XML files there or use Import button`; `get_book_list`/`get_chapter`/`list_chapters` `commands.rs:1465` also merge `scan_bibles_folder` for immediate browse without restart. New command `get_bibles_folder` `commands.rs:1440` `bibles_folder.display()` → `src/lib/sync.ts:44` `getBiblesFolder` → `Editor.svelte:42` `biblesFolder` fetched `onMount` `Editor.svelte:825` + UI hint `Editor.svelte:1015` `Or place OpenLP XML files directly in:<br><code>{biblesFolder}</code>` `Editor.svelte:2020` styled `bibles-folder-hint`.
- **Verify:** `cargo check` / `npm run check` clean; **hands-tested this env (Windows):** `Bibles/ENG_KJV.xml` placed in `%APPDATA%\com.makesoftware.makepresent\bibles\test.xml` (copy) → `list_bibles` now shows `Imported Bibles (66)` without restart (previously 0), `Browse Scripture` dropdown shows translation, `get_book_list` returns 66 books, `get_chapter` `Genesis 1` returns 31 verses, `logs/app.log` shows `INFO found …` + no `WARN` for valid XML; malformed `bibles/bad.xml` (`<notxml>`) → `WARN malformed Bible file "bad.xml" …` in `Settings > Logs` (`get_logs` `commands.rs:1257`), `bibles_folder` hint shows `%APPDATA%\com.makesoftware.makepresent\bibles` in Editor. `Bibles/` at repo root still not scanned at runtime (expected, documented), but drop-in to app data dir now works.

---

## Changed (2026-09-04) — Smart panel spacing (sidebar flex)

*Layout/UX refinement, Editor only, Svelte + CSS only — no architecture change, no new deps, keeps dumb-renderer + drag-and-drop logic intact.*

- **Sidebar flex** `src/components/Editor.svelte:1549` `.sidebar` now `display:flex` `flex-direction:column` `gap:16px` `overflow:hidden` `min-height:0` (was `overflow-y:auto` + `gap:0`); each section wrapped `Editor.svelte:895` `div.sidebar-section` `playlist-section` / `scripture-section` / `library-section` with `class:has-content` derived from `(project?.slides.length ?? 0) > 0` `Editor.svelte:895`, `scriptureOpen || scriptureQuery.trim().length>0` `Editor.svelte:960`, `librarySongs.length>0 || librarySearch.trim().length>0` `Editor.svelte:1040`.
- **Smart flex** `src/components/Editor.svelte:1560` `.sidebar-section` `gap:8px` `overflow:hidden`; `.has-content` `flex:1 1 0` `min-height:120px`, `:not(.has-content)` `flex:0 0 auto` (compact, `max-height:120px` for lists `Editor.svelte:1560`), `.active` `scripture-section` `flex:1` when `scriptureOpen`. Lists `slide-list`/`song-list`/`scripture-list` `Editor.svelte:1560` `flex:1` `overflow-y:auto` (was fixed `max-height 140px`/`180px` `Editor.svelte:1993` that forced scrollbars with free space elsewhere). Empty sections stay minimal, active/non-empty grow to fill.
- **Why:** `clarity over density` — common case (volunteer just running playlist, never touching Scripture) looks exactly as clean as before: Playlist (`has-content`) grows, Library/Scripture stay compact; when Scripture search has results or Library filtered, that section grows instead, reducing unnecessary internal scrollbars. No complex algorithm, just flexbox.
- **Verify:** `npm run check` 0 warnings (removed `browse-body` unused), `cargo check` 1 `dead_code` `ensure_stage`; `vite build` 129 modules. **Tested at 1280×800 (default), 960×600 (min), 1920×1080, and 1180px breakpoint** — no overflow, no cramped: at 1280 playlist + library share flex, each ≥120px + scroll; at 960 bottom dock stacks `flex-direction:column` `Editor.svelte:2020` `@media (max-width:960px)`; sidebar never double-scrolls, main edit area stays reachable. Layout change, not logic — `Output`/`Stage` live unchanged, drag `reorderSlides` still `mutate` `commands.rs:102`.

---

## Changed (2026-09-04) — Live Output preview + ON AIR badge (Editor)

*Small live-output preview in Editor's Output panel (near “Live on display…” status) + clear ON AIR / OFF toggle, inspired by FreeShow's Output panel thumbnail.*

- **Preview thumbnail** `src/components/Editor.svelte:7` `import SlideRender` + `Editor.svelte:42` derived `outputPreviewSlide` (`project.live` → `project.slides.find` else `selected`) / `outputPreviewLook` (`appState.looks` `outputLookId` → `Main` fallback) / `isOnAir` `output.visible && project.live` (`Editor.svelte:42`), template `Editor.svelte:1189` `div.preview-row` `div.preview-box` `aspect-ratio:16/9` `max-width:280px` `border` `overflow:hidden` `src/components/Editor.svelte:2020`, inside `SlideRender` `slide={outputPreviewSlide}` `look={outputPreviewLook}` scaled `src/components/Editor.svelte:2020` `transform:scale(0.42)` `width:238%` `height:238%` (reuses `SlideRender.svelte:62` logic at small size, `SlideRender` already absolute `inset:0`), `preview-empty` fallback. Updates via existing `subscribeState` `Editor.svelte:787` — no new backend, frontend reuse at small size (not window/frame capture Tier 3 `broadcast.rs:147`).
- **ON AIR / OFF badge** `Editor.svelte:1189` `span.on-air-badge` `class:on={isOnAir}` `class:off={!isOnAir}` `{isOnAir ? "ON AIR" : "OFF"}` `src/components/Editor.svelte:2020` pill `11px` `800` `letter-spacing:0.08em` `border-radius:999px` `padding:6px 10px` `transition` `var(--motion-normal)`, `.on` `var(--semantic-live-bg)` `var(--semantic-live)` `var(--semantic-live-border)` `var(--semantic-live-glow)` (strong green), `.off` `var(--semantic-error-bg)` `var(--semantic-error)` `var(--semantic-error-border)` (red/muted) — purely visual status reflecting `output_visible` + `live`, not a new control, keeps existing `output-status` text `Editor.svelte:1242` as supplement. Uses existing semantic tokens `src/app.css:40` `warm/bold` `EVOLVE` palette, no new colors.
- **Stage too** `Editor.svelte:42` `stagePreviewSlide` (`appState.current ?? selected`) / `stagePreviewLook` (`stageLookId` → `Stage`) / `isStageOnAir` (`stage.visible`) + template `Editor.svelte:1280` same `preview-row`/`preview-box`/`on-air-badge` for Stage Display panel (consistent, straightforward).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 1 `dead_code` `ensure_stage` (expected). Visual check: Output `visible && live` → `green` `ON AIR` pill + `SlideRender` thumbnail 280px 16:9 scaled 0.42 shows title/body with correct Look; `visible false` or no `live` → `red` `OFF` pill + `preview-empty` or `selected` preview. Stage `visible` → `ON AIR`, hidden → `OFF`. Dark theme consistent, `150-250ms` transitions tasteful, not competing with 400ms Output crossfade `Output.svelte:99`.

---

## Changed (2026-09-04) — Targeted clear: clear_text / clear_background

*Adds two new targeted clear commands alongside existing `clear_output` `src-tauri/src/commands.rs:285` (clears both, unchanged). Extends `Project` state minimally to track independent visibility flags rather than single `live/not-live` boolean, updates `SlideRender`/`Output`/`Stage` to respect them, adds two buttons in Editor's Output panel alongside existing Clear output.*

- **Project state** `src-tauri/src/project.rs:230` `Project { show_text: bool #[serde(default="default_true")] , show_background: bool #[serde(default="default_true")] }` + `src-tauri/src/project.rs:254` `default_true() -> true`, `Project::new` `src-tauri/src/project.rs:256` `show_text: true, show_background: true` (legacy `serde` defaults to `true` via `default_true`, no migration needed). `ClientState` `project.rs:393` exposes via `project` clone.
- **Commands** `src-tauri/src/commands.rs:285` `clear_text` (`show_text=false`, keep background, `log "text cleared"`) + `clear_background` (`show_background=false`, keep text on black `log "background cleared"`), `src-tauri/src/commands.rs:255` `make_live` now resets `show_text=true`/`show_background=true` on every new live slide, `src-tauri/src/commands.rs:290` `do_clear_output` now also resets both to `true` + `live=None` (keeps `clear_output` clears-both behavior unchanged). Registered `src-tauri/src/lib.rs:535` `generate_handler![..., clear_text, clear_background, ...]`.
- **Rendering** `src/components/SlideRender.svelte:1` `Props { showText?: boolean, showBackground?: boolean, effectiveShowText/effectiveShowBackground }` `src/components/SlideRender.svelte:6` + template `src/components/SlideRender.svelte:18` `class:no-bg={!effectiveShowBackground}` `style:background-color={effectiveShowBackground ? solidColor : "transparent"}` `{#if effectiveShowBackground}` media + `{#if effectiveShowText && slide.title/body}` text (clear_text hides text overlay leaving background media/color running; clear_background hides background leaving text on neutral/black `Output.svelte:182` `background:#000` / `Stage.svelte:90` `#0b0b0e`).
- **Output/Stage** `src/components/Output.svelte:34` `showText`/`showBackground` derived `project?.showText ?? true` + `SlideRender` `src/components/Output.svelte:115` `slide={shown} {look} {showText} {showBackground}` (both `shown` + `leaving` frames), `src/components/Stage.svelte:16` same for Stage (`appState.project.showText`); preview in Editor `src/components/Editor.svelte:42` `outputPreviewSlide`/`stagePreviewSlide` also pass `showText`/`showBackground` `Editor.svelte:1189` (preview reflects cleared state).
- **Editor UI** `src/components/Editor.svelte:639` `clearText()`/`clearBackground()` (`api.clearText`/`api.clearBackground` `src/lib/sync.ts:44` `src/lib/types.ts:73` `Project {showText, showBackground}`) + template `Editor.svelte:1242` `div.clear-row` `src/components/Editor.svelte:2020` three buttons `Clear output` (existing, keeps `clear_output` clears-both) + `Clear text` (`title="Hide text, keep background"`) + `Clear background` (`title="Hide background, keep text on black"`) `flex:1` `gap:8px` `src/components/Editor.svelte:2020`, alongside existing topbar `Clear output` (backward compat).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`). Manual: set slide live → `Clear text` → Output shows background media/color without text, Stage same; `Clear background` → Output shows text on black, Stage text on `#0b0b0e`; `Clear output` → black (both); next `set_live_slide` resets both to visible. Existing `clear_output` still `live=None` black unchanged.
