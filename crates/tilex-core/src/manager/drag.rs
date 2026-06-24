//! Reacting to what happens on the desktop, and to the user dragging a window.
//!
//! The interesting part is [`WindowManager::finish_drag`]. A tiling manager
//! that simply snapped a window back after every drag would make manual
//! resizing impossible, so instead the drag is read as an intent: a move swaps
//! two windows, a resize is folded into the split ratio it pushed against and
//! stays that way.

use crate::geometry::{Axis, Rect};
use crate::manager::WindowManager;
use crate::platform::events::DesktopEvent;
use crate::platform::monitor::cursor_position;
use crate::platform::window::{NativeWindow, WindowId};

/// How far an edge has to move before it counts as a deliberate resize.
const EDGE_TOLERANCE: i32 = 8;
/// How far a tiled window may drift from its tile before it is put back.
const DRIFT_TOLERANCE: i32 = 24;

/// What the manager decided a finished drag meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// Nothing changed.
    Ignored,
    /// The window swapped places with another one.
    Swapped,
    /// The window moved to a different display.
    Moved,
    /// One or more split ratios were adjusted.
    Resized,
}

impl WindowManager {
    /// Feed one desktop event in. Returns `true` if the layout needs re-running.
    pub fn handle_event(&mut self, event: DesktopEvent) -> bool {
        match event {
            DesktopEvent::Shown(id) => self.on_shown(id),
            DesktopEvent::Hidden(id) | DesktopEvent::Destroyed(id) => self.on_gone(id),
            DesktopEvent::Focused(id) => self.on_focused(id),
            DesktopEvent::Minimized(id) => self.set_minimized(id, true),
            DesktopEvent::Restored(id) => self.set_minimized(id, false),
            DesktopEvent::DragStarted(id) => {
                self.begin_drag(id);
                false
            }
            DesktopEvent::DragFinished(id) => self.finish_drag(id) != DragOutcome::Ignored,
            DesktopEvent::Moved(id) => self.on_moved(id),
            DesktopEvent::DisplayChanged => {
                self.refresh_monitors();
                self.refresh();
                true
            }
        }
    }

    fn on_shown(&mut self, id: WindowId) -> bool {
        if self.is_tracked(id) {
            return false;
        }
        let native = NativeWindow::from_id(id);
        if !native.is_manageable() {
            return false;
        }
        self.track_window(&native)
    }

    fn on_gone(&mut self, id: WindowId) -> bool {
        if !self.is_tracked(id) {
            return false;
        }
        let native = NativeWindow::from_id(id);
        // `EVENT_OBJECT_HIDE` also fires while a window is being restyled, so
        // check whether it really went away before dropping it.
        if native.exists() && native.is_visible() && !native.is_cloaked() {
            return false;
        }
        self.forget_window(id);
        true
    }

    fn on_focused(&mut self, id: WindowId) -> bool {
        if self.is_tracked(id) {
            self.set_focused(Some(id));
            return false;
        }
        // Some windows only become manageable once they are shown and focused.
        let native = NativeWindow::from_id(id);
        if !native.is_manageable() {
            return false;
        }
        let tracked = self.track_window(&native);
        if tracked {
            self.set_focused(Some(id));
        }
        tracked
    }

    fn on_moved(&mut self, id: WindowId) -> bool {
        if self.is_applying() || self.drag_target().is_some() || !self.tiling_enabled() {
            return false;
        }
        let Some(tile) = self.assigned_tile(id) else {
            return false;
        };
        let actual = NativeWindow::from_id(id).frame_rect();
        drifted(tile, actual, DRIFT_TOLERANCE)
    }

    fn set_minimized(&mut self, id: WindowId, minimized: bool) -> bool {
        self.update_minimized(id, minimized)
    }

    /// Remember where a window was when the user grabbed it.
    pub fn begin_drag(&mut self, id: WindowId) {
        if !self.is_tracked(id) {
            return;
        }
        let rect = NativeWindow::from_id(id).frame_rect();
        self.set_drag(Some((id, rect)));
    }

    /// Work out what the finished drag meant and record it.
    pub fn finish_drag(&mut self, id: WindowId) -> DragOutcome {
        let Some((dragged, start)) = self.take_drag() else {
            return DragOutcome::Ignored;
        };
        if dragged != id || !self.is_tracked(id) {
            return DragOutcome::Ignored;
        }

        let native = NativeWindow::from_id(id);
        if !native.exists() {
            return DragOutcome::Ignored;
        }
        let end = native.frame_rect();

        // Floating windows keep whatever the user did to them; they only need
        // their display recorded in case they were dragged across a boundary.
        if !self.is_tiled(id) {
            self.reassign_monitor_of(id);
            return DragOutcome::Ignored;
        }

        let resized = (end.width - start.width).abs() > EDGE_TOLERANCE
            || (end.height - start.height).abs() > EDGE_TOLERANCE;

        if resized {
            if self.absorb_manual_resize() && self.absorb_resize(id, start, end) {
                return DragOutcome::Resized;
            }
            // Absorbing failed, so put the window back where the layout wants
            // it rather than leaving a hole.
            return DragOutcome::Swapped;
        }

        let moved = (end.x - start.x).abs() > EDGE_TOLERANCE
            || (end.y - start.y).abs() > EDGE_TOLERANCE;
        if !moved {
            return DragOutcome::Ignored;
        }

        self.drop_at_cursor(id)
    }

    /// Turn a resize into a change of the split ratios it pushed against.
    ///
    /// Each edge that moved is matched to the split that produced it. An edge
    /// with no split behind it is an outer screen edge and is simply ignored.
    fn absorb_resize(&mut self, id: WindowId, start: Rect, end: Rect) -> bool {
        let Some((workspace_index, position)) = self.locate(id) else {
            return false;
        };
        let Some(arrangement) = self.arrangement_of(workspace_index) else {
            return false;
        };
        let Some(raw) = arrangement.raw_tile(position) else {
            return false;
        };

        // Undo the gap so the edges line up with the split positions.
        let half = arrangement.gap / 2;
        let grown = end.inset(-half);
        let _ = start;

        // For each edge: the axis it lives on, where it was, where it is now,
        // and whether this window sits before the split that made it.
        let edges = [
            (Axis::Horizontal, raw.left(), grown.left(), false),
            (Axis::Horizontal, raw.right(), grown.right(), true),
            (Axis::Vertical, raw.top(), grown.top(), false),
            (Axis::Vertical, raw.bottom(), grown.bottom(), true),
        ];

        let mut updates = Vec::new();
        for (axis, was, now, on_first_side) in edges {
            if (now - was).abs() <= EDGE_TOLERANCE {
                continue;
            }
            if let Some(split) =
                arrangement.find_split(position, axis, was, on_first_side, EDGE_TOLERANCE)
            {
                updates.push((split.key, split.ratio_at(now)));
            }
        }

        if updates.is_empty() {
            return false;
        }
        for (key, ratio) in updates {
            self.set_ratio(workspace_index, key, ratio);
        }
        true
    }

    /// Swap the dragged window with whatever tile it was dropped on.
    fn drop_at_cursor(&mut self, id: WindowId) -> DragOutcome {
        let (x, y) = cursor_position();

        let Some(target_workspace) = self.workspace_at_point(x, y) else {
            return DragOutcome::Ignored;
        };
        let source_workspace = self.locate(id).map(|(workspace, _)| workspace);

        if source_workspace != Some(target_workspace) {
            self.transfer(id, target_workspace);
            return DragOutcome::Moved;
        }

        match self.tile_at_point(target_workspace, x, y) {
            Some(other) if other != id => {
                self.swap_windows(target_workspace, id, other);
                DragOutcome::Swapped
            }
            // Dropped on empty space or on itself: the layout puts it back.
            _ => DragOutcome::Swapped,
        }
    }
}

/// Whether `actual` has drifted away from `tile` by more than `tolerance`.
fn drifted(tile: Rect, actual: Rect, tolerance: i32) -> bool {
    (tile.x - actual.x).abs() > tolerance
        || (tile.y - actual.y).abs() > tolerance
        || (tile.width - actual.width).abs() > tolerance
        || (tile.height - actual.height).abs() > tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_ignores_small_differences() {
        let tile = Rect::new(0, 0, 800, 600);
        assert!(!drifted(tile, Rect::new(2, 2, 800, 600), DRIFT_TOLERANCE));
        assert!(drifted(tile, Rect::new(200, 0, 800, 600), DRIFT_TOLERANCE));
        assert!(drifted(tile, Rect::new(0, 0, 400, 600), DRIFT_TOLERANCE));
    }
}
