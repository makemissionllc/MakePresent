mod commands;
mod project;
mod state;
mod windows;

use project::{persist, read_session, recover_or_seed, write_session};
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
    let mut session = read_session(&data_dir).unwrap_or_default();
    session.clean_shutdown = true;
    let _ = write_session(&data_dir, &session);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            // Single source of truth: load the last autosaved project (with
            // crash-recovery notice) or seed the Phase 1 test project.
            let (project, notice) = recover_or_seed(&data_dir);
            let state = app.state::<AppState>();
            *state.project.write().unwrap() = project;
            state.set_notice(notice);
            *state.data_dir.write().unwrap() = data_dir.clone();
            state.apply_settings(project::read_settings(&data_dir));

            let tx = project::spawn_autosave(
                state.project.clone(),
                data_dir.clone(),
                app.handle().clone(),
            );
            *state.save_tx.lock().unwrap() = Some(tx);

            // Output window on the configured (or auto-picked) display.
            if let Err(e) = windows::place_default_output(app.handle(), &state) {
                eprintln!("could not place output window: {e}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_live_slide,
            commands::clear_output,
            commands::new_project,
            commands::add_slide,
            commands::update_slide,
            commands::delete_slide,
            commands::list_displays,
            commands::set_output_display,
            commands::toggle_output_fullscreen,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            finalize(app_handle);
        }
    });
}