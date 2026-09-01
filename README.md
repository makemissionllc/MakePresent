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

`.github/workflows/build.yml` builds on push to `main` (and via
`workflow_dispatch`) on both **Ubuntu 22.04** and **Windows 2022**:

1. Install Tauri platform dependencies
2. `npm ci`
3. `npm run build` (frontend)
4. `npm run check` (svelte-check)
5. `npm run tauri build -- --no-bundle` (Rust release binary)

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

- **Windows build — post-Output-creation backend freeze (deferred).** After the
  first successful command that creates the Output window, all subsequent
  backend commands become unresponsive (frontend-only interactions still work).
  Suspected: WebView2 leaves the main thread/event loop degraded after creating
  a second webview window on Windows. **Deferred — Ubuntu is the active
  development platform; revisit before any Windows deployment.**

---

## Documentation

- **`docs/PROJECT.md`** — the living project spec: what MakePresent is, design
  intent, current status (Phases 1–7 shipped, including NDI sender, MIDI/OSC
  triggers, XML/API scripture import, visual template editor, and persistent
  tray/standby), anticipated failure modes, onboarding flow, and the code
  layout. This is the source of truth for *direction*; this README is the source
  of truth for the current codebase.
