//! Prints the effective configuration, after any migration has been applied.

fn main() {
    let config = wintilex_core::Config::load_or_default();
    println!("version {}", config.version);
    for hotkey in &config.hotkeys {
        println!(
            "  {:<22} {}{}",
            hotkey.binding.to_string(),
            hotkey.action.label(),
            if hotkey.disabled { "  (off)" } else { "" }
        );
    }
}
