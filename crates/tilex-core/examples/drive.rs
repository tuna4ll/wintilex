//! Drives the manager through a few actions and prints the layout after each
//! one. Useful for checking the hotkey pipeline without pressing any keys.

fn main() {
    #[cfg(windows)]
    {
        use std::time::Duration;
        use tilex_core::{config::Config, manager::Engine, Action, Direction};

        let mut config = Config::load_or_default();
        config.general.hotkey_backend = tilex_core::config::HotkeyBackend::System;
        let handle = Engine::spawn(config);
        std::thread::sleep(Duration::from_millis(400));

        let report = |label: &str, handle: &tilex_core::manager::EngineHandle| {
            let snapshot = handle.snapshot();
            let mut tiled: Vec<String> = snapshot
                .windows
                .iter()
                .filter(|w| !w.floating && !w.minimized)
                .map(|w| {
                    let t = w.tile.unwrap_or(tilex_core::Rect::ZERO);
                    format!("{}@{},{} {}x{}", w.process, t.x, t.y, t.width, t.height)
                })
                .collect();
            tiled.sort();
            println!("{label}: {}", tiled.join("  |  "));
        };

        report("start      ", &handle);

        for (label, action) in [
            ("move right ", Action::Move(Direction::Right)),
            ("focus left ", Action::Focus(Direction::Left)),
            ("grow right ", Action::Grow(Direction::Right)),
            ("grow right ", Action::Grow(Direction::Right)),
            ("grow right ", Action::Grow(Direction::Right)),
            ("cycle layout", Action::CycleLayout),
            ("back to bsp", Action::SetLayout(tilex_core::LayoutKind::Bsp)),
            ("reset sizes", Action::ResetRatios),
        ] {
            handle.dispatch(action);
            std::thread::sleep(Duration::from_millis(350));
            report(label, &handle);
        }

        handle.shutdown();
    }
}
