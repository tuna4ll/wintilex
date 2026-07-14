//! Desktop event hooks.
//!
//! `SetWinEventHook` delivers its callbacks on the thread that installed the
//! hook, while that thread is pumping messages. The callback therefore only
//! pushes onto a thread-local queue and pokes the message loop awake; all the
//! real work happens back in the loop where the manager state lives.

use std::cell::RefCell;
use std::collections::VecDeque;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    PostThreadMessageW, CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART,
    EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use crate::platform::window::WindowId;

/// Posted to the hook thread whenever the queue gains an entry.
pub const WM_WINTILEX_EVENT: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

/// Something happened on the desktop that the manager may care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEvent {
    /// A window appeared, or came back from being cloaked.
    Shown(WindowId),
    /// A window disappeared, was cloaked, or was destroyed.
    Hidden(WindowId),
    Destroyed(WindowId),
    Focused(WindowId),
    Minimized(WindowId),
    Restored(WindowId),
    /// The user grabbed a title bar or a resize edge.
    DragStarted(WindowId),
    /// The user let go. This is where a manual resize gets folded back in.
    DragFinished(WindowId),
    /// A window moved or resized without the user dragging it.
    Moved(WindowId),
    /// Displays were added, removed or rearranged.
    DisplayChanged,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<DesktopEvent>> = const { RefCell::new(VecDeque::new()) };
    static OWNER_THREAD: RefCell<u32> = const { RefCell::new(0) };
}

/// Installed hooks, unhooked again on drop.
pub struct EventHooks {
    handles: Vec<HWINEVENTHOOK>,
}

impl EventHooks {
    /// Install the hooks on the calling thread.
    ///
    /// The thread must pump messages for the callbacks to ever run.
    pub fn install() -> EventHooks {
        OWNER_THREAD.with(|owner| *owner.borrow_mut() = unsafe { GetCurrentThreadId() });

        // Grouped into contiguous ranges so the system has fewer hooks to call.
        let ranges = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ];

        let handles = ranges
            .into_iter()
            .map(|(low, high)| unsafe {
                SetWinEventHook(
                    low,
                    high,
                    None,
                    Some(callback),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            })
            .filter(|handle| !handle.is_invalid())
            .collect::<Vec<_>>();

        log::debug!("installed {} win event hooks", handles.len());
        EventHooks { handles }
    }

    /// Take everything the callback has collected since the last call.
    pub fn drain(&self) -> Vec<DesktopEvent> {
        QUEUE.with(|queue| queue.borrow_mut().drain(..).collect())
    }
}

impl Drop for EventHooks {
    fn drop(&mut self) {
        for handle in self.handles.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(handle);
            }
        }
    }
}

/// Push an event from outside the hook, used for display changes seen through
/// the window procedure rather than an accessibility event.
pub fn push_event(event: DesktopEvent) {
    QUEUE.with(|queue| queue.borrow_mut().push_back(event));
    wake_hook_thread();
}

/// Wake the hook thread so it drains its queues. Also used by the keyboard
/// hook, which lives on the same thread.
pub fn wake_hook_thread() {
    let thread = OWNER_THREAD.with(|owner| *owner.borrow());
    if thread != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread, WM_WINTILEX_EVENT, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _timestamp: u32,
) {
    // Accessibility events also fire for menus, carets and scroll bars; only
    // the window object itself is interesting here.
    if id_object != OBJID_WINDOW.0 || id_child != CHILDID_SELF as i32 || hwnd.is_invalid() {
        return;
    }

    let id = WindowId::from(hwnd);
    let translated = match event {
        EVENT_OBJECT_SHOW | EVENT_OBJECT_UNCLOAKED => DesktopEvent::Shown(id),
        EVENT_OBJECT_HIDE | EVENT_OBJECT_CLOAKED => DesktopEvent::Hidden(id),
        EVENT_OBJECT_DESTROY => DesktopEvent::Destroyed(id),
        EVENT_SYSTEM_FOREGROUND => DesktopEvent::Focused(id),
        EVENT_SYSTEM_MINIMIZESTART => DesktopEvent::Minimized(id),
        EVENT_SYSTEM_MINIMIZEEND => DesktopEvent::Restored(id),
        EVENT_SYSTEM_MOVESIZESTART => DesktopEvent::DragStarted(id),
        EVENT_SYSTEM_MOVESIZEEND => DesktopEvent::DragFinished(id),
        EVENT_OBJECT_LOCATIONCHANGE => DesktopEvent::Moved(id),
        _ => return,
    };

    QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        // Location changes arrive by the hundred while a window is being
        // dragged. Collapsing the repeats keeps the queue from growing without
        // losing the fact that the window moved.
        if matches!(translated, DesktopEvent::Moved(_)) && queue.back() == Some(&translated) {
            return;
        }
        queue.push_back(translated);
    });

    wake_hook_thread();
}
