//! Tilex window management core.
//!
//! Everything in this crate is UI-agnostic: it talks to the Win32 API directly
//! and exposes a small command/event surface for the shell to drive.

pub mod geometry;

pub use geometry::{Axis, Direction, Rect};
