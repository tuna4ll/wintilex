//! Per-application behaviour.
//!
//! Rules are matched top to bottom and the first hit wins, so a specific rule
//! placed above a broad one overrides it.

use serde::{Deserialize, Serialize};

/// What WinTilex does with a window that matches a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleAction {
    /// Take part in the layout. This is the default for everything.
    #[default]
    Tile,
    /// Track the window but leave its position alone.
    Float,
    /// Pretend the window does not exist.
    Ignore,
}

/// A single window matcher.
///
/// Every field that is set has to match. An empty rule matches nothing, which
/// keeps a half-filled row in the settings UI from swallowing every window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WindowRule {
    /// Executable file name, for example `devenv.exe`. Case-insensitive.
    pub process: Option<String>,
    /// Win32 window class, matched exactly.
    pub class: Option<String>,
    /// Substring of the window title. Case-insensitive.
    pub title_contains: Option<String>,
    pub action: RuleAction,
    /// Pin matching windows to a monitor, by index in the display list.
    pub monitor: Option<usize>,
    /// Turned off rows stay in the file so they can be re-enabled later.
    pub disabled: bool,
}

/// The properties a rule is tested against.
#[derive(Debug, Clone, Copy)]
pub struct WindowFacts<'a> {
    pub process: &'a str,
    pub class: &'a str,
    pub title: &'a str,
}

impl WindowRule {
    pub fn is_empty(&self) -> bool {
        self.process.is_none() && self.class.is_none() && self.title_contains.is_none()
    }

    pub fn matches(&self, facts: WindowFacts<'_>) -> bool {
        if self.disabled || self.is_empty() {
            return false;
        }
        if let Some(process) = &self.process {
            if !process.eq_ignore_ascii_case(facts.process) {
                return false;
            }
        }
        if let Some(class) = &self.class {
            if class != facts.class {
                return false;
            }
        }
        if let Some(needle) = &self.title_contains {
            if !contains_ignore_case(facts.title, needle) {
                return false;
            }
        }
        true
    }
}

/// The first rule that matches, if any.
pub fn first_match<'a>(rules: &'a [WindowRule], facts: WindowFacts<'_>) -> Option<&'a WindowRule> {
    rules.iter().find(|rule| rule.matches(facts))
}

fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

/// Sensible starting point: float the dialog-heavy system apps that behave
/// badly when they are forced into a tile.
pub fn defaults() -> Vec<WindowRule> {
    let float = |process: &str| WindowRule {
        process: Some(process.to_string()),
        action: RuleAction::Float,
        ..Default::default()
    };
    vec![
        float("applicationframehost.exe"),
        float("systemsettings.exe"),
        float("taskmgr.exe"),
        float("snippingtool.exe"),
        WindowRule {
            class: Some("#32770".to_string()),
            action: RuleAction::Float,
            ..Default::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(process: &'a str, class: &'a str, title: &'a str) -> WindowFacts<'a> {
        WindowFacts { process, class, title }
    }

    #[test]
    fn process_match_ignores_case() {
        let rule = WindowRule {
            process: Some("Code.exe".into()),
            action: RuleAction::Float,
            ..Default::default()
        };
        assert!(rule.matches(facts("code.exe", "Chrome_WidgetWin_1", "main.rs")));
        assert!(!rule.matches(facts("brave.exe", "Chrome_WidgetWin_1", "main.rs")));
    }

    #[test]
    fn all_set_fields_must_match() {
        let rule = WindowRule {
            process: Some("explorer.exe".into()),
            title_contains: Some("Downloads".into()),
            ..Default::default()
        };
        assert!(rule.matches(facts("explorer.exe", "CabinetWClass", "Downloads")));
        assert!(!rule.matches(facts("explorer.exe", "CabinetWClass", "Documents")));
    }

    #[test]
    fn empty_and_disabled_rules_never_match() {
        assert!(!WindowRule::default().matches(facts("a.exe", "C", "t")));
        let rule =
            WindowRule { process: Some("a.exe".into()), disabled: true, ..Default::default() };
        assert!(!rule.matches(facts("a.exe", "C", "t")));
    }

    #[test]
    fn first_rule_wins() {
        let rules = vec![
            WindowRule {
                process: Some("code.exe".into()),
                title_contains: Some("settings".into()),
                action: RuleAction::Float,
                ..Default::default()
            },
            WindowRule {
                process: Some("code.exe".into()),
                action: RuleAction::Ignore,
                ..Default::default()
            },
        ];
        let hit = first_match(&rules, facts("code.exe", "X", "Settings")).unwrap();
        assert_eq!(hit.action, RuleAction::Float);
        let hit = first_match(&rules, facts("code.exe", "X", "main.rs")).unwrap();
        assert_eq!(hit.action, RuleAction::Ignore);
    }
}
