//! Configuration for the status bar.
//!
//! The bar itself lives in `wintilex-bar`, but its settings ride along in the
//! same file as everything else so the settings window has one thing to save.

use serde::{Deserialize, Serialize};

/// Which edge of the display the bar sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
}

/// One thing the bar can show. The order inside a section is the order they
/// are drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BarModule {
    /// The layout this display is using, clickable to cycle it.
    Layout,
    /// One pill per window on this display, in tiling order.
    Windows,
    /// The title of the focused window.
    Title,
    /// Shown only while tiling is paused.
    Tiling,
    /// The number of the display this bar belongs to.
    Monitor,
    Cpu,
    Memory,
    Battery,
    Clock,
}

impl BarModule {
    pub const ALL: &'static [BarModule] = &[
        BarModule::Layout,
        BarModule::Windows,
        BarModule::Title,
        BarModule::Tiling,
        BarModule::Monitor,
        BarModule::Cpu,
        BarModule::Memory,
        BarModule::Battery,
        BarModule::Clock,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            BarModule::Layout => "Layout",
            BarModule::Windows => "Windows",
            BarModule::Title => "Focused title",
            BarModule::Tiling => "Tiling paused",
            BarModule::Monitor => "Display number",
            BarModule::Cpu => "CPU",
            BarModule::Memory => "Memory",
            BarModule::Battery => "Battery",
            BarModule::Clock => "Clock",
        }
    }
}

/// The glyphs in front of each module.
///
/// The defaults come from *Segoe Fluent Icons*, which ships with Windows 11, so
/// the bar has icons out of the box. Anything can be put here instead: point
/// `icon-font` at a Nerd Font and paste its glyphs in.
///
/// `battery` and `battery-charging` are the first of ten glyphs that run in
/// order from empty to full, which is how the Windows icon font lays them out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BarIcons {
    pub layout: String,
    pub monitor: String,
    pub paused: String,
    pub cpu: String,
    pub memory: String,
    pub battery: String,
    pub battery_charging: String,
    pub clock: String,
}

impl Default for BarIcons {
    fn default() -> Self {
        Self {
            layout: "\u{ea61}".into(),
            monitor: "\u{e7f4}".into(),
            paused: "\u{e769}".into(),
            cpu: "\u{e950}".into(),
            memory: "\u{e964}".into(),
            battery: "\u{e850}".into(),
            battery_charging: "\u{e85a}".into(),
            clock: "\u{e823}".into(),
        }
    }
}

/// Colours, as `#rgb`, `#rrggbb` or `#rrggbbaa`. Anything that does not parse
/// falls back to the default for that slot rather than stopping the bar from
/// starting.
///
/// The last six are the accents the modules take, which is where most of the
/// character of a bar comes from: they colour the icons while the text stays
/// readable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BarTheme {
    /// Fill behind each group of modules. Give it an alpha to see through it.
    pub background: String,
    pub foreground: String,
    /// Text that should stay in the background.
    pub muted: String,
    /// Pill behind an unfocused window.
    pub surface: String,
    /// Pill behind the focused window.
    pub accent: String,
    /// Text on top of the accent colour.
    pub accent_text: String,
    /// Tiling paused, and a battery about to run out.
    pub urgent: String,
    pub layout: String,
    pub monitor: String,
    pub cpu: String,
    pub memory: String,
    pub battery: String,
    pub clock: String,
}

impl Default for BarTheme {
    fn default() -> Self {
        Self {
            background: "#1e1e2eeb".into(),
            foreground: "#cdd6f4".into(),
            muted: "#7f849c".into(),
            surface: "#313244".into(),
            accent: "#89b4fa".into(),
            accent_text: "#1e1e2e".into(),
            urgent: "#f38ba8".into(),
            layout: "#cba6f7".into(),
            monitor: "#94e2d5".into(),
            cpu: "#89b4fa".into(),
            memory: "#a6e3a1".into(),
            battery: "#f9e2af".into(),
            clock: "#f5c2e7".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BarConfig {
    pub enabled: bool,
    pub position: BarPosition,
    /// The whole strip, margins included, in logical pixels.
    pub height: u32,
    /// Space between the groups and the edge of the strip. Anything above zero
    /// leaves the bar floating over the wallpaper instead of touching the edge.
    pub margin: u32,
    /// Corner radius of a group.
    pub radius: u32,
    /// How solid the bar is, from 0.2 to 1. The alpha in the background colour
    /// is applied on top of this.
    pub opacity: f32,
    /// Take the space out of the work area so nothing else is drawn under the
    /// bar. Turning this off leaves the bar floating over the windows.
    pub reserve_space: bool,
    /// Only put a bar on the primary display.
    pub primary_only: bool,
    pub font_family: String,
    pub font_size: f32,
    pub icons: bool,
    pub icon_font: String,
    /// `%H` `%I` `%M` `%S` `%p` `%d` `%m` `%y` `%Y` `%a` `%b`, and `%%`.
    pub clock_format: String,
    pub left: Vec<BarModule>,
    pub center: Vec<BarModule>,
    pub right: Vec<BarModule>,
    pub theme: BarTheme,
    pub glyphs: BarIcons,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: BarPosition::Top,
            height: 40,
            margin: 8,
            radius: 10,
            opacity: 1.0,
            reserve_space: true,
            primary_only: false,
            font_family: "Segoe UI".into(),
            font_size: 13.0,
            icons: true,
            icon_font: "Segoe Fluent Icons".into(),
            clock_format: "%H:%M".into(),
            left: vec![BarModule::Layout, BarModule::Windows],
            center: vec![BarModule::Title],
            right: vec![
                BarModule::Tiling,
                BarModule::Cpu,
                BarModule::Memory,
                BarModule::Battery,
                BarModule::Clock,
            ],
            theme: BarTheme::default(),
            glyphs: BarIcons::default(),
        }
    }
}

impl BarConfig {
    /// Height of the whole strip in physical pixels on a display with this
    /// scale factor.
    pub fn scaled_height(&self, scale: f32) -> i32 {
        (self.height.clamp(20, 120) as f32 * scale).round() as i32
    }

    /// The opacity clamped into the range the window can actually use. Fully
    /// transparent would leave the user with a bar they cannot find.
    pub fn window_opacity(&self) -> f32 {
        self.opacity.clamp(0.2, 1.0)
    }

    /// The glyph for a battery at this charge, taken from the ten that run
    /// from empty to full.
    pub fn battery_glyph(&self, percent: u8, charging: bool) -> String {
        let base = if charging { &self.glyphs.battery_charging } else { &self.glyphs.battery };
        let Some(first) = base.chars().next() else {
            return String::new();
        };
        // A glyph that is not part of a run is left alone; only the shipped
        // sequences are stepped through.
        let step = (percent.min(100) as u32 * 9) / 100;
        match char::from_u32(first as u32 + step) {
            Some(glyph) if base.chars().count() == 1 => glyph.to_string(),
            _ => base.clone(),
        }
    }
}
