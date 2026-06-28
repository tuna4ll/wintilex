//! System tray icon and menu.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use tilex_core::command::Action;
use tilex_core::LayoutKind;

const ID_TILING: &str = "tiling";
const ID_RETILE: &str = "retile";
const ID_SETTINGS: &str = "settings";
const ID_QUIT: &str = "quit";
const LAYOUT_PREFIX: &str = "layout:";

/// Menu items the rest of the app needs to keep in step with manager state.
pub struct TrayState<R: Runtime> {
    tiling: CheckMenuItem<R>,
}

pub fn build<R: Runtime>(app: &AppHandle<R>, tiling_enabled: bool) -> tauri::Result<()> {
    let tiling = CheckMenuItem::with_id(
        app,
        ID_TILING,
        "Tiling enabled",
        true,
        tiling_enabled,
        None::<&str>,
    )?;

    let layout_items: Vec<MenuItem<R>> = LayoutKind::ALL
        .iter()
        .map(|kind| {
            MenuItem::with_id(
                app,
                format!("{LAYOUT_PREFIX}{}", serde_json::to_string(kind).unwrap_or_default()),
                kind.label(),
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;

    let layout_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        layout_items.iter().map(|item| item as &dyn tauri::menu::IsMenuItem<R>).collect();
    let layouts = Submenu::with_items(app, "Layout", true, &layout_refs)?;

    let menu = Menu::with_items(
        app,
        &[
            &tiling,
            &layouts,
            &MenuItem::with_id(app, ID_RETILE, "Retile now", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, ID_SETTINGS, "Settings…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, ID_QUIT, "Quit Tilex", true, None::<&str>)?,
        ],
    )?;

    app.manage(TrayState { tiling: tiling.clone() });

    TrayIconBuilder::with_id("tilex")
        .icon(app.default_window_icon().cloned().expect("bundled icon"))
        .tooltip("Tilex")
        .menu(&menu)
        // The menu is on right click only, so a left click can open settings.
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_icon_event)
        .build(app)?;

    Ok(())
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    if let Some(raw) = id.strip_prefix(LAYOUT_PREFIX) {
        if let Ok(kind) = serde_json::from_str::<LayoutKind>(raw) {
            dispatch(app, Action::SetLayout(kind));
        }
        return;
    }

    match id {
        ID_TILING => dispatch(app, Action::ToggleTiling),
        ID_RETILE => dispatch(app, Action::Retile),
        ID_SETTINGS => show_settings(app),
        ID_QUIT => {
            if let Some(engine) = app.try_state::<crate::commands::AppState>() {
                engine.engine.shutdown();
            }
            app.exit(0);
        }
        _ => {}
    }
}

fn handle_icon_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent) {
    if let TrayIconEvent::Click { button: MouseButton::Left, button_state, .. } = event {
        if button_state == MouseButtonState::Up {
            show_settings(tray.app_handle());
        }
    }
}

fn dispatch<R: Runtime>(app: &AppHandle<R>, action: Action) {
    if let Some(state) = app.try_state::<crate::commands::AppState>() {
        state.engine.dispatch(action);
    }
    // The checkbox and the window both mirror manager state, so nudge the UI.
    let _ = app.emit_to("settings", "tilex://changed", ());
}

pub fn show_settings<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Keep the tray checkbox in step with the manager, which a hotkey can also
/// toggle while the menu is closed.
pub fn sync_tiling_state<R: Runtime>(app: &AppHandle<R>, enabled: bool) {
    if let Some(state) = app.try_state::<TrayState<R>>() {
        let _ = state.tiling.set_checked(enabled);
    }
}
