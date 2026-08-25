//! Registering the bar as a shell appbar.
//!
//! An appbar is how a window asks Windows for a strip of the screen that
//! nothing else may use. The work area shrinks by exactly that much, and since
//! WinTilex lays its tiles out inside the work area, the bar costs the layout
//! engine nothing at all: maximized windows and `Win`+`Arrow` snapping stay off
//! it as well.
//!
//! The one rule is that the registration has to be given back. A process that
//! exits without `ABM_REMOVE` leaves a dead strip along the edge of the screen
//! that nothing will reclaim until the next sign-in.

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABM_WINDOWPOSCHANGED, APPBARDATA,
};

use wintilex_core::config::BarPosition;
use wintilex_core::geometry::Rect;

/// Sent to the bar window when the shell wants it to move or get out of the
/// way. `wParam` carries the `ABN_*` notification.
pub const WM_APPBAR: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 12;

pub const ABN_STATECHANGE: usize = 0x0000;
pub const ABN_POSCHANGED: usize = 0x0001;
pub const ABN_FULLSCREENAPP: usize = 0x0002;

fn data(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APPBAR,
        uEdge: ABE_TOP,
        rc: RECT::default(),
        lParam: LPARAM(0),
    }
}

fn edge(position: BarPosition) -> u32 {
    match position {
        BarPosition::Top => ABE_TOP,
        BarPosition::Bottom => ABE_BOTTOM,
    }
}

fn to_rect(rect: Rect) -> RECT {
    RECT { left: rect.x, top: rect.y, right: rect.x + rect.width, bottom: rect.y + rect.height }
}

fn from_rect(rect: RECT) -> Rect {
    Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
}

/// Claim a slot. Returns false if the shell would not take the registration,
/// in which case the bar still runs, just without reserved space.
pub fn register(hwnd: HWND) -> bool {
    let mut payload = data(hwnd);
    unsafe { SHAppBarMessage(ABM_NEW, &mut payload) != 0 }
}

/// Give the slot back. Safe to call on a window that was never registered.
pub fn unregister(hwnd: HWND) {
    let mut payload = data(hwnd);
    unsafe {
        SHAppBarMessage(ABM_REMOVE, &mut payload);
    }
}

/// Ask for a strip and take whatever the shell hands back.
///
/// `ABM_QUERYPOS` is what moves the bar out from under a taskbar that is
/// already sitting on the same edge, so the answer, and not the request, is
/// where the window has to go.
pub fn place(hwnd: HWND, position: BarPosition, proposed: Rect) -> Rect {
    let mut payload = data(hwnd);
    payload.uEdge = edge(position);
    payload.rc = to_rect(proposed);

    unsafe {
        SHAppBarMessage(ABM_QUERYPOS, &mut payload);
    }

    // `ABM_QUERYPOS` only pushes the edge it owns; the height has to be put
    // back or the bar ends up as tall as whatever room was left.
    match position {
        BarPosition::Top => payload.rc.bottom = payload.rc.top + proposed.height,
        BarPosition::Bottom => payload.rc.top = payload.rc.bottom - proposed.height,
    }

    unsafe {
        SHAppBarMessage(ABM_SETPOS, &mut payload);
    }
    from_rect(payload.rc)
}

/// Tell the shell the window moved, so it can restack the other appbars.
pub fn moved(hwnd: HWND) {
    let mut payload = data(hwnd);
    unsafe {
        SHAppBarMessage(ABM_WINDOWPOSCHANGED, &mut payload);
    }
}
