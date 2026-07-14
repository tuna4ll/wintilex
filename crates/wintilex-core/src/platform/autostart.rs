//! Start with Windows.
//!
//! Writes to the per-user `Run` key rather than a scheduled task or a service,
//! so enabling it never needs elevation and the user can see and remove the
//! entry from Task Manager like any other startup item.

use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::platform::util::{to_wide, wide_to_string};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "WinTilex";

/// Argument the entry passes, so a startup launch can skip showing the window.
pub const AUTOSTART_FLAG: &str = "--autostart";

#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    #[error("could not work out where WinTilex is installed: {0}")]
    ExePath(#[from] std::io::Error),
    #[error("registry error 0x{0:08X}")]
    Registry(u32),
}

/// The command line the `Run` entry holds, quoted for paths with spaces.
fn command_line() -> Result<String, AutostartError> {
    let exe: PathBuf = std::env::current_exe()?;
    Ok(format!("\"{}\" {AUTOSTART_FLAG}", exe.display()))
}

fn open_run_key(
    access: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> Result<HKEY, AutostartError> {
    let path = to_wide(RUN_KEY);
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            access,
            None,
            &mut key,
            None,
        )
    };
    if status.is_err() {
        return Err(AutostartError::Registry(status.0 as u32));
    }
    Ok(key)
}

/// Whether the startup entry exists and still points at this executable.
pub fn is_enabled() -> bool {
    match read_entry() {
        Ok(Some(existing)) => command_line().map(|wanted| existing == wanted).unwrap_or(false),
        _ => false,
    }
}

/// The raw value currently stored, if any.
pub fn read_entry() -> Result<Option<String>, AutostartError> {
    let key = open_run_key(KEY_READ)?;
    let name = to_wide(VALUE_NAME);

    let mut size = 0u32;
    let status =
        unsafe { RegQueryValueExW(key, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size)) };
    if status == ERROR_FILE_NOT_FOUND {
        unsafe {
            let _ = RegCloseKey(key);
        };
        return Ok(None);
    }
    if status.is_err() {
        unsafe {
            let _ = RegCloseKey(key);
        };
        return Err(AutostartError::Registry(status.0 as u32));
    }

    let mut buffer = vec![0u8; size as usize];
    let status = unsafe {
        RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            None,
            Some(buffer.as_mut_ptr()),
            Some(&mut size),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    };
    if status.is_err() {
        return Err(AutostartError::Registry(status.0 as u32));
    }

    let wide: Vec<u16> =
        buffer.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
    Ok(Some(wide_to_string(&wide)))
}

pub fn enable() -> Result<(), AutostartError> {
    let key = open_run_key(KEY_WRITE)?;
    let name = to_wide(VALUE_NAME);
    let value = to_wide(&command_line()?);
    let bytes: Vec<u8> = value.iter().flat_map(|unit| unit.to_le_bytes()).collect();

    let status = unsafe { RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes)) };
    unsafe {
        let _ = RegCloseKey(key);
    };

    if status.is_err() {
        Err(AutostartError::Registry(status.0 as u32))
    } else {
        Ok(())
    }
}

pub fn disable() -> Result<(), AutostartError> {
    let key = open_run_key(KEY_WRITE)?;
    let name = to_wide(VALUE_NAME);
    let status = unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) };
    unsafe {
        let _ = RegCloseKey(key);
    };

    // Removing something that is not there is the state we wanted anyway.
    if status.is_err() && status != ERROR_FILE_NOT_FOUND {
        Err(AutostartError::Registry(status.0 as u32))
    } else {
        Ok(())
    }
}

pub fn set(enabled: bool) -> Result<(), AutostartError> {
    if enabled {
        enable()
    } else {
        disable()
    }
}

/// True when this process was started by the `Run` entry.
pub fn launched_at_startup() -> bool {
    std::env::args().any(|argument| argument == AUTOSTART_FLAG)
}
