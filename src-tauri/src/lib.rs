//! Tauri shell.
//!
//! The window management runs entirely in `tilex-core` on its own thread. This
//! crate only puts a tray icon and a settings window in front of it.

mod commands;
mod tray;

use std::sync::atomic::Ordering;

use tauri::{Manager, WindowEvent};

use tilex_core::config::Config;
use tilex_core::manager::Engine;
use tilex_core::platform::{autostart, instance};

pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Two managers on one desktop would fight over every window, so a second
    // launch just raises the settings window of the one already running.
    let instance::Instance::First(guard, reopen) = instance::acquire() else {
        log::info!("Tilex is already running; asked it to show its window");
        return;
    };

    let config = Config::load_or_default();
    // Only a launch from the Run key starts silently; opening Tilex by hand
    // should always show the settings window.
    let start_hidden = autostart::launched_at_startup();
    let tiling_enabled = config.general.tiling_enabled;
    let close_to_tray = config.general.minimize_to_tray;

    // Make sure the registry entry matches what the config says, in case the
    // executable was moved since it was written.
    if let Err(error) = autostart::set(config.general.start_on_login) {
        log::error!("could not update the startup entry: {error}");
    }

    let engine = Engine::spawn(config);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState {
            engine: engine.clone(),
            close_to_tray: std::sync::atomic::AtomicBool::new(close_to_tray),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::reset_config,
            commands::get_snapshot,
            commands::run_action,
            commands::get_hotkey_issues,
            commands::get_environment,
            commands::reveal_config,
            commands::preview_layout,
        ])
        .setup(move |app| {
            tray::build(app.handle(), tiling_enabled)?;
            if !start_hidden {
                tray::show_settings(app.handle());
            }

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                while reopen.recv().is_ok() {
                    tray::show_settings(&handle);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // With close-to-tray on, closing the settings window leaves the
            // manager running so tiling never stops by accident.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let hide = window
                    .try_state::<commands::AppState>()
                    .is_some_and(|state| state.close_to_tray.load(Ordering::Relaxed));
                if hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to start Tilex")
        .run(move |app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<commands::AppState>() {
                    state.engine.shutdown();
                }
            }
        });

    drop(guard);
}
