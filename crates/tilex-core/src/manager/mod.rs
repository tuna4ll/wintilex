//! The window manager itself.
//!
//! [`WindowManager`] owns all the state and is deliberately single-threaded:
//! it is driven from the message loop in [`engine`], which is also the thread
//! the Win32 event hooks and hotkeys are installed on.

mod drag;
mod engine;
mod state;

pub use drag::DragOutcome;
pub use engine::{Engine, EngineHandle};
pub use state::{ManagedWindow, MonitorView, Snapshot, Workspace};

use std::collections::HashMap;

use crate::command::{Action, Cycle};
use crate::config::{Config, RuleAction, WindowFacts};
use crate::geometry::{Direction, Rect};
use crate::layout::{arrange, Arrangement, LayoutKind, RatioKey, MAX_RATIO, MIN_RATIO};
use crate::navigate;
use crate::platform::monitor::{enumerate_monitors, monitor_at, Monitor, MonitorId};
use crate::platform::window::{enumerate_manageable, NativeWindow, WindowId};

/// How far a split ratio moves per resize hotkey press.
const RATIO_STEP: f32 = 0.02;

pub struct WindowManager {
    config: Config,
    windows: HashMap<WindowId, ManagedWindow>,
    workspaces: Vec<Workspace>,
    focused: Option<WindowId>,
    /// The window the user is currently dragging, and where it started.
    drag: Option<(WindowId, Rect)>,
    /// Set while the manager is moving windows itself, so the resulting event
    /// storm is not mistaken for the user rearranging things.
    applying: bool,
}

impl WindowManager {
    pub fn new(config: Config) -> Self {
        let layout = config.layout;
        let workspaces = enumerate_monitors()
            .into_iter()
            .map(|monitor| Workspace::new(monitor, layout))
            .collect();

        let mut manager = Self {
            config,
            windows: HashMap::new(),
            workspaces,
            focused: None,
            drag: None,
            applying: false,
        };
        manager.refresh();
        manager
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn tiling_enabled(&self) -> bool {
        self.config.general.tiling_enabled
    }

    /// Swap in a new configuration and rebuild everything that depends on it.
    pub fn set_config(&mut self, config: Config) {
        let layout_changed = config.layout != self.config.layout;
        self.config = config;
        if layout_changed {
            for workspace in &mut self.workspaces {
                workspace.layout = self.config.layout;
                workspace.ratios.clear();
            }
        }
        self.refresh();
        self.apply();
    }

    // -- desktop scanning ---------------------------------------------------

    /// Re-read the display topology, keeping per-monitor state where the
    /// display is still there.
    pub fn refresh_monitors(&mut self) {
        let monitors = enumerate_monitors();
        if monitors.is_empty() {
            return;
        }

        let mut rebuilt = Vec::with_capacity(monitors.len());
        for monitor in monitors {
            match self.workspaces.iter().position(|w| w.monitor.id == monitor.id) {
                Some(index) => {
                    let mut workspace = self.workspaces.remove(index);
                    workspace.monitor = monitor;
                    workspace.arrangement = None;
                    rebuilt.push(workspace);
                }
                None => rebuilt.push(Workspace::new(monitor, self.config.layout)),
            }
        }

        // Windows that were on a display which is now gone move to the first
        // one that is left.
        let orphans: Vec<WindowId> =
            self.workspaces.iter().flat_map(|w| w.order.iter().copied()).collect();
        if let Some(first) = rebuilt.first_mut() {
            for id in orphans {
                first.order.push(id);
                if let Some(window) = self.windows.get_mut(&id) {
                    window.monitor = first.monitor.id.clone();
                }
            }
        }

        self.workspaces = rebuilt;
    }

    /// Scan the desktop and bring the tracked set in line with what is there.
    pub fn refresh(&mut self) {
        if self.workspaces.is_empty() {
            self.refresh_monitors();
        }

        let mut seen = Vec::with_capacity(self.windows.len() + 4);
        for native in enumerate_manageable() {
            let id = native.id();
            seen.push(id);
            if self.windows.contains_key(&id) {
                if let Some(window) = self.windows.get_mut(&id) {
                    window.sync(&native);
                }
                self.reassign_monitor(&native);
            } else {
                self.track(&native);
            }
        }

        let gone: Vec<WindowId> =
            self.windows.keys().copied().filter(|id| !seen.contains(id)).collect();
        for id in gone {
            self.forget(id);
        }
    }

    fn facts_action(&self, native: &NativeWindow) -> (RuleAction, Option<usize>) {
        let process = native.process_name();
        let class = native.class_name();
        let title = native.title();
        let facts = WindowFacts { process: &process, class: &class, title: &title };
        (self.config.action_for(facts), self.config.pinned_monitor(facts))
    }

    /// Start managing a window that just appeared.
    fn track(&mut self, native: &NativeWindow) -> bool {
        let (action, pinned) = self.facts_action(native);
        if action == RuleAction::Ignore {
            return false;
        }

        let index = pinned
            .filter(|index| *index < self.workspaces.len())
            .or_else(|| self.workspace_index_for(native))
            .unwrap_or(0);
        let Some(workspace) = self.workspaces.get_mut(index) else {
            return false;
        };

        let monitor_id = workspace.monitor.id.clone();
        let window = ManagedWindow::from_native(native, monitor_id, action);
        let floating = window.floating;
        let id = window.id;

        self.windows.insert(id, window);
        if !floating {
            let after = self.focused;
            if let Some(workspace) = self.workspaces.get_mut(index) {
                workspace.insert(id, after);
            }
        }
        log::debug!("tracking {id} ({action:?})");
        true
    }

    fn forget(&mut self, id: WindowId) {
        if self.windows.remove(&id).is_some() {
            for workspace in &mut self.workspaces {
                workspace.remove(id);
            }
            if self.focused == Some(id) {
                self.focused = None;
            }
            log::debug!("dropped {id}");
        }
    }

    /// Move a window between workspaces when it ends up on another display.
    fn reassign_monitor(&mut self, native: &NativeWindow) {
        let id = native.id();
        let Some(index) = self.workspace_index_for(native) else {
            return;
        };
        let Some(target) = self.workspaces.get(index).map(|w| w.monitor.id.clone()) else {
            return;
        };
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };
        if window.monitor == target {
            return;
        }

        window.monitor = target;
        let tiled = window.is_tiled();
        for workspace in &mut self.workspaces {
            workspace.remove(id);
        }
        if tiled {
            if let Some(workspace) = self.workspaces.get_mut(index) {
                workspace.insert(id, None);
            }
        }
    }

    fn workspace_index_for(&self, native: &NativeWindow) -> Option<usize> {
        let rect = native.frame_rect();
        let (x, y) = rect.center();
        let monitor = monitor_at(x, y)?;
        self.workspaces.iter().position(|w| w.monitor.id == monitor.id)
    }

    fn workspace_of(&self, id: WindowId) -> Option<usize> {
        self.workspaces.iter().position(|w| w.order.contains(&id))
    }

    /// The workspace the user is currently working on: the one holding the
    /// focused window, or failing that the one under the pointer.
    fn active_workspace(&self) -> usize {
        self.focused
            .and_then(|id| self.workspace_of(id))
            .or_else(|| {
                let (x, y) = crate::platform::monitor::cursor_position();
                let monitor = monitor_at(x, y)?;
                self.workspaces.iter().position(|w| w.monitor.id == monitor.id)
            })
            .unwrap_or(0)
    }

    // -- layout -------------------------------------------------------------

    /// Compute and apply the layout on every display.
    pub fn apply(&mut self) {
        if !self.config.general.tiling_enabled {
            return;
        }
        self.applying = true;
        for index in 0..self.workspaces.len() {
            self.apply_workspace(index);
        }
        self.applying = false;
    }

    fn apply_workspace(&mut self, index: usize) {
        let Some(workspace) = self.workspaces.get(index) else {
            return;
        };

        // Drop entries whose window went away or stopped being tileable.
        let tiled: Vec<WindowId> = workspace
            .order
            .iter()
            .copied()
            .filter(|id| self.windows.get(id).is_some_and(|w| w.is_tiled()))
            .collect();

        if tiled.is_empty() {
            if let Some(workspace) = self.workspaces.get_mut(index) {
                workspace.arrangement = None;
            }
            return;
        }

        let algorithm = workspace.layout.build();
        let mut options = self.config.layout_options;
        options.reversed = workspace.reversed;
        let area = workspace.monitor.work_area;

        let arrangement = arrange(algorithm.as_ref(), area, tiled.len(), &options, &workspace.ratios);

        for (position, id) in tiled.iter().enumerate() {
            let Some(tile) = arrangement.tiles.get(position).copied() else {
                continue;
            };
            let native = NativeWindow::from_id(*id);
            if !native.exists() {
                continue;
            }
            if native.is_maximized() {
                // A maximized window ignores SetWindowPos until it is restored.
                native.restore();
            }
            native.set_frame_rect(tile);
            if let Some(window) = self.windows.get_mut(id) {
                window.tile = Some(tile);
            }
        }

        if let Some(workspace) = self.workspaces.get_mut(index) {
            workspace.arrangement = Some(arrangement);
        }
    }

    // -- snapshot -----------------------------------------------------------

    pub fn snapshot(&self) -> Snapshot {
        let mut windows: Vec<ManagedWindow> = self.windows.values().cloned().collect();
        windows.sort_by(|a, b| a.process.cmp(&b.process).then(a.title.cmp(&b.title)));

        Snapshot {
            tiling_enabled: self.config.general.tiling_enabled,
            windows,
            monitors: self
                .workspaces
                .iter()
                .map(|workspace| MonitorView {
                    id: workspace.monitor.id.to_string(),
                    work_area: workspace.monitor.work_area,
                    is_primary: workspace.monitor.is_primary,
                    layout: workspace.layout,
                    tiled_windows: workspace
                        .order
                        .iter()
                        .filter(|id| self.windows.get(id).is_some_and(|w| w.is_tiled()))
                        .count(),
                })
                .collect(),
            focused: self.focused,
        }
    }

    pub fn monitors(&self) -> Vec<Monitor> {
        self.workspaces.iter().map(|w| w.monitor.clone()).collect()
    }

    // -- actions ------------------------------------------------------------

    /// Run one action. Returns `true` when the layout has to be re-applied.
    pub fn dispatch(&mut self, action: Action) -> bool {
        match action {
            Action::Focus(direction) => {
                self.focus_direction(direction);
                false
            }
            Action::FocusCycle(cycle) => {
                self.focus_cycle(cycle);
                false
            }
            Action::Move(direction) => self.move_direction(direction),
            Action::Grow(direction) => self.resize_focused(direction, RATIO_STEP),
            Action::Shrink(direction) => self.resize_focused(direction, -RATIO_STEP),
            Action::MoveToMonitor(direction) => self.move_to_monitor(direction),
            Action::Promote => self.promote_focused(),
            Action::ToggleFloating => self.toggle_floating(),
            Action::ToggleTiling => {
                self.config.general.tiling_enabled = !self.config.general.tiling_enabled;
                self.config.general.tiling_enabled
            }
            Action::CycleLayout => {
                let index = self.active_workspace();
                if let Some(workspace) = self.workspaces.get_mut(index) {
                    workspace.layout = workspace.layout.next();
                    workspace.ratios.clear();
                }
                true
            }
            Action::SetLayout(kind) => {
                let index = self.active_workspace();
                if let Some(workspace) = self.workspaces.get_mut(index) {
                    workspace.layout = kind;
                    workspace.ratios.clear();
                }
                true
            }
            Action::ToggleReversed => {
                let index = self.active_workspace();
                if let Some(workspace) = self.workspaces.get_mut(index) {
                    workspace.reversed = !workspace.reversed;
                }
                true
            }
            Action::ResetRatios => {
                let index = self.active_workspace();
                if let Some(workspace) = self.workspaces.get_mut(index) {
                    workspace.ratios.clear();
                }
                true
            }
            Action::Retile => {
                self.refresh_monitors();
                self.refresh();
                true
            }
            Action::ReloadConfig => {
                self.set_config(Config::load_or_default());
                false
            }
            Action::Minimize => {
                if let Some(id) = self.focused {
                    NativeWindow::from_id(id).minimize();
                }
                false
            }
            Action::Quit => false,
        }
    }

    fn focus_direction(&mut self, direction: Direction) {
        let Some(focused) = self.focused else {
            self.focus_first();
            return;
        };
        let Some(workspace_index) = self.workspace_of(focused) else {
            return;
        };

        let tiles = self.tiles_of(workspace_index);
        let ids = self.tiled_ids(workspace_index);
        let Some(position) = ids.iter().position(|id| *id == focused) else {
            return;
        };

        match navigate::neighbour(&tiles, position, direction) {
            Some(next) => self.focus_window(ids[next]),
            // Nothing that way on this display, so try the next one over.
            None => self.focus_monitor(direction),
        }
    }

    fn focus_monitor(&mut self, direction: Direction) {
        let current = self.active_workspace();
        let areas: Vec<Rect> = self.workspaces.iter().map(|w| w.monitor.work_area).collect();
        let Some(next) = navigate::neighbour_monitor(&areas, current, direction) else {
            return;
        };
        let ids = self.tiled_ids(next);
        if let Some(id) = ids.first() {
            self.focus_window(*id);
        }
    }

    fn focus_cycle(&mut self, cycle: Cycle) {
        let index = self.active_workspace();
        let ids = self.tiled_ids(index);
        if ids.is_empty() {
            return;
        }
        let current = self.focused.and_then(|id| ids.iter().position(|other| *other == id));
        let next = match (current, cycle) {
            (Some(position), Cycle::Next) => (position + 1) % ids.len(),
            (Some(position), Cycle::Previous) => (position + ids.len() - 1) % ids.len(),
            (None, _) => 0,
        };
        self.focus_window(ids[next]);
    }

    fn focus_first(&mut self) {
        let index = self.active_workspace();
        if let Some(id) = self.tiled_ids(index).first() {
            self.focus_window(*id);
        }
    }

    fn focus_window(&mut self, id: WindowId) {
        let native = NativeWindow::from_id(id);
        if !native.exists() {
            self.forget(id);
            return;
        }
        native.focus();
        self.focused = Some(id);

        if self.config.general.warp_cursor_to_focus {
            let rect = native.frame_rect();
            let (x, y) = rect.center();
            crate::platform::window::warp_cursor(x, y);
        }
    }

    fn move_direction(&mut self, direction: Direction) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        let Some(workspace_index) = self.workspace_of(focused) else {
            return false;
        };

        let tiles = self.tiles_of(workspace_index);
        let ids = self.tiled_ids(workspace_index);
        let Some(position) = ids.iter().position(|id| *id == focused) else {
            return false;
        };

        match navigate::neighbour(&tiles, position, direction) {
            Some(target) => {
                let (a, b) = (ids[position], ids[target]);
                if let Some(workspace) = self.workspaces.get_mut(workspace_index) {
                    let (Some(a), Some(b)) = (workspace.index_of(a), workspace.index_of(b)) else {
                        return false;
                    };
                    workspace.swap(a, b);
                }
                true
            }
            None => self.move_to_monitor(direction),
        }
    }

    fn move_to_monitor(&mut self, direction: Direction) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        let current = self.active_workspace();
        let areas: Vec<Rect> = self.workspaces.iter().map(|w| w.monitor.work_area).collect();
        let Some(next) = navigate::neighbour_monitor(&areas, current, direction) else {
            return false;
        };

        for workspace in &mut self.workspaces {
            workspace.remove(focused);
        }
        let Some(workspace) = self.workspaces.get_mut(next) else {
            return false;
        };
        workspace.insert(focused, None);
        let monitor_id = workspace.monitor.id.clone();
        let work_area = workspace.monitor.work_area;

        if let Some(window) = self.windows.get_mut(&focused) {
            window.monitor = monitor_id;
            // A floating window is not tiled, so it has to be carried over by
            // hand or it would stay on the old display.
            if window.floating {
                let native = NativeWindow::from_id(focused);
                let rect = native.frame_rect();
                let (cx, cy) = work_area.center();
                native.set_frame_rect(Rect::new(
                    cx - rect.width / 2,
                    cy - rect.height / 2,
                    rect.width,
                    rect.height,
                ));
            }
        }
        true
    }

    fn promote_focused(&mut self) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        let Some(index) = self.workspace_of(focused) else {
            return false;
        };
        self.workspaces.get_mut(index).is_some_and(|w| w.promote(focused))
    }

    fn toggle_floating(&mut self) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        let Some(window) = self.windows.get_mut(&focused) else {
            return false;
        };

        window.floating = !window.floating;
        let floating = window.floating;
        let monitor = window.monitor.clone();

        if floating {
            for workspace in &mut self.workspaces {
                workspace.remove(focused);
            }
            self.centre_floating(focused, &monitor);
        } else if let Some(index) =
            self.workspaces.iter().position(|w| w.monitor.id == monitor)
        {
            self.workspaces[index].insert(focused, None);
        }
        true
    }

    /// Give a window that has just started floating a sensible size instead of
    /// leaving it stretched to whatever tile it used to occupy.
    fn centre_floating(&self, id: WindowId, monitor: &MonitorId) {
        let Some(workspace) = self.workspaces.iter().find(|w| w.monitor.id == *monitor) else {
            return;
        };
        let area = workspace.monitor.work_area;
        let width = (area.width as f32 * 0.6) as i32;
        let height = (area.height as f32 * 0.7) as i32;
        let (cx, cy) = area.center();
        let native = NativeWindow::from_id(id);
        native.set_frame_rect(Rect::new(cx - width / 2, cy - height / 2, width, height));
        native.raise();
    }

    /// Grow or shrink the focused window by moving the split it sits against.
    fn resize_focused(&mut self, direction: Direction, delta: f32) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        let Some(workspace_index) = self.workspace_of(focused) else {
            return false;
        };
        let ids = self.tiled_ids(workspace_index);
        let Some(position) = ids.iter().position(|id| *id == focused) else {
            return false;
        };

        let Some(workspace) = self.workspaces.get(workspace_index) else {
            return false;
        };
        let Some(arrangement) = workspace.arrangement.as_ref() else {
            return false;
        };

        // Growing to the right means pushing the split on the window's right
        // edge further right, which is the split that has it on the first side.
        let on_first_side = direction.is_positive();
        let Some(tile) = arrangement.raw_tile(position) else {
            return false;
        };
        let edge = match direction {
            Direction::Left => tile.left(),
            Direction::Right => tile.right(),
            Direction::Up => tile.top(),
            Direction::Down => tile.bottom(),
        };

        let Some(split) =
            arrangement.find_split(position, direction.axis(), edge, on_first_side, 2)
        else {
            return false;
        };
        let key = split.key;
        let current = split.ratio_at(edge);
        let step = if on_first_side { delta } else { -delta };
        let next = (current + step).clamp(MIN_RATIO, MAX_RATIO);

        if let Some(workspace) = self.workspaces.get_mut(workspace_index) {
            workspace.ratios.set(key, next);
        }
        true
    }

    // -- helpers ------------------------------------------------------------

    fn tiled_ids(&self, workspace: usize) -> Vec<WindowId> {
        self.workspaces
            .get(workspace)
            .map(|w| {
                w.order
                    .iter()
                    .copied()
                    .filter(|id| self.windows.get(id).is_some_and(|w| w.is_tiled()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn tiles_of(&self, workspace: usize) -> Vec<Rect> {
        self.workspaces
            .get(workspace)
            .and_then(|w| w.arrangement.as_ref())
            .map(|arrangement| arrangement.tiles.clone())
            .unwrap_or_default()
    }

    pub fn set_layout_for_active(&mut self, kind: LayoutKind) -> bool {
        self.dispatch(Action::SetLayout(kind))
    }

    // -- accessors used by the event handling in `drag` ----------------------

    pub fn is_tracked(&self, id: WindowId) -> bool {
        self.windows.contains_key(&id)
    }

    pub fn is_tiled(&self, id: WindowId) -> bool {
        self.windows.get(&id).is_some_and(|window| window.is_tiled())
    }

    pub fn is_applying(&self) -> bool {
        self.applying
    }

    pub fn absorb_manual_resize(&self) -> bool {
        self.config.general.absorb_manual_resize
    }

    pub fn focused(&self) -> Option<WindowId> {
        self.focused
    }

    pub fn set_focused(&mut self, id: Option<WindowId>) {
        self.focused = id;
    }

    pub fn track_window(&mut self, native: &NativeWindow) -> bool {
        self.track(native)
    }

    pub fn forget_window(&mut self, id: WindowId) {
        self.forget(id);
    }

    pub fn update_minimized(&mut self, id: WindowId, minimized: bool) -> bool {
        match self.windows.get_mut(&id) {
            Some(window) if window.minimized != minimized => {
                window.minimized = minimized;
                if minimized {
                    window.tile = None;
                }
                true
            }
            _ => false,
        }
    }

    pub fn reassign_monitor_of(&mut self, id: WindowId) {
        let native = NativeWindow::from_id(id);
        if native.exists() {
            self.reassign_monitor(&native);
        }
    }

    pub fn assigned_tile(&self, id: WindowId) -> Option<Rect> {
        self.windows.get(&id).filter(|window| window.is_tiled()).and_then(|window| window.tile)
    }

    pub fn drag_target(&self) -> Option<WindowId> {
        self.drag.map(|(id, _)| id)
    }

    pub fn set_drag(&mut self, drag: Option<(WindowId, Rect)>) {
        self.drag = drag;
    }

    pub fn take_drag(&mut self) -> Option<(WindowId, Rect)> {
        self.drag.take()
    }

    /// Which workspace holds a window, and its position among the tiled ones.
    pub fn locate(&self, id: WindowId) -> Option<(usize, usize)> {
        let workspace = self.workspace_of(id)?;
        let position = self.tiled_ids(workspace).iter().position(|other| *other == id)?;
        Some((workspace, position))
    }

    pub fn arrangement_of(&self, workspace: usize) -> Option<Arrangement> {
        self.workspaces.get(workspace)?.arrangement.clone()
    }

    pub fn set_ratio(&mut self, workspace: usize, key: RatioKey, ratio: f32) {
        if let Some(workspace) = self.workspaces.get_mut(workspace) {
            workspace.ratios.set(key, ratio);
        }
    }

    pub fn workspace_at_point(&self, x: i32, y: i32) -> Option<usize> {
        let monitor = monitor_at(x, y)?;
        self.workspaces.iter().position(|w| w.monitor.id == monitor.id)
    }

    /// The tiled window whose tile contains a point.
    pub fn tile_at_point(&self, workspace: usize, x: i32, y: i32) -> Option<WindowId> {
        let tiles = self.tiles_of(workspace);
        let ids = self.tiled_ids(workspace);
        ids.into_iter().zip(tiles).find(|(_, tile)| tile.contains(x, y)).map(|(id, _)| id)
    }

    /// Move a window to another workspace, keeping its floating state.
    pub fn transfer(&mut self, id: WindowId, workspace: usize) {
        for existing in &mut self.workspaces {
            existing.remove(id);
        }
        let Some(target) = self.workspaces.get_mut(workspace) else {
            return;
        };
        let monitor_id = target.monitor.id.clone();
        if self.windows.get(&id).is_some_and(|window| !window.floating) {
            target.insert(id, None);
        }
        if let Some(window) = self.windows.get_mut(&id) {
            window.monitor = monitor_id;
        }
    }

    pub fn swap_windows(&mut self, workspace: usize, a: WindowId, b: WindowId) {
        if let Some(workspace) = self.workspaces.get_mut(workspace) {
            if let (Some(first), Some(second)) = (workspace.index_of(a), workspace.index_of(b)) {
                workspace.swap(first, second);
            }
        }
    }
}
