//! Low-level keyboard hook.
//!
//! `RegisterHotKey` refuses most `Win+<letter>` combinations because the shell
//! has already claimed them: `Win+H` is dictation, `Win+K` is casting, `Win+L`
//! locks the machine. A `WH_KEYBOARD_LL` hook sees the key first and can
//! swallow it, which is the only way to get the bindings this project is built
//! around. The cost is that the hook has to stay fast, so the callback does
//! nothing but a table lookup.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::command::Action;
use crate::hotkey::{Binding, Modifiers};
use crate::platform::events::wake_hook_thread;

thread_local! {
    static BINDINGS: RefCell<HashMap<(u32, ModifierMask), Action>> =
        RefCell::new(HashMap::new());
    static PENDING: RefCell<VecDeque<Action>> = const { RefCell::new(VecDeque::new()) };
    /// Set when a `Win+…` combination was swallowed, so the start menu can be
    /// cancelled before the user lets go of the Windows key.
    static SWALLOWED_WIN: RefCell<bool> = const { RefCell::new(false) };
}

/// Modifier state packed into four bits so it can be a map key.
type ModifierMask = u8;

const MASK_ALT: u8 = 1;
const MASK_CONTROL: u8 = 2;
const MASK_SHIFT: u8 = 4;
const MASK_WIN: u8 = 8;

fn mask_of(modifiers: &Modifiers) -> ModifierMask {
    let mut mask = 0;
    if modifiers.alt {
        mask |= MASK_ALT;
    }
    if modifiers.control {
        mask |= MASK_CONTROL;
    }
    if modifiers.shift {
        mask |= MASK_SHIFT;
    }
    if modifiers.win {
        mask |= MASK_WIN;
    }
    mask
}

fn current_mask() -> ModifierMask {
    let down = |key: VIRTUAL_KEY| unsafe { GetAsyncKeyState(key.0 as i32) as u16 & 0x8000 != 0 };
    let mut mask = 0;
    if down(VK_MENU) {
        mask |= MASK_ALT;
    }
    if down(VK_CONTROL) {
        mask |= MASK_CONTROL;
    }
    if down(VK_SHIFT) {
        mask |= MASK_SHIFT;
    }
    if down(VK_LWIN) || down(VK_RWIN) {
        mask |= MASK_WIN;
    }
    mask
}

/// An installed keyboard hook. Removed again on drop.
pub struct KeyboardHook {
    handle: HHOOK,
}

impl KeyboardHook {
    /// Install the hook on the calling thread, which must pump messages.
    pub fn install() -> Option<KeyboardHook> {
        let handle = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) };
        match handle {
            Ok(handle) => {
                log::debug!("keyboard hook installed");
                Some(KeyboardHook { handle })
            }
            Err(error) => {
                log::error!("could not install the keyboard hook: {error}");
                None
            }
        }
    }

    /// Replace the binding table. Must be called from the hook thread.
    pub fn set_bindings(&self, bindings: &[(Binding, Action)]) {
        BINDINGS.with(|table| {
            let mut table = table.borrow_mut();
            table.clear();
            for (binding, action) in bindings {
                table.insert((binding.key, mask_of(&binding.modifiers)), *action);
            }
        });
    }

    /// Take the actions the hook has matched since the last call.
    pub fn drain(&self) -> Vec<Action> {
        PENDING.with(|pending| pending.borrow_mut().drain(..).collect())
    }

    pub fn len(&self) -> usize {
        BINDINGS.with(|table| table.borrow().len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.handle);
        }
        BINDINGS.with(|table| table.borrow_mut().clear());
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let is_key_down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;

    // Keys Tilex itself synthesized must not be looked at again.
    let injected = event.flags.0 & LLKHF_INJECTED.0 != 0;

    if is_key_down && !injected {
        let mask = current_mask();
        let matched =
            BINDINGS.with(|table| table.borrow().get(&(event.vkCode, mask)).copied());

        if let Some(action) = matched {
            PENDING.with(|pending| pending.borrow_mut().push_back(action));
            wake_hook_thread();

            if mask & MASK_WIN != 0 {
                SWALLOWED_WIN.with(|flag| *flag.borrow_mut() = true);
            }
            // Returning non-zero stops the key from reaching anything else,
            // including the shell shortcut that owns this combination.
            return LRESULT(1);
        }
    }

    // The shell opens the start menu when the Windows key goes up without any
    // other key in between. Since the other key was swallowed above, tap a
    // harmless modifier first to break that sequence.
    if !is_key_down && !injected && (event.vkCode == VK_LWIN.0 as u32 || event.vkCode == VK_RWIN.0 as u32)
    {
        let swallowed = SWALLOWED_WIN.with(|flag| flag.replace(false));
        if swallowed {
            send_dummy_key();
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn send_dummy_key() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_LCONTROL,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
