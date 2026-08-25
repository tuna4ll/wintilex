//! Reacting to what happens on the desktop, and to the user dragging a window.
//!
//! The interesting part is [`WindowManager::finish_drag`]. A tiling manager
//! that simply snapped a window back after every drag would make manual
//! resizing impossible, so instead the drag is read as an intent: a move swaps
//! two windows, a resize is folded into the split ratio it pushed against and
//! stays that way.

use crate::geometry::{Axis, Rect};
use crate::layout::{Arrangement, RatioKey};
use crate::manager::WindowManager;
use crate::platform::events::DesktopEvent;
use crate::platform::monitor::cursor_position;
use crate::platform::window::{NativeWindow, WindowId};

/// How far an edge has to move before it counts as a deliberate resize.
const EDGE_TOLERANCE: i32 = 8;
/// How far a tiled window may drift from its tile before it is put back.
const DRIFT_TOLERANCE: i32 = 24;

/// What one desktop event asks for.
///
/// Moving windows and redrawing whatever is watching the manager are separate
/// concerns: a window that only changed its title still has to reach the bar,
/// but re-running the layout for it would be pure waste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reaction {
    /// The layout has to be applied again.
    pub relayout: bool,
    /// The snapshot changed.
    pub view: bool,
}

impl Reaction {
    /// Nothing to do.
    pub const IGNORED: Reaction = Reaction { relayout: false, view: false };
    /// Redraw, but leave the windows where they are.
    pub const VIEW: Reaction = Reaction { relayout: false, view: true };
    /// Lay the windows out again, which redraws as well.
    pub const RELAYOUT: Reaction = Reaction { relayout: true, view: true };

    fn relayout_if(needed: bool) -> Reaction {
        if needed {
            Reaction::RELAYOUT
        } else {
            Reaction::IGNORED
        }
    }

    pub fn merge(self, other: Reaction) -> Reaction {
        Reaction { relayout: self.relayout || other.relayout, view: self.view || other.view }
    }
}

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
    /// Feed one desktop event in and say what it asks for.
    pub fn handle_event(&mut self, event: DesktopEvent) -> Reaction {
        match event {
            DesktopEvent::Shown(id) => Reaction::relayout_if(self.on_shown(id)),
            DesktopEvent::Hidden(id) | DesktopEvent::Destroyed(id) => {
                Reaction::relayout_if(self.on_gone(id))
            }
            // A focus change never moves anything, but it does decide what the
            // bar highlights, so it still has to be published.
            DesktopEvent::Focused(id) => {
                if self.on_focused(id) {
                    Reaction::RELAYOUT
                } else {
                    Reaction::VIEW
                }
            }
            DesktopEvent::Minimized(id) => Reaction::relayout_if(self.set_minimized(id, true)),
            DesktopEvent::Restored(id) => Reaction::relayout_if(self.set_minimized(id, false)),
            DesktopEvent::DragStarted(id) => {
                self.begin_drag(id);
                Reaction::IGNORED
            }
            DesktopEvent::DragFinished(id) => {
                Reaction::relayout_if(self.finish_drag(id) != DragOutcome::Ignored)
            }
            DesktopEvent::Moved(id) => Reaction::relayout_if(self.on_moved(id)),
            DesktopEvent::Renamed(id) => {
                if self.on_renamed(id) {
                    Reaction::VIEW
                } else {
                    Reaction::IGNORED
                }
            }
            DesktopEvent::DisplayChanged => {
                self.refresh_monitors();
                self.refresh();
                Reaction::RELAYOUT
            }
        }
    }

    /// Pick up a new window title. Titles change while the user types, so this
    /// only reports back when the text really is different.
    fn on_renamed(&mut self, id: WindowId) -> bool {
        if !self.is_tracked(id) {
            return false;
        }
        let title = NativeWindow::from_id(id).title();
        self.rename_window(id, title)
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
            if self.absorb_manual_resize() && self.absorb_resize(id, end) {
                return DragOutcome::Resized;
            }
            // Absorbing failed, so put the window back where the layout wants
            // it rather than leaving a hole.
            return DragOutcome::Swapped;
        }

        let moved =
            (end.x - start.x).abs() > EDGE_TOLERANCE || (end.y - start.y).abs() > EDGE_TOLERANCE;
        if !moved {
            return DragOutcome::Ignored;
        }

        self.drop_at_cursor(id)
    }

    /// Turn a resize into a change of the split ratios it pushed against.
    ///
    /// Each edge that moved is matched to the split that produced it. An edge
    /// with no split behind it is an outer screen edge and is simply ignored.
    fn absorb_resize(&mut self, id: WindowId, end: Rect) -> bool {
        let Some((workspace_index, position)) = self.locate(id) else {
            return false;
        };
        let Some(arrangement) = self.arrangement_of(workspace_index) else {
            return false;
        };

        let updates = ratio_updates(&arrangement, position, end);
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

/// Work out which split ratios a resize was really asking to change.
///
/// The window at `position` used to fill `arrangement`'s tile and now fills
/// `resized`. Every edge that moved far enough is matched against the split
/// that produced it; an edge with no split behind it is a screen edge and is
/// left alone.
fn ratio_updates(
    arrangement: &Arrangement,
    position: usize,
    resized: Rect,
) -> Vec<(RatioKey, f32)> {
    let Some(tile) = arrangement.raw_tile(position) else {
        return Vec::new();
    };

    // Undo the gap so the edges line up with the split positions.
    let grown = resized.inset(-(arrangement.gap / 2));

    // Per edge: the axis it lives on, where it was, where it is now, and
    // whether this window sits before the split that made it.
    let edges = [
        (Axis::Horizontal, tile.left(), grown.left(), false),
        (Axis::Horizontal, tile.right(), grown.right(), true),
        (Axis::Vertical, tile.top(), grown.top(), false),
        (Axis::Vertical, tile.bottom(), grown.bottom(), true),
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
    updates
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

    use crate::layout::{arrange, Bsp, LayoutOptions, Ratios, ROOT_KEY};

    fn plain() -> LayoutOptions {
        LayoutOptions { gap: 0, outer_gap: 0, ..Default::default() }
    }

    /// Two windows side by side across a 1000x600 screen.
    fn two_up() -> Arrangement {
        arrange(&Bsp, Rect::new(0, 0, 1000, 600), 2, &plain(), &Ratios::new())
    }

    #[test]
    fn drift_ignores_small_differences() {
        let tile = Rect::new(0, 0, 800, 600);
        assert!(!drifted(tile, Rect::new(2, 2, 800, 600), DRIFT_TOLERANCE));
        assert!(drifted(tile, Rect::new(200, 0, 800, 600), DRIFT_TOLERANCE));
        assert!(drifted(tile, Rect::new(0, 0, 400, 600), DRIFT_TOLERANCE));
    }

    #[test]
    fn dragging_the_shared_edge_right_moves_the_split() {
        let arrangement = two_up();
        // The left window is pulled from 500 wide out to 700.
        let updates = ratio_updates(&arrangement, 0, Rect::new(0, 0, 700, 600));
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, ROOT_KEY);
        assert!((updates[0].1 - 0.7).abs() < 0.01, "got {}", updates[0].1);
    }

    #[test]
    fn the_same_split_is_found_from_either_side() {
        let arrangement = two_up();
        // Now the right window is dragged by its left edge, to the same place.
        let updates = ratio_updates(&arrangement, 1, Rect::new(700, 0, 300, 600));
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, ROOT_KEY);
        assert!((updates[0].1 - 0.7).abs() < 0.01, "got {}", updates[0].1);
    }

    #[test]
    fn outer_screen_edges_are_ignored() {
        let arrangement = two_up();
        // Dragging the left window's outer edge has no split behind it.
        let updates = ratio_updates(&arrangement, 0, Rect::new(-200, 0, 700, 600));
        assert_eq!(updates.len(), 0);
    }

    #[test]
    fn a_nudge_smaller_than_the_tolerance_changes_nothing() {
        let arrangement = two_up();
        let updates = ratio_updates(&arrangement, 0, Rect::new(0, 0, 504, 600));
        assert!(updates.is_empty());
    }

    #[test]
    fn resizing_a_corner_moves_both_splits() {
        // A 2x2 grid: the top-left window shares a vertical edge with the top
        // right one and a horizontal edge with the bottom left one.
        let arrangement = arrange(&Bsp, Rect::new(0, 0, 1000, 800), 4, &plain(), &Ratios::new());
        let updates = ratio_updates(&arrangement, 0, Rect::new(0, 0, 700, 300));
        assert_eq!(updates.len(), 2);
        assert!(updates.iter().any(|(key, _)| *key == ROOT_KEY));
    }

    #[test]
    fn absorbed_ratios_reproduce_the_dragged_size() {
        let area = Rect::new(0, 0, 1000, 600);
        let arrangement = arrange(&Bsp, area, 2, &plain(), &Ratios::new());
        let dragged = Rect::new(0, 0, 640, 600);

        let mut ratios = Ratios::new();
        for (key, ratio) in ratio_updates(&arrangement, 0, dragged) {
            ratios.set(key, ratio);
        }

        // Running the layout again has to land where the user let go.
        let again = arrange(&Bsp, area, 2, &plain(), &ratios);
        assert_eq!(again.tiles[0], dragged);
    }

    #[test]
    fn gaps_do_not_shift_the_absorbed_ratio() {
        let area = Rect::new(0, 0, 1000, 600);
        let options = LayoutOptions { gap: 8, outer_gap: 0, ..Default::default() };
        let arrangement = arrange(&Bsp, area, 2, &options, &Ratios::new());

        // The user drags the visible edge of the left tile to 696, which is the
        // gapped form of a 700 pixel split.
        let dragged = Rect::new(4, 4, 692, 592);
        let updates = ratio_updates(&arrangement, 0, dragged);
        assert_eq!(updates.len(), 1);
        assert!((updates[0].1 - 0.7).abs() < 0.01, "got {}", updates[0].1);
    }
}
