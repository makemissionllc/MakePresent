# MakePresent — Native Windows Build (`windows` branch)

This branch (`windows`) is the native Windows distribution of MakePresent. It builds a signed-ready NSIS installer and MSI via Tauri 2 + WebView2.

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
- **Icon** — `src-tauri/icons/icon.ico` used for both bundles; `src-tauri/icons/*` required.
- **No console** on release — `src-tauri/src/main.rs:2` `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`.

## CI

Push to `windows` triggers `.github/workflows/build.yml:windows` on `windows-2022`:
1. `npm ci` → `npm run build` → `npm run check`
2. `npm run tauri build` (full bundle)
3. Upload artifacts `MakePresent-windows-installers`

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
