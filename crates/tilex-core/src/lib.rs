//! Tilex window management core.
//!
//! Everything in this crate is UI-agnostic: it talks to the Win32 API directly
//! and exposes a small command/event surface for the shell to drive.

pub mod command;
pub mod config;
pub mod geometry;
pub mod hotkey;
pub mod layout;

#[cfg(windows)]
pub mod platform;

pub use command::{Action, Cycle};
pub use config::{Config, RuleAction, WindowRule};
pub use geometry::{Axis, Direction, Rect};
pub use hotkey::Binding;
pub use layout::{LayoutKind, LayoutOptions};
