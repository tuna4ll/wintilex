//! The manager thread.
//!
//! Win32 ties event hooks and global hotkeys to the thread that registered
//! them, and both are delivered through a message loop. So the manager gets a
//! thread of its own that owns all the state; everyone else talks to it by
//! sending a message and waking the loop up.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use parking_lot::RwLock;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, PostThreadMessageW, SetTimer,
    TranslateMessage, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY, WM_QUIT, WM_TIMER, WM_USER,
};

use crate::command::Action;
use crate::config::{Config, HotkeyBackend};
use crate::manager::state::Snapshot;
use crate::manager::WindowManager;
use crate::platform::events::{EventHooks, WM_TILEX_EVENT};
use crate::platform::hotkey::{FailedBinding, HotkeyRegistry};
use crate::platform::keyboard::KeyboardHook;

/// Sent when a message has been pushed onto the command channel.
const WM_TILEX_COMMAND: u32 = WM_APP + 2;

/// Events are collected for this long before the layout runs, so opening an
/// application that shows three windows in a row only re-tiles once.
const RELAYOUT_DELAY_MS: u32 = 45;

/// Slow sweep that catches display changes and any event the hooks missed.
const HOUSEKEEPING_MS: u32 = 2000;

/// Poll interval for focus-follows-mouse. Fast enough to feel immediate,
/// slow enough that it costs nothing when the pointer is not moving.
const FOCUS_FOLLOW_MS: u32 = 120;

enum Message {
    Run(Action),
    ApplyConfig(Box<Config>),
    Shutdown,
}

/// Handle to a running manager thread. Cheap to clone and safe to share.
#[derive(Clone)]
pub struct EngineHandle {
    inner: Arc<Inner>,
}

struct Inner {
    /// Filled in by the manager thread once its message queue exists.
    thread_id: AtomicU32,
    sender: Sender<Message>,
    snapshot: RwLock<Snapshot>,
    failed_hotkeys: RwLock<Vec<FailedBinding>>,
}

impl EngineHandle {
    /// Queue an action and wake the manager thread.
    pub fn dispatch(&self, action: Action) {
        let _ = self.inner.sender.send(Message::Run(action));
        self.wake(WM_TILEX_COMMAND);
    }

    /// Hand the manager a new configuration.
    pub fn apply_config(&self, config: Config) {
        let _ = self.inner.sender.send(Message::ApplyConfig(Box::new(config)));
        self.wake(WM_TILEX_COMMAND);
    }

    /// The most recent view of the desktop.
    pub fn snapshot(&self) -> Snapshot {
        self.inner.snapshot.read().clone()
    }

    /// Bindings Windows would not give us, so the settings window can say so.
    pub fn failed_hotkeys(&self) -> Vec<FailedBinding> {
        self.inner.failed_hotkeys.read().clone()
    }

    pub fn shutdown(&self) {
        let _ = self.inner.sender.send(Message::Shutdown);
        self.wake(WM_TILEX_COMMAND);
    }

    fn wake(&self, message: u32) {
        let thread_id = self.inner.thread_id.load(Ordering::Acquire);
        if thread_id == 0 {
            return;
        }
        unsafe {
            let _ = PostThreadMessageW(thread_id, message, WPARAM(0), LPARAM(0));
        }
    }
}

pub struct Engine;

impl Engine {
    /// Start the manager on its own thread and wait until it is listening.
    pub fn spawn(config: Config) -> EngineHandle {
        let (sender, receiver) = channel();
        let (ready_sender, ready_receiver) = channel();

        let snapshot = Snapshot {
            tiling_enabled: config.general.tiling_enabled,
            windows: Vec::new(),
            monitors: Vec::new(),
            focused: None,
        };
        let shared = Arc::new(Inner {
            thread_id: AtomicU32::new(0),
            sender,
            snapshot: RwLock::new(snapshot),
            failed_hotkeys: RwLock::new(Vec::new()),
        });

        let thread_state = Arc::clone(&shared);
        thread::Builder::new()
            .name("tilex-manager".into())
            .spawn(move || run(config, receiver, thread_state, ready_sender))
            .expect("failed to start the manager thread");

        // Block until the queue is ready, so the first dispatch cannot be lost.
        let _ = ready_receiver.recv();

        EngineHandle { inner: shared }
    }
}

fn run(config: Config, receiver: Receiver<Message>, shared: Arc<Inner>, ready: Sender<u32>) {
    unsafe {
        // Without this the work areas and window rects come back scaled, and
        // every layout on a high-DPI display would be a few pixels off.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Force the thread message queue into existence before anyone can post
        // to it, otherwise the first wake-up would be dropped on the floor.
        let mut probe = MSG::default();
        let _ = PeekMessageW(&mut probe, None, WM_USER, WM_USER, PM_NOREMOVE);
    }

    let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    shared.thread_id.store(thread_id, Ordering::Release);
    let _ = ready.send(thread_id);

    let hooks = EventHooks::install();
    let mut hotkeys = Hotkeys::new(config.general.hotkey_backend);
    let mut manager = WindowManager::new(config);

    hotkeys.rebind(&manager, &shared);
    manager.apply();
    publish(&manager, &shared);

    // A thread timer ignores the id it is given and hands back one of its own,
    // so the returned value is what `WM_TIMER` will actually carry.
    let housekeeping_timer = unsafe { SetTimer(None, 0, HOUSEKEEPING_MS, None) };
    let mut relayout_timer: Option<usize> = None;
    let mut follow_timer = set_follow_timer(None, manager.focus_follows_mouse());

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }

        let mut dirty = false;
        let mut stop = false;

        match message.message {
            WM_HOTKEY => {
                if let Some(action) = hotkeys.action_for_id(message.wParam.0 as i32) {
                    log::debug!("hotkey -> {action:?}");
                    if action == Action::Quit {
                        stop = true;
                    } else {
                        dirty |= manager.dispatch(action);
                    }
                }
            }
            WM_TILEX_COMMAND => {
                while let Ok(incoming) = receiver.try_recv() {
                    match incoming {
                        Message::Run(Action::Quit) | Message::Shutdown => stop = true,
                        Message::Run(action) => dirty |= manager.dispatch(action),
                        Message::ApplyConfig(config) => {
                            let backend = config.general.hotkey_backend;
                            manager.set_config(*config);
                            hotkeys.switch_backend(backend);
                            hotkeys.rebind(&manager, &shared);
                            follow_timer =
                                set_follow_timer(follow_timer, manager.focus_follows_mouse());
                            dirty = true;
                        }
                    }
                }
            }
            WM_TILEX_EVENT => {
                for event in hooks.drain() {
                    dirty |= manager.handle_event(event);
                }
                for action in hotkeys.drain() {
                    log::debug!("hotkey -> {action:?}");
                    if action == Action::Quit {
                        stop = true;
                    } else {
                        dirty |= manager.dispatch(action);
                    }
                }
            }
            WM_TIMER => {
                let fired = message.wParam.0;
                if Some(fired) == relayout_timer {
                    unsafe {
                        let _ = KillTimer(None, fired);
                    }
                    relayout_timer = None;
                    manager.apply();
                    publish(&manager, &shared);
                } else if Some(fired) == follow_timer {
                    manager.focus_under_cursor();
                } else if fired == housekeeping_timer {
                    manager.refresh_monitors();
                    manager.refresh();
                    dirty = true;
                }
            }
            WM_QUIT => stop = true,
            _ => unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            },
        }

        if stop {
            break;
        }

        if dirty {
            publish(&manager, &shared);
            if relayout_timer.is_none() {
                relayout_timer = Some(unsafe { SetTimer(None, 0, RELAYOUT_DELAY_MS, None) });
            }
        }
    }

    unsafe {
        let _ = KillTimer(None, housekeeping_timer);
        if let Some(timer) = follow_timer {
            let _ = KillTimer(None, timer);
        }
        if let Some(timer) = relayout_timer {
            let _ = KillTimer(None, timer);
        }
    }
    drop(hooks);
    log::info!("manager thread stopped");
}

/// Whichever way hotkeys are being captured right now.
enum Hotkeys {
    Hook(Option<KeyboardHook>),
    System(HotkeyRegistry),
}

impl Hotkeys {
    fn new(backend: HotkeyBackend) -> Self {
        match backend {
            HotkeyBackend::Hook => Hotkeys::Hook(KeyboardHook::install()),
            HotkeyBackend::System => Hotkeys::System(HotkeyRegistry::new()),
        }
    }

    fn switch_backend(&mut self, backend: HotkeyBackend) {
        let same = matches!(
            (&self, backend),
            (Hotkeys::Hook(_), HotkeyBackend::Hook) | (Hotkeys::System(_), HotkeyBackend::System)
        );
        if !same {
            // Dropping the old one releases the hook or the registrations.
            *self = Hotkeys::new(backend);
        }
    }

    fn rebind(&mut self, manager: &WindowManager, shared: &Arc<Inner>) {
        let bindings: Vec<_> = manager
            .config()
            .active_hotkeys()
            .into_iter()
            .map(|hotkey| (hotkey.binding, hotkey.action))
            .collect();

        let failed = match self {
            Hotkeys::Hook(Some(hook)) => {
                hook.set_bindings(&bindings);
                log::info!("{} hotkeys active via the keyboard hook", hook.len());
                Vec::new()
            }
            // The hook could not be installed, so nothing is bound at all.
            Hotkeys::Hook(None) => bindings
                .iter()
                .map(|(binding, _)| FailedBinding {
                    binding: *binding,
                    reason: "the keyboard hook could not be installed".into(),
                })
                .collect(),
            Hotkeys::System(registry) => {
                let failed = registry.set(&bindings);
                log::info!("{} hotkeys active via RegisterHotKey", registry.len());
                failed
            }
        };

        if !failed.is_empty() {
            log::warn!("{} hotkeys could not be registered", failed.len());
        }
        *shared.failed_hotkeys.write() = failed;
    }

    fn action_for_id(&self, id: i32) -> Option<Action> {
        match self {
            Hotkeys::System(registry) => registry.action(id),
            Hotkeys::Hook(_) => None,
        }
    }

    fn drain(&self) -> Vec<Action> {
        match self {
            Hotkeys::Hook(Some(hook)) => hook.drain(),
            _ => Vec::new(),
        }
    }
}

fn publish(manager: &WindowManager, shared: &Arc<Inner>) {
    *shared.snapshot.write() = manager.snapshot();
}

/// Start or stop the focus-follows-mouse poll, returning the live timer id.
fn set_follow_timer(current: Option<usize>, wanted: bool) -> Option<usize> {
    match (current, wanted) {
        (Some(timer), true) => Some(timer),
        (Some(timer), false) => {
            unsafe {
                let _ = KillTimer(None, timer);
            }
            None
        }
        (None, true) => Some(unsafe { SetTimer(None, 0, FOCUS_FOLLOW_MS, None) }),
        (None, false) => None,
    }
}
