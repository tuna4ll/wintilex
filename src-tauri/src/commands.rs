//! The bridge between the settings window and the manager thread.
//!
//! Every command is a thin wrapper: the UI never computes anything about the
//! layout itself, it only reads a snapshot and posts actions.

use serde::Serialize;
use tauri::State;

use wintilex_bar::BarHandle;
use wintilex_core::command::Action;
use wintilex_core::config::{BarModule, Config};
use wintilex_core::manager::{EngineHandle, Snapshot};
use wintilex_core::platform::autostart;
use wintilex_core::LayoutKind;

pub struct AppState {
    pub engine: EngineHandle,
    pub bar: BarHandle,
    /// Mirrors `general.minimize-to-tray` so the close handler can read it
    /// without going through the manager thread.
    pub close_to_tray: std::sync::atomic::AtomicBool,
}

/// Commands return a plain string on failure so the UI can show it as-is.
type CommandResult<T> = Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyIssue {
    pub binding: String,
    pub reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutInfo {
    pub id: LayoutKind,
    pub label: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub version: String,
    pub config_path: String,
    pub layouts: Vec<LayoutInfo>,
    pub bar_modules: Vec<BarModuleInfo>,
    pub autostart_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarModuleInfo {
    pub id: BarModule,
    pub label: String,
}

#[tauri::command]
pub fn get_config() -> CommandResult<Config> {
    Config::load().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: Config) -> CommandResult<()> {
    config.save().map_err(|error| error.to_string())?;

    state
        .close_to_tray
        .store(config.general.minimize_to_tray, std::sync::atomic::Ordering::Relaxed);

    // Keep the registry entry in step with the setting, but never let a failure
    // there block saving the rest of the configuration.
    if let Err(error) = autostart::set(config.general.start_on_login) {
        log::error!("could not update the startup entry: {error}");
    }

    // The bar first: it may hand a strip of the screen back or take one away,
    // and the manager should lay the windows out against the work area it is
    // going to end up with.
    state.bar.apply(config.bar.clone());
    state.engine.apply_config(config);
    Ok(())
}

/// Turn the bar on or off on its own, for the tray menu and the switch on the
/// bar page. The change is written to the file like any other setting.
#[tauri::command]
pub fn set_bar_enabled(state: State<'_, AppState>, enabled: bool) -> CommandResult<Config> {
    let mut config = Config::load_or_default();
    config.bar.enabled = enabled;
    config.save().map_err(|error| error.to_string())?;

    state.bar.apply(config.bar.clone());
    Ok(config)
}

/// Write the defaults back out, for when a config has been edited into a corner.
#[tauri::command]
pub fn reset_config(state: State<'_, AppState>) -> CommandResult<Config> {
    let config = Config::default();
    config.save().map_err(|error| error.to_string())?;
    state.bar.apply(config.bar.clone());
    state.engine.apply_config(config.clone());
    Ok(config)
}

#[tauri::command]
pub fn get_snapshot(app: tauri::AppHandle, state: State<'_, AppState>) -> Snapshot {
    let snapshot = state.engine.snapshot();
    // Cheapest place to notice that a hotkey toggled tiling, or that the bar
    // page turned the bar off, behind the tray menu's back.
    crate::tray::sync_tiling_state(&app, snapshot.tiling_enabled);
    crate::tray::sync_bar_state(&app, state.bar.is_running());
    snapshot
}

#[tauri::command]
pub fn run_action(state: State<'_, AppState>, action: Action) -> CommandResult<()> {
    state.engine.dispatch(action);
    Ok(())
}

#[tauri::command]
pub fn get_hotkey_issues(state: State<'_, AppState>) -> Vec<HotkeyIssue> {
    state
        .engine
        .failed_hotkeys()
        .into_iter()
        .map(|failure| HotkeyIssue { binding: failure.binding.to_string(), reason: failure.reason })
        .collect()
}

#[tauri::command]
pub fn get_environment() -> Environment {
    Environment {
        version: env!("CARGO_PKG_VERSION").to_string(),
        config_path: Config::path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "unavailable".into()),
        layouts: LayoutKind::ALL
            .iter()
            .map(|kind| LayoutInfo { id: *kind, label: kind.label().to_string() })
            .collect(),
        bar_modules: BarModule::ALL
            .iter()
            .map(|module| BarModuleInfo { id: *module, label: module.label().to_string() })
            .collect(),
        autostart_enabled: autostart::is_enabled(),
    }
}

/// Open the folder holding the config file in Explorer.
#[tauri::command]
pub fn reveal_config(app: tauri::AppHandle) -> CommandResult<()> {
    use tauri_plugin_opener::OpenerExt;

    let directory = Config::directory().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(directory.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

/// Run a layout against a made-up screen so the settings window can draw a
/// preview. Calling into the real algorithm keeps the preview honest.
#[tauri::command]
pub fn preview_layout(
    layout: LayoutKind,
    count: usize,
    width: i32,
    height: i32,
    options: wintilex_core::LayoutOptions,
) -> Vec<wintilex_core::Rect> {
    let area = wintilex_core::Rect::new(0, 0, width, height);
    wintilex_core::layout::arrange(
        layout.build().as_ref(),
        area,
        count,
        &options,
        &wintilex_core::layout::Ratios::new(),
    )
    .tiles
}
