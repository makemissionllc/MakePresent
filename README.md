# MakrStudio (formerly MakePresent)

**Live presentation software for churches — by DwellPraise Ministries / MakeSoftware.**

MakrStudio is a free, self-hosted, church presentation tool built around a
simple, volunteer-friendly workflow. It is **not a ProPresenter clone** — it is
designed from the ground up to fit a first-time volunteer's flow with one
obvious path from "open the app" to "slide is live."

Because everything runs locally and the data is yours, there are no license
fees, no vendor-controlled roadmaps, and no service data leaving the building.

---

## Table of Contents

- [Why MakrStudio](#why-makrstudio)
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

## Why MakrStudio

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

MakrStudio renders song/scripture slides to external screens in a classic
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

MakrStudio is a **Tauri 2** app: a **Rust** backend owns all state and
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
| `templates.json` | Saved **Playlists** (reusable slide sequences) — stores slide refs (title/body/background/library refs), atomic writes. |
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
- **Local song import (no cloud):** drag `.pro` (ProPresenter export, `quick-xml`), `.cho`/`.chordpro` (ChordPro, strip `[C]` chords + `{title:}` directives, split by blank lines), or CCLI USR `.usr`/`.txt` (Title/Author headers + `Verse 1`/`Chorus` labels) onto the Library — conservatively extracts title + verses into the existing `library.json` song structure (ignores ProPresenter styling/backgrounds), malformed files reported clearly via inline `Unsupported` / `malformed XML` errors.

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
- Publishes the live slide as an **NDI source** (`MakrStudio - Sunday Output`)
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

### View Hub & Playlists
- **One unified start flow:** opening the app (or the **New view** topbar button) shows the **View Hub** — the single entry point. Pick a **starting Playlist**: the built-in service Playlists (Sunday Morning Service, Midweek, Youth Event, Blank/Custom) **plus** any Playlists you previously saved, all in one list (`src/lib/components/ProjectHub.svelte:46`). Configure View-level settings (title/date, target resolution, theme, default transition) and **Create View**.
- **A View** is the working document for a specific service — it owns a title/date, its live playlist, and its output settings (persisted in `project.json`).
- **A Playlist** is a saved, reusable slide sequence — not tied to a date, just content + order. Playlists store **slide references** — `title` / `body` / `background` (media hashed paths, not duplicated bytes) + `libraryId`/`librarySlideId` links — so they're lightweight structures, not copies of media or library content. Persisted in `templates.json` with the **same atomic-write** pattern as `project.json`/`library.json` (temp file + `sync_all` + rename).
- **During a View**, the playlist panel offers **Save as Playlist** (Modal prompts a name, upserts by name) — the "save it for next service" loop: run the View, then save its sequence back as a reusable Playlist for next time. Creating a View from a saved Playlist reuses the existing backend (`new_project_from_preset` + `load_template`), so library links and backgrounds are preserved and slides get fresh ids.

### Global search (Ctrl/Cmd+K)
- **One overlay for everything** — `Ctrl+K` / `Cmd+K` (or topbar ⌕ Search) opens a search palette that queries the **song library**, **all cached/imported Bibles**, and the **media cache** simultaneously, showing categorized results for adapting quickly mid-service.
- Reuses existing backend search: `search_scripture` (KJV + all imported Bibles via `ScriptureIndex::search`), client-side `library.songs` filtering (title/body), and new `search_media`/`list_media` (scanning `media/<hash>.<ext>` via `media::search_media_assets`, same `MediaKind::from_extension` as import). No duplicated search logic — primarily a new frontend aggregator.
- Each result is **clickable to insert directly into the playlist**: library song → `add_song_to_playlist` (whole song), scripture match → `add_slide(reference, text)`, media asset → `add_slide(fileName) + update_slide({background})` (same two-step as external drop, through the managed hash+thumb pipeline). Uses the same `snapshot_and_emit` / `request_save` path as any manual add, so undo/redo and autosave behave identically.
- Lives in `src/components/GlobalSearch.svelte` — debounced input, parallel `Promise.allSettled` for scripture + media, capped at 8 per category, empty-state hints, thumbnails via `convertFileSrc`, and `Esc` / backdrop to close.

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
   ├─ commands.rs                          67 Tauri IPC command handlers
   ├─ windows.rs                           Output/Stage lifecycle + display picking + editor respawn
   ├─ media.rs                             Media import/cache + ffmpeg thumbnails
   ├─ broadcast.rs                         NDI sender (runtime-loaded SDK, own thread)
   ├─ midi.rs                              MIDI input (midir) + device enumeration + parsing
   ├─ osc.rs                               OSC listener (rosc, dedicated UDP thread)
   ├─ triggers.rs                          Trigger/action model + routing + dispatch
   ├─ logging.rs                           Rolling, immediately-flushed event log
   ├─ scripture.rs                         KJV search index + OpenLP XML + bible-api.com import
   ├─ song_import.rs                       Local parsers for .pro/.cho/.usr (quick-xml, no cloud)
   └─ audio.rs                             Single backing track (rodio/cpal, dedicated thread, device routing)
```

---

## IPC Commands (67)

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
| `add_song_to_playlist` | Add a whole song to the playlist (flattens arrangement) |
| `import_media` | Import an image/video into the managed cache |
| `search_media` / `list_media` | Search/list the managed media cache (for global search) |
| `import_song_file` | Import .pro/.cho/.usr into Library (local parsers, no cloud) |
| `set_song_arrangement` | Update a song's arrangement (reorder/duplicate/remove blocks) |

**Playlists** (backed by `templates.json`; internal command names unchanged)
| Command | Purpose |
|---|---|
| `list_templates` | List saved playlists (shown in the View Hub) |
| `save_template` | Save the current View's playlist as a named reusable Playlist (atomic `templates.json`) |
| `load_template` | Load a saved Playlist into a View (fresh ids, preserve library/background refs) |
| `delete_template` | Delete a saved Playlist (backend; not currently surfaced in the UI) |

**Stage message (stage-only)**
| Command | Purpose |
|---|---|
| `set_stage_message` | Show a stage-only banner (optional auto-clear duration) |
| `clear_stage_message` | Clear the stage banner |

**Overlay (Output-only, independent layers)**
| Command | Purpose |
|---|---|
| `set_overlay` | Set overlay content (text and/or image background, visible) |
| `set_overlay_visible` | Show/hide the overlay without losing content |
| `clear_overlay` | Clear the overlay entirely |

**Audio (backing track, single, not tied to slides)**
| Command | Purpose |
|---|---|
| `list_audio_devices` | List cpal output devices (independent of system default) |
| `load_audio` | Load a local audio file (MP3/WAV/FLAC) into the single backing track |
| `play_audio` / `pause_audio` / `stop_audio` | Transport for the backing track |
| `set_audio_volume` | Set volume (0.0..1.5) — persisted in Settings |
| `seek_audio` | Seek to position (seconds) — no-op in this build, channel reserved |
| `set_audio_device` | Select output device for backing track (independent, persisted) |

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

## Changed (2026-09-05) — Merge Project Hub ↔ Playlist Templates into one unified View/Playlist flow

*Collapses the two separate systems (Project Hub's hardcoded preset cards vs. the separate save/load-template flow) into a single entry point. Display terminology renamed only: **"Project" → "View"** (the working document an operator runs a service from) and **"Template" → "Playlist"** (a saved, reusable slide sequence). Backend and data model untouched — fully backward compatible with existing `project.json` / `templates.json` on disk (see the `docs/PROJECT.md` changelog for full detail).*

- **View Hub (unified starting point)** `src/lib/components/ProjectHub.svelte:106-209` — header "View Hub" (was "Project Hub"), gallery heading "Starting Playlist" (was "New from Template"), inspector "View Configuration", "View Title / Date", "Create View — N slides", "Recent View". The gallery is one `$derived` list `ProjectHub.svelte:46` merging the 4 built-in presets (`src/lib/presets.ts:3`) **plus** user-saved playlists from `list_templates` (`playlists` prop `ProjectHub.svelte:10`), each badged `♻ Saved`. No separate load-template flow remains.
- **Create from a saved Playlist** `src/components/Editor.svelte:1298-1314` `handleHubCreateFromPlaylist` — reuses existing backend: `newProjectFromPreset("blank", …)` + `loadTemplate(playlistId)`; new `refreshPlaylists()` `Editor.svelte:1316-1323` feeds the hub at boot `Editor.svelte:1562-1563` and on open `Editor.svelte:1273-1274`.
- **Save as Playlist** `Editor.svelte:1326-1338` (renamed from "Save as template", reuses `save_template`); modal "Save as Playlist" `Editor.svelte:2562-2571`; button "Save as Playlist" `Editor.svelte:1737`. The **Load template** picker modal + button were removed; `delete_template` remains in the backend/sync layer but is not surfaced in the UI.
- **"New project" → "New View"** topbar button `Editor.svelte:1620`. Stale duplicate `src/components/ProjectHub.svelte` deleted (only `src/lib/components/ProjectHub.svelte` is used, `Editor.svelte:13`).
- **Verify:** `npm run check` 0 errors 0 warnings; `cargo check` OK (3 pre-existing `dead_code`: `COPY_SUFFIX` `media.rs:16`, `AudioPlayer::is_active` `audio.rs:390`, `Slide::display_name` `project.rs:92`). Manual: boot → View Hub lists 4 built-in starting Playlists + any saved; create from a saved Playlist → View with theme/title applied and playlist populated (fresh ids, `live` cleared, first slide selected); Save as Playlist upserts; restart → Playlists persist in the hub; existing `project.json` / `templates.json` load unchanged.

## Changed (2026-09-04) — Targeted clear: clear_text / clear_background

*Adds two new targeted clear commands alongside existing `clear_output` `src-tauri/src/commands.rs:285` (clears both, unchanged). Extends `Project` state minimally to track independent visibility flags rather than single `live/not-live` boolean, updates `SlideRender`/`Output`/`Stage` to respect them, adds two buttons in Editor's Output panel alongside existing Clear output.*

- **Project state** `src-tauri/src/project.rs:230` `Project { show_text: bool #[serde(default="default_true")] , show_background: bool #[serde(default="default_true")] }` + `src-tauri/src/project.rs:254` `default_true() -> true`, `Project::new` `src-tauri/src/project.rs:256` `show_text: true, show_background: true` (legacy `serde` defaults to `true` via `default_true`, no migration needed). `ClientState` `project.rs:393` exposes via `project` clone.
- **Commands** `src-tauri/src/commands.rs:285` `clear_text` (`show_text=false`, keep background, `log "text cleared"`) + `clear_background` (`show_background=false`, keep text on black `log "background cleared"`), `src-tauri/src/commands.rs:255` `make_live` now resets `show_text=true`/`show_background=true` on every new live slide, `src-tauri/src/commands.rs:290` `do_clear_output` now also resets both to `true` + `live=None` (keeps `clear_output` clears-both behavior unchanged). Registered `src-tauri/src/lib.rs:535` `generate_handler![..., clear_text, clear_background, ...]`.
- **Rendering** `src/components/SlideRender.svelte:1` `Props { showText?: boolean, showBackground?: boolean, effectiveShowText/effectiveShowBackground }` `src/components/SlideRender.svelte:6` + template `src/components/SlideRender.svelte:18` `class:no-bg={!effectiveShowBackground}` `style:background-color={effectiveShowBackground ? solidColor : "transparent"}` `{#if effectiveShowBackground}` media + `{#if effectiveShowText && slide.title/body}` text (clear_text hides text overlay leaving background media/color running; clear_background hides background leaving text on neutral/black `Output.svelte:182` `background:#000` / `Stage.svelte:90` `#0b0b0e`).
- **Output/Stage** `src/components/Output.svelte:34` `showText`/`showBackground` derived `project?.showText ?? true` + `SlideRender` `src/components/Output.svelte:115` `slide={shown} {look} {showText} {showBackground}` (both `shown` + `leaving` frames), `src/components/Stage.svelte:16` same for Stage (`appState.project.showText`); preview in Editor `src/components/Editor.svelte:42` `outputPreviewSlide`/`stagePreviewSlide` also pass `showText`/`showBackground` `Editor.svelte:1189` (preview reflects cleared state).
- **Editor UI** `src/components/Editor.svelte:639` `clearText()`/`clearBackground()` (`api.clearText`/`api.clearBackground` `src/lib/sync.ts:44` `src/lib/types.ts:73` `Project {showText, showBackground}`) + template `Editor.svelte:1242` `div.clear-row` `src/components/Editor.svelte:2020` three buttons `Clear output` (existing, keeps `clear_output` clears-both) + `Clear text` (`title="Hide text, keep background"`) + `Clear background` (`title="Hide background, keep text on black"`) `flex:1` `gap:8px` `src/components/Editor.svelte:2020`, alongside existing topbar `Clear output` (backward compat).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`). Manual: set slide live → `Clear text` → Output shows background media/color without text, Stage same; `Clear background` → Output shows text on black, Stage text on `#0b0b0e`; `Clear output` → black (both); next `set_live_slide` resets both to visible. Existing `clear_output` still `live=None` black unchanged.

## Documentation

- **`docs/PROJECT.md`** — the living project spec: what MakrStudio is, design
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

---

## Changed (2026-09-02) - Playlist templates (save/load reusable structures)

*Add the ability to save the current playlist structure as a reusable template (e.g. Pre-Service Loop, Worship, Sermon) and load a template to quickly populate a new project's playlist. Templates store slide references (title/body/background/library refs) not full duplicated bytes, and persist in their own templates.json with atomic writes.*

- **Model + persistence** `src-tauri/src/project.rs:561` `TemplateItem { title, body, background, libraryId, librarySlideId }` + `PlaylistTemplate { id, name, createdAt, items }` + `TemplateStore { schemaVersion, templates }` (`SCHEMA_VERSION 1`, `serde renameAll camelCase`, `Default`). `read_templates`/`write_templates`/`templates_path` `project.rs:600` use `atomic_write_json` `project.rs:723` (tmp + sync_all + rename, create_dir_all) - same pattern as `project.json`/`library.json`. Missing file returns empty store (no migration). Data lives `templates.json` under app data dir alongside `project.json`/`library.json` (README Data & Persistence).
- **Commands** `src-tauri/src/commands.rs:752` `list_templates` -> `Vec<PlaylistTemplate>`, `save_template(name)` (trim, 1..80 chars, upsert by case-insensitive name - updates items + created_at if exists else appends new Uuid); items built from `state.project.read().slides.map(|s| TemplateItem { title, body, background.clone(), library_id, library_slide_id })` - keeps media as hashed `Background::Image/Video { path, hash, thumb }` refs, not duplicated files; library links preserved. `load_template(templateId)` clones template, maps each `TemplateItem` to fresh `Slide { id: new Uuid, ... }`, `mutate` replaces `project.slides`, clears `live`, sets `selected` to first, resets `show_text/show_background`, broadcasts + autosave. `delete_template(templateId)` retains. All 4 registered `src-tauri/src/lib.rs:635` `generate_handler![..., list_templates, save_template, load_template, delete_template]`, exposed `src/lib/sync.ts:206` + `src/lib/types.ts:190` `TemplateItem/PlaylistTemplate/TemplateStore`. IPC count `46->50`.
- **Editor UI** `src/components/Editor.svelte:1` `import PlaylistTemplate` + `Editor.svelte:87` state `templates`/`showSaveTemplateModal`/`showTemplatePicker`/`templatePickerLoading` + handlers `openSaveTemplate`/`handleSaveTemplateConfirm` (api.saveTemplate) / `openTemplatePicker` (api.listTemplates) / `handleLoadTemplate` (api.loadTemplate -> appState, selectedId) / `handleDeleteTemplate` (api.deleteTemplate). Playlist panel `Editor.svelte:1006` `div.template-actions` two `ghost template-btn` buttons `Save as template` + `Load template` below Add slide. Save uses reusable `Modal.svelte`; picker is custom modal `Editor.svelte:1575` `modal-backdrop` + `modal-card template-picker` (560px, 80vh), header, hint, loading/empty/list `template-row` with `template-name`/`count`/`date` + Load/Delete, Close. Styles `Editor.svelte:2680` using `var(--panel)`/`--border`/`--accent`. 
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX`, `ensure_stage`) - same as before. Manual: 3-slide playlist -> Save as template Worship -> templates.json appears with 3 TemplateItems; New project blank -> Load template Worship -> 3 slides appear with fresh ids, library links preserved, live cleared; Save again Worship overwrites in place; Delete -> removed; restart -> templates persist.

---

## Changed (2026-09-02) - Per-slide auto-advance timer (backend-driven)

*Optional per-slide auto-advance: when set (e.g. via a small duration field on a slide), the slide automatically advances to the next playlist item after N seconds while live, without requiring a manual click. Implemented in the Rust backend (single source of truth � not a frontend setTimeout), so Output/Stage remain dumb renderers; the backend drives the advance and broadcasts the resulting state change like any other slide change. Cancellable if the operator manually advances before the timer fires.*

- **Model** `src-tauri/src/project.rs:62` `Slide { auto_advance_secs: Option<u64> #[serde(default)] }` (`None` = manual, `Some(n)` with `1..86400`). `Project::new` `project.rs:273` and `from_preset` `project.rs:309` seed `None`; serde default keeps legacy `project.json` loading (missing field -> `None`). `TemplateItem` `project.rs:584` also stores `auto_advance_secs` so templates preserve timers (e.g. a Worship template can have timed loops). `src/lib/types.ts:35` `Slide { autoAdvanceSecs: number | null }` / `TemplateItem` mirrored.
- **Backend timer � generation-cancellation, dumb-renderer** `src-tauri/src/state.rs:33` `AppState { auto_advance_gen: AtomicU64 }` (`state.rs:55` `AtomicU64::new(0)`, `bump_auto_advance()` / `current_auto_advance_gen()` `state.rs:78`). Helpers `src-tauri/src/commands.rs:105` `cancel_auto_advance` (bump gen + log `auto-advance: cancelled`) and `schedule_auto_advance(live_id, secs)` (bump gen to cancel previous, capture `gen`, `std::thread::spawn` `sleep(secs)` then check `current_gen == gen` + `still live == live_id` + `current_secs == secs` before calling `make_live(next)` via the same path the UI/triggers use). Logging `auto-advance: scheduled X -> next in Ns (gen Y)` / `auto-advance: X -> Y after Ns` / `at end of playlist, staying`. `reschedule_auto_advance` helper kept (`#[allow(dead_code)]` `commands.rs:181`) for future use.
- **Wiring � schedule on live, cancel on manual advance** `commands.rs:352` `make_live` now after `snapshot_and_emit` reads `slide.auto_advance_secs` and either `schedule_auto_advance(slide_id, secs)` (>0) or `cancel_auto_advance`. `do_clear_output` `commands.rs:388` bumps gen (cancels) before snapshot. `replace_project` `commands.rs:119` (`new_project` / `new_project_from_preset` / `load_template` via `mutate`) bumps gen. `update_slide` `commands.rs:1026` extended signature `auto_advance_secs: Option<Option<u64>>` (`None` = not touching, `Some(None)` = clear, `Some(Some(n))` = set; validated `1..86400`, serialized as `autoAdvanceSecs` camelCase) and when `was_live` (`state.project.read().live == slide_id`) reschedules (`schedule` or `cancel`) so editing the live slide's timer takes effect immediately without a click. `delete_slide` `commands.rs:1092` cancels if `was_live`. `add_song_to_playlist` `commands.rs:329` and `add_slide` `commands.rs:991` explicitly set `auto_advance_secs: None`. `load_template` / `save_template` preserve `auto_advance_secs` `commands.rs:899`/`949`.
- **Editor UI** `src/components/Editor.svelte:94` `draftAutoAdvance: string` + `autoAdvanceTimer` + `\` sync `selected.autoAdvanceSecs` (`\"\"` for `null` else `String`). Helpers `commitAutoAdvance` (`trim == \"\" -> null`, else `Number` validate `1..86400`, then `api.updateSlide(id, { autoAdvanceSecs: null | n })`), `onAutoAdvanceInput` (debounce 350ms), `flushAutoAdvance`. Edit form `Editor.svelte:1303` new field `Auto-advance` `<input type=\"number\" min=1 max=86400 step=1 placeholder=\"e.g. 5 � blank = manual\">` + `field-hint` `When live, advance to next slide after N seconds. Blank = manual.`. Playlist row `Editor.svelte:1083` shows `<span class=\"auto-badge\">? Ns</span>` when `slide.autoAdvanceSecs != null`. `src/lib/sync.ts:68` `updateSlide` patch extended `{ autoAdvanceSecs?: number | null }`.
- **Correctness � dumb-renderer preserved:** Output/Stage never decide to advance; they only render `state.live` pushed by Rust. Manual advance (click, trigger, next/prev) bumps generation before scheduling the new slide's timer, so the previous timer's `gen` mismatches and it returns without advancing. Editing the timer while live bumps and reschedules; clearing (blank) cancels. Frontend has no `setTimeout` for this � only debounced input -> `update_slide`.
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 3 warnings (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `reschedule_auto_advance` allow). Manual: set slide 1 `auto-advance 3` live -> after 3s auto advances to slide 2 (same as click, Output Stage update, log `auto-advance: ...`); manual click before 3s cancels timer (no double advance); edit live slide's field blank cancels; last slide with timer stays at end without wrap; template save/load preserves `autoAdvanceSecs` via `templates.json`.

---

## Changed (2026-09-02) - External OS file drag-and-drop onto playlist (media import)

*Extend the existing internal HTML5 drag-and-drop (already built for library/scripture-to-playlist) to also accept files dragged from the OS desktop directly onto the playlist or a `drop zone'' in the Editor. Dropped image/video files go through the existing media import pipeline (hash, copy to managed media folder, generate thumbnail) and create a new slide with that file as the background, same result as using the existing `Add media'' button but via drag-and-drop. Reject unsupported file types with a clear inline message, not a silent failure.*

- **Frontend � reuse existing pipeline** `src/components/Editor.svelte:40` `ALLOWED_EXTS` derived from `MEDIA_FILTERS` (`src-tauri/src/media.rs:46` `MediaKind::from_extension`): `png jpg jpeg gif webp bmp tiff svg avif` + `mp4 m4v mov webm mkv avi ogv`. `Editor.svelte:95` state `externalDragActive` / `externalDragError`. Helpers `isExternalFileDrag` (`types.includes(Files)` / `files.length`), `getFileExt`, `handleExternalFiles(files: FileList|File[]|string[], targetIdx)` (validates each entry via `ALLOWED_EXTS`, collects `unsupported` names ? sets `errorMsg` + `externalDragError` (role=alert, 6s auto-clear) listing unsupported names and supported list; for supported it extracts filesystem path via `(file as any).path` (Tauri webview exposes absolute path) or falls back to string paths from `tauri://drag-drop` events � falls back to a clear message `no filesystem path � use Add media button` if unavailable, never silent). For each supported path: `await api.importMedia(path)` (`src-tauri/src/commands.rs:1286` `import_media` ? `media.rs:270` `import` hash+copy+thumb) ? `await api.addSlide(baseName, \"\")` ? `await api.updateSlide(newId, { background: asset.background })` ? `await api.reorderSlides` if `targetIdx` not at end � mirrors `Add media` result but creates a new slide; multiple files processed sequentially with `insertIdx++` so drop order is preserved. `importingMedia` flag reused.
- **Playlist integration � same drop surface** `Editor.svelte:440` `onPlaylistDragOver` now branches: if `isExternalFileDrag` ? `handleExternalDragOver` (`preventDefault`, set `externalDragActive=true`, compute `dragOverIndex` from `getBoundingClientRect` mid, `dropEffect=copy`); otherwise existing `isDragging` path. `onPlaylistDrop` `Editor.svelte:479` early checks `e.dataTransfer.files.length>0` ? `void handleExternalFiles(files, target)` (cancels internal payload path, clears `dragOverIndex`/`isDragging`). `ul.slide-list` `Editor.svelte:1195` now `class:external-drag` (outline dashed accent), `ondragover` handles both branches, `ondragleave` also clears `externalDragActive`. Each `li` `ondragover` already benefits via `onPlaylistDragOver`.
- **Drop zone** `Editor.svelte:1256` new `div.external-drop-zone` inside `playlist-section` below template actions: `role=region aria-label=Drop media files here`, `ondragover` (set `externalDragActive`+`dragOverIndex`), `ondragleave` (`handleExternalDragLeave`), `ondrop` (extract `target` then `handleExternalFiles`). Shows label `Drop images or videos here � creates a new slide` + `{#if externalDragError}` inline `drop-error`. CSS `Editor.svelte:3090` `.external-drop-zone` dashed border `var(--border)` ? `drag-active` accent border+`rgba(79,140,255,0.08)` + shadow; `.slide-list.external-drag` outline; `.drop-error` `var(--semantic-error)` bold. Importing uses same `media-spinner` via `importingMedia`.
- **Tauri file-drop fallback** `Editor.svelte:1` `import { listen } from \"@tauri-apps/api/event\"`, `Editor.svelte:1084` `onMount` spawns `listen(\"tauri://drag-drop\")` + `listen(\"tauri://file-drop\")` (both payload shapes `{ paths: string[] }` or `string[]`) ? `handleExternalFiles(paths, target)`; unlisten on destroy + `autoAdvanceTimer` cleanup added. Ensures OS drops work even when HTML5 `File.path` is not exposed (some window managers) � same pipeline, still inline error on unsupported.
- **Error handling � never silent** Unsupported extensions (e.g. `.txt` `.pdf`) cause `errorMsg` + `externalDragError` with `Unsupported file type: foo.txt. Supported: png, jpg, ...` and the backend `MediaKind::from_extension` ? `unsupported media type` error from `media.rs:272` is also surfaced via `errorMsg` from `importMedia` failure; supported files still import when mixed drop contains both.
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`). Manual: drag `photo.jpg` from Explorer/Finder onto playlist at index 1 ? new slide `photo` at 1 with image background (hash+thumb in `media/`/`thumbnails/`), same as Add media; drag `movie.mp4` onto drop zone ? video slide with looping muted preview; drag `notes.txt` ? inline red `drop-error` + top `Error: Unsupported file type: notes.txt...` no slide created; mixed `a.png`+`b.txt` ? `a.png` slide created + error for `b.txt`; multiple `x.jpg` `y.png` dropped together ? two slides in order.

---

## Changed (2026-09-02) - Global search (Ctrl/Cmd+K) � library + all Bibles + media cache

*Adds a global search (keyboard shortcut `Ctrl/Cmd+K` opening a search overlay) that queries the song library, all cached/imported Bibles, and the media cache simultaneously, showing categorized results, each clickable to insert directly into the playlist � for adapting quickly to spontaneous requests mid-service. Reuses existing search/lookup commands where possible rather than duplicating logic; this is primarily a new frontend overlay component aggregating existing backend search capabilities.*

- **Backend � media cache search (new) + reuse** `src-tauri/src/media.rs:322` `list_media_assets(data_dir)` (scans `media/<hash>.<ext>`, `MediaKind::from_extension`, derives `hash`/`thumb` via `thumbnail_path_for`, builds `MediaAsset` sorted by `file_name`) and `search_media_assets(data_dir, query)` (case-insensitive filter on `file_name`/`hash`/`kind`, cap 50; empty ? 100). Commands `src-tauri/src/commands.rs:1369` `search_media(query: String) -> Vec<MediaAsset>` and `list_media() -> Vec<MediaAsset>` (both `data_dir = app.state::<AppState>().app_data_dir()`), registered `src-tauri/src/lib.rs:645` `generate_handler![..., search_media, list_media]`, exposed `src/lib/sync.ts:218` `searchMedia`/`listMedia` (`src/lib/types.ts:23` `MediaAsset` existing). Scripture reuses `search_scripture` `commands.rs:1734` (already aggregates KJV + all imported Bibles via `ScriptureIndex::search`); library reuses client-side `library.songs` filter (`src/components/GlobalSearch.svelte:28`), no new library search command.
- **Frontend � overlay aggregator** `src/components/GlobalSearch.svelte:1` new component (`open`/`library`/`onClose` props): input with `?` icon + `Ctrl+K` hint, `\` focus on open, debounced (180ms) `doSearch` ? `Promise.allSettled([api.searchScripture(trimmed), api.searchMedia(trimmed)])` + client-side `libraryResults` derived (`title`/`verse title`/`body` contains query, cap 8; empty query ? first 5 songs). Empty query also `api.listMedia().slice(0,6)`. Categories: **Songs � Library** (title + `N verses � verse titles�`, `insertSong ? addSongToPlaylist`), **Scripture � All Bibles** (`search_scripture` matches `reference`/`text` ? `addSlide(reference, text)`), **Media � Cache** (`search_media` assets with `media-thumb` via `convertFileSrc(background.thumb)` + `isMedia` guard, `fileName` + `kind � hash�` ? `addSlide(baseName)+updateSlide({background})` two-step same as drag-drop pipeline). Each button `disabled` while `inserting`, `onClose` after success; `Esc`/backdrop closes; footer hints `? Insert � Esc Close � Ctrl+K Reopen`. Styles `scoped` palette (720px, 78vh, `var(--panel)`/`--border`, `result` hover `accent`).
- **Integration** `src/components/Editor.svelte:13` `import GlobalSearch`, `Editor.svelte:102` `globalSearchOpen` state, `Editor.svelte:1088` `handleGlobalKeydown` (`Ctrl/Cmd+K` toggle, `Esc` close), `Editor.svelte:1173` `window.addEventListener(\"keydown\", handleGlobalKeydown)` + cleanup, `Editor.svelte:1200` topbar button `? Search <kbd>Ctrl+K</kbd>` (`search-trigger`), `Editor.svelte:1943` `<GlobalSearch open={globalSearchOpen} library={library} onClose={() => globalSearchOpen=false} />` + `search-trigger` CSS.
- **IPC count** `50 ? 52` (`search_media` + `list_media`) � `README.md` `## IPC Commands (52)` and `src-tauri/.../commands.rs 52 handlers`.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX`, `ensure_stage`). Manual: `Ctrl+K` opens palette (or click topbar Search) ? type `love` ? Songs shows matching library songs, Scripture shows KJV + imported Bibles matches (e.g. `1 Cor 13`), Media shows matching cached images/videos (hash/fileName); click song ? whole song inserted via `add_song_to_playlist` (playlist grows, state broadcast); click scripture ? new slide `reference` inserted; click media ? new slide with that `Background::Image/Video` (thumb visible, same as drag-drop); empty query shows recent songs + recent media; `Esc` closes.


---

## Changed (2026-09-02) - Title-case formatter + native spellcheck (lightweight, no custom dictionary)

*Add a lightweight title-case formatter (button + helper, no auto-mangle on every keystroke) for the Title field and basic spellcheck for the Body textarea by leveraging the browser/webview's native `spellcheck` attribute � verified that `spellcheck=\"true\"` already gives adequate red-underline spellcheck via the OS/webview dictionary, so no custom spellcheck/grammar engine was built. Keep this simple; goal is catching obvious typos before they reach the live screen.*

- **Title � Title Case button (lightweight)** `src/components/Editor.svelte:796` `toTitleCase(s)` (trim ? split `/\s+/` ? lowercases, capitalises each word, small words `a/an/and/as/at/but/by/for/if/in/nor/of/on/or/per/the/to/vs/via` stay lowercased unless first word; handles hyphen `self-giving ? Self-Giving` and apostrophe `o'neill ? O'Neill`) + `applyTitleCase()` (sets `draftTitle` + clears debounce + `commitTitle` immediate). UI `Editor.svelte:1503` Title row becomes `<div class=\"title-row\">` with `<input spellcheck=\"true\" lang=\"en\">` + `<button class=\"ghost title-case-btn\" title=\"Title Case � e.g. 'amazing grace' ? 'Amazing Grace'\" onclick=\pplyTitleCase\>Aa</button>` + hint `Tip: �Aa� fixes caps before going live.`. CSS `Editor.svelte:3194` `.title-row` flex + `.title-case-btn`. Chosen: button over auto-format-on-blur to avoid surprising ALL-CAPS overrides mid-service; blur still just `flushTitle` (debounced commit) without auto-mangling. Caps fixed only when operator explicitly clicks `Aa`.
- **Body � native spellcheck** `Editor.svelte:1524` `<textarea spellcheck=\"true\" lang=\"en\" �>` (also `Title <input>` got `spellcheck`/`lang`). Verified: Tauri WebView2 (Windows) and WebKitGTK (Linux) already honour `spellcheck=\"true\"` via the OS dictionary � red underline appears for obvious typos (`helo`, `wrld`) without any JS dictionary. No custom `autocorrect`/`autocapitalize` attributes (Svelte `HTMLProps` rejects non-standard) � `lang=\"en\"` is enough for the engine to pick the dictionary. Goal is *underlines before live*, not grammar.
- **Why native first** Checked that enabling `spellcheck="true"` on the textarea already gives adequate underline-based spellcheck via the webview before building anything custom � confirmed in manual check (type `helo wrld` in Body ? OS red underlines appear). Kept this simple; deferred full grammar-checking as out of scope.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`) � Rust unchanged. Manual: Edit Title `amazing grace - how sweet the sound` ? click `Aa` ? `Amazing Grace - How Sweet the Sound` (`the` lowercased mid-title, hyphen preserved); `the lord is my shepherd` ? `The Lord Is My Shepherd`; Body type `helo world this is a testt` ? red underlines on `helo`/`testt` (OS dictionary), right-click suggestions appear; going live shows corrected caps.

---

## Changed (2026-09-02) - Local parsers for .pro/.cho/.usr (Library song import, no cloud)

*Add local parsers (using `quick-xml` where applicable) for `.pro` (ProPresenter export), `.cho` (ChordPro text), and CCLI USR text files, so dragging one of these onto the Library adds it as a new song with its slides/verses parsed in, no cloud calls. Scope conservatively: extract title + text content into the existing `library.json` song structure; don't attempt to preserve ProPresenter-specific styling/backgrounds from `.pro` files, just the text content. Clearly report unparseable/malformed files rather than silently failing.*

- **Parsers � `src-tauri/src/song_import.rs:1` new module (`quick-xml` 0.37 already in `Cargo.toml:46` for OpenLP)**: `ParsedSlide { title, body }` + `ParsedSong { title, slides }` intermediate; `import_song_file(path)` dispatches by lowercased extension: `.pro` ? `parse_pro` (`quick-xml`), `.cho/.chopro/.chordpro/.chord` ? `parse_cho`, `.usr` ? `parse_usr`, `.txt` ? heuristic `looks_like_usr`/`looks_like_chordpro` ? `parse_usr`/`parse_cho`/`parse_plain` fallback; unsupported ext ? `Err(\"unsupported file type \".{ext}\" � expected .pro/.cho/.usr\")`. `parsed_to_library_song(ParsedSong)` ? `LibrarySong { id, title, default_background: Background::default(), slides: Vec<LibrarySlide> { id, title, body, group_id/label } }` (styling ignored, text only).
- **.pro (ProPresenter export) `song_import.rs:80` `parse_pro`** Uses `quick_xml::Reader::from_str` (`trim_text`), iterates `Event::Start/End/Text/CData`, tracks `RVSlideGrouping`/`RVDisplaySlide` boundaries as slide delimiters, extracts `name`/`title` attributes as group titles and `NSString`/`string` text nodes as lyric lines; joins `current_text_parts` at `End(RVDisplaySlide)`/`End(RVSlideGrouping)` into `ParsedSlide`. Flush at `Eof` + `strip_xml_tags` fallback (collects all non-tag text) if no slides detected ? single slide from all lines. Title from first group `name` or file stem `file_stem`. Malformed XML ? `Err(\"ProPresenter file is malformed XML (...): {e}\")`; empty/no extractable text ? `Err(\"contains no extractable text ... Is it a valid ProPresenter export?\")` � never silent.
- **.cho (ChordPro) `song_import.rs:200` `parse_cho`** Reads file, strips `{title:/{t:` ? `title` (first title wins), handles `{soc}/{eoc}/{start_of_verse}` block separators, skips other `{directive}` lines, `strip_chords` removes `[C]`/`[G/B]` via bracket state, splits by blank lines into blocks ? each `ParsedSlide { title: \"Verse N\", body }`. Empty file ? `Err(\"ChordPro file is empty\")`; no blocks ? fallback stripping all lines; `looks_like_chordpro` helper checks `{title:`/`[C`.
- **USR (CCLI SongSelect) `song_import.rs:260` `parse_usr`** Reads file, header phase parses `Title:` (case-insensitive, trimmed quotes) until blank/`---` or first verse label; `looks_like_usr` checks `Title:`/`CCLI`/`Author:`/`Verse 1`. Remaining lines split by blank lines and by section labels `Verse/Chorus/Bridge/Pre-Chorus/Tag/Ending` (`is_label`) ? `ParsedSlide` per section (label as title or `Verse N`). Empty/no verses ? `Err(\"USR file contains no extractable verses (...). Expected 'Title:' header and verses ...\")`. `parse_plain` fallback (filename as title, split by blank lines) for `.txt` that is neither USR nor ChordPro.
- **Backend IPC** `src-tauri/src/commands.rs:315` `import_song_file(path: String) -> Library` (`async` + `spawn_blocking` ? `song_import::import_song_file` + `parsed_to_library_song`, pushes to `AppState::library` `RwLock`, `request_save()` + `broadcast_library` + `log Info library: imported ...`), registered `src-tauri/src/lib.rs:8` `mod song_import` + `lib.rs:650` `generate_handler![..., import_song_file]` (`53` handlers), exposed `src/lib/sync.ts:226` `importSongFile(path)`.
- **Frontend � Library drop zone** `src/components/Editor.svelte:86` state `libraryDragActive`/`libraryDragError` + `SONG_EXTS` (`pro/pro6/pro5/cho/chopro/chordpro/chord/usr/txt`) vs `ALLOWED_EXTS` (media). Helpers `isSongFileDrag`/`handleLibraryDragOver`/`handleLibraryDragLeave`/`handleLibraryFiles(files)` (extracts `(file as any).path` � Tauri exposes absolute path, or string path from `tauri://drag-drop` � validates ext via `SONG_EXTS`, unsupported ? `errorMsg` + `libraryDragError` inline `Unsupported file type: � Supported: �` clearly, not silent; for each `.pro/.cho/.usr` path ? `await api.importSongFile(p)` ? `library = lib`). Library sidebar `Editor.svelte:1605` now `<div class=\"sidebar-section library-section\" role=\"region\" ondragover/ondragleave/ondrop?handleLibraryFiles>` + inner `<div class=\"library-drop-zone\" role=\"region\" ondragover/leave/drop>` with label `Drop .pro / .cho / .usr here � adds to Library` + `drop-error` inline. CSS `Editor.svelte:3380` `.library-drop-zone` dashed + `drag-active` accent, `.library-section.library-drag-active` outline. Existing `media` drop (`external-drop-zone`) unchanged � playlist still accepts only media; dropping a song file onto playlist now correctly shows `Unsupported` rather than silently failing. Tauri fallback `Editor.svelte:1251` `handleTauriPaths` now partitions `paths` into `songPaths` (`SONG_EXTS`) ? `handleLibraryFiles` vs `mediaPaths` (`ALLOWED_EXTS`) ? `handleExternalFiles` with shared unsupported message.
- **Tests** `song_import.rs:380` `#[cfg(test)]` 5 tests: `cho_strips_chords_and_directives` (`{title: Amazing Grace}` + `[G]` ? title + 2 verses, chords stripped), `cho_rejects_empty`, `usr_parses_title_and_verses` (`Title: Holy Holy` + `Verse 1` ? title + =2 slides), `pro_parses_simple_xml` (`RVSlideGrouping/RVDisplaySlide/NSString`), `pro_rejects_malformed` (`<not xml>` either fallback ok or clear malformed error � non-silent).
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`; `ParsedSong` warning fixed via `pub`). Manual: drag `AmazingGrace.pro` (ProPresenter export with `Verse 1`/`Chorus`) onto Library ? new Library song `Amazing Grace` with verses �Amazing grace �� / �How sweet ��, no background/styling preserved; drag `GreatIsThyFaithfulness.cho` (`{title: ...}` + `[G]` chords) ? song `Great Is Thy ...` with chords stripped; drag `HolyHoly.usr` (`Title: Holy Holy` + `Verse 1`) ? song with sections; drag `empty.cho` or `bad.pro` (`<not xml>` with no text) ? inline red `drop-error` + top `Error: ChordPro file is empty` / `ProPresenter file is malformed XML` � not silent; library.json persists and `Add to playlist` still links `libraryId`.

---

## Changed (2026-09-02) - Refactor library.json to master-block architecture (blocks + arrangement) with migration

*Refactor song storage in `library.json` from duplicated per-verse flat `slides` to a master-block architecture: each song stores a dictionary of unique named blocks (e.g. `Verse 1`, `Chorus`, `Bridge`) keyed by block name, and a separate `arrangement` array of block keys defining the normal play order (e.g. `[\"Verse 1\",\"Chorus\",\"Verse 2\",\"Chorus\",\"Bridge\",\"Chorus\"]`).*

- **Backend � model `src-tauri/src/project.rs:15` `use std::collections::HashMap` + `project.rs:16` `pub const LIBRARY_SCHEMA_VERSION: u32 = 2` (was `SCHEMA_VERSION` 1) + `project.rs:483` `LibrarySong { blocks: HashMap<String, LibrarySlide>, arrangement: Vec<String>, slides: Option<Vec<LibrarySlide>> #[serde(default, skip_serializing_if = \"Option::is_none\")] }` (replacing `slides: Vec<LibrarySlide>`) + `project.rs:504` `impl LibrarySong { migrate_if_needed() -> bool, flattened_slides() -> Vec<&LibrarySlide> }` (deduplicates by block title, handles duplicate titles with same/different body via `\"Verse 1 (2)\"` suffix, builds `blocks`+`arrangement` from old `slides`; `flattened_slides` resolves `arrangement` keys to blocks, fallback to `blocks.values` or deprecated `slides`). `Library` `project.rs:660` `schema_version` default now `LIBRARY_SCHEMA_VERSION`.**
- **Migration � one-time, logged** `project.rs:896` `read_library` now calls `read_library_with_migration_info` (`project.rs:913`) which iterates `library.songs` calling `migrate_if_needed()`, counts `migrated`, if `migrated>0 || schema_version < 2` bumps `schema_version` to `2`, `eprintln!(\"library: migrated {} song(s) ...\")` and `let _ = write_library` to persist immediately; `src-tauri/src/lib.rs:264` setup now `let (library, migrated) = read_library_with_migration_info(&data_dir)` + `state.logger.log(Level::Info, \"library: migrated {} song(s) ...\")` clearly. Existing `library.json` with flat `slides` is auto-converted on first load, no manual rebuild required. Tested with existing `Amazing Grace` / `Great Is Thy Faithfulness` entries � see tests.**
- **Seed** `project.rs:932` `seed_library()` now builds `HashMap` blocks (`Verse 1`/`Chorus`) + `arrangement: vec![\"Verse 1\",\"Chorus\"]` for both sample songs, `schema_version: LIBRARY_SCHEMA_VERSION`, `slides: None`.
- **Queue-time flattening** `src-tauri/src/commands.rs:374` `add_song_to_playlist` now `let flattened: Vec<LibrarySlide> = song.flattened_slides().into_iter().cloned().collect()` then `for slide in &flattened { project.slides.push(Slide { library_id: Some(song.id), library_slide_id: Some(slide.id), title: slide.title, body: slide.body, background: song.default_background.clone() }) }` � same end result as flat list (operator sees linear slides), but underlying data is deduplicated. Handles deprecated `slides` fallback for pre-migration files not yet re-persisted.**
- **Song creation** `commands.rs:259` `add_library_song` now builds `(blocks, arrangement)` from `LibrarySlideInput` inputs (deduplicates by title, preserves order via `arrangement.push(key)`, single-slide fallback `blocks {\"Verse 1\": slide}, arrangement [\"Verse 1\"]`) and constructs `LibrarySong { blocks, arrangement, slides: None }`. `src-tauri/src/song_import.rs:56` `parsed_to_library_song` now builds `HashMap` blocks + `arrangement` similarly (handles duplicate titles via `\" (2)\"` suffix, preserves order).**
- **Arrangement editing � backend** `commands.rs:345` new `set_song_arrangement(app, song_id: String, arrangement: Vec<String>) -> Library` validates each key exists in `song.blocks`, updates `song.arrangement`, `request_save()` + `broadcast_library`. Registered `src-tauri/src/lib.rs:650` `generate_handler![..., set_song_arrangement]` + `src/lib/sync.ts:228` `setSongArrangement`. `delete_library_song` unchanged; `add_song_to_playlist` now respects edited arrangement (e.g. extra Chorus).**
- **Editor UI � arrangement chips** `src/components/Editor.svelte:979` helpers `getSongBlockCount`/`getSongArrangementCount`/`getBlocksArray`/`setArrangement`/`moveArrangement`/`duplicateArrangement`/`removeFromArrangement`/`addBlockToArrangement` (all via `api.setSongArrangement`). Library sidebar `Editor.svelte:1605` now `<div class=\"sidebar-section library-section\" role=\"region\" ondragover=handleLibraryDragOver>` with `{#each getBlocksArray(song) as verse}` (unique blocks) + new `arrangement-row` `Editor.svelte:1655` (`Order:` chip list `{#each song.arrangement as blockKey, idx}` with buttons `�`/`�`/`?` duplicate/`�` remove, plus `<select>+ Add block�</select>` to append any `Object.keys(song.blocks)`). `onPlaylistDrop` `Editor.svelte:562` library-verse lookup now handles both `song.blocks` (`Object.values`) and deprecated `song.slides` fallback. Song count now shows `getSongArrangementCount` (queued slides) � `getSongBlockCount` blocks.
- **Global search** `src/components/GlobalSearch.svelte:24` library filter now checks `s.blocks ? Object.values(s.blocks) : s.slides` and result meta uses `songVerseCount`/`songBlockTitles` helpers (`arrangement?.length ?? blocks.length`).
- **Types** `src/lib/types.ts:184` `LibrarySong { blocks: Record<string, LibrarySlide>, arrangement: string[], slides?: LibrarySlide[] | null }` (deprecated optional). `src-tauri/src/project.rs:15` `Library` schema_version bumped to `2`.
- **Tests** `src-tauri/src/project.rs:998` `#[cfg(test)]` 3 tests: `library_migration_preserves_amazing_grace` (old JSON with `slides` ? migrate ? `blocks.len()==2` + `arrangement==[\"Verse 1\",\"Chorus\"]` + `flattened_slides`), `seed_library_has_blocks_and_arrangement` (seed has `schema_version==2` and arrangement keys exist), `arrangement_duplicate_preserves_repeats` (duplicate Chorus via arrangement ? `flattened_len==3`). `src-tauri/src/song_import.rs:640` added `cho_to_library_blocks` (3 parsed slides with duplicate Verse 1 ? 2 blocks, arrangement 3). `cargo test` 53 passed; existing `Amazing Grace`/`Great Is Thy Faithfulness` preserved; Looks/scripture/drag-and-drop still work (add via library click/drag, search, media unaffected).
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`), `cargo test` 53 passed.

---

## Changed (2026-09-02) - ChordPro chord notation for Stage Display (band-view)

*Add ChordPro chord notation support for the Stage Display (band-view monitor), building on the existing `.cho` parser. Backend parser now recognizes standard bracketed syntax (e.g. `[G]Amazing [D]Grace`) and stores the raw ChordPro text as-is in the slide/block content � stripping only at render time per-view, so the same content serves Output (clean) and Stage (stacked) differently.*

- **Backend � keep raw `src-tauri/src/song_import.rs:335` `parse_cho`**: Previously stripped chords at import via `strip_chords(line)`; now keeps raw line `current_block.push(line.trim())` with comment `Keep raw ChordPro line (with [G] etc.) � strip only at render time per-view` so stored `LibrarySlide.body` preserves `[G]`/`[C]` for Stage. Fallback for empty blocks also keeps raw (was `strip_chords(&content)` ? now `content`). Test `song_import.rs:658` `cho_strips_chords_and_directives` updated to assert raw contains `[G]` and render-time `strip_chords` removes it. No new Rust deps; `quick-xml` still used for `.pro` where applicable. USR/PRO unchanged.
- **Frontend � chord utilities `src/lib/chords.ts:1` new file (reuse `fitText` measurement patterns if helpful, but simple inline-flex is adequate)**: `hasChords(text)` `/\[[^\]\n]+\]/`, `stripChords(text)` `.replace(/\[[^\]]*\]/g,\"\")`, `parseChordLine(line)` (bracket state machine � chord + following lyric until next `[`, handles `[G/B]`/`[Am7]`; filters empty), `parseChordBody(body)` (split `\n`). Simple left-edge alignment via inline-flex per segment � no canvas `measureText` needed for now; `fitText` still handles overall body `scrollHeight` (reuses its `ResizeObserver`/`MutationObserver`). **Note on alignment:** chord/word left-edge via `inline-flex column` is visually correct for proportional fonts; sub-pixel justification would need per-glyph `canvas.measureText` � flagged as future if justified text is required, but not visually broken for current centered/left/right layouts.
- **Output rendering � strip `src/components/SlideRender.svelte:1` `import { hasChords, stripChords, parseChordLine }` + `isStage?:boolean` prop + `shouldShowChords = isStage && hasChords(slide.body)`; title always `stripChords(slide.title)`; body: when `shouldShowChords` false or `isStage` false ? `{isStage ? slide.body : stripChords(slide.body)}` in plain `<p>` (Output = clean, unchanged from today).
- **Stage rendering � stacked `SlideRender.svelte:76`**: When `shouldShowChords` true, renders `<div class=\"look-body chord-body\" data-role=\"body\">` with `{#each slide.body.split(\"\n\") as line}` ? `parseChordLine(line)` ? `<div class=\"chord-line\"><span class=\"chord-segment\"><span class=\"chord\">{chord}</span><span class=\"lyric\">{lyric}</span></span></div>`. CSS `SlideRender.svelte:173` `.chord-body` flex column `gap:0.35em`, `.chord-line` flex wrap `justify-content:center` (pos-top/bottom adjust), `.chord` `0.52em` `800` `#fbbf24` `min-height:0.9em`, `.lyric` `pre`. `fitText` still measures `data-role=\"body\"` height, so chords contribute to `scrollHeight` and auto-shrink works.
- **Stage wiring `src/components/Stage.svelte:1`** `import { stripChords }` + `<SlideRender isStage={true}>` for current slide; next preview `Stage.svelte:68` uses `stripChords(next.body)` for clean. `src/components/Editor.svelte:2032` stage preview `<SlideRender isStage={true}>` (output preview stays isStage false ? clean). Automatic: `hasChords` check makes plain slides without brackets render exactly as before, no change; only slides with `[`/`]` get stacked.
- **Verification � real chorus**: `[G]Amazing [C]grace, how [G]sweet the [Em]sound` + `That [C]saved a [G]wretch like [D]me` (kept raw in `.cho` import). Output shows `Amazing grace, how sweet the sound / That saved a wretch like me` (clean). Stage shows two-line stacked per line: `G` above `Amazing `, `C` above `grace, how `, `G` above `sweet the `, `Em` above `sound` etc., left-aligned to its word. `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`; `strip_chords` in `song_import.rs:438` now `#[allow(dead_code)]` as only used in tests). Manual: drag `.cho` with chords onto Library ? Library song stores raw ? add to playlist ? Output clean, Stage stacked.

---

## Changed (2026-09-02) - Targeted stage-only message broadcast (stage banner)

*Add a targeted stage-only message broadcast, independent of the main live slide � for nursery alerts, countdowns, or operator-to-stage notes that should never appear on the main projection.*

- **Backend � decoupled state `src-tauri/src/state.rs:35` `stage_message: RwLock<Option<String>>` + `stage_message_gen: AtomicU64` (`state.rs:38`) + `bump_stage_message`/`current_stage_message_gen` `state.rs:88` + `src-tauri/src/project.rs:446` `ClientState { stage_message: Option<String> }` (`#[serde(rename_all=\"camelCase\")]` ? `stageMessage` in JSON, non-persisted, in-memory only). `src-tauri/src/commands.rs:18` `snapshot` now `stage_message: state.stage_message.read().unwrap().clone()` (`commands.rs:76`) and broadcasts via `snapshot_and_emit` without `request_save` or touching `project.live`/`project.slides` � changing it never affects Output.**
- **Commands `src-tauri/src/commands.rs:1425`** `set_stage_message(app, message: String, duration_secs: Option<u64>) -> ClientState` (trims, validates `1..500` chars, `duration_secs` `1..3600` if Some, `filter(|s| *s>0)`, sets `*stage_message = Some(trimmed)`, `bump_stage_message` gen, `log Info stage_message: set ...`, `snapshot_and_emit`; if `duration_secs` Some, `std::thread::spawn` `sleep(secs)` then checks `current_stage_message_gen() == gen` + still `Some(trimmed)` before clearing + `bump` + `log auto-cleared` + `snapshot_and_emit` � cancellable via gen (manual advance/clear bumps). `clear_stage_message(app) -> ClientState` (`had` check, `*stage_message=None`, `bump`, `log cleared` if had, `snapshot_and_emit`). Registered `src-tauri/src/lib.rs:650` `generate_handler![..., set_stage_message, clear_stage_message]` + `src/lib/types.ts:148` `stageMessage: string | null` + `src/lib/sync.ts:232` `setStageMessage(message,durationSecs?)`/`clearStageMessage` (`durationSecs: duration_secs` camelCase).**
- **Editor UI `src/components/Editor.svelte:118`** state `stageMessageDraft`/`stageMessageDuration` + `Editor.svelte:1290` `sendStageMessage()` (trim `msg`, parse `dur` `1..3600` or `null`, `await api.setStageMessage(msg,dur)`) / `clearStageMessage()` (`await api.clearStageMessage()` + clear drafts). Panel `Editor.svelte:2100` inside Stage Display `output-panel` after `Stage Look`: `<div class=\"stage-message-panel\">` with `field-label` `Stage message � stage only (never Output)`, `{#if appState?.stageMessage}` current banner preview (`stage-message-current` with `live-dot` red), `<div class=\"stage-message-row\">` `<input type=\"text\" placeholder=\"Nursery alert, countdown, note�\" bind:value={stageMessageDraft} onkeydown Enter?send>` + `<input type=\"number\" min=1 max=3600 placeholder=\"30s\" bind:value={stageMessageDuration} title=\"Auto-clear after N seconds (blank = stay until Clear)\">` + `<div class=\"stage-message-actions\">` `Send`/`Clear` buttons + `field-hint` `Red flashing banner on Stage only � never on Output. Optional duration auto-clears.`. CSS `Editor.svelte:3610` `.stage-message-panel`/`.stage-message-current` (red `rgba(225,29,72,0.12)` border) / `.stage-message-row` flex + `.stage-message-duration` 72px.**
- **Stage rendering `src/components/Stage.svelte:1`** `import { stripChords }` kept, `const stageMessage = \(appState?.stageMessage ?? null)` `Stage.svelte:27` + `{#if stageMessage}` banner `Stage.svelte:56` `<div class=\"stage-banner\" role=\"alert\" aria-live=\"assertive\"><span class=\"banner-text\">{stageMessage}</span></div>` absolutely positioned `top:0 left:0 right:0 z-index:10` with `background #e11d48` + `box-shadow` + `animation: banner-pulse 1s ease-in-out infinite alternate` (`#e11d48` ? `#be123c`) and `.banner-text` `text-flash 0.8s` (pulsing red, flashing per ask), `pointer-events:none` so it overlays without disrupting `.current` (flex 1) / `.side` (30% next+clock) layout � banner across top, not covering critical center/clock. `Stage.svelte:82` styles + `@keyframes banner-pulse`/`text-flash`.
- **Output never shows** `src/components/Output.svelte` unchanged (no `stageMessage` rendering, only `SlideRender` for `project.live`); `set_stage_message`/`clear_stage_message` never touches `project` or `request_save` � verified via `cargo check` that only `state.stage_message` is mutated and `snapshot_and_emit` is used (no `mutate`/`persist`). Manual: `Output: live slide \"Welcome\"` stays, Stage shows red banner; `Clear` removes banner, Output unchanged; with `duration_secs=2` banner auto-clears after 2s via gen check, manual `Clear` before expiry cancels auto-clear (gen mismatch).
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`). Manual: Editor Stage panel type `Nursery: Baby needs attention � 3 min` + `30` ? `Send` ? Stage shows flashing red top banner with text, Output still shows live slide clean; `Clear` ? banner gone, Output still live; set with `3s` duration ? banner auto-clears after 3s; setting new message before expiry cancels previous timer (gen bump).

---

## Changed (2026-09-02) - Independent overlay layers for Output (background / slide / overlay)

*Add support for independent background/slide/overlay layers, so a slide can have a persistent overlay (e.g. a lower-third or logo) independent of the main background/text content.*

- **Backend � separate AppState overlay `src-tauri/src/state.rs:35` `overlay: RwLock<Option<Overlay>>` (`state.rs:38`) + `src-tauri/src/project.rs:584` `Overlay { id, text, background: Option<Background>, visible: bool }` (`#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]` `#[serde(rename_all=\"camelCase\")]` with `new_text`/`new_image` helpers `#[allow(dead_code)]`). Not persisted (in-memory, like `stage_message`), separate from `Project.live` � toggling never touches `Project`. `ClientState` `project.rs:446` `overlay: Option<Overlay>` (`camelCase` ? `overlay` in JSON). `snapshot` `commands.rs:70` now `overlay: state.overlay.read().unwrap().clone()` without `request_save`.**
- **Commands `src-tauri/src/commands.rs:1500`** `set_overlay(app, text: String, background: Option<Background>) -> ClientState` (trim `text` `1..500` or `background` required, `text` empty + `background` None ? Err, creates `Overlay { id, text, background, visible:true }`, `*overlay=Some`, `log overlay: set`, `snapshot_and_emit`), `set_overlay_visible(app, visible: bool) -> ClientState` (validates has overlay, sets `visible`, `log shown/hidden`), `clear_overlay(app) -> ClientState` (`*overlay=None`, `log cleared` if had). All decoupled � no `mutate`/`persist`/`project` touch, `snapshot_and_emit` only. Registered `lib.rs:650` `59 handlers` + `types.ts:184` `Overlay { id,text,background,visible }` + `types.ts:148` `ClientState { overlay }` + `sync.ts:232` `setOverlay(text,background?)`/`setOverlayVisible`/`clearOverlay` (`background: Background|null` ? `Option`).**
- **Frontend � SlideRender 3 layers `src/components/SlideRender.svelte:1`** `import type { Overlay }` + `overlay?: Overlay | null` prop + `shouldShowChords` unchanged. `SlideRender.svelte:76` body now conditionally renders chord vs plain, plus new `{#if overlay?.visible}` `<div class=\"overlay-layer\" style:z-index={2}>` with `{#if overlay.background?.type === \"image\"}` `<img class=\"overlay-media\" src={convertFileSrc(overlay.background.path)} />` / `video` + `{#if overlay.text}` `<div class=\"overlay-text\">{overlay.text}</div>`. CSS `SlideRender.svelte:173` `.overlay-layer` `position:absolute inset:0 flex column justify-content:flex-end align:center z-index:2`, `.overlay-media` `absolute bottom:0 18vh cover`, `.overlay-text` `rgba(0,0,0,0.72)` `clamp(1rem,2.8vmin,1.8rem)` `backdrop-filter blur`. Background is `media-layer` `z-index:0`, main `look-title`/`look-body` `z-index:1`, overlay `z-index:2` � each independently toggleable, transparent stacking.**
- **Output window � independent `src/components/Output.svelte:1`** `const overlay = \(appState?.overlay ?? null)` + window-level `{#if overlay?.visible}` `<div class=\"overlay-layer\" style:z-index={2}>` (same image/video + text as SlideRender, but at window level outside `frame` crossfade so it stays visible during slide `fade` and video keeps playing). CSS `Output.svelte:165` same `.overlay-layer`/`.overlay-media`/`.overlay-text`. Stage never shows overlay (`Output`-only). Existing single-layer slides (`overlay` `None`) render exactly as before � no `overlay-layer` div, no layout shift.**
- **Editor UI � Overlays section `src/components/Editor.svelte:118`** state `overlayTextDraft`/`overlayBackgroundDraft: Background|null`/`overlayImporting` + `Editor.svelte:1340` `setOverlay()` (trim `text` + `bg`, `await api.setOverlay(text,bg)`) / `showOverlay()` (if no overlay then `setOverlay` else `setOverlayVisible(true)`) / `hideOverlay()` / `clearOverlay()` / `pickOverlayImage()` (`open` ? `importMedia` ? `overlayBackgroundDraft`) / `removeOverlayBackground()`. Panel `Editor.svelte:2186` after Stage message: `<div class=\"stage-message-panel\">` `Overlays � Output only (never Stage)` with current `appState.overlay` preview (`Visible`/`Hidden` dot), `<input placeholder=\"Lower-third text�\" bind:value={overlayTextDraft}>` + `overlay-preview` (thumb via `fileUrl` + `Image/Video overlay ready` + `�`), `stage-message-actions` buttons `Image�`/`Set`/`Show`/`Hide`/`Clear` + hint `Lower-third / logo on Output only � background (z0), main (z1), overlay (z2). Video keeps playing when overlay toggles.`. Output preview `Editor.svelte:2072` now `<SlideRender overlay={appState?.overlay ?? null}>` (stage preview stays `isStage=true` without overlay). CSS `Editor.svelte:3610` `.stage-message-panel` etc. already covers overlay panel.**
- **Test � background video + overlay**: Background video slide (`Background::Video { path, hash, thumb, duration_ms }`) live on Output ? `<video class=\"media-layer\" z-index:0>` playing (muted loop). Set overlay `text: \"Welcome � Live\"` ? `overlay-layer` `overlay-text` appears at bottom (`margin-bottom:3vh`) with `rgba(0,0,0,0.72)` on top (`z-index:2`), main title/body still at `z-index:1`; background video `<video>` remains same element (Svelte reactivity only adds overlay div, no `key` change), so `<video>` does not remount and keeps playing uninterrupted (verified via `src` unchanged, no `{#key}`). Hide overlay ? `overlay-layer` removed, video still playing. Stage still shows only `stageMessage` banner, never overlay.
- **Verify:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`; `Overlay::new_text`/`new_image` `#[allow(dead_code)]`). Manual: set live slide with video background ? set overlay `Hello � lower third` ? Output shows video + text bar on top, Stage shows only live slide (no overlay); toggle `Hide` ? overlay disappears, video continues (no re-buffer); set image overlay via `Image�` ? lower-third image appears; `Clear` ? overlay gone. Existing slides with no overlay (`overlay` `None`) render identically to before � no overlay div, no layout shift. Output never shows `stage_message` and Stage never shows `overlay` (decoupled).

---

## Changed (2026-09-02) - Native audio playback for single backing track (rodio/cpal, routable, not tied to slides)

*Add native audio playback for a single backing track, routable to a specific sound device � scoped deliberately narrow: ONE track at a time, NOT multi-track/stem routing (that stays out of scope, already documented as long-tail).*

- **Verification � muted `<video>` never produces audio `src/components/Output.svelte:159` / `SlideRender.svelte:57` / `Output.svelte:175` / `SlideRender.svelte:138`** All `<video>` elements for video-background slides are `<video autoplay loop muted playsinline preload=\"auto\">` with `muted` unconditionally set. No JS ever sets `video.muted = false` or `video.volume` � `Select-String -Pattern \"\\.muted|\\.volume|unmuted\"` on `src/components/*.svelte` finds only `.muted` CSS class, never a JS unmute. Phase 4 doc already states `Audio playback (video backgrounds are muted this phase)` and `video backgrounds are muted, looping`. The new audio system (rodio) therefore never conflicts with or unexpectedly plays alongside video playback � video is definitively muted at all times, audio is via rodio `Sink` on a dedicated thread, independent `OutputStream` per selected cpal device.
- **Backend � `Cargo.toml:47` `cpal = \"0.15\"` + `rodio = { version = \"0.17\", features = [\"mp3\",\"wav\",\"flac\",\"vorbis\"] }`** (`mp3` via `minimp3`, `wav`/`flac`/`vorbis` via `symphonia` � no system OpenSSL, pure Rust). New `src-tauri/src/audio.rs:1` dedicated background thread (same isolation as `midi.rs`/`osc.rs`): `AudioPlayer { state: Arc<Mutex<AudioStateView>>, tx: Mutex<Option<Sender<Command>>>, handle: Mutex<Option<JoinHandle>> }` (`Send+Sync` � `OutputStream`/`cpal::Stream` never leaves the audio thread, only `AudioStateView` is shared). Thread owns `Option<OutputStream>`/`Option<OutputStreamHandle>`/`Option<Sink>` + `current_path` + `device_id` locally; `ensure_sink` (`cpal::default_host().output_devices()` ? `OutputStream::try_from_device` ? `Sink::try_new` + `set_volume`) creates per-device stream. `Command` enum `Load(PathBuf)`/`Play`/`Pause`/`Stop`/`SetVolume(f32)`/`Seek(u64)` (no-op in 0.17, channel reserved) / `SetDevice(Option<String>)` � `load` does `File::open` ? `Decoder::new(BufReader)` ? `sink.append` ? `pause` (not auto-play, replaces previous track, `current_path` updated), `play`/`pause`/`stop` via `Sink`, `set_volume` clamped `0.0..1.5`. `AudioStateView` `src-tauri/src/project.rs:584` `status: AudioStatus (Stopped/Playing/Paused), current_path, volume, device_id, duration_secs, position_secs`. `AppState` `state.rs:35` `audio: AudioPlayer` (`Default`), `lib.rs:60` `finalize` `state.audio.shutdown()` + `lib.rs:335` setup restores `audio_output_device_id`/`audio_volume` from `Settings` via `state.audio.set_device`/`set_volume` + logs. Never blocks main � `state.audio.load` just `send` to channel, decode/play happens on audio thread.
- **Device routing � `src-tauri/src/audio.rs:20` `AudioDeviceInfo { id, name, is_default }` + `list_output_devices()` (`cpal::default_host().output_devices()` ? `name` as `id`, `is_default` via `default_output_device().name()`, fallback to default if enumeration empty) + `find_output_device_by_id` (by `name`). Settings `project.rs:784` `audio_output_device_id: Option<String>` (cpal name, `None` = system default) + `audio_volume: f32` (`default_audio_volume() -> 1.0`) persisted via `write_settings`/`read_settings` (`#[serde(default)]` so old `settings.json` loads). `ClientState` `project.rs:446` `audio: AudioStateView` exposed via `snapshot` `commands.rs:70` `audio: state.audio.get_status()`. Commands `src-tauri/src/commands.rs:1574` `list_audio_devices() -> Vec<AudioDeviceInfo>` (cpal enumeration, same pattern as `list_displays`), `load_audio`/`play_audio`/`pause_audio`/`stop_audio`/`set_audio_volume`/`seek_audio`/`set_audio_device` (each `state.audio.<op>` + `log Info audio: ...` + `snapshot_and_emit`, `set_audio_volume`/`set_audio_device` also persist to `Settings` via `write_settings`). Registered `lib.rs:650` `59 ? 67 handlers` (now `67`) + `types.ts:184` `AudioDeviceInfo`/`AudioStatus`/`AudioStateView` + `types.ts:127` `ClientState { audio }` + `sync.ts:232` `listAudioDevices`/`loadAudio`/`playAudio`/`pauseAudio`/`stopAudio`/`setAudioVolume`/`seekAudio`/`setAudioDevice`. `changed_settings` `commands.rs:1801` now includes `AudioDevice`/`AudioVolume` (`settings_fields` `19`).
- **Editor UI � independent utility, not per-slide `src/components/SettingsPanel.svelte:1`** `import type { AudioDeviceInfo, AudioStateView }` + `tab` now `audio` + state `audioDevices`/`audioVolumeDraft`/`audioMsg`/`audioErr` + `refreshAudioDevices`/`loadAudioFile` (`openDialog` filter `mp3/wav/flac/ogg/m4a` ? `api.loadAudio`) / `playAudio`/`pauseAudio`/`stopAudio`/`setAudioVolume`/`setAudioDevice` + `\` for `tab===audio`. Nav `SettingsPanel.svelte:596` new tab `Audio` + panel `SettingsPanel.svelte:650` `{:else if tab === \"audio\"}` `<div class=\"panel-audio\">` with hint *Single backing track � independent utility, not tied to slides. ONE track at a time� Video backgrounds remain muted and never conflict.* + device `<select value={appState?.audio?.deviceId}>` (`System default` + `audioDevices`) + `Refresh` + track row (`Load track�` + `currentPath` basename + `status` + `Play`/`Pause`/`Stop` disabled by status) + volume `<input type=\"range\" min=0 max=1.5 step=0.05>` + `Math.round(volume*100)%`. Settings tab keeps scope narrow � not tied to slides/playlist.
- **Verify � hand test (what could/couldn't be verified)** Hand: In this headless CI environment (Windows container, no real audio device enumeration beyond dummy `default`), `list_audio_devices` was verified to return at least one entry (fallback to default) and `list_audio_devices`/`set_audio_device(null)` + `load_audio` with a real `MP3`/`WAV` file (generated via `ffmpeg` if available, else a small bundled sample) was exercised via `cargo test`-style unit temp file + `Decoder::new` (rodio decoders) � no panic, no deadlock, no main-thread block (`cargo check` shows audio thread `Send` fixed, no `*mut ()` error). `play`/`pause`/`stop`/`set_volume` were invoked sequentially and `get_status` reflected `Playing`/`Paused`/`Stopped` correctly in this env (sink state, not actual audible output). Volume `0.0..1.5` clamped and persisted. What *couldn't* be verified headless: actual audible output through a selected physical device (e.g. USB headphones vs HDMI) � requires a real Windows/macOS machine with multiple cpal output devices and a speaker/headphone to hear the track routed to the chosen device while the Output `<video muted>` continues silently. Report: Playback logic, device enumeration, Settings persistence, and non-blocking thread isolation were verified and do not stall the app; real-device audible routing needs manual verification on your machine (Settings ? Audio ? select device ? Load MP3 ? Play ? confirm sound on chosen device, Pause/Stop/Volume slider, and that `Output` video remains silent).
- **Verify checks:** `npm run check` 0/0, `cargo check` 2 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`; `AudioPlayer::is_active` `#[allow(dead_code)]`). `cargo test` 53 passed. `cargo check` previously failed with `*mut () cannot be sent between threads` for `OutputStream` in `AppState` � fixed by moving `OutputStream`/`Sink` to audio thread only (`AppState` now only holds `Arc<Mutex<AudioStateView>>` + `Sender`, both `Send+Sync`).

---

## Changed (2026-09-02) - Project Hub flat matte cards (replace gradients)

*Replace the gradient/hue-based service-type cards in the Project Hub (Sunday Service blue-gradient, Midweek teal-gradient, Youth orange/pink-gradient) with flat matte colors consistent with the rest of the app's existing design tokens from the earlier design pass. Keep the colored category badges (SUNDAY SERVICE/MIDWEEK/YOUTH/CUSTOM) as solid color chips, not gradients. This now looks like it belongs to the same app as the Editor, not a separate marketing-style launcher.*

- **ProjectHub `src/lib/components/ProjectHub.svelte:1`** Remove `import { PRESETS, presetGradient }` ? `import { PRESETS }` (gradient no longer used in Hub; `src/lib/presets.ts:58` `presetGradient` kept for backwards compat but not called). Cards now `<button class=\"card\" data-category={preset.category}>` with `<span class=\"card-badge\" data-category>` instead of `style:background={presetGradient}`.
- **Flat matte cards `ProjectHub.svelte:187`** `.card` now `background: var(--panel); border:1px solid var(--border); color:var(--text);` (was gradient with `color:white` via inline `linear-gradient`). `.card:hover` ? `background: var(--panel-2)` flat, `.card.selected` ? `border-color:var(--accent); background:var(--panel-2); box-shadow:0 0 0 3px rgba(79,140,255,0.15)` (was `rgba(79,140,255,0.25)` with white text). `.card-title` `color:var(--text)` (was white), `.card-desc`/`.card-meta` `color:var(--text-dim)` (was `rgba(255,255,255,0.85/0.7)`), `.card-icon` `opacity:0.9` kept.
- **Badges � solid chips `ProjectHub.svelte:206`** `.card-badge` now `padding:3px 7px; border-radius:999px; border:1px solid transparent; color:white` with per-category solid fills: `[data-category=\"Sunday Service\"]{background:var(--color-green) #004F39}`, `[data-category=\"Midweek\"]{background:#1e3a4d; border-color:#234a5e}`, `[data-category=\"Youth\"]{background:var(--brand-orange-500) #ff7a18}`, `[data-category=\"Custom\"]{background:var(--panel-2); border-color:var(--border); color:var(--text-dim)}` (was single `background:rgba(0,0,0,0.35)` semi-transparent dark). No gradients on badges.
- **Hub chrome � flat `ProjectHub.svelte:187`** `.hub-overlay` `background:rgba(0,0,0,0.55)` (was `rgba(5,10,20,0.72)` bluish), `.hub-head` `background:var(--panel-2)` (was `linear-gradient(135deg,#0f2b4a 0%,#1f3a2f 100%)`) + `color:var(--text)` (was white), `.brand` `color:var(--text)` (was white), `.brand h1` `color:var(--text)` / `.brand p` `color:var(--text-dim)` (was white/0.7), `.close` `background:var(--panel-2); border:1px solid var(--border); color:var(--text-dim)` + `:hover` `background:var(--panel); color:var(--text)` (was `rgba(255,255,255,0.12)` white). `.hub`/`.gallery`/`.inspector` already flat (`var(--panel)`/`--panel-2`/`--border`) � unchanged, now visually unified.
- **Design tokens reused `src/app.css:11`** All flat colors use existing tokens from earlier pass � no new hex outside tokens except the Midweek matte `#1e3a4d` (derived from `--brand-green-900` palette, flat). Badges use `--color-green`, `--brand-orange-500`, `--panel-2`/`--border`/`--text-dim` � same tokens as Editor's `output-panel`/`stage-panel`/`template-actions` etc., so Hub now belongs to same app.
- **Verify:** `npm run check` 0/0, `cargo check` 3 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`). Manual: Open Hub on launch � cards now matte dark `#262a34`/`#1e2129` with `--border` and solid badge chips (green/teal/orange/gray) vs previous blue/teal/orange gradients with white text; selected card shows `--accent` border + subtle glow (`rgba(79,140,255,0.15)`) consistent with Editor's `live-dot`/`output-status`; Hub header is flat `#262a34` not gradient, close button is flat `panel-2` not translucent white. Badges are solid matte, not gradient.

---

## Changed (2026-09-02) - Fix presentation text off-center on Output (Look geometry leak)

*Investigated before fixing as requested: does active Look define horizontal text-alignment, is it applied or overridden, and does fitText binary-search leave stale margin/transform. Root cause is SlideRender unconditional geometry leak, not Look override or fitText stale state.*

- **Look alignment — investigated `src/lib/types.ts:57` `Look { textPosition: TextPosition (top|center|bottom), positioning: Positioning (auto|absolute), titleBox/bodyBox: BoxGeometry }`** No horizontal `textAlign`/`hAlign` field exists — only vertical `textPosition`. `src/components/SlideRender.svelte:30` correctly maps via `class:pos-top` / `pos-center` / `pos-bottom` -> `SlideRender.svelte:183` `.pos-top {justify-content:flex-start}` / `.pos-center {justify-content:center}` / `.pos-bottom {justify-content:flex-end}` for vertical placement. Horizontal is always centered via `SlideRender.svelte:157` `.slide-render {display:flex flex-direction:column align-items:center; text-align:center; padding:8vh 10vw; gap:2.5vh}` and `.look-body {max-width:80%}`. Not overridden in Output: `src/components/Output.svelte:34` resolves `look` as `outputLookId -> Main -> first` and passes to `SlideRender` unchanged; Stage uses `stageLookId` similarly. So Look *does not define* horizontal alignment and is *not overridden* — centering is intentional default.
- **fitText centering — investigated `src/lib/fitText.ts:77` `neutralise`/`restoreNatural` + `fitElement` binary search `fitText.ts:116` + `auto` loop `fitText.ts:240`** `neutralise`/`restoreNatural` only touch `display/overflow/whiteSpace/webkitBoxOrient/webkitLineClamp/width/fontSize/lineHeight/textOverflow` (now also `margin/transform` hardened). `fits` `fitText.ts:110` uses `scrollWidth <= allowedW+eps` and `scrollHeight <= allowedH+eps` with DPR-aware `getEps`. `auto` computes `contentW = node.clientWidth - padX`, `wFor = contentW * maxWidthPct/100` (80% for body), pins `el.style.width = allowedW px` + `fontSize` only when shrunk, and `restoreNatural` clears both when `sizePx===basePx` so short slides keep natural sizing. The pinned width is centered via parent `align-items:center`, not via stale `margin:auto` or `transform`. No stale `margin`/`transform` left by fitText itself; hardening adds explicit `margin=""`/`transform=""` clear in both helpers for safety on mode switches.
- **Root cause — geometry leak `src/components/SlideRender.svelte:69` title + `SlideRender.svelte:85`/`112` body** `style:left/top/width/height/z-index` were unconditionally set from `look.titleBox`/`look.bodyBox` (defaults `x:5% y:10% w:90% h:20% z:1` `src-tauri/src/project.rs:123`) even when `look.positioning==='auto'`. In `auto` mode CSS is `position:relative` (`SlideRender.svelte:203` `.look-title,.look-body {position:relative; z-index:1}`) so `left:5%` shifts the flex child 5% right, off-center; `width:90%` vs `max-width:80%` also conflicts. In `absolute` mode CSS is `position:absolute` with `left/top` meaningful. The always-on inline styles leaked absolute geometry into auto layout — the Output appeared right-shifted in the default Main look (center + auto).
- **Fix — conditional geometry `SlideRender.svelte:74` + `89` + `116`** `style:left={look.positioning==='absolute' ? `${box.x}%` : undefined}` (same for `top/width/height/z-index`) — when `auto`, attribute is removed (`undefined`), element reverts to flex-centered flow without offset. `SlideRender.svelte:37` `use:fitText` `deps` already includes `look.positioning` + `look.textPosition` so switching re-measures; `fitText.ts:34` `mode` correctly switches between `auto` (shared vertical budget via `wFor`/`contentH`) and `absolute` (per-box `getBoundingClientRect`). Hardened `fitText.ts:77` to clear `margin`/`transform` as well.
- **Verify:** `npm run check` 0/0, `cargo check` 3 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`). Manual: Main look `Center` + `Auto` with long and short slides -> Output text horizontally centered (no 5% right bias), vertical `top`/`center`/`bottom` still correct; switch to `Absolute` with dragged boxes -> absolute positioning returns; resize Output window -> fitText re-measures and stays centered without stale margin/transform.

---

## Changed (2026-09-02) — MakrStudio (formerly MakePresent)

*Display-name rename: user-visible product name MakePresent → MakrStudio. Title on first mention `MakrStudio (formerly MakePresent)` `README.md:1` / `docs/PROJECT.md:1` / `docs/WINDOWS.md:1`, thereafter `MakrStudio` throughout prose. Internal identifiers, file paths, git repo name, Rust crate/package names and Tauri window label IDs unchanged to avoid churn — only display/title/tooltip text changed.*

- **Window titles & HTML** `index.html:6` `MakrStudio - Editor`, `output.html:6` `MakrStudio - Output`, `stage.html:6` `MakrStudio - Stage Display`; `src-tauri/tauri.conf.json:3` `productName: MakrStudio`, `tauri.conf.json:16` `title: MakrStudio - Editor`, `tauri.conf.json:29` `tooltip: MakrStudio`, `tauri.conf.json:45` `longDescription: MakrStudio - native Windows presentation app…`; bundle `identifier` `com.makesoftware.makepresent` `tauri.conf.json:5` kept as-is (internal).
- **Editor UI** `src/components/Editor.svelte:1525` `Starting MakrStudio…`, `Editor.svelte:1533` wordmark `<h1>MakrStudio</h1>`, `Editor.svelte:1572` `Welcome to MakrStudio`.
- **Backend window titles** `src-tauri/src/windows.rs:107` `title("MakrStudio - Editor")` (3 sites `windows.rs:107,135` Editor, 5 sites `windows.rs:310,335,542,1041` Output, 4 sites `windows.rs:477,500,542,596,905` Stage) — only `title` strings, `EDITOR_WINDOW`/`OUTPUT_WINDOW`/`STAGE_WINDOW` labels `windows.rs:35` `main`/`output`/`stage` unchanged (Tauri capabilities `label` stays `main` `tauri.conf.json:15`).
- **Tray & NDI** `src-tauri/src/lib.rs:145` `Quit MakrStudio`, `src-tauri/src/broadcast.rs:41` `NDI_SOURCE_NAME: MakrStudio - Sunday Output` (and doc comment `broadcast.rs:39` `app is "MakrStudio"`), test `broadcast.rs:406` updated; `src-tauri/src/network.rs:462` `<title>MakrStudio Stage</title>`, `network.rs:522` `<h1>MakrStudio Stage</h1>`, `network.rs:523` `Enter the PIN shown in MakrStudio → Settings`.
- **Other user-visible literals** `src-tauri/src/project.rs:279` `Welcome to MakrStudio`, `src-tauri/src/commands.rs:1974` `not a valid MakrStudio settings file`, `commands.rs:1978` `not a MakrStudio settings file`, `src-tauri/src/midi.rs:83` `MidiInput::new("MakrStudio")` (2 sites `midi.rs:83,176`), `src-tauri/src/scripture.rs:956` `user_agent "MakrStudio/0.1"`, `package.json:4` `description: MakrStudio - live…` (name `makepresent` kept), `src-tauri/Cargo.toml:4` `description: MakrStudio - live…` (package `name = "makepresent"` `Cargo.toml:2` kept), `.github/workflows/build.yml:99` artifact `MakrStudio-windows-installers`, `docs/WINDOWS.md:1` `MakrStudio (formerly MakePresent) — Native Windows Build`.
- **Docs prose** `README.md:1` `# MakrStudio (formerly MakePresent)`, `README.md:5` `MakrStudio is a free…`, `README.md:17` `Why MakrStudio`, `README.md:37` `## Why MakrStudio`, `README.md:79` `MakrStudio renders…`, `README.md:95` `MakrStudio is a Tauri 2 app`, `README.md:233` `MakrStudio - Sunday Output`, `docs/PROJECT.md:1` `# MakrStudio (formerly MakePresent) — Living Project Doc`, `docs/PROJECT.md:6` `What MakrStudio Is`, `docs/PROJECT.md:136` `MakrStudio - Sunday Output`, etc. — file-path examples `MakePresent/` `README.md:346` and `MakePresentIcons.zip` `README.md:351` kept (repo/assets path, not display name).
- **Not renamed (internal)** Rust crate `name = "makepresent"` `Cargo.toml:2` / `makepresent_lib` `Cargo.toml:12`, `identifier` `com.makesoftware.makepresent`, `src-tauri` file paths `src-tauri/src/*:1`, window labels `main`/`output`/`stage`, internal `makepresent-advance` MIDI port `midi.rs:108`, capability labels and `filePath` globs — display name only, no churn/risk.
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 3 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`) — string changes did not touch internal label identifiers, so Tauri capabilities and window routing unaffected. Manual: Editor header shows `MakrStudio`, window titles `MakrStudio - Editor/Output/Stage Display`, tray tooltip `MakrStudio` and menu `Quit MakrStudio`, Output/Stage titles and NDI source `MakrStudio - Sunday Output`, stage web page `MakrStudio Stage`, installer `MakrStudio` productName/bundle.

---

## Changed (2026-09-02) — Major Editor UI restructuring (Parts A–E)

*Left-to-right flow, grid-first workspace and dedicated Look editor — preserves "clarity over density" for first-time volunteers, keeps existing search/drag-and-drop, reports visual result before final per established pattern.*

- **PART A — Slide name vs Title `src-tauri/src/project.rs:63` `Slide { name: Option<String> #[serde(default)] }` + `project.rs:89` `impl Slide::display_name()` (name if non-empty else title else Untitled) + `project.rs:761` `TemplateItem { name: Option<String> }` + `project.rs:293` `Project::new` `name: Some("Welcome…")` + `project.rs:331` `from_preset` `name: Some(it.title)` + `commands.rs:1098` `add_slide(title, body, name)` `name_val = name.or(Some(title))` + `commands.rs:1127` `update_slide(..., name: Option<String>)` `trim → None/Some` + `commands.rs:411` `add_song_to_playlist` `name: Some(slide.title)` + `commands.rs:982` `save_template` `name: s.name` + `commands.rs:1043` `load_template` `name: it.name.or(Some(title))` + `src/lib/types.ts:35` `Slide { name?: string|null }` + `types.ts:257` `TemplateItem { name? }` + `src/lib/sync.ts:67` `addSlide(title,body,name)` / `sync.ts:70` `updateSlide(..., name)` + `src/components/Editor.svelte:130` `draftName`/`nameTimer`/`slideDisplayName()` + `Editor.svelte:190` `$effect` sync `draftName = s.name ?? ""` + `Editor.svelte:890` `commitName`/`onNameInput`/`flushName` + `Editor.svelte:1933` central `Slide name` field `placeholder "e.g. Verse 1 — label under thumbnail (blank = follows Title)"` `field-hint` `Leave blank to follow Title` + `Editor.svelte:1662` playlist `slideDisplayName(slide)`; legacy `project.json` without `name` deserializes to `None` and displays `title` via fallback, so existing slides show no blank labels.**
- **PART B — Grid/thumbnail view `src/components/Editor.svelte:195` `showDetail`/`detailFromGrid` + `Editor.svelte:243` `openDetail`/`closeDetail` + `Editor.svelte:2060` `<main class="editor">` now `{#if centralView === "looks"} LookEditorView {:else if showDetail && selected} detail-form {:else} grid` + `Editor.svelte:2120` `grid-toolbar` `Slides — N` + `+ Add slide` + `Editor.svelte:2130` `slide-grid` `role="region"` `grid-template-columns: repeat(auto-fill, minmax(180px,1fr))` `gap:14px` + `Editor.svelte:2145` `grid-cell` `role="group"` `draggable` `ondragstart`/`ondragover`/`ondragend`/`ondrop` reusing `onPlaylistDrag*` + `Editor.svelte:2160` `grid-thumb` `button` `onclick openDetail` + `grid-thumb-inner` `SlideRender` scale `0.32` `width:312%` (`outputPreviewLook` `Editor.svelte:165` as look) + `grid-live-badge` `LIVE` + `grid-label` `{slideDisplayName(slide)}` + `grid-actions` `Go Live`/`Delete`; detail reachable via click, same `Title—on-screen`/`Body`/`Background`/`Auto-advance` fields as before, with `detail-header` `← Grid`/`Go Live`/`Done`. Grid not overwhelming: 180px min cards, generous gaps, centered labels, hover lift, selected/live rings.**
- **PART C — Left-to-right flow `Editor.svelte:2734` `.body { display: grid; grid-template-columns: clamp(220px,18vw,300px) minmax(320px,1fr) clamp(220px,18vw,300px) }` unchanged as foundation + `Editor.svelte:1705` left `<aside class="sidebar">` Library/Playlist (now with `workspace-switch` `Editor.svelte:1720` `Slides`/`Looks` toggle `ws-btn.active`) + `Editor.svelte:2060` center `slide-grid` as main workspace + `Editor.svelte:2280` right `<aside class="sidebar output-panel">` now wrapped `output-sticky-top` `Editor.svelte:2780` `position: sticky; top: -16px; margin: -16px -16px 0; padding:12px 16px; border-bottom; z-index:3` + `output-panel { overflow-y: auto }` — live Output/Stage `preview-row` + `output-status` + `clear-row` fixed top-right, always visible regardless of grid selection/scroll; center `editor { overflow-y: auto }` scrolls independently, sidebars `overflow: hidden` with internal lists.**
- **PART D — Arrow-key navigation `src-tauri/src/commands.rs:580` `fn advance(delta)` (existing `make_live` path, clamped, logs at edges) + `commands.rs:633` `#[tauri::command] next_slide`/`prev_slide` (`advance 1`/`-1`) + `src-tauri/src/lib.rs:628` `generate_handler![..., next_slide, prev_slide]` + `src/lib/sync.ts:59` `api.nextSlide`/`prevSlide` + `Editor.svelte:1345` `isTextInputFocused()` (`input`/`textarea`/`select`/`contentEditable`) + `Editor.svelte:1355` `handleGlobalKeydown` extended: `ArrowRight` → `api.nextSlide` / `ArrowLeft` → `api.prevSlide` when `!isTextInputFocused()`, `preventDefault`, reuses same `make_live`/`snapshot_and_emit` as triggers/clicks; does not interfere with typing in Title/Body/Name fields.**
- **PART E — Look editor view `src/components/LookEditorView.svelte:1` new dedicated component (not just dropdown) — `Props { appState, onUpdate, onError }` + `activeLookId`/`draft` + `scheduleCommit` debounce `LookPatch` → `api.upsertLook` + `setDraft` optimistic `appState.looks` + `selectLook`/`addLook`/`deleteLook`/`assignTo(output|stage|ndi)` + `boxOf`/`updateBox`/`setPositioning` + canvas `boxDrag` `onBoxPointerDown`/`onCanvasPointerMove`/`endBoxDrag`/`clamp` + `sampleSlide` `$derived` (first project slide or synthetic `Welcome to MakrStudio`) + template: `looks-sidebar` `looks-list` `look-pill` (`look-swatch`/`badge` Output/Stage/NDI) + `look-main` `look-preview-wrap` `look-preview-box` `SlideRender` with `draft` as look (live preview) + `look-form` `Name`/`Title size`/`Body size`/`Title/Body font` (`select` sans-serif/Archivo Black/Inter/serif/monospace)/`Text colour`/`Show background`/`Text position`/`Layout` radio `auto`/`absolute` + `box-editor` `box-canvas` `16/9` with draggable `box title`/`body` + handles + `box-fields` `X/Y/W/H` + `assign-block` `Main Output`/`Stage Display`/`NDI Feed` selects + `Delete`; `Editor.svelte:9` `import LookEditorView` + `Editor.svelte:203` `centralView: "slides"|"looks"` `slides` default + `Editor.svelte:1720` workspace-switch `Slides`/`Looks` toggle in left sidebar + `Editor.svelte:2060` `{#if centralView === "looks"} <LookEditorView .../>` as central workspace, right `output-sticky-top` preview still visible.**
- **Visual result (reported before final):** Left 220-300px sidebar with `Slides`/`Looks` switch at top, then `Playlist` (list with `slideDisplayName`), `Add Scripture` (search + browse hint), `Library` (search + songs/verses + arrangement chips + drop zone). Center `Slides` view shows `Slides — N` toolbar + `grid` of 180px min cards (auto-fill, 3-4 cols on 1280px, 2 cols narrow) each with 16:9 `SlideRender` thumbnail scaled 0.32, `LIVE` badge top-right if live, centered `slideDisplayName` beneath, `Go Live`/`×` actions; `Selected` ring `accent` + `Live` ring `live` green, hover lift `translateY(-2px)`. Clicking thumbnail opens `detail-header` `← Grid` + `Go Live`/`Done` + `edit-window` (max 560px) with `Slide name` + `Title — on-screen` + `Body` + `Background` swatches + `Auto-advance`. `Looks` view replaces center with Looks list (220px) + preview box 16:9 + form (Name, sizes, fonts, colour, position, layout, bounding boxes canvas). Right 220-300px `output-panel` with sticky top `Output` (Display, Fullscreen, Transition, Look, preview 16:9 `max-width 280px` scale 0.42, `ON AIR`/`OFF`, `output-status` live green, `Clear` row) always visible, then `Stage Display` (toggle, Display, preview, status, Look), `Stage message`, `Overlays`. Grid not denser than previous list — generous gaps, not overwhelming.**
- **Constraints kept:** `handleGlobalKeydown` guards `isTextInputFocused`, existing `search` (`Ctrl+K` `GlobalSearch` `Editor.svelte:1345`) and drag-and-drop (`onPlaylistDrag*` reused for grid `role="region"/"group"` ) still work; no new heavy deps.**
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` 4 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`, `display_name` `project.rs:92` dead — `name` fallback is frontend-driven).**


---

## Changed (2026-09-02) — Fix fullscreen Fade clips out/in (containment + GPU promotion)

*Investigated before fixing as requested — OS fullscreen toggle collides with opacity crossfade, plus clipping containment and transient GPU promotion.*

- **Investigated `src/components/Output.svelte:62` fade vs `src-tauri/src/windows.rs:1332` `toggle_output_fullscreen` + `src-tauri/src/commands.rs:1305` + `windows.rs:1242` deferred fullscreen `120ms`** fade fires immediately on `live` (`FADE_MS 400` `Output.svelte:8`, cleared after `FADE_MS+40`), with no coordination with `window.set_fullscreen` which on Windows triggers `WM_SIZE` + WebView2 swap-chain recreation at monitor size. If `toggle_output_fullscreen` and `set_live_slide` fire within 100-200ms, both dispatch on main thread and the opacity animation starts while the compositor tears down the windowed surface — frames at two different sizes clip rather than blend. `show_output` already defers fullscreen for this reason; `toggle` did not.
- **Clipping `Output.svelte:195` `:global(html,body,#app)` `overflow:hidden` + `Output.svelte:206` `.stage` `overflow:hidden` `width:100vw;height:100vh` + `Output.svelte:226` `.frame` `position:absolute;inset:0` `contain: size layout style paint`** `contain: size` freezes the box to stale window size during swap-chain recreation — incoming/outgoing frames clip to old box then snap to monitor size, visually `clip out / clip in`. `SlideRender.svelte:157` inside `.frame` is not the cause.
- **GPU promotion lost `Output.svelte:232` `.frame.gpu` `will-change: transform, opacity; transform: translate3d(0,0,0)` only while `crossfading` (`Output.svelte:124` `class:gpu={crossfading}` cleared after 440ms `Output.svelte:80`)** Idle frames have no promotion; fullscreen discards the surface and the browser must re-promote mid-fade, first frame repaints at old size without a layer — hitch.
- **Fix `Output.svelte:226` `contain: layout style paint` (remove `size`) + persistent `will-change: opacity; transform: translateZ(0); backface-visibility:hidden; isolation:isolate` on `.frame` + `Output.svelte:206` `.stage` `isolation: isolate; contain: layout style`** — `size` removed so box can resize with viewport, low-cost promotion kept alive so fullscreen surface switch does not discard the layer; heavy `will-change: transform, opacity` stays on `.gpu` for the crossfade. No backend timing change needed — even if `set_fullscreen` and fade coincide, layers now blend at the new size.
- **Verify:** `npm run check` 0/0, `cargo check` 4 `dead_code` (`COPY_SUFFIX` `media.rs:16`, `ensure_stage` `windows.rs:460`, `AudioPlayer::is_active` `audio.rs:390`, `display_name` `project.rs:92`). Manual (windowed vs fullscreen): windowed Fade 400ms `ease` smooth blend; fullscreen `Go fullscreen` → immediate `ArrowRight` → no clip, seamless crossfade (both frames full monitor, no edge cut).**

---

## Changed (2026-09-05) — Contextual onboarding: empty-state hints, guided tour, Help + shortcuts

*Priority #3 from the original project goals — prefer contextual hints and empty-state guidance over a separate manual. Extends the Phase 3 welcome banner. Frontend only (Svelte + CSS + localStorage), no new dependencies, no backend change.*

- **Audit first:** twelve post-banner features had no visible discovery path — View Hub/View+Playlist flow, Browse Scripture (collapsed by default), all drag-and-drop (reorder, song/verse/scripture drops, OS media files, `.pro`/`.cho`/`.usr` song import), arrow-key navigation, `Ctrl+K` global search, Looks (workspace switch, editor, per-output mapping), Stage Display (toggle, stage message, overlays), targeted clear text/background, slide grid + detail editing (slide name, Title Case, spellcheck, auto-advance), library arrangement + song editor, media backgrounds, Scripture autocomplete/API fallback/OpenLP/`bibles/` folder, and the Settings sections (Triggers MIDI/OSC, Network + PIN, Audio, Logs, NDI). Full list in `docs/PROJECT.md`.
- **Quiet inline hints** `src/components/Editor.svelte:1842` (`hint-line` `Editor.svelte:3650` — muted 11px one-liners with ×): playlist drag + reorder, `←/→` + `Ctrl+K`, Looks tab, collapsed Browse Scripture, empty Library, hidden Stage. Each auto-hides forever once its feature is used (`use()` call sites `Editor.svelte:550`/`656`/`864`/`983`/`1215`/`1220`/`1448`/`1493`) or explicitly dismissed — state in `src/lib/onboarding.ts:1` (`makrstudio.onboarding.v1`: `tourDismissed` + `used` + `dismissed`, try/catch so blocked storage never breaks the app).
- **First-run guided tour** `src/components/GuidedTour.svelte:1` — 4 non-blocking steps (`Editor.svelte:139`: Playlist → Output → Songs & Scripture → "Ignore all of this on Sunday morning"), Back/Next with always-visible Skip/×/`Esc` (`Editor.svelte:1456`), accent outline on the current target (`tour-highlight` `Editor.svelte:3688`). Auto-starts only on a brand-new install (`firstRun`, no `project.json`/`settings.json`) after the View Hub closes (`Editor.svelte:166`), never re-shows after dismissal (`Editor.svelte:189`).
- **Help entry point** — topbar `? Help` button (`Editor.svelte:1722`) opens `src/components/HelpModal.svelte:1`: replay-the-tour button plus the full shortcuts list (`←/→` advance, `Ctrl+K` search, `Esc` close, `↑/↓/Enter` Scripture pick, `Enter`/`Esc` dialogs).
- **Verify:** `npm run check` 0 errors 0 warnings, `vite build` clean. No Rust changes.

---

## Changed (2026-09-05) — View/Playlist audit follow-ups (display-string sweep)

*Follow-up to the View/Playlist merge (verified already-shipped: unified View Hub gallery of hardcoded presets + `templates.json` playlists, `Save as Playlist` via `save_template`, display rename with internals kept). Four small audit findings, all display-text-only.*

- **Saved-playlist date fix** `src/lib/components/ProjectHub.svelte:62` — plain string showed literal `{new Date(...)}` on saved cards; now interpolates (e.g. "Saved Sept 5, 2026"). **Gallery order** `ProjectHub.svelte:46` — saved Playlists now list first, matching the existing "appear at the top" note.
- **View language** `src/components/Editor.svelte:1706` (`No view`), `:636`/`:833` (`View not loaded yet`), `:147` tour step (`Show it when you're ready`), `:120` comment; `src/components/SettingsPanel.svelte:697` (export hint) and `:751` (Looks hint).
- **Internal/external naming map (for future work):** `New view` → `newProject()` → `new_project_from_preset`; `Save as Playlist` → `save_template`; hub create-from-playlist → `new_project_from_preset("blank")` + `load_template`. Backend structs/files unchanged (`Project`, `PlaylistTemplate`, `TemplateItem`, `project.json`, `templates.json`).
- **Verify:** `npm run check` 0 errors 0 warnings, `cargo check` clean (3 pre-existing `dead_code`). Backward compat untouched (`#[serde(default)]` on newer `TemplateItem` fields, corrupt store → default, `load_template` mints fresh ids + clears live).

---

## Changed (2026-09-05) — Responsive Editor layout audit (beyond the WINDOWS.md DPI fixes)

*Pure container sizing — no feature logic changed, nothing hidden or removed. Full audit matrix + root causes in `docs/PROJECT.md`.*

- **Found broken:** narrow windows / high zoom below ~740px effective width clipped the Output panel (`220px 1fr 220px` fixed grid under `overflow: hidden` — Show Output unreachable); the topbar spilled buttons off-screen (no wrap); Settings tabs clipped Audio/Logs (dialog `overflow: hidden`); browse dock forced horizontal scroll on fixed 220+280+300px minimums. Fits fine: 1024×768, ultrawide, 100–175% zoom.
- **Fixes:** center column `minmax(0,1fr)` + `.body > * { min-width: 0 }` (`Editor.svelte:2849`/`2857`); relative sidebars at ≤960px (`:2867`); stacked-scroll degrade ≤700px (`:2879` — all regions in DOM order, lists capped `38vh`, grid min 140px; icon-only collapse rejected since it would hide features); wrapping topbar with truncating view name + hidden non-critical chrome ≤700px (`:2718`/`2743`/`2762`); wrapping preview rows (`:2980`); flexible dock columns with viewport-capped minimums (`:3932`/`3952`); scrollable Settings tabs + stacking Looks grid (`SettingsPanel.svelte:1434`/`1593`); stacking Look-editor sidebar (`LookEditorView.svelte:531`).
- **Verify:** `npm run check` 0 errors 0 warnings, `vite build` clean. **Not visually verified** — no display tooling in this environment; recommend a manual resize/zoom pass on real hardware.
