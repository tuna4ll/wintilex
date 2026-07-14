//! Reports which of the configured bindings Windows lets Tilex have.

fn main() {
    #[cfg(windows)]
    {
        use tilex_core::config::{Config, HotkeyBackend};
        use tilex_core::manager::Engine;

        let backend = match std::env::args().nth(1).as_deref() {
            Some("system") => HotkeyBackend::System,
            _ => HotkeyBackend::Hook,
        };

        let mut config = Config::load_or_default();
        config.general.hotkey_backend = backend;
        config.general.tiling_enabled = false;

        let handle = Engine::spawn(config);
        std::thread::sleep(std::time::Duration::from_millis(1200));

        let failed = handle.failed_hotkeys();
        println!("backend {backend:?}: {} of the configured bindings are inactive", failed.len());
        for failure in failed {
            println!("  {} -> {}", failure.binding, failure.reason);
        }
        handle.shutdown();
    }
}
