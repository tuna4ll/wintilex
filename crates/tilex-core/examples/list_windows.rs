//! Dumps every window Tilex considers manageable. Handy for tuning the filter.

fn main() {
    #[cfg(windows)]
    {
        use tilex_core::platform::{enumerate_manageable, enumerate_monitors};

        for monitor in enumerate_monitors() {
            println!(
                "monitor {} bounds {:?} work {:?} dpi {} primary {}",
                monitor.id, monitor.bounds, monitor.work_area, monitor.dpi, monitor.is_primary
            );
        }
        println!();

        for window in enumerate_manageable() {
            let rect = window.frame_rect();
            println!(
                "{} {:<28} {:<24} {:>5}x{:<5} @ {:>5},{:<5} {}",
                window.id(),
                truncate(&window.process_name(), 28),
                truncate(&window.class_name(), 24),
                rect.width,
                rect.height,
                rect.x,
                rect.y,
                truncate(&window.title(), 48),
            );
        }
    }
}

#[cfg(windows)]
fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value.chars().take(max - 1).chain(['…']).collect()
    }
}
