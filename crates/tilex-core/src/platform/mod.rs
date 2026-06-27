//! Win32 bindings. Nothing above this module talks to windows-rs directly.

pub mod autostart;
pub mod events;
pub mod hotkey;
pub mod keyboard;
pub mod monitor;
pub mod util;
pub mod window;

pub use autostart::AUTOSTART_FLAG;
pub use events::{DesktopEvent, EventHooks};
pub use hotkey::{FailedBinding, HotkeyRegistry};
pub use keyboard::KeyboardHook;
pub use monitor::{
    cursor_position, enumerate_monitors, monitor_at, monitor_for_window, monitor_under_cursor,
    primary_monitor, Monitor, MonitorId,
};
pub use window::{
    enumerate_manageable, enumerate_windows, foreground_window, NativeWindow, WindowId,
};
