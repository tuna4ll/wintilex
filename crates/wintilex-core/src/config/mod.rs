//! On-disk configuration.
//!
//! The file lives at `%APPDATA%\WinTilex\config.json`. A missing or partly written
//! file is not an error: every field has a default, so WinTilex starts with a
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

pub const APP_DIR: &str = "WinTilex";
pub const CONFIG_FILE: &str = "config.json";

/// Current shape of the configuration. See [`Config::version`].
pub const CONFIG_VERSION: u32 = 2;

/// What a file that predates the version field reads as.
fn no_version() -> u32 {
    0
}

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
    /// Master switch. When off WinTilex still tracks windows but moves nothing.
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

/// Which mechanism WinTilex uses to grab its hotkeys.
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
    /// Bumped whenever the shipped defaults move, so a file written by an older
    /// build can be recognised. Files from before this existed read as 0.
    #[serde(default = "no_version")]
    pub version: u32,
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
            version: CONFIG_VERSION,
            general: General::default(),
            layout: LayoutKind::default(),
            layout_options: LayoutOptions::default(),
            hotkeys: default_hotkeys(),
            rules: rules::defaults(),
        }
    }
}

/// The bindings from the readme.
///
/// The directional actions sit on the arrow keys because that is the corner of
/// the keyboard Windows already spends on window arranging: `Win+Arrow` snaps,
/// `Win+Shift+Arrow` throws a window at the next display. WinTilex does all of
/// that better, so taking those over costs nothing. Letters were the obvious
/// first choice, but `Win+L` locks the screen and no focus key is worth that.
///
/// `Win+Ctrl+Arrow` is left alone: those are the virtual desktops, which have
/// nothing to do with tiling. Resizing lives on `Win+Alt+Arrow` instead.
pub fn default_hotkeys() -> Vec<Hotkey> {
    use Direction::{Down, Left, Right, Up};

    let mut hotkeys = Vec::with_capacity(24);

    for (key, direction) in [("Left", Left), ("Down", Down), ("Up", Up), ("Right", Right)] {
        hotkeys.push(Hotkey::new(&format!("Win+{key}"), Action::Focus(direction)));
        hotkeys.push(Hotkey::new(&format!("Win+Shift+{key}"), Action::Move(direction)));
        hotkeys.push(Hotkey::new(&format!("Win+Alt+{key}"), Action::Grow(direction)));
    }

    hotkeys.extend([
        Hotkey::new("Win+Alt+Shift+Left", Action::MoveToMonitor(Left)),
        Hotkey::new("Win+Alt+Shift+Right", Action::MoveToMonitor(Right)),
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

/// Bindings that shipped as defaults in an earlier build.
///
/// Used to recognise a hotkey the user never touched, so it can be carried onto
/// the current scheme while anything they picked themselves is left alone.
fn superseded_defaults() -> Vec<Hotkey> {
    use Direction::{Down, Left, Right, Up};

    let mut hotkeys = Vec::with_capacity(24);

    // 0.1 put the directional actions on hjkl.
    for (key, direction) in [("H", Left), ("J", Down), ("K", Up), ("L", Right)] {
        hotkeys.push(Hotkey::new(&format!("Win+{key}"), Action::Focus(direction)));
        hotkeys.push(Hotkey::new(&format!("Win+Shift+{key}"), Action::Move(direction)));
        hotkeys.push(Hotkey::new(&format!("Win+Ctrl+{key}"), Action::Grow(direction)));
    }

    hotkeys.extend([
        // 0.1, before the shell shortcuts were given back.
        Hotkey::new("Win+Ctrl+Left", Action::MoveToMonitor(Left)),
        Hotkey::new("Win+Ctrl+Right", Action::MoveToMonitor(Right)),
        Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next)),
        Hotkey::new("Win+Shift+Tab", Action::FocusCycle(Cycle::Previous)),
        Hotkey::new("Win+Enter", Action::Promote),
        Hotkey::new("Win+Space", Action::CycleLayout),
        Hotkey::new("Win+Shift+Space", Action::ToggleFloating),
        Hotkey::new("Win+Shift+M", Action::ToggleReversed),
        Hotkey::new("Win+Shift+R", Action::ResetRatios),
        Hotkey::new("Win+Ctrl+T", Action::ToggleTiling),
        Hotkey::new("Win+Shift+Q", Action::Minimize),
        // The short-lived Win+Alt scheme that kept hjkl.
        Hotkey::new("Win+Alt+Left", Action::MoveToMonitor(Left)),
        Hotkey::new("Win+Alt+Right", Action::MoveToMonitor(Right)),
    ]);

    hotkeys
}

impl Config {
    /// `%APPDATA%\WinTilex`.
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
                if config.migrate() > 0 {
                    // Write the result straight back, so the migration happens
                    // once rather than on every start.
                    if let Err(error) = config.save_to(path) {
                        log::warn!("could not save the migrated config: {error}");
                    }
                }
                Ok(config)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Carry a file written by an older build onto the current defaults.
    ///
    /// Only bindings the user never changed are touched: a hotkey counts as
    /// untouched when its binding is exactly what some earlier build shipped
    /// for that same action. Anything else is theirs and stays put, even if it
    /// now clashes with a shell shortcut, in which case the settings window
    /// shows it as inactive rather than rewriting it behind their back.
    fn migrate(&mut self) -> usize {
        if self.version >= CONFIG_VERSION {
            return 0;
        }

        let current = default_hotkeys();
        let superseded = superseded_defaults();

        let was_default = |hotkey: &Hotkey| {
            superseded
                .iter()
                .any(|old| old.action == hotkey.action && old.binding == hotkey.binding)
        };
        let wanted = |hotkey: &Hotkey| {
            current.iter().find(|new| new.action == hotkey.action).map(|new| new.binding)
        };

        // Work out the whole plan first. Bindings that are staying put are the
        // only ones a moving binding has to avoid, since the current defaults
        // never clash with each other.
        let planned: Vec<Option<Binding>> = self
            .hotkeys
            .iter()
            .map(|hotkey| {
                wanted(hotkey).filter(|target| was_default(hotkey) && *target != hotkey.binding)
            })
            .collect();

        let staying: Vec<Binding> = self
            .hotkeys
            .iter()
            .zip(&planned)
            .filter(|(_, target)| target.is_none())
            .map(|(hotkey, _)| hotkey.binding)
            .collect();

        let mut moved = 0;
        for (hotkey, target) in self.hotkeys.iter_mut().zip(planned) {
            let Some(target) = target.filter(|target| !staying.contains(target)) else {
                continue;
            };
            log::info!("moved {} to {target}", hotkey.binding);
            hotkey.binding = target;
            moved += 1;
        }

        self.version = CONFIG_VERSION;
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
        // A file with no version predates it, which is what triggers migration.
        assert_eq!(parsed.version, 0);
        assert_eq!(Config { version: CONFIG_VERSION, ..parsed }, Config::default());
    }

    #[test]
    fn a_saved_file_carries_the_current_version() {
        let text = serde_json::to_string(&Config::default()).unwrap();
        let parsed: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.version, CONFIG_VERSION);
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
        let dir = std::env::temp_dir().join("wintilex-config-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");

        let mut config = Config::default();
        config.layout_options.gap = 24;
        config.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.layout_options.gap, 24);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A config as WinTilex 0.1 wrote it, before the arrow-key scheme.
    fn version_zero() -> Config {
        Config {
            version: 0,
            hotkeys: vec![
                Hotkey::new("Win+H", Action::Focus(Direction::Left)),
                Hotkey::new("Win+L", Action::Focus(Direction::Right)),
                Hotkey::new("Win+Shift+H", Action::Move(Direction::Left)),
                Hotkey::new("Win+Ctrl+H", Action::Grow(Direction::Left)),
                Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next)),
                Hotkey::new("Win+Space", Action::CycleLayout),
                Hotkey::new("Win+Ctrl+Left", Action::MoveToMonitor(Direction::Left)),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn an_old_config_moves_onto_the_arrow_keys() {
        let mut config = version_zero();
        assert_eq!(config.migrate(), 7);

        let binding_for = |action: Action| {
            config
                .hotkeys
                .iter()
                .find(|hotkey| hotkey.action == action)
                .map(|hotkey| hotkey.binding.to_string())
                .unwrap()
        };

        assert_eq!(binding_for(Action::Focus(Direction::Left)), "Win+Left");
        assert_eq!(binding_for(Action::Focus(Direction::Right)), "Win+Right");
        assert_eq!(binding_for(Action::Move(Direction::Left)), "Win+Shift+Left");
        assert_eq!(binding_for(Action::Grow(Direction::Left)), "Win+Alt+Left");
        assert_eq!(binding_for(Action::MoveToMonitor(Direction::Left)), "Win+Alt+Shift+Left");
    }

    #[test]
    fn migrating_gives_every_windows_shortcut_back() {
        let mut config = version_zero();
        config.migrate();
        for hotkey in &config.hotkeys {
            assert_eq!(
                crate::hotkey::system_reserved(&hotkey.binding),
                None,
                "{} still belongs to Windows",
                hotkey.binding
            );
        }
    }

    #[test]
    fn migrating_runs_only_once() {
        let mut config = version_zero();
        assert!(config.migrate() > 0);
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.migrate(), 0);
    }

    #[test]
    fn migrating_leaves_a_binding_the_user_chose_alone() {
        let mut config = Config {
            version: 0,
            hotkeys: vec![
                // Never a default, so it is the user's own choice.
                Hotkey::new("Win+Ctrl+Alt+P", Action::Focus(Direction::Left)),
                Hotkey::new("Win+Tab", Action::FocusCycle(Cycle::Next)),
            ],
            ..Default::default()
        };
        assert_eq!(config.migrate(), 1);
        assert_eq!(config.hotkeys[0].binding.to_string(), "Win+Ctrl+Alt+P");
    }

    #[test]
    fn migrating_never_overwrites_a_binding_in_use() {
        let mut config = Config {
            version: 0,
            hotkeys: vec![
                Hotkey::new("Win+H", Action::Focus(Direction::Left)),
                // The slot the migration wants is already spoken for.
                Hotkey::new("Win+Left", Action::Retile),
            ],
            ..Default::default()
        };
        assert_eq!(config.migrate(), 0);
        assert_eq!(config.hotkeys[0].binding.to_string(), "Win+H");
    }

    #[test]
    fn a_current_file_is_left_exactly_as_it_is() {
        let mut config = Config::default();
        // Somebody deliberately put focus-left back on Win+H.
        config.hotkeys.push(Hotkey::new("Win+H", Action::Focus(Direction::Left)));
        let before = config.clone();
        assert_eq!(config.migrate(), 0);
        assert_eq!(config, before);
    }

    #[test]
    fn loading_an_old_file_writes_the_migration_back() {
        let dir = std::env::temp_dir().join("wintilex-migrate-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");
        version_zero().save_to(&path).unwrap();

        // Saving stamps the current version, so put it back the way an old
        // build would have left it.
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace(r#""version": 2"#, r#""version": 0"#)).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.version, CONFIG_VERSION);

        // The second read finds a file that needs nothing done to it.
        let again = Config::load_from(&path).unwrap();
        assert_eq!(again, loaded);
        assert_eq!(again.hotkeys[0].binding.to_string(), "Win+Left");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_yields_the_defaults() {
        let path = std::env::temp_dir().join("wintilex-does-not-exist-9f3a.json");
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    }
}
