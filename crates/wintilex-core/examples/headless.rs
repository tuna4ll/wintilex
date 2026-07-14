//! Runs the manager without any UI. Useful for trying the tiling on its own.
//!
//! Press Ctrl+C to stop, or use the Win+Ctrl+T binding to pause tiling.

fn main() {
    #[cfg(windows)]
    {
        use wintilex_core::{config::Config, manager::Engine};

        let seconds: u64 =
            std::env::args().nth(1).and_then(|value| value.parse().ok()).unwrap_or(10);

        let handle = Engine::spawn(Config::load_or_default());
        std::thread::sleep(std::time::Duration::from_secs(seconds));

        let snapshot = handle.snapshot();
        println!("tiling enabled: {}", snapshot.tiling_enabled);
        for monitor in &snapshot.monitors {
            println!(
                "monitor {} {:?} layout {:?} tiled {}",
                monitor.id, monitor.work_area, monitor.layout, monitor.tiled_windows
            );
        }
        for window in &snapshot.windows {
            println!(
                "  {} {:<24} floating={} minimized={} tile={:?}",
                window.id, window.process, window.floating, window.minimized, window.tile
            );
        }
        for failure in handle.failed_hotkeys() {
            println!("hotkey {} rejected: {}", failure.binding, failure.reason);
        }
        handle.shutdown();
    }
}
