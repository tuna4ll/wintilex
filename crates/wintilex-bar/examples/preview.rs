//! Put the bar on the screen for a few seconds, without the settings window.
//!
//! ```text
//! cargo run -p wintilex-bar --example preview -- 20
//! ```
//!
//! Tiling is switched off for the run, so the bar can be looked at on a desktop
//! that WinTilex is not laying out without a single window being moved. The
//! appbar registration is still made and given back, which is the part worth
//! watching: the work area shrinks when the bar appears and comes back the
//! moment it goes.

fn main() {
    #[cfg(windows)]
    {
        use std::time::Duration;

        use wintilex_bar::BarHandle;
        use wintilex_core::config::Config;
        use wintilex_core::manager::Engine;

        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

        let seconds: u64 =
            std::env::args().nth(1).and_then(|value| value.parse().ok()).unwrap_or(15);

        let mut config = Config::load_or_default();
        config.general.tiling_enabled = false;
        config.bar.enabled = true;

        let bar_config = config.bar.clone();
        let engine = Engine::spawn(config);
        let bar = BarHandle::new(engine.clone());

        print_work_areas("before");
        bar.apply(bar_config);
        // The shell needs a moment to push the new work area out to everyone.
        std::thread::sleep(Duration::from_millis(500));
        print_work_areas("with the bar");

        println!("bar running: {}", bar.is_running());
        println!("bar is up for {seconds}s");
        std::thread::sleep(Duration::from_secs(seconds));

        bar.shutdown();
        std::thread::sleep(Duration::from_millis(500));
        print_work_areas("after");

        engine.shutdown();
    }
}

#[cfg(windows)]
fn print_work_areas(label: &str) {
    for monitor in wintilex_core::platform::monitor::enumerate_monitors() {
        println!("{label:>14}: {} work area {:?}", monitor.id, monitor.work_area);
    }
}
