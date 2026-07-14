//! Small helpers around the Win32 string and error conventions.

use windows::core::PCWSTR;

/// Decode a NUL-terminated UTF-16 buffer that a Win32 call filled in.
pub fn wide_to_string(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Build a NUL-terminated UTF-16 buffer to hand to a `*W` function.
pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Borrow a wide buffer as a `PCWSTR`. The buffer must outlive the pointer.
pub fn pcwstr(buffer: &[u16]) -> PCWSTR {
    PCWSTR(buffer.as_ptr())
}

/// Case-insensitive comparison without allocating for the common ASCII path.
pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.eq_ignore_ascii_case(b)
}
