//! Making sure only one Tilex is running.
//!
//! Two window managers fighting over the same desktop is worse than none, so a
//! second launch hands its request over to the first one and exits. A named
//! event does the signalling; nothing needs a window for it.

use std::sync::mpsc::{channel, Receiver};
use std::thread;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
    INFINITE,
};

use crate::platform::util::to_wide;

const MUTEX_NAME: &str = "Local\\TilexSingleInstance";
const EVENT_NAME: &str = "Local\\TilexShowSettings";

/// Held for as long as this process is the one running Tilex.
pub struct InstanceGuard {
    mutex: HANDLE,
    event: HANDLE,
}

/// Outcome of trying to become the running instance.
pub enum Instance {
    /// This process owns the desktop. The receiver yields a message every time
    /// another launch asks for the settings window.
    First(InstanceGuard, Receiver<()>),
    /// Somebody else is already running; the request has been handed over.
    Already,
}

/// Try to claim the single-instance slot.
pub fn acquire() -> Instance {
    let name = to_wide(MUTEX_NAME);
    let mutex = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) };

    // `CreateMutexW` succeeds either way; the last error is what says whether
    // the name was already taken.
    let taken = match mutex {
        Ok(_) => (unsafe { GetLastError() }) == ERROR_ALREADY_EXISTS,
        Err(_) => true,
    };

    if taken {
        signal_existing();
        if let Ok(handle) = mutex {
            unsafe {
                let _ = CloseHandle(handle);
            }
        }
        return Instance::Already;
    }

    let mutex = mutex.expect("mutex handle exists when the slot was free");
    let event_name = to_wide(EVENT_NAME);
    let event = match unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) } {
        Ok(event) => event,
        Err(error) => {
            log::error!("could not create the single-instance event: {error}");
            return Instance::First(InstanceGuard { mutex, event: HANDLE::default() }, channel().1);
        }
    };

    let (sender, receiver) = channel();
    let watched = event.0 as isize;
    thread::Builder::new()
        .name("tilex-instance".into())
        .spawn(move || {
            let handle = HANDLE(watched as *mut std::ffi::c_void);
            loop {
                let result = unsafe { WaitForSingleObject(handle, INFINITE) };
                if result != WAIT_OBJECT_0 || sender.send(()).is_err() {
                    break;
                }
            }
        })
        .ok();

    Instance::First(InstanceGuard { mutex, event }, receiver)
}

/// Ask the running instance to show its settings window.
fn signal_existing() {
    let name = to_wide(EVENT_NAME);
    unsafe {
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.event.is_invalid() {
                let _ = CloseHandle(self.event);
            }
            let _ = CloseHandle(self.mutex);
        }
    }
}
