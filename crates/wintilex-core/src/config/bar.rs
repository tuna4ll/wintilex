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

/// Colours, as `#rrggbb`. Anything that does not parse falls back to the
/// default for that slot rather than stopping the bar from starting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BarTheme {
    pub background: String,
    pub foreground: String,
    /// Text that should stay in the background, such as the clock date.
    pub muted: String,
    /// Pill behind an unfocused window.
    pub surface: String,
    pub accent: String,
    /// Text on top of the accent colour.
    pub accent_text: String,
    /// Tiling paused, and anything else that wants to be noticed.
    pub urgent: String,
}

impl Default for BarTheme {
    fn default() -> Self {
        Self {
            background: "#1b1b1b".into(),
            foreground: "#f0f0f0".into(),
            muted: "#a0a0a0".into(),
            surface: "#2b2b2b".into(),
            accent: "#5b8def".into(),
            accent_text: "#10141c".into(),
            urgent: "#ef6a70".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BarConfig {
    pub enabled: bool,
    pub position: BarPosition,
    /// Height in logical pixels, scaled by the DPI of each display.
    pub height: u32,
    /// Uniform window opacity, from 0.2 to 1.
    pub opacity: f32,
    /// Take the space out of the work area so nothing else is drawn under the
    /// bar. Turning this off leaves the bar floating over the windows.
    pub reserve_space: bool,
    /// Only put a bar on the primary display.
    pub primary_only: bool,
    pub font_family: String,
    pub font_size: f32,
    /// `%H` `%I` `%M` `%S` `%p` `%d` `%m` `%y` `%Y` `%a` `%b`, and `%%`.
    pub clock_format: String,
    pub left: Vec<BarModule>,
    pub center: Vec<BarModule>,
    pub right: Vec<BarModule>,
    pub theme: BarTheme,
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: BarPosition::Top,
            height: 32,
            opacity: 1.0,
            reserve_space: true,
            primary_only: false,
            font_family: "Segoe UI".into(),
            font_size: 12.0,
            clock_format: "%a %d %b  %H:%M".into(),
            left: vec![BarModule::Layout, BarModule::Windows],
            center: vec![BarModule::Title],
            right: vec![BarModule::Tiling, BarModule::Battery, BarModule::Clock],
            theme: BarTheme::default(),
        }
    }
}

impl BarConfig {
    /// Height in physical pixels on a display with this scale factor.
    pub fn scaled_height(&self, scale: f32) -> i32 {
        (self.height.clamp(16, 96) as f32 * scale).round() as i32
    }

    /// The opacity clamped into the range the window can actually use. Fully
    /// transparent would leave the user with a bar they cannot find.
    pub fn window_opacity(&self) -> f32 {
        self.opacity.clamp(0.2, 1.0)
    }
}
