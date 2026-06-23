//! Display enumeration.
//!
//! Layouts are always computed against the *work area* so the taskbar and any
//! other appbar keeps its space.

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::platform::util::wide_to_string;
use crate::platform::window::NativeWindow;

use windows::Win32::Foundation::{LPARAM, POINT, RECT, TRUE};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, MONITORINFOF_PRIMARY};

/// Identifies a display across re-enumerations.
///
/// `HMONITOR` values are recycled when the display topology changes, so the
/// device name (`\\.\DISPLAY1`) is what actually keeps a workspace attached to
/// the right screen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MonitorId(pub String);

impl std::fmt::Display for MonitorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub id: MonitorId,
    /// Full display bounds in virtual-screen coordinates.
    pub bounds: Rect,
    /// Bounds minus the taskbar and any other reserved edge.
    pub work_area: Rect,
    pub is_primary: bool,
    pub dpi: u32,
    handle: isize,
}

impl Monitor {
    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }
}

/// All connected displays, primary first, then left to right.
pub fn enumerate_monitors() -> Vec<Monitor> {
    let mut monitors: Vec<Monitor> = Vec::with_capacity(4);
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_proc),
            LPARAM(&mut monitors as *mut Vec<Monitor> as isize),
        );
    }
    monitors.sort_by(|a, b| {
        b.is_primary
            .cmp(&a.is_primary)
            .then(a.bounds.x.cmp(&b.bounds.x))
            .then(a.bounds.y.cmp(&b.bounds.y))
    });
    monitors
}

/// The display a window sits on, falling back to the nearest one.
pub fn monitor_for_window(window: &NativeWindow) -> Option<Monitor> {
    let handle = unsafe { MonitorFromWindow(window.hwnd(), MONITOR_DEFAULTTONEAREST) };
    monitor_from_handle(handle)
}

/// The display a point sits on.
pub fn monitor_at(x: i32, y: i32) -> Option<Monitor> {
    let handle = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    monitor_from_handle(handle)
}

pub fn primary_monitor() -> Option<Monitor> {
    let handle = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    monitor_from_handle(handle)
}

pub fn cursor_position() -> (i32, i32) {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_ok() {
        (point.x, point.y)
    } else {
        (0, 0)
    }
}

pub fn monitor_under_cursor() -> Option<Monitor> {
    let (x, y) = cursor_position();
    monitor_at(x, y)
}

fn monitor_from_handle(handle: HMONITOR) -> Option<Monitor> {
    if handle.is_invalid() {
        return None;
    }

    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    let ok = unsafe { GetMonitorInfoW(handle, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if !ok.as_bool() {
        return None;
    }

    let device = wide_to_string(&info.szDevice);
    let bounds = rect_from(info.monitorInfo.rcMonitor);
    let work_area = rect_from(info.monitorInfo.rcWork);

    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let _ = unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };

    Some(Monitor {
        id: MonitorId(device),
        bounds,
        work_area,
        is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        dpi: dpi_x,
        handle: handle.0 as isize,
    })
}

fn rect_from(rect: RECT) -> Rect {
    Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
}

unsafe extern "system" fn monitor_proc(
    handle: HMONITOR,
    _hdc: HDC,
    _clip: *mut RECT,
    lparam: LPARAM,
) -> windows::core::BOOL {
    let collected = unsafe { &mut *(lparam.0 as *mut Vec<Monitor>) };
    if let Some(monitor) = monitor_from_handle(handle) {
        collected.push(monitor);
    }
    TRUE
}

/// Kept so callers can round-trip a monitor back into a raw handle.
impl Monitor {
    pub fn raw_handle(&self) -> HMONITOR {
        HMONITOR(self.handle as *mut std::ffi::c_void)
    }
}

