//! Thin, safe-ish wrapper over the HWND based window API.

use crate::geometry::Rect;
use crate::platform::util::wide_to_string;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, TRUE};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
    SetForegroundWindow, SetWindowPos, ShowWindow, GA_ROOTOWNER, GWL_EXSTYLE, GWL_STYLE,
    HWND_BOTTOM, HWND_TOP, SET_WINDOW_POS_FLAGS, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
    SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SW_MINIMIZE,
    SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, WS_CAPTION, WS_CHILD, WS_DISABLED,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Stable, thread-safe identifier for a native window.
///
/// `HWND` is a raw pointer and therefore not `Send`; every value that crosses a
/// thread boundary uses this instead and converts back on the Win32 side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub isize);

impl WindowId {
    pub fn raw(self) -> isize {
        self.0
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:x}", self.0)
    }
}

impl From<HWND> for WindowId {
    fn from(hwnd: HWND) -> Self {
        WindowId(hwnd.0 as isize)
    }
}

impl From<WindowId> for HWND {
    fn from(id: WindowId) -> Self {
        HWND(id.0 as *mut std::ffi::c_void)
    }
}

/// Window classes that belong to the shell and must never be touched.
const CLASS_BLOCKLIST: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "TaskListThumbnailWnd",
    "ForegroundStaging",
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
    "Windows.UI.Core.CoreWindow",
    "Windows.UI.Composition.DesktopWindowContentBridge",
    "Xaml_WindowedPopupClass",
    "TopLevelWindowForOverflowXamlIsland",
    "Microsoft.UI.Content.PopupWindowSiteBridge",
];

/// Processes that only ever put shell surfaces on screen.
const PROCESS_BLOCKLIST: &[&str] = &[
    "searchhost.exe",
    "searchapp.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "peopleexperiencehost.exe",
    "lockapp.exe",
];

// SWP_NOMOVE | SWP_NOSIZE, used when only the z-order should change.
const SWP_ZORDER_ONLY: SET_WINDOW_POS_FLAGS =
    SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0);

/// A live handle to one top-level window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWindow(HWND);

impl NativeWindow {
    pub fn new(hwnd: HWND) -> Self {
        Self(hwnd)
    }

    pub fn from_id(id: WindowId) -> Self {
        Self(id.into())
    }

    pub fn id(&self) -> WindowId {
        WindowId::from(self.0)
    }

    pub fn hwnd(&self) -> HWND {
        self.0
    }

    pub fn exists(&self) -> bool {
        unsafe { IsWindow(Some(self.0)).as_bool() }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.0).as_bool() }
    }

    pub fn is_minimized(&self) -> bool {
        unsafe { IsIconic(self.0).as_bool() }
    }

    pub fn is_maximized(&self) -> bool {
        unsafe { IsZoomed(self.0).as_bool() }
    }

    pub fn style(&self) -> u32 {
        unsafe { GetWindowLongPtrW(self.0, GWL_STYLE) as u32 }
    }

    pub fn ex_style(&self) -> u32 {
        unsafe { GetWindowLongPtrW(self.0, GWL_EXSTYLE) as u32 }
    }

    /// UWP and virtual-desktop hidden windows stay "visible" but are cloaked by
    /// DWM. Without this check the tree fills up with ghosts.
    pub fn is_cloaked(&self) -> bool {
        let mut cloaked: u32 = 0;
        let result = unsafe {
            DwmGetWindowAttribute(
                self.0,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
            )
        };
        result.is_ok() && cloaked != 0
    }

    pub fn title(&self) -> String {
        let mut buffer = [0u16; 512];
        let len = unsafe { GetWindowTextW(self.0, &mut buffer) };
        if len <= 0 {
            String::new()
        } else {
            wide_to_string(&buffer[..len as usize])
        }
    }

    pub fn class_name(&self) -> String {
        let mut buffer = [0u16; 256];
        let len = unsafe { GetClassNameW(self.0, &mut buffer) };
        if len <= 0 {
            String::new()
        } else {
            wide_to_string(&buffer[..len as usize])
        }
    }

    pub fn process_id(&self) -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(self.0, Some(&mut pid)) };
        pid
    }

    pub fn thread_id(&self) -> u32 {
        unsafe { GetWindowThreadProcessId(self.0, None) }
    }

    /// Executable file name, lowercased (`"code.exe"`). Empty when the process
    /// cannot be opened, which happens for elevated windows.
    pub fn process_name(&self) -> String {
        let pid = self.process_id();
        if pid == 0 {
            return String::new();
        }
        unsafe {
            let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return String::new();
            };
            let mut buffer = [0u16; 512];
            let mut len = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            if result.is_err() {
                return String::new();
            }
            wide_to_string(&buffer[..len as usize])
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        }
    }

    /// The window rect including the invisible resize border.
    pub fn outer_rect(&self) -> Rect {
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(self.0, &mut rect) }.is_err() {
            return Rect::ZERO;
        }
        Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
    }

    /// The rect the user actually sees. Since Vista the window rect is a few
    /// pixels larger than the drawn frame, so tiling against it leaves gaps.
    pub fn frame_rect(&self) -> Rect {
        let mut rect = RECT::default();
        let result = unsafe {
            DwmGetWindowAttribute(
                self.0,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut RECT as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
        };
        if result.is_err() {
            return self.outer_rect();
        }
        Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
    }

    /// Difference between the window rect and the visible frame, per edge.
    fn shadow_offsets(&self) -> (i32, i32, i32, i32) {
        let outer = self.outer_rect();
        let frame = self.frame_rect();
        if frame.is_empty() || outer.is_empty() {
            return (0, 0, 0, 0);
        }
        (
            outer.left() - frame.left(),
            outer.top() - frame.top(),
            outer.right() - frame.right(),
            outer.bottom() - frame.bottom(),
        )
    }

    /// Place the *visible* frame at `target`, compensating for the shadow.
    pub fn set_frame_rect(&self, target: Rect) -> bool {
        let (dl, dt, dr, db) = self.shadow_offsets();
        let adjusted = Rect::from_edges(
            target.left() + dl,
            target.top() + dt,
            target.right() + dr,
            target.bottom() + db,
        );
        self.set_outer_rect(adjusted)
    }

    pub fn set_outer_rect(&self, target: Rect) -> bool {
        let flags = SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOCOPYBITS | SWP_NOSENDCHANGING;
        self.apply_pos(target, flags)
    }

    /// Same as `set_outer_rect` but never blocks on an unresponsive window.
    pub fn set_outer_rect_async(&self, target: Rect) -> bool {
        let flags = SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOCOPYBITS | SWP_ASYNCWINDOWPOS;
        self.apply_pos(target, flags)
    }

    fn apply_pos(&self, target: Rect, flags: SET_WINDOW_POS_FLAGS) -> bool {
        unsafe {
            SetWindowPos(
                self.0,
                None,
                target.x,
                target.y,
                target.width.max(1),
                target.height.max(1),
                flags,
            )
        }
        .is_ok()
    }

    pub fn minimize(&self) {
        unsafe { let _ = ShowWindow(self.0, SW_MINIMIZE); };
    }

    pub fn restore(&self) {
        unsafe { let _ = ShowWindow(self.0, SW_RESTORE); };
    }

    pub fn maximize(&self) {
        unsafe { let _ = ShowWindow(self.0, SW_SHOWMAXIMIZED); };
    }

    pub fn show_no_activate(&self) {
        unsafe { let _ = ShowWindow(self.0, SW_SHOWNOACTIVATE); };
    }

    pub fn raise(&self) {
        unsafe {
            let _ = SetWindowPos(self.0, Some(HWND_TOP), 0, 0, 0, 0, SWP_ZORDER_ONLY);
        }
    }

    pub fn lower(&self) {
        unsafe {
            let _ = SetWindowPos(self.0, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_ZORDER_ONLY);
        }
    }

    /// Focus the window. `SetForegroundWindow` is refused unless the calling
    /// thread owns the foreground, so temporarily attach to the input queue of
    /// the thread that does.
    pub fn focus(&self) -> bool {
        if self.is_minimized() {
            self.restore();
        }
        unsafe {
            if SetForegroundWindow(self.0).as_bool() {
                return true;
            }

            let foreground = GetForegroundWindow();
            let target_thread = self.thread_id();
            let current_thread = GetCurrentThreadId();
            let foreground_thread = if foreground.is_invalid() {
                0
            } else {
                GetWindowThreadProcessId(foreground, None)
            };

            let mut attached = Vec::new();
            for thread in [foreground_thread, target_thread] {
                if thread != 0 && thread != current_thread && !attached.contains(&thread) {
                    if AttachThreadInput(current_thread, thread, true).as_bool() {
                        attached.push(thread);
                    }
                }
            }

            let ok = SetForegroundWindow(self.0).as_bool();

            for thread in attached {
                let _ = AttachThreadInput(current_thread, thread, false);
            }
            ok
        }
    }

    pub fn is_foreground(&self) -> bool {
        unsafe { GetForegroundWindow() == self.0 }
    }

    /// The owner window, if this is a dialog or tool window of another one.
    pub fn root_owner(&self) -> NativeWindow {
        NativeWindow(unsafe { GetAncestor(self.0, GA_ROOTOWNER) })
    }

    /// Whether Tilex should track this window at all.
    ///
    /// Minimized windows still pass: they are managed, just not tiled right now.
    pub fn is_manageable(&self) -> bool {
        if !self.exists() || !self.is_visible() || self.is_cloaked() {
            return false;
        }

        let style = self.style();
        let ex_style = self.ex_style();

        if style & WS_CHILD.0 != 0 || style & WS_DISABLED.0 != 0 {
            return false;
        }
        if ex_style & WS_EX_NOACTIVATE.0 != 0 {
            return false;
        }
        // Tool windows are palettes unless they opt into the taskbar.
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 && ex_style & WS_EX_APPWINDOW.0 == 0 {
            return false;
        }
        // Owned windows are dialogs of some other window.
        if self.root_owner().0 != self.0 && ex_style & WS_EX_APPWINDOW.0 == 0 {
            return false;
        }
        // Without a caption there is nothing meaningful to resize.
        if style & WS_CAPTION.0 == 0 {
            return false;
        }

        let class = self.class_name();
        if CLASS_BLOCKLIST.contains(&class.as_str()) {
            return false;
        }
        if PROCESS_BLOCKLIST.contains(&self.process_name().as_str()) {
            return false;
        }
        if self.title().is_empty() {
            return false;
        }

        let rect = self.outer_rect();
        rect.width > 32 && rect.height > 32
    }
}

/// Every top-level window on the desktop, in z-order (topmost first).
pub fn enumerate_windows() -> Vec<NativeWindow> {
    let mut windows: Vec<NativeWindow> = Vec::with_capacity(64);
    unsafe {
        let _ = EnumWindows(
            Some(enum_proc),
            LPARAM(&mut windows as *mut Vec<NativeWindow> as isize),
        );
    }
    windows
}

/// Every window Tilex is willing to manage.
pub fn enumerate_manageable() -> Vec<NativeWindow> {
    enumerate_windows()
        .into_iter()
        .filter(|w| w.is_manageable())
        .collect()
}

pub fn foreground_window() -> Option<NativeWindow> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        None
    } else {
        Some(NativeWindow(hwnd))
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let collected = unsafe { &mut *(lparam.0 as *mut Vec<NativeWindow>) };
    collected.push(NativeWindow(hwnd));
    TRUE
}
