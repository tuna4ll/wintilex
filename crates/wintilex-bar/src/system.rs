//! The parts of the bar that come from the machine rather than the manager.

use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{
    GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows::Win32::System::Threading::GetSystemTimes;

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// CPU load is a difference between two readings, so the sampler has to
/// remember where it was last time.
#[derive(Debug, Default)]
pub struct Cpu {
    previous: Option<(u64, u64)>,
    load: u32,
}

impl Cpu {
    /// Busy percentage since the previous call. The first call has nothing to
    /// compare against and reports zero.
    pub fn sample(&mut self) -> u32 {
        let mut idle = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();

        let ok =
            unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() };
        if !ok {
            return self.load;
        }

        // The kernel time already includes the idle time.
        let idle = ticks(idle);
        let total = ticks(kernel) + ticks(user);

        if let Some((last_idle, last_total)) = self.previous {
            let busy =
                total.saturating_sub(last_total).saturating_sub(idle.saturating_sub(last_idle));
            let elapsed = total.saturating_sub(last_total);
            if elapsed > 0 {
                self.load =
                    ((busy as f64 / elapsed as f64) * 100.0).round().clamp(0.0, 100.0) as u32;
            }
        }
        self.previous = Some((idle, total));
        self.load
    }
}

fn ticks(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

/// Share of the physical memory in use.
pub fn memory_load() -> u32 {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    match unsafe { GlobalMemoryStatusEx(&mut status) } {
        Ok(()) => status.dwMemoryLoad,
        Err(_) => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

/// `None` on a machine that has no battery, which is also what a desktop
/// reports, so the module simply draws nothing there.
pub fn battery() -> Option<Battery> {
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;

    // 128 means "no system battery", 255 means the driver has no idea.
    if status.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery { percent: status.BatteryLifePercent, charging: status.ACLineStatus == 1 })
}

/// Local time through a small subset of the `strftime` spelling.
pub fn clock(format: &str) -> String {
    let now = unsafe { GetLocalTime() };
    render_time(format, &now)
}

fn render_time(format: &str, now: &SYSTEMTIME) -> String {
    let mut out = String::with_capacity(format.len() + 8);
    let mut chars = format.chars();

    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('H') => out.push_str(&format!("{:02}", now.wHour)),
            Some('I') => out.push_str(&format!("{:02}", hour12(now.wHour))),
            Some('M') => out.push_str(&format!("{:02}", now.wMinute)),
            Some('S') => out.push_str(&format!("{:02}", now.wSecond)),
            Some('p') => out.push_str(if now.wHour < 12 { "AM" } else { "PM" }),
            Some('d') => out.push_str(&format!("{:02}", now.wDay)),
            Some('m') => out.push_str(&format!("{:02}", now.wMonth)),
            Some('Y') => out.push_str(&now.wYear.to_string()),
            Some('y') => out.push_str(&format!("{:02}", now.wYear % 100)),
            Some('a') => out.push_str(DAYS.get(now.wDayOfWeek as usize).copied().unwrap_or("")),
            Some('b') => {
                let index = now.wMonth.saturating_sub(1) as usize;
                out.push_str(MONTHS.get(index).copied().unwrap_or(""));
            }
            Some('%') => out.push('%'),
            // An unknown token is left alone rather than swallowed, so a typo
            // is visible on the bar instead of silently disappearing.
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

fn hour12(hour: u16) -> u16 {
    match hour % 12 {
        0 => 12,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment() -> SYSTEMTIME {
        SYSTEMTIME {
            wYear: 2026,
            wMonth: 8,
            wDayOfWeek: 0,
            wDay: 23,
            wHour: 21,
            wMinute: 7,
            wSecond: 4,
            wMilliseconds: 0,
        }
    }

    #[test]
    fn renders_the_default_format() {
        assert_eq!(render_time("%a %d %b  %H:%M", &moment()), "Sun 23 Aug  21:07");
    }

    #[test]
    fn renders_the_twelve_hour_clock() {
        assert_eq!(render_time("%I:%M %p", &moment()), "09:07 PM");
    }

    #[test]
    fn keeps_unknown_tokens() {
        assert_eq!(render_time("100%% %q", &moment()), "100% %q");
    }
}
