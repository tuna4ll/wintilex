//! What the manager knows about the desktop.

use serde::Serialize;

use crate::config::RuleAction;
use crate::geometry::Rect;
use crate::layout::{Arrangement, LayoutKind, Ratios};
use crate::platform::monitor::{Monitor, MonitorId};
use crate::platform::window::{NativeWindow, WindowId};

/// A window Tilex is tracking.
#[derive(Debug, Clone, Serialize)]
pub struct ManagedWindow {
    #[serde(serialize_with = "serialize_id")]
    pub id: WindowId,
    pub title: String,
    pub class: String,
    pub process: String,
    /// Taken out of the layout, either by a rule or by the user.
    pub floating: bool,
    pub minimized: bool,
    /// The display the window currently belongs to.
    pub monitor: MonitorId,
    /// Where the layout last put it, if it is tiled.
    pub tile: Option<Rect>,
}

fn serialize_id<S: serde::Serializer>(id: &WindowId, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&id.to_string())
}

impl ManagedWindow {
    pub fn from_native(window: &NativeWindow, monitor: MonitorId, action: RuleAction) -> Self {
        Self {
            id: window.id(),
            title: window.title(),
            class: window.class_name(),
            process: window.process_name(),
            floating: action == RuleAction::Float,
            minimized: window.is_minimized(),
            monitor,
            tile: None,
        }
    }

    /// Whether the window should take part in the layout right now.
    pub fn is_tiled(&self) -> bool {
        !self.floating && !self.minimized
    }

    /// Refresh the fields that change while a window is open.
    pub fn sync(&mut self, window: &NativeWindow) {
        self.title = window.title();
        self.minimized = window.is_minimized();
    }
}

/// One display and the windows on it.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub monitor: Monitor,
    /// Tiling order. The first entry is the main window in layouts that have
    /// one, and hotkeys reorder this list rather than moving pixels directly.
    pub order: Vec<WindowId>,
    pub layout: LayoutKind,
    pub ratios: Ratios,
    pub reversed: bool,
    /// The last arrangement that was applied, used to turn a drag back into a
    /// ratio change.
    pub arrangement: Option<Arrangement>,
}

impl Workspace {
    pub fn new(monitor: Monitor, layout: LayoutKind) -> Self {
        Self {
            monitor,
            order: Vec::new(),
            layout,
            ratios: Ratios::new(),
            reversed: false,
            arrangement: None,
        }
    }

    pub fn index_of(&self, id: WindowId) -> Option<usize> {
        self.order.iter().position(|other| *other == id)
    }

    pub fn remove(&mut self, id: WindowId) -> bool {
        match self.index_of(id) {
            Some(index) => {
                self.order.remove(index);
                self.arrangement = None;
                true
            }
            None => false,
        }
    }

    /// Insert a new window right after the focused one so it lands next to what
    /// the user was looking at instead of at the end of the list.
    pub fn insert(&mut self, id: WindowId, after: Option<WindowId>) {
        if self.order.contains(&id) {
            return;
        }
        match after.and_then(|other| self.index_of(other)) {
            Some(index) => self.order.insert(index + 1, id),
            None => self.order.push(id),
        }
        self.arrangement = None;
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a < self.order.len() && b < self.order.len() && a != b {
            self.order.swap(a, b);
            self.arrangement = None;
        }
    }

    /// Move a window to the front of the order.
    pub fn promote(&mut self, id: WindowId) -> bool {
        match self.index_of(id) {
            Some(0) | None => false,
            Some(index) => {
                let id = self.order.remove(index);
                self.order.insert(0, id);
                self.arrangement = None;
                true
            }
        }
    }
}

/// A read-only view of the manager, handed to the settings window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub tiling_enabled: bool,
    pub windows: Vec<ManagedWindow>,
    pub monitors: Vec<MonitorView>,
    #[serde(serialize_with = "serialize_optional_id")]
    pub focused: Option<WindowId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorView {
    pub id: String,
    pub work_area: Rect,
    pub is_primary: bool,
    pub layout: LayoutKind,
    pub tiled_windows: usize,
}

fn serialize_optional_id<S: serde::Serializer>(
    id: &Option<WindowId>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match id {
        Some(id) => serializer.serialize_some(&id.to_string()),
        None => serializer.serialize_none(),
    }
}
