//! On-disk configuration.
//!
//! The file lives at `%APPDATA%\Tilex\config.json`. A missing or partly written
//! file is not an error: every field has a default, so Tilex starts with a
//! working setup and fills the gaps back in on the next save.

pub mod rules;

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::command::{Action, Cycle};
use crate::geometry::Direction;
use crate::hotkey::Binding;
use crate::layout::{LayoutKind, LayoutOptions};

pub use rules::{RuleAction, WindowFacts, WindowRule};

pub const APP_DIR: &str = "Tilex";
pub const CONFIG_FILE: &str = "config.json";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not locate the application data directory")]
    NoConfigDir,
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("config file is not valid json: {0}")]
    Parse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct General {
    /// Master switch. When off Tilex still tracks windows but moves nothing.
    pub tiling_enabled: bool,
    /// Start with Windows.
    pub start_on_login: bool,
    /// Hide the settings window when it is closed instead of quitting.
    pub minimize_to_tray: bool,
    /// Focus whatever window the pointer is over.
    pub focus_follows_mouse: bool,
    /// Move the pointer to the middle of a window when it gains focus.
    pub warp_cursor_to_focus: bool,
    /// Fold a manual drag-resize back into the layout instead of undoing it.
    pub absorb_manual_resize: bool,
    /// How global hotkeys are captured.
    pub hotkey_backend: HotkeyBackend,
    /// Never take over the handful of shortcuts Windows itself needs, such as
    /// `Win+Tab` and `Win+Ctrl+Arrow`. See [`crate::hotkey::system_reserved`].
    pub protect_system_shortcuts: bool,
}

/// Which mechanism Tilex uses to grab its hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HotkeyBackend {
    /// A low-level keyboard hook. Sees the key before the shell does, which is
    /// the only way to bind combinations Windows reserves for itself such as
    /// `Win+H` or `Win+Tab`.
    #[default]
    Hook,
    /// `RegisterHotKey`. Less invasive, but Windows refuses most `Win+letter`
    /// combinations.
    System,
}

impl Default for General {
    fn default() -> Self {
        Self {
            tiling_enabled: true,
            start_on_login: false,
            minimize_to_tray: true,
            focus_follows_mouse: false,
            warp_cursor_to_focus: false,
            absorb_manual_resize: true,
            hotkey_backend: HotkeyBackend::default(),
            protect_system_shortcuts: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub general: General,
    pub layout: LayoutKind,
    #[serde(flatten)]
    pub layout_options: LayoutOptions,
    pub hotkeys: Vec<Hotkey>,
    pub rules: Vec<WindowRule>,
}

/// One row of the hotkey table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Hotkey {
    pub binding: Binding,
    #[serde(flatten)]
    pub action: Action,
    #[serde(default)]
    pub disabled: bool,
}

impl Hotkey {
    pub fn new(binding: &str, action: Action) -> Self {
        Self {
            binding: binding.parse().expect("built-in binding must parse"),
            action,
            disabled: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            layout: LayoutKind::default(),
            layout_options: LayoutOptions::default(),
            hotkeys: default_hotkeys(),
            rules: rules::defaults(),
        }
    }
}

/// The vim-style bindings from the readme.
pub fn default_hotkeys() -> Vec<Hotkey> {
    use Direction::{Down, Left, Right, Up};

    let mut hotkeys = Vec::with_capacity(20);

    for (key, direction) in [("H", Left), ("J", Down), ("K", Up), ("L", Right)] {
        hotkeys.push(Hotkey::new(&format!("Win+{key}"), Action::Focus(direction)));
        hotkeys.push(Hotkey::new(&format!("Win+Shift+{key}"), Action::Move(direction)));
        hotkeys.push(Hotkey::new(&format!("Win+Ctrl+{key}"), Action::Grow(direction)));
    }

    // Everything that is not part of the directional scheme lives under
    // `Win+Alt`, which Windows leaves almost entirely alone. Earlier versions
    // used `Win+Tab`, `Win+Space` and `Win+Ctrl+Arrow` and swallowed the task
    // view, the layout switcher and the virtual desktops along with them.
    hotkeys.extend([
        Hotkey::new("Win+Alt+Left", Action::MoveToMonitor(Left)),
        Hotkey::new("Win+Alt+Right", Action::MoveToMonitor(Right)),
        Hotkey::new("Win+Alt+J", Action::FocusCycle(Cycle::Next)),
        Hotkey::new("Win+Alt+K", Action::FocusCycle(Cycle::Previous)),
        Hotkey::new("Win+Alt+Enter", Action::Promote),
        Hotkey::new("Win+Alt+Space", Action::CycleLayout),
        Hotkey::new("Win+Alt+F", Action::ToggleFloating),
        Hotkey::new("Win+Alt+X", Action::ToggleReversed),
        Hotkey::new("Win+Alt+Z", Action::ResetRatios),
        Hotkey::new("Win+Alt+P", Action::ToggleTiling),
        Hotkey::new("Win+Alt+Q", Action::Minimize),
    ]);

    hotkeys
}

impl Config {
    /// `%APPDATA%\Tilex`.
    pub fn directory() -> Result<PathBuf, ConfigError> {
        let appdata = std::env::var_os("APPDATA").ok_or(ConfigError::NoConfigDir)?;
        Ok(PathBuf::from(appdata).join(APP_DIR))
    }

    pub fn path() -> Result<PathBuf, ConfigError> {
        Ok(Self::directory()?.join(CONFIG_FILE))
    }

    /// Read the config, falling back to the defaults when the file is missing.
    ///
    /// A file that exists but cannot be parsed is a real error; silently
    /// resetting somebody's settings would be worse than refusing to start.
    pub fn load() -> Result<Config, ConfigError> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut config: Config = serde_json::from_str(&text)?;
                config.rehome_reserved_bindings();
                Ok(config)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Move bindings off the shortcuts Windows needs.
    ///
    /// Tilex used to bind `Win+Tab`, `Win+Space` and `Win+Ctrl+Arrow`, which
    /// took the task view, the keyboard layout switcher and the virtual
    /// desktops away from the shell. Those bindings are now refused, so a
    /// config written by an older build would quietly lose the actions along
    /// with them. Each one is moved to whatever the current defaults use for
    /// the same action instead.
    fn rehome_reserved_bindings(&mut self) -> usize {
        if !self.general.protect_system_shortcuts {
            return 0;
        }

        let defaults = default_hotkeys();
        let mut moved = 0;

        for index in 0..self.hotkeys.len() {
            let hotkey = &self.hotkeys[index];
            if crate::hotkey::system_reserved(&hotkey.binding).is_none() {
                continue;
            }
            let Some(replacement) = defaults
                .iter()
                .find(|other| other.action == hotkey.action)
                .map(|other| other.binding)
            else {
                continue;
            };
            // Never collide with a binding the user set up themselves.
            if self.hotkeys.iter().any(|other| other.binding == replacement) {
                continue;
            }

            log::info!("moved {} to {replacement}", self.hotkeys[index].binding);
            self.hotkeys[index].binding = replacement;
            moved += 1;
        }

        moved
    }

    /// Like [`load`](Self::load) but never fails: a broken file is logged and
    /// the defaults are used. The manager uses this so a typo cannot lock the
    /// user out of their window manager.
    pub fn load_or_default() -> Config {
        match Self::load() {
            Ok(config) => config,
            Err(error) => {
                log::error!("failed to read config, using defaults: {error}");
                Config::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(&Self::path()?)
    }

    /// Write via a temporary file so an interrupted save cannot truncate the
    /// existing config.
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }

    /// Bindings that are actually active, newest definition winning on a clash.
    pub fn active_hotkeys(&self) -> Vec<&Hotkey> {
        let mut seen = std::collections::HashSet::new();
        let mut active = Vec::new();
        for hotkey in self.hotkeys.iter().rev() {
            if !hotkey.disabled && seen.insert(hotkey.binding) {
                active.push(hotkey);
            }
        }
        active.reverse();
        active
    }

    /// How a window should be treated, according to the rule list.
    pub fn action_for(&self, facts: WindowFacts<'_>) -> RuleAction {
        rules::first_match(&self.rules, facts).map(|rule| rule.action).unwrap_or_default()
    }

    pub fn pinned_monitor(&self, facts: WindowFacts<'_>) -> Option<usize> {
        rules::first_match(&self.rules, facts).and_then(|rule| rule.monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_json() {
        let config = Config::default();
        let text = serde_json::to_string_pretty(&config).unwrap();
        let parsed: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn a_partial_file_keeps_the_rest_of_the_defaults() {
        let parsed: Config = serde_json::from_str(r#"{"layout":"columns","gap":16}"#).unwrap();
        assert_eq!(parsed.layout, LayoutKind::Columns);
        assert_eq!(parsed.layout_options.gap, 16);
        assert_eq!(parsed.layout_options.outer_gap, LayoutOptions::default().outer_gap);
        assert!(parsed.general.tiling_enabled);
    }

    #[test]
    fn every_default_binding_is_unique() {
        let config = Config::default();
        assert_eq!(config.active_hotkeys().len(), config.hotkeys.len());
    }

    #[test]
    fn no_default_binding_steals_a_windows_shortcut() {
        for hotkey in Config::default().hotkeys {
            assert_eq!(
                crate::hotkey::system_reserved(&hotkey.binding),
                None,
                "{} is reserved by Windows",
                hotkey.binding
            );
        }
    }

    #[test]
    fn later_bindings_win_a_clash() {
        let mut config = Config::default();
        config.hotkeys.push(Hotkey::new("Win+H", Action::Retile));
        let active = config.active_hotkeys();
        let binding: Binding = "Win+H".parse().unwrap();
        let hit = active.iter().find(|h| h.binding == binding).unwrap();
        assert_eq!(hit.action, Action::Retile);
    }

    #[test]
    fn rules_decide_how_a_window_is_treated() {
        let config = Config::default();
        let facts = WindowFacts { process: "taskmgr.exe", class: "TaskManagerWindow", title: "" };
        assert_eq!(config.action_for(facts), RuleAction::Float);

        let facts = WindowFacts { process: "code.exe", class: "Chrome_WidgetWin_1", title: "" };
        assert_eq!(config.action_for(facts), RuleAction::Tile);
    }

    #[test]
    fn save_and_load_a_file() {
        let dir = std::env::temp_dir().join("tilex-config-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");

        let mut config = Config::default();
        config.layout_options.gap = 24;
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.layout_options.gap, 24);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_old_config_gives_the_windows_shortcuts_back() {
        // What Tilex 0.1 wrote out.
        let mut config = Config {
            hotkeys: vec![
                Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next)),
                Hotkey::new("Win+Space", Action::CycleLayout),
                Hotkey::new("Win+Ctrl+Left", Action::MoveToMonitor(Direction::Left)),
                Hotkey::new("Win+H", Action::Focus(Direction::Left)),
            ],
            ..Default::default()
        };

        assert_eq!(config.rehome_reserved_bindings(), 3);

        for hotkey in &config.hotkeys {
            assert_eq!(crate::hotkey::system_reserved(&hotkey.binding), None);
        }
        // The untouched binding stays exactly where it was.
        let focus = config.hotkeys.iter().find(|h| h.action == Action::Focus(Direction::Left));
        assert_eq!(focus.unwrap().binding.to_string(), "Win+H");
    }

    #[test]
    fn rehoming_runs_only_once() {
        let mut config = Config {
            hotkeys: vec![Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next))],
            ..Default::default()
        };
        assert_eq!(config.rehome_reserved_bindings(), 1);
        assert_eq!(config.rehome_reserved_bindings(), 0);
    }

    #[test]
    fn rehoming_never_overwrites_a_binding_in_use() {
        let mut config = Config {
            hotkeys: vec![
                Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next)),
                // The slot the migration would want is already taken.
                Hotkey::new("Win+Alt+J", Action::Retile),
            ],
            ..Default::default()
        };
        assert_eq!(config.rehome_reserved_bindings(), 0);
    }

    #[test]
    fn rehoming_is_skipped_when_protection_is_off() {
        let mut config = Config {
            general: General { protect_system_shortcuts: false, ..Default::default() },
            hotkeys: vec![Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next))],
            ..Default::default()
        };
        assert_eq!(config.rehome_reserved_bindings(), 0);
    }

    #[test]
    fn a_missing_file_yields_the_defaults() {
        let path = std::env::temp_dir().join("tilex-does-not-exist-9f3a.json");
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    }
}
