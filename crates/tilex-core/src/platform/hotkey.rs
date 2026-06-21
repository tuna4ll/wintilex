//! Global hotkey registration.
//!
//! Registering with a null window sends `WM_HOTKEY` to the calling thread's own
//! message queue, so the manager loop picks it up next to everything else.

use std::collections::HashMap;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

use crate::command::Action;
use crate::hotkey::Binding;

/// Hotkey ids have to be unique per thread; start above the range Windows
/// itself uses for shell hotkeys.
const FIRST_ID: i32 = 0x4000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedBinding {
    pub binding: Binding,
    pub reason: String,
}

/// The hotkeys currently owned by this thread.
#[derive(Default)]
pub struct HotkeyRegistry {
    actions: HashMap<i32, Action>,
    next_id: i32,
}

impl HotkeyRegistry {
    pub fn new() -> Self {
        Self { actions: HashMap::new(), next_id: FIRST_ID }
    }

    /// Replace every registration with `bindings`.
    ///
    /// Returns the bindings Windows refused. That happens when another process
    /// already owns the combination, and for the handful of shortcuts the shell
    /// reserves for itself such as `Win+L`.
    pub fn set(&mut self, bindings: &[(Binding, Action)]) -> Vec<FailedBinding> {
        self.clear();

        let mut failed = Vec::new();
        for (binding, action) in bindings {
            let id = self.next_id;
            let modifiers = to_win32_modifiers(binding);
            let result = unsafe { RegisterHotKey(None, id, modifiers, binding.key) };
            match result {
                Ok(()) => {
                    self.actions.insert(id, *action);
                    self.next_id += 1;
                }
                Err(error) => {
                    log::warn!("could not register {binding}: {error}");
                    failed.push(FailedBinding {
                        binding: *binding,
                        reason: describe(error.code().0),
                    });
                }
            }
        }
        failed
    }

    /// The action bound to the id carried by a `WM_HOTKEY` message.
    pub fn action(&self, id: i32) -> Option<Action> {
        self.actions.get(&id).copied()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn clear(&mut self) {
        for id in self.actions.keys() {
            unsafe {
                let _ = UnregisterHotKey(None, *id);
            }
        }
        self.actions.clear();
        self.next_id = FIRST_ID;
    }
}

impl Drop for HotkeyRegistry {
    fn drop(&mut self) {
        self.clear();
    }
}

fn to_win32_modifiers(binding: &Binding) -> HOT_KEY_MODIFIERS {
    let mut modifiers = MOD_NOREPEAT;
    if binding.modifiers.alt {
        modifiers |= MOD_ALT;
    }
    if binding.modifiers.control {
        modifiers |= MOD_CONTROL;
    }
    if binding.modifiers.shift {
        modifiers |= MOD_SHIFT;
    }
    if binding.modifiers.win {
        modifiers |= MOD_WIN;
    }
    modifiers
}

fn describe(code: i32) -> String {
    // ERROR_HOTKEY_ALREADY_REGISTERED, as an HRESULT.
    if code as u32 == 0x8007_0581 {
        "already registered by another application".to_string()
    } else {
        format!("windows error 0x{code:08X}")
    }
}
