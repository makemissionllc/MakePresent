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
use tauri::Manager;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
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

            // NDI: restore the broadcast if it was left enabled (the sender
            // runs on its own thread; only the NDI-installed machine starts it).
            let ndi_on = state.current_settings().ndi_enabled;
            if ndi_on {
                match state.broadcaster.start(crate::broadcast::NDI_SOURCE_NAME) {
                    Ok(()) => state.logger.log(
                        Level::Info,
                        &format!(
                            "ndi: restored broadcast — source \"{}\"",
                            crate::broadcast::NDI_SOURCE_NAME
                        ),
                    ),
                    Err(e) => state.logger.log(
                        Level::Warn,
                        &format!("ndi: restored state was enabled but could not start: {e}"),
                    ),
                }
            }
            if !state.broadcaster.is_active() {
                state
                    .logger
                    .log(Level::Info, &format!("ndi: broadcast off (looks for {})", crate::broadcast::lib_filename()));
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
                std::thread::spawn(move || {
                    let st = handle.state::<AppState>();
                    match kjv_resolved {
                        Ok(kjv_path) => match scripture::try_load(&kjv_path) {
                            Ok(scripture) => {
                                st.logger.log(
                                    Level::Info,
                                    &format!(
                                        "scripture: loaded {} books from {}",
                                        scripture.book_count(),
                                        kjv_path.display()
                                    ),
                                );
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

            // Onboarding: only the Editor window exists at launch. The Output
            // window is created on demand (first live slide or "Show Output");
            // the Stage Display is restored below if it was left switched on.
            if state.current_settings().stage_visible {
                if let Some(index) = state.current_settings().stage_display_index {
                    match windows::move_stage_to(app.handle(), index) {
                        Ok(_) => state.logger.log(Level::Info, "stage: restored on startup"),
                        Err(e) => {
                            state.logger.log(Level::Warn, &format!("stage: restore failed: {e}"));
                        }
                    }
                }
            }

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
        if let tauri::RunEvent::Exit = event {
            finalize(app_handle);
        }
    });
}