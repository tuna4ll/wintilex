//! Turning the colours from the config file into something Direct2D takes.

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

use wintilex_core::config::BarTheme;

use crate::segments::Tone;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: D2D1_COLOR_F,
    pub foreground: D2D1_COLOR_F,
    pub muted: D2D1_COLOR_F,
    pub surface: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub accent_text: D2D1_COLOR_F,
    pub urgent: D2D1_COLOR_F,
    layout: D2D1_COLOR_F,
    monitor: D2D1_COLOR_F,
    cpu: D2D1_COLOR_F,
    memory: D2D1_COLOR_F,
    battery: D2D1_COLOR_F,
    clock: D2D1_COLOR_F,
}

impl Palette {
    /// A colour that will not parse falls back to the shipped default for that
    /// slot: a typo in one field should cost that field, not the whole bar.
    pub fn from_theme(theme: &BarTheme) -> Palette {
        let fallback = BarTheme::default();
        Palette {
            background: parse(&theme.background, &fallback.background),
            foreground: parse(&theme.foreground, &fallback.foreground),
            muted: parse(&theme.muted, &fallback.muted),
            surface: parse(&theme.surface, &fallback.surface),
            accent: parse(&theme.accent, &fallback.accent),
            accent_text: parse(&theme.accent_text, &fallback.accent_text),
            urgent: parse(&theme.urgent, &fallback.urgent),
            layout: parse(&theme.layout, &fallback.layout),
            monitor: parse(&theme.monitor, &fallback.monitor),
            cpu: parse(&theme.cpu, &fallback.cpu),
            memory: parse(&theme.memory, &fallback.memory),
            battery: parse(&theme.battery, &fallback.battery),
            clock: parse(&theme.clock, &fallback.clock),
        }
    }

    /// The accent a module's icon is drawn in.
    pub fn tone(&self, tone: Tone) -> D2D1_COLOR_F {
        match tone {
            Tone::Text => self.foreground,

            Tone::Urgent => self.urgent,
            Tone::Layout => self.layout,
            Tone::Monitor => self.monitor,
            Tone::Cpu => self.cpu,
            Tone::Memory => self.memory,
            Tone::Battery => self.battery,
            Tone::Clock => self.clock,
        }
    }
}

fn parse(value: &str, fallback: &str) -> D2D1_COLOR_F {
    colour(value).or_else(|| colour(fallback)).unwrap_or(D2D1_COLOR_F {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    })
}

/// `#rgb`, `#rrggbb` and `#rrggbbaa`, with or without the hash.
fn colour(value: &str) -> Option<D2D1_COLOR_F> {
    let digits = value.trim().trim_start_matches('#');
    let expanded: String = match digits.len() {
        3 | 4 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => digits.to_string(),
        _ => return None,
    };

    let channel = |index: usize| {
        u8::from_str_radix(expanded.get(index * 2..index * 2 + 2)?, 16).ok().map(f32::from)
    };

    Some(D2D1_COLOR_F {
        r: channel(0)? / 255.0,
        g: channel(1)? / 255.0,
        b: channel(2)? / 255.0,
        a: channel(3).unwrap_or(255.0) / 255.0,
    })
}

/// The same colour at a different opacity, for a pill that should only just be
/// there.
pub fn fade(colour: D2D1_COLOR_F, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { a: colour.a * alpha, ..colour }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_usual_spellings() {
        let short = colour("#fff").unwrap();
        assert_eq!((short.r, short.g, short.b, short.a), (1.0, 1.0, 1.0, 1.0));

        let long = colour("5b8def").unwrap();
        assert!((long.r - 0.357).abs() < 0.01);
        assert_eq!(long.a, 1.0);

        let with_alpha = colour("#00000080").unwrap();
        assert!((with_alpha.a - 0.502).abs() < 0.01);
    }

    #[test]
    fn rejects_nonsense() {
        assert!(colour("").is_none());
        assert!(colour("#12345").is_none());
        assert!(colour("#gggggg").is_none());
    }

    #[test]
    fn a_broken_colour_falls_back_to_its_own_default() {
        let theme = BarTheme { accent: "nonsense".into(), ..Default::default() };
        let palette = Palette::from_theme(&theme);
        let default = Palette::from_theme(&BarTheme::default());
        assert_eq!(palette.accent.r, default.accent.r);
    }
}
