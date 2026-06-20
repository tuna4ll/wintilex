//! The one place every input path funnels into.
//!
//! Hotkeys, the tray menu and the settings window all end up sending an
//! `Action` to the manager thread, which keeps the behaviour identical no
//! matter where the request came from.

use serde::{Deserialize, Serialize};

use crate::geometry::Direction;
use crate::layout::LayoutKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum Action {
    /// Move the focus to the neighbouring window in a direction.
    Focus(Direction),
    /// Swap the focused window with its neighbour.
    Move(Direction),
    /// Grow the focused window towards a direction.
    Grow(Direction),
    /// Shrink the focused window from a direction.
    Shrink(Direction),
    /// Move the focus to the next or previous window in layout order.
    FocusCycle(Cycle),
    /// Send the focused window to the neighbouring monitor.
    MoveToMonitor(Direction),
    /// Put the focused window at the front of the layout order.
    Promote,
    /// Take the focused window out of the layout, or put it back.
    ToggleFloating,
    /// Pause or resume tiling everywhere.
    ToggleTiling,
    /// Switch the focused monitor to the next layout.
    CycleLayout,
    SetLayout(LayoutKind),
    /// Mirror the current layout.
    ToggleReversed,
    /// Forget any manual resize on the focused monitor.
    ResetRatios,
    /// Re-read the desktop and apply the layout again.
    Retile,
    /// Re-read the configuration file.
    ReloadConfig,
    /// Minimize the focused window.
    Minimize,
    /// Stop the manager thread.
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cycle {
    Next,
    Previous,
}

impl Action {
    /// Short label for the settings window and the tray menu.
    pub fn label(&self) -> String {
        match self {
            Action::Focus(d) => format!("Focus {}", direction_label(*d)),
            Action::Move(d) => format!("Move {}", direction_label(*d)),
            Action::Grow(d) => format!("Grow {}", direction_label(*d)),
            Action::Shrink(d) => format!("Shrink {}", direction_label(*d)),
            Action::FocusCycle(Cycle::Next) => "Focus next window".into(),
            Action::FocusCycle(Cycle::Previous) => "Focus previous window".into(),
            Action::MoveToMonitor(d) => format!("Send to {} monitor", direction_label(*d)),
            Action::Promote => "Promote to first".into(),
            Action::ToggleFloating => "Toggle floating".into(),
            Action::ToggleTiling => "Toggle tiling".into(),
            Action::CycleLayout => "Next layout".into(),
            Action::SetLayout(kind) => format!("Layout: {}", kind.label()),
            Action::ToggleReversed => "Mirror layout".into(),
            Action::ResetRatios => "Reset sizes".into(),
            Action::Retile => "Retile".into(),
            Action::ReloadConfig => "Reload config".into(),
            Action::Minimize => "Minimize window".into(),
            Action::Quit => "Quit".into(),
        }
    }
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Left => "left",
        Direction::Down => "down",
        Direction::Up => "up",
        Direction::Right => "right",
    }
}
