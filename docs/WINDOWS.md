# MakrStudio (formerly MakePresent) — Native Windows Build (`windows` branch)

This branch (`windows`) is the native Windows distribution of MakrStudio. It builds a signed-ready NSIS installer and MSI via Tauri 2 + WebView2.

## What changed vs `main`

| Area | Change |
|---|---|
| `src-tauri/tauri.conf.json` | `bundle.targets` → `["nsis","msi"]`, added `bundle.windows` (NSIS/WiX + `webviewInstallMode.embedBootstrapper`), publisher/category metadata |
| `package.json` | Added `tauri:build:windows` (`tauri build --bundles nsis,msi`) |
| `.github/workflows/build.yml` | Triggers on `main` **and** `windows`; `windows` job now runs `tauri build` (full bundle) + uploads `*.exe`/`*.msi` artifacts |
| Runtime fixes (inherited from `main`) | `windows.rs` WebView2 main-thread dispatch, `output_visible` HashMap check, `show_output` fire-and-forget, single-monitor cover mitigation, `project.rs` `output_fullscreen` default `false`, viewport/DPI & drag-zone guards |

## Prerequisites (Windows 11)

- Node.js 22+, Rust stable (`rustup`), WebView2 Runtime (evergreen — `embedBootstrapper` will install silently if missing)
- `ffmpeg`/`ffprobe` on PATH for thumbnail generation (optional at runtime, required for media import)

## Build locally

```powershell
npm ci
npm run build        # vite frontend
npm run check        # svelte-check
npm run tauri:build:windows   # produces NSIS + MSI
# or dev
npm run tauri:dev:windows
```

Artifacts land in `src-tauri/target/release/bundle/nsis/*.exe` and `.../msi/*.msi`.

## Installer details

- **NSIS** (`installMode: both`, `lzma`, no language selector) — per-machine or per-user, standard Windows installer.
- **MSI** via WiX — enterprise deployment.
- **WebView2** — `embedBootstrapper` + `silent:true` bundles the bootstrapper; first run installs WebView2 if absent, no manual download.
- **NDI Runtime (bundled)** — the official **NDI 6 Runtime redistributable** (`src-tauri/resources/NDI_Runtime_V6.exe` 9.6 MB, DLL-only, ~300 MB full SDK *not* vendored) is bundled into both NSIS/MSI via `bundle.resources` (`tauri.conf.json:46` `resources/NDI_Runtime_V6.exe` + `resources/NDI_VERSION.txt`) and the NSIS installer runs it silently (`/verysilent`) as part of MakrStudio's own install via `src-tauri/windows/hooks.nsi` `NSIS_HOOK_POSTINSTALL` (mirroring `embedBootstrapper` for WebView2). After install, `NDI_RUNTIME_DIR_V5` (set by the redistributable to `C:\Program Files\NDI\NDI 6 Runtime\`) is checked first by `src-tauri/src/broadcast.rs:144` (`NDI_RUNTIME_DIR_V6` → `V5` → fallback `Processing.NDI.Lib.x64.dll` next to .exe). No manual DLL placement needed on Windows — fresh install on a clean VM immediately shows `ndi: broadcast enabled — source "MakrStudio - Sunday Output"` in `logs/app.log` with no `NDI SDK not found` error, and OBS (obs-ndi) discovers it. Bundled version is documented in `src-tauri/resources/NDI_VERSION.txt:3` (**NDI 6 Runtime 6.0.1, Apr 16 2026** from `https://downloads.ndi.tv/SDK/NDI_SDK/NDI%206%20Runtime.exe` via `https://ndi.link/NDIRedistV6`) — check quarterly and before each release per NDI terms ("make all reasonable efforts to keep the versions you distribute up to date", `https://docs.ndi.video/all/developing-with-ndi/sdk/software-distribution`). This does **not** affect **Linux/macOS**, which still require manual `libndi.so.5` / `libndi.dylib` install (see README.md).
- **Icon** — `src-tauri/icons/icon.ico` used for both bundles; `src-tauri/icons/*` required.
- **No console** on release — `src-tauri/src/main.rs:2` `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.

## CI

Push to `windows` triggers `.github/workflows/build.yml:windows` on `windows-2022`:
1. `npm ci` → `npm run build` → `npm run check`
2. `npm run tauri build` (full bundle)
3. Upload artifacts `MakrStudio-windows-installers`

Download from Actions → Artifacts.

## Troubleshooting (Windows 11)

- **Frozen UI after Output appears** — fixed in this branch via `windows.rs:496` (no `is_visible` dispatch) and `show_output` async dispatch. If still frozen, check `logs/app.log` for `main thread dispatch timed out`.
- **0x0 WebView2 canvas** — fixed via `src/app.css:14` explicit `100vw/100vh`.
- **125%/150% DPI clipping** — fixed via `Editor.svelte:887` viewport-relative grid + `fitText.ts:46` DPR-aware epsilon.
- **Header clicks swallowed** — drag zone isolated to `.spacer` only (`Editor.svelte:813`, `app.css:40`).

## Releasing

```powershell
# bump version in package.json + tauri.conf.json + Cargo.toml
git tag v0.1.0-windows
git push origin windows --tags
# CI builds installers; attach to GitHub Release manually or via `gh release create`
```
