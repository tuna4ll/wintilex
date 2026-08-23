//! Turning a snapshot into the pieces of text the bar draws.
//!
//! Nothing in here touches Win32, which is what makes the interesting part of
//! the bar testable.

use std::collections::HashMap;

use wintilex_core::config::{BarConfig, BarModule};
use wintilex_core::manager::{ManagedWindow, MonitorView, Snapshot};
use wintilex_core::platform::window::WindowId;
use wintilex_core::LayoutKind;

use crate::system::Battery;

/// How a segment is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    /// Plain text on the bar background.
    Normal,
    /// Plain text, dimmed.
    Muted,
    /// Text in a filled pill.
    Pill,
    /// The pill for whatever has the focus.
    Focused,
    /// Something the user should notice.
    Urgent,
}

/// What clicking a segment does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Focus(WindowId),
    CycleLayout,
    ToggleTiling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub emphasis: Emphasis,
    pub action: Option<Act>,
    /// Give up width first when the bar runs out of room.
    pub flexible: bool,
}

impl Segment {
    fn new(text: impl Into<String>, emphasis: Emphasis) -> Segment {
        Segment { text: text.into(), emphasis, action: None, flexible: false }
    }

    fn on_click(mut self, action: Act) -> Segment {
        self.action = Some(action);
        self
    }

    fn flexible(mut self) -> Segment {
        self.flexible = true;
        self
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sections {
    pub left: Vec<Segment>,
    pub center: Vec<Segment>,
    pub right: Vec<Segment>,
}

/// Everything on the bar that did not come from the manager.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Readings {
    pub cpu: u32,
    pub memory: u32,
    pub battery: Option<Battery>,
}

type Index<'a> = HashMap<WindowId, &'a ManagedWindow>;

/// Build the three sections for one display.
pub fn build(
    snapshot: &Snapshot,
    config: &BarConfig,
    monitor: &MonitorView,
    number: usize,
    readings: &Readings,
    clock: &str,
) -> Sections {
    let by_id: Index = snapshot.windows.iter().map(|window| (window.id, window)).collect();

    let section = |modules: &[BarModule]| -> Vec<Segment> {
        modules
            .iter()
            .flat_map(|module| {
                build_module(*module, snapshot, monitor, number, readings, clock, &by_id)
            })
            .collect()
    };

    Sections {
        left: section(&config.left),
        center: section(&config.center),
        right: section(&config.right),
    }
}

fn build_module(
    module: BarModule,
    snapshot: &Snapshot,
    monitor: &MonitorView,
    number: usize,
    readings: &Readings,
    clock: &str,
    by_id: &Index,
) -> Vec<Segment> {
    match module {
        BarModule::Layout => {
            let label = short_layout(monitor.layout);
            vec![Segment::new(label, Emphasis::Normal).on_click(Act::CycleLayout)]
        }
        BarModule::Windows => window_pills(snapshot, monitor, by_id),
        BarModule::Title => match focused_on(snapshot, monitor, by_id) {
            Some(window) => vec![Segment::new(window.title.clone(), Emphasis::Normal).flexible()],
            None => Vec::new(),
        },
        // Only worth the space when it has something to say.
        BarModule::Tiling if snapshot.tiling_enabled => Vec::new(),
        BarModule::Tiling => {
            vec![Segment::new("paused", Emphasis::Urgent).on_click(Act::ToggleTiling)]
        }
        BarModule::Monitor => vec![Segment::new(number.to_string(), Emphasis::Muted)],
        BarModule::Cpu => vec![Segment::new(format!("cpu {}%", readings.cpu), Emphasis::Muted)],
        BarModule::Memory => {
            vec![Segment::new(format!("ram {}%", readings.memory), Emphasis::Muted)]
        }
        BarModule::Battery => match readings.battery {
            Some(battery) => {
                let charging = if battery.charging { "+" } else { "" };
                let emphasis = if battery.percent <= 15 && !battery.charging {
                    Emphasis::Urgent
                } else {
                    Emphasis::Muted
                };
                vec![Segment::new(format!("{charging}{}%", battery.percent), emphasis)]
            }
            None => Vec::new(),
        },
        BarModule::Clock => vec![Segment::new(clock, Emphasis::Normal)],
    }
}

/// One pill per window on this display: the tiled ones in layout order, then
/// anything floating or minimized.
fn window_pills(snapshot: &Snapshot, monitor: &MonitorView, by_id: &Index) -> Vec<Segment> {
    let mut ordered: Vec<&ManagedWindow> =
        monitor.order.iter().filter_map(|id| by_id.get(id).copied()).collect();

    ordered.extend(
        snapshot
            .windows
            .iter()
            .filter(|window| window.monitor == monitor.id)
            .filter(|window| !monitor.order.contains(&window.id)),
    );

    ordered
        .iter()
        .enumerate()
        .map(|(index, window)| {
            let emphasis = match () {
                _ if snapshot.focused == Some(window.id) => Emphasis::Focused,
                _ if window.minimized => Emphasis::Muted,
                _ => Emphasis::Pill,
            };
            let label = format!("{} {}", index + 1, app_name(window));
            Segment::new(label, emphasis).on_click(Act::Focus(window.id))
        })
        .collect()
}

fn focused_on<'a>(
    snapshot: &Snapshot,
    monitor: &MonitorView,
    by_id: &Index<'a>,
) -> Option<&'a ManagedWindow> {
    let window = by_id.get(&snapshot.focused?).copied()?;
    (window.monitor == monitor.id).then_some(window)
}

/// `firefox.exe` reads better as `Firefox` on a bar.
fn app_name(window: &ManagedWindow) -> String {
    let stem = window.process.strip_suffix(".exe").unwrap_or(&window.process);
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => window.class.clone(),
    }
}

fn short_layout(layout: LayoutKind) -> &'static str {
    match layout {
        LayoutKind::Bsp => "bsp",
        LayoutKind::Columns => "cols",
        LayoutKind::Rows => "rows",
        LayoutKind::MainStack => "main",
        LayoutKind::Monocle => "mono",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wintilex_core::geometry::Rect;
    use wintilex_core::platform::monitor::MonitorId;

    const DISPLAY: &str = "\\\\.\\DISPLAY1";

    fn window(id: isize, process: &str, display: &str) -> ManagedWindow {
        ManagedWindow {
            id: WindowId(id),
            title: format!("{process} window"),
            class: "Test".into(),
            process: process.into(),
            floating: false,
            minimized: false,
            monitor: MonitorId(display.into()),
            tile: None,
        }
    }

    fn monitor(order: &[isize]) -> MonitorView {
        MonitorView {
            id: MonitorId(DISPLAY.into()),
            work_area: Rect::new(0, 0, 1920, 1080),
            is_primary: true,
            layout: LayoutKind::Bsp,
            tiled_windows: order.len(),
            order: order.iter().map(|id| WindowId(*id)).collect(),
            reversed: false,
        }
    }

    fn snapshot(windows: Vec<ManagedWindow>, focused: Option<isize>, order: &[isize]) -> Snapshot {
        Snapshot {
            tiling_enabled: true,
            windows,
            monitors: vec![monitor(order)],
            focused: focused.map(WindowId),
        }
    }

    fn index(snapshot: &Snapshot) -> Index<'_> {
        snapshot.windows.iter().map(|window| (window.id, window)).collect()
    }

    #[test]
    fn pills_follow_the_tiling_order() {
        let snap = snapshot(
            vec![window(2, "code.exe", DISPLAY), window(1, "firefox.exe", DISPLAY)],
            Some(1),
            &[1, 2],
        );

        let pills = window_pills(&snap, &snap.monitors[0], &index(&snap));
        assert_eq!(pills[0].text, "1 Firefox");
        assert_eq!(pills[0].emphasis, Emphasis::Focused);
        assert_eq!(pills[1].text, "2 Code");
        assert_eq!(pills[1].emphasis, Emphasis::Pill);
    }

    #[test]
    fn floating_windows_come_after_the_tiled_ones() {
        let mut floating = window(9, "calc.exe", DISPLAY);
        floating.floating = true;
        let snap = snapshot(vec![window(1, "code.exe", DISPLAY), floating], None, &[1]);

        let pills = window_pills(&snap, &snap.monitors[0], &index(&snap));
        assert_eq!(pills.len(), 2);
        assert_eq!(pills[1].text, "2 Calc");
    }

    #[test]
    fn another_display_is_left_out() {
        let snap = snapshot(vec![window(1, "code.exe", "\\\\.\\DISPLAY2")], Some(1), &[]);

        assert!(window_pills(&snap, &snap.monitors[0], &index(&snap)).is_empty());
        assert!(focused_on(&snap, &snap.monitors[0], &index(&snap)).is_none());
    }

    #[test]
    fn the_paused_badge_only_shows_while_tiling_is_off() {
        let mut snap = snapshot(vec![], None, &[]);
        let config = BarConfig { right: vec![BarModule::Tiling], ..Default::default() };
        let view = snap.monitors[0].clone();

        let sections = build(&snap, &config, &view, 1, &Readings::default(), "");
        assert!(sections.right.is_empty());

        snap.tiling_enabled = false;
        let sections = build(&snap, &config, &view, 1, &Readings::default(), "");
        assert_eq!(sections.right[0].text, "paused");
        assert_eq!(sections.right[0].action, Some(Act::ToggleTiling));
    }
}
