mod commands;
mod logging;
mod project;
mod state;
mod windows;

use logging::Level;
use project::{persist, read_library, read_session, recover_or_seed, write_library, write_session};
use state::AppState;
use tauri::Manager;

/// Called once at exit (clean path): flush the last state and confirm the
/// shutdown was clean, so startup knows it is NOT recovering from a crash.
fn finalize(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let data_dir = state.app_data_dir();
    {
        let project = state.project.read().unwrap();
        let _ = persist(&project, &data_dir);
    }
    {
        let library = state.library.read().unwrap();
        let _ = write_library(&data_dir, &library);
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
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            let state = app.state::<AppState>();
            state.logger.open(data_dir.clone());
            state.logger.log(Level::Info, "app: started");

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
            state.apply_settings(project::read_settings(&data_dir));

            let tx = project::spawn_autosave(
                state.project.clone(),
                state.library.clone(),
                data_dir.clone(),
                app.handle().clone(),
            );
            *state.save_tx.lock().unwrap() = Some(tx);

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
            commands::clear_output,
            commands::new_project,
            commands::add_slide,
            commands::update_slide,
            commands::delete_slide,
            commands::list_displays,
            commands::set_output_display,
            commands::toggle_output_fullscreen,
            commands::show_output,
            commands::set_stage_display,
            commands::toggle_stage,
            commands::export_settings,
            commands::import_settings,
            commands::get_logs,
            commands::export_logs_to,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            finalize(app_handle);
        }
    });
}