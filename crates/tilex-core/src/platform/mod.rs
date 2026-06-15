//! Win32 bindings. Nothing above this module talks to windows-rs directly.

pub mod util;
pub mod window;

pub use window::{
    enumerate_manageable, enumerate_windows, foreground_window, NativeWindow, WindowId,
};
