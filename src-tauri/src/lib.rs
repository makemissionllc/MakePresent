mod commands;
mod broadcast;
mod logging;
mod media;
mod midi;
mod network;
mod osc;
mod project;
mod scripture;
mod state;
mod triggers;
mod windows;

use logging::Level;
use project::{persist, read_library, read_session, recover_or_seed, write_library, write_session};
use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};

/// Set to `true` the moment the user chooses "Quit" from the system tray. When
/// set, the application really exits; otherwise `ExitRequested` is prevented so
/// closing the Editor keeps the Output/Stage (and the process) alive.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Diagnostic: snapshot every live window's label, visibility, focus, inner
/// position and size. Called at startup and again a couple of seconds in so a
/// real Windows run shows whether any window is overlapping/on top of the
/// Editor window at the moment input stops working.
fn log_window_state(app: &tauri::AppHandle, state: &AppState, prefix: &str) {
    let windows = app.webview_windows();
    state.logger.log(
        Level::Info,
        &format!("{prefix}: window count = {}", windows.len()),
    );
    for (label, window) in &windows {
        let visible = window.is_visible().unwrap_or(false);
        let focused = window.is_focused().unwrap_or(false);
        let pos = window
            .inner_position()
            .map(|p| format!("({}, {})", p.x, p.y))
            .unwrap_or_else(|_| "(?, ?)".to_string());
        let size = window
            .inner_size()
            .map(|s| format!("{}x{}", s.width, s.height))
            .unwrap_or_else(|_| "?x?".to_string());
        state.logger.log(
            Level::Info,
            &format!(
                "{prefix}: label={}, visible={}, focused={}, inner_pos={pos}, inner_size={size}",
                label, visible, focused,
            ),
        );
    }
}

/// Called once at exit (clean path): flush the last state and confirm the
/// shutdown was clean, so startup knows it is NOT recovering from a crash.
fn finalize(app: &tauri::AppHandle) {
    // Signal the self-healing handler to stop so window destruction during
    // shutdown does not trigger auto-recreation.
    crate::windows::set_shutting_down();

    let state = app.state::<AppState>();
    // Tear down NDI before the SDK lib could be unloaded / windows close.
    state.broadcaster.stop();
    // Stop external input listeners on a clean exit.
    state.midi.stop();
    state.osc.stop();
    // Stop the local-network stage server (closes any connected phones).
    state.network.stop();

    let data_dir = state.app_data_dir();
    {
        let snapshot = state.project.read().unwrap().clone();
        let _ = persist(&snapshot, &data_dir);
    }
    {
        let snapshot = state.library.read().unwrap().clone();
        let _ = write_library(&data_dir, &snapshot);
    }
    let mut session = read_session(&data_dir).unwrap_or_default();
    session.clean_shutdown = true;
    let _ = write_session(&data_dir, &session);
    state.logger.log(Level::Info, "app: exited cleanly");
}

/// Bring the Editor window back to the foreground, recreating it if it was
/// destroyed. Once visible, re-broadcast state so the Output/Stage (and any
/// connected Stage-Display clients) resync to the latest project.
fn show_editor(app: &tauri::AppHandle) {
    let editor = match crate::windows::ensure_editor(app) {
        Ok(w) => w,
        Err(e) => {
            app.state::<AppState>()
                .logger
                .log(Level::Error, &format!("tray: could not open editor: {e}"));
            return;
        }
    };
    let _ = editor.unminimize();
    let _ = editor.show();
    let _ = editor.set_focus();
    // Re-sync every view to the current state (idempotent when nothing changed).
    let _ = crate::commands::snapshot_and_emit(app);
}

/// Set by the tray "Quit" action so the real exit path proceeds; harmless
/// otherwise.
fn quit_app(app: &tauri::AppHandle) {
    crate::windows::set_shutting_down();
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// Menu id shared between the tray menu item and the left-click handler.
fn handle_tray_action(app: &tauri::AppHandle, id: &str) {
    match id {
        "open_editor" => show_editor(app),
        "quit" => quit_app(app),
        _ => {}
    }
}

/// Put the Open Editor / Quit menu on the (config-defined) system tray icon.
/// The icon itself is created automatically from `tauri.conf.json` -> `app.trayIcon`;
/// here we attach the menu and the app-level menu/tray event handlers (registered
/// on the builder) do the rest.
fn setup_tray(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let open = match MenuItem::with_id(app, "open_editor", "Open Editor", true, None::<&str>) {
        Ok(i) => i,
        Err(e) => {
            state
                .logger
                .log(Level::Error, &format!("tray: failed to build menu item: {e}"));
            return;
        }
    };
    let quit = match MenuItem::with_id(app, "quit", "Quit MakePresent", true, None::<&str>) {
        Ok(i) => i,
        Err(e) => {
            state
                .logger
                .log(Level::Error, &format!("tray: failed to build menu item: {e}"));
            return;
        }
    };
    let menu = match Menu::with_items(app, &[&open, &quit]) {
        Ok(m) => m,
        Err(e) => {
            state
                .logger
                .log(Level::Error, &format!("tray: failed to build menu: {e}"));
            return;
        }
    };
    // Attach the menu to the config-created tray ("main" id) if present.
    match app.tray_by_id("main") {
        Some(tray) => match tray.set_menu(Some(menu)) {
            Ok(()) => state
                .logger
                .log(Level::Info, "tray: system tray menu attached"),
            Err(e) => state
                .logger
                .log(Level::Error, &format!("tray: failed to set tray menu: {e}")),
        },
        None => state
            .logger
            .log(Level::Info, "tray: config tray not yet created — events still wired"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .on_window_event(|window, event| {
            // Persistent renderers: closing the Editor must NOT take the whole
            // app down. Intercept the close request, log it, and hide the
            // editor instead — the process (and any Output/Stage windows) stay
            // alive, and the user brings the editor back from the tray.
            if window.label() == crate::windows::EDITOR_WINDOW {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let app_handle = window.app_handle();
                    let st = app_handle.state::<AppState>();
                    st.logger.log(
                        Level::Info,
                        "editor: close requested — hiding window; app keeps running in tray",
                    );
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .on_menu_event(|app_handle, event| {
            handle_tray_action(app_handle, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            // Left-click (or touch) on the icon toggles/reopens the editor, so
            // it is reachable even without opening the tray menu.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                handle_tray_action(tray.app_handle(), "open_editor");
            }
        })
        .setup(|app| {
            // `setup` runs on the GUI main thread. Mark it so `windows::run_on_main`
            // can detect re-entrant calls (stage restore, nested ensure_*) and run
            // inline instead of self-deadlocking by queuing to itself and blocking.
            crate::windows::mark_as_main_thread();

            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let state = app.state::<AppState>();
            state.logger.open(data_dir.clone());
            state.logger.log(Level::Info, "app: started");

            // Persistent-renderer lifecycle: install the tray icon (always
            // staying alive) and, when the Editor's close button is pressed,
            // hide instead of destroy so the Output/Stage keep running and the
            // editor can be reopened from the tray.
            setup_tray(app.handle());

            // Diagnostic logging for startup windows:
            log_window_state(app.handle(), &state, "windows: startup");

            // Offload ffmpeg probe (spawns ffmpeg -version) off the main thread so the
            // WebView2 message pump stays responsive at startup on Windows 11.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    if media::ffmpeg_available() {
                        handle
                            .state::<AppState>()
                            .logger
                            .log(Level::Info, "media: ffmpeg available for thumbnails");
                    } else {
                        handle.state::<AppState>().logger.log(
                            Level::Error,
                            "media: ffmpeg NOT available — media import thumbnails disabled",
                        );
                    }
                });
            }

            // Single source of truth: load the last autosaved project (with
            // crash-recovery notice) or seed the sample project, and load (or
            // seed) the reusable slide library.
            let (project, notice) = recover_or_seed(&data_dir);
            let library = read_library(&data_dir);

            state.logger.log(
                Level::Info,
                &format!("project: loaded \"{}\"", project.name),
            );
            if let Some(n) = &notice {
                state
                    .logger
                    .log(Level::Warn, &format!("project: {} — {}", n.kind, n.message));
            }
            state
                .logger
                .log(Level::Info, &format!("library: loaded {} songs", library.songs.len()));

            *state.project.write().unwrap() = project;
            *state.library.write().unwrap() = library;
            state.set_notice(notice);
            *state.data_dir.write().unwrap() = data_dir.clone();
            {
                let mut settings = project::read_settings(&data_dir);
                // Fix: previously defaulted to true, causing fullscreen on first
                // show even when user never opted in. Migrate persisted `true`
                // to `false` once so startup is windowed by default.
                if settings.output_fullscreen {
                    settings.output_fullscreen = false;
                    let _ = project::write_settings(&data_dir, &settings);
                    state.logger.log(
                        Level::Info,
                        "settings: migrated output_fullscreen true -> false (default corrected)",
                    );
                }
                state.apply_settings(settings);
            }

            // NDI: restore the broadcast if it was left enabled. Wrapped in a
            // spawned thread so slow LoadLibraryExW (antivirus scanning the DLL) cannot
            // block the main thread pump before the event loop is fully running.
            // Graceful failure when DLL missing is preserved.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let st = handle.state::<AppState>();
                    let ndi_on = st.current_settings().ndi_enabled;
                    if ndi_on {
                        match st.broadcaster.start(crate::broadcast::NDI_SOURCE_NAME) {
                            Ok(()) => st.logger.log(
                                Level::Info,
                                &format!(
                                    "ndi: restored broadcast — source \"{}\" (off-main-thread LoadLibraryExW)",
                                    crate::broadcast::NDI_SOURCE_NAME
                                ),
                            ),
                            Err(e) => st.logger.log(
                                Level::Warn,
                                &format!("ndi: restored state was enabled but could not start: {e}"),
                            ),
                        }
                    }
                    if !st.broadcaster.is_active() {
                        st.logger.log(
                            Level::Info,
                            &format!("ndi: broadcast off (looks for {})", crate::broadcast::lib_filename()),
                        );
                    }
                });
            }

            // MIDI: restore the listener if a device was left selected. The
            // connection runs on its own midir I/O thread; setup just opens it.
            let midi_cfg = state.current_settings();
            if midi_cfg.midi_enabled {
                if let Some(device_id) = midi_cfg.midi_device_id.clone() {
                    match state.midi.start(app.handle().clone(), &device_id) {
                        Ok(()) => state.logger.log(
                            Level::Info,
                            "midi: restored input listener from settings",
                        ),
                        Err(e) => state.logger.log(
                            Level::Warn,
                            &format!("midi: saved device could not be opened at startup: {e}"),
                        ),
                    }
                }
            } else {
                state.logger.log(Level::Info, "midi: input listener disabled");
            }

            // OSC: restore the UDP listener if it was left enabled.
            let osc_cfg = state.current_settings();
            if osc_cfg.osc_enabled {
                match state.osc.start(app.handle().clone(), osc_cfg.osc_port) {
                    Ok(()) => state.logger.log(
                        Level::Info,
                        &format!("osc: restored listener on UDP :{}", osc_cfg.osc_port),
                    ),
                    Err(e) => state.logger.log(
                        Level::Warn,
                        &format!("osc: saved listener could not start at startup: {e}"),
                    ),
                }
            } else {
                state.logger.log(Level::Info, "osc: listener disabled");
            }

            // Stage Network: restore the local WebSocket server if it was left
            // enabled, and broadcast the current state once it is up so a phone
            // that connects immediately has something to render.
            let net_cfg = state.current_settings();
            if net_cfg.stage_network_enabled {
                let addr = format!(
                    "0.0.0.0:{}",
                    net_cfg.stage_network_port
                );
                match addr.parse() {
                    Ok(socket_addr) => match state
                        .network
                        .start(app.handle().clone(), socket_addr, net_cfg.stage_network_pin.clone())
                    {
                        Ok(()) => state.logger.log(
                            Level::Info,
                            &format!(
                                "stage-network: restored server on :{}",
                                net_cfg.stage_network_port
                            ),
                        ),
                        Err(e) => state.logger.log(
                            Level::Warn,
                            &format!("stage-network: restore failed: {e}"),
                        ),
                    },
                    Err(e) => state.logger.log(
                        Level::Warn,
                        &format!("stage-network: bad bound address: {e}"),
                    ),
                }
                // Push the current stage state so already-connected clients (if
                // any survived) resync, and future connects get fresh data.
                state.network.broadcast(&crate::network::stage_broadcast(app.handle()));
            } else {
                state.logger.log(Level::Info, "stage-network: server disabled");
            }

            // Load the KJV scripture index once at startup for fast
            // autocomplete search. The ~6 MB JSON is read and parsed into a
            // HashMap-backed index in well under 500ms on any modern hardware.
            // bundle.resources maps resources/kjv.json → $RESOURCE/kjv.json.
            // Offloaded to a background thread so heavy JSON parse doesn't block the
            // main thread / WebView2 pump on Windows 11.
            {
                let handle = app.handle().clone();
                let kjv_resolved = app.path().resolve("kjv.json", tauri::path::BaseDirectory::Resource);
                let data_dir = app.path().app_data_dir();
                std::thread::spawn(move || {
                    let st = handle.state::<AppState>();
                    match kjv_resolved {
                        Ok(kjv_path) => match scripture::try_load(&kjv_path) {
                            Ok(mut scripture) => {
                                st.logger.log(
                                    Level::Info,
                                    &format!(
                                        "scripture: loaded {} books from {}",
                                        scripture.book_count(),
                                        kjv_path.display()
                                    ),
                                );
                                // Fold in any Bibles the user imported in an
                                // earlier session so they stay searchable.
                                if let Ok(data_dir) = data_dir {
                                    let imported = scripture::load_imported_books(&data_dir);
                                    if !imported.is_empty() {
                                        let verses = scripture.merge_books(imported);
                                        st.logger.log(
                                            Level::Info,
                                            &format!(
                                                "scripture: restored imported Bibles \
                                                 ({} books, {} verses)",
                                                scripture.book_count(),
                                                verses
                                            ),
                                        );
                                    }
                                }
                                *st.scripture.write().unwrap() = Some(scripture);
                            }
                            Err(e) => {
                                st.logger.log(
                                    Level::Error,
                                    &format!("scripture: {e} — search disabled"),
                                );
                            }
                        },
                        Err(e) => {
                            st.logger.log(
                                Level::Error,
                                &format!("scripture: failed to resolve kjv.json resource: {e}"),
                            );
                        }
                    }
                });
            }

            let tx = project::spawn_autosave(
                state.project.clone(),
                state.library.clone(),
                data_dir.clone(),
                app.handle().clone(),
            );
            *state.save_tx.lock().unwrap() = Some(tx);

            // Rebuild any missing/corrupt cached thumbnails for assets the
            // project or library references (never show a silent blank).
            {
                let app = app.handle().clone();
                std::thread::spawn(move || media::verify_on_startup(app));
            }

            // Windows deadlock fix: pre-create Output+Stage hidden via deferred next-tick
            // so live command handlers never call builder().build().
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(180));
                    let handle_clone = handle.clone();
                    let dispatch = handle.run_on_main_thread(move || {
                        crate::windows::mark_as_main_thread();
                        crate::windows::precreate_hidden_windows(&handle_clone);
                        let st = handle_clone.state::<AppState>();
                        if st.current_settings().stage_visible {
                            if let Some(idx) = st.current_settings().stage_display_index {
                                match crate::windows::move_stage_to(&handle_clone, idx) {
                                    Ok(_) => st.logger.log(Level::Info, "stage: restored on startup (deferred pre-create)"),
                                    Err(e) => st.logger.log(Level::Warn, &format!("stage: deferred restore failed: {e}")),
                                }
                            }
                        }
                    });
                    if let Err(e) = dispatch {
                        handle.state::<AppState>().logger.log(
                            Level::Error,
                            &format!("windows: deferred pre-create dispatch failed: {e}"),
                        );
                    }
                });
            }

            // Display disconnect/reconnect self-healing: poll available_monitors() every 3s
            // (cheap Win32 EnumDisplayMonitors, ~0.1ms) and fallback Output/Stage to largest
            // remaining display without ever calling builder().build() from this path.
            // Chosen fallback for all-external-disconnect: windowed on remaining display (72% centered,
            // decorations true via move_output_to single-monitor mitigation) — keeps live slide visible
            // and Editor reachable, simpler than hiding and requiring re-show.
            crate::windows::spawn_display_watcher(app.handle().clone());

            // Diagnostic re-check a couple of seconds in: by then the stage
            // restore (if any) has settled, so this shows the same window
            // set/focus state a user sees while the editor appears frozen.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let st = handle.state::<AppState>();
                    log_window_state(&handle, &st, "windows: delayed @2.5s");
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_library,
            commands::add_library_song,
            commands::delete_library_song,
            commands::add_song_to_playlist,
            commands::set_live_slide,
            commands::set_transition,
            commands::upsert_look,
            commands::delete_look,
            commands::set_output_look,
            commands::set_stage_look,
            commands::set_ndi_look,
            commands::set_ndi_enabled,
            commands::clear_output,
            commands::new_project,
            commands::add_slide,
            commands::update_slide,
            commands::delete_slide,
            commands::list_displays,
            commands::set_output_display,
            commands::toggle_output_fullscreen,
            commands::show_output,
            commands::log_output_intentionally_closed,
            commands::set_stage_display,
            commands::toggle_stage,
            commands::import_media,
            commands::export_settings,
            commands::import_settings,
            commands::get_logs,
            commands::export_logs_to,
            commands::search_scripture,
            commands::import_openlp_bible,
            commands::import_api_bible,
            commands::lookup_api_scripture,
            commands::list_midi_devices,
            commands::set_midi_enabled,
            commands::set_midi_device,
            commands::set_osc_enabled,
            commands::set_osc_port,
            commands::add_trigger,
            commands::delete_trigger,
            commands::set_trigger_enabled,
            commands::get_stage_network_info,
            commands::set_stage_network_enabled,
            commands::set_stage_network_port,
            commands::set_stage_network_pin,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            finalize(app_handle);
        } else if let RunEvent::ExitRequested { api, .. } = event {
            // Keep the process alive in the background (tray) unless the user
            // explicitly chose Quit — that way closing the Editor (which we
            // already hide instead of destroy) or losing every window never
            // silently kills a live service.
            if !QUIT_REQUESTED.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}