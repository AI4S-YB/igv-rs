//! Color theme: built-in presets + user overrides via TOML.
//!
//! `t` cycles through `ThemePreset` variants in declaration order. New
//! presets should be added to both the enum and `Theme::for_preset` (the
//! compiler will flag the latter via the exhaustive match).

use std::collections::HashMap;
use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    Dark,
    Light,
    Paper,
    SolarizedDark,
    SolarizedLight,
    Dracula,
    GruvboxDark,
}

impl ThemePreset {
    /// All variants in cycle order (matches declaration order).
    pub const ALL: [ThemePreset; 7] = [
        ThemePreset::Dark,
        ThemePreset::Light,
        ThemePreset::Paper,
        ThemePreset::SolarizedDark,
        ThemePreset::SolarizedLight,
        ThemePreset::Dracula,
        ThemePreset::GruvboxDark,
    ];

    /// Next preset in `ALL`, wrapping at the end.
    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    /// Human-readable name for status messages.
    pub fn name(self) -> &'static str {
        match self {
            ThemePreset::Dark => "dark",
            ThemePreset::Light => "light",
            ThemePreset::Paper => "paper",
            ThemePreset::SolarizedDark => "solarized-dark",
            ThemePreset::SolarizedLight => "solarized-light",
            ThemePreset::Dracula => "dracula",
            ThemePreset::GruvboxDark => "gruvbox-dark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "dark" => Some(ThemePreset::Dark),
            "light" => Some(ThemePreset::Light),
            "paper" | "white" => Some(ThemePreset::Paper),
            "solarized-dark" | "solarized_dark" => Some(ThemePreset::SolarizedDark),
            "solarized-light" | "solarized_light" => Some(ThemePreset::SolarizedLight),
            "dracula" => Some(ThemePreset::Dracula),
            "gruvbox-dark" | "gruvbox" => Some(ThemePreset::GruvboxDark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    map: HashMap<String, Style>,
}

impl Theme {
    pub fn dark() -> Self {
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(Color::White));
        m.insert("MATCH_FWD".into(), Style::default().fg(Color::Cyan));
        m.insert("MATCH_REV".into(), Style::default().fg(Color::Magenta));
        m.insert(
            "MISMATCH".into(),
            Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "DELETION".into(),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "INSERTION".into(),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "VARIANT".into(),
            Style::default().fg(Color::White).bg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "HEADER".into(),
            Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD),
        );
        m.insert("FOOTER".into(), Style::default().fg(Color::White).bg(Color::DarkGray));
        m.insert("OVERVIEW".into(), Style::default().fg(Color::Yellow));
        m.insert("BORDER".into(), Style::default().fg(Color::DarkGray));
        m.insert("COVERAGE".into(), Style::default().fg(Color::Cyan));
        m.insert("SIGNAL".into(), Style::default().fg(Color::Cyan));
        m.insert("WARNING".into(), Style::default().fg(Color::Yellow));
        m.insert("ERROR".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(Color::Green));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(Color::Green));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(Color::DarkGray));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    pub fn light() -> Self {
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(Color::Rgb(180, 100, 0)).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(Color::Black));
        m.insert("MATCH_FWD".into(), Style::default().fg(Color::Blue));
        m.insert("MATCH_REV".into(), Style::default().fg(Color::Magenta));
        m.insert(
            "MISMATCH".into(),
            Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "DELETION".into(),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "INSERTION".into(),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "VARIANT".into(),
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "HEADER".into(),
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert("FOOTER".into(), Style::default().fg(Color::Black).bg(Color::Green));
        m.insert("OVERVIEW".into(), Style::default().fg(Color::Rgb(200, 100, 0)));
        m.insert("BORDER".into(), Style::default().fg(Color::Gray));
        m.insert("COVERAGE".into(), Style::default().fg(Color::Blue));
        m.insert("SIGNAL".into(), Style::default().fg(Color::Blue));
        m.insert("WARNING".into(), Style::default().fg(Color::Rgb(180, 100, 0)));
        m.insert("ERROR".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(Color::Green));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(Color::Rgb(0, 100, 0)).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(Color::Rgb(0, 100, 0)));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(Color::Gray));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    pub fn for_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::Dark => Self::dark(),
            ThemePreset::Light => Self::light(),
            ThemePreset::Paper => Self::paper(),
            ThemePreset::SolarizedDark => Self::solarized_dark(),
            ThemePreset::SolarizedLight => Self::solarized_light(),
            ThemePreset::Dracula => Self::dracula(),
            ThemePreset::GruvboxDark => Self::gruvbox_dark(),
        }
    }

    /// Paper — explicit white background everywhere, ink-on-paper aesthetic.
    /// Unlike `Light` (which relies on the terminal's own bg color), every
    /// styled key sets `bg(#ffffff)` so panels stay paper-white even on
    /// terminals configured with a tinted background. Foreground colors are
    /// deeply saturated to retain contrast on white.
    pub fn paper() -> Self {
        let bg = Color::Rgb(0xff, 0xff, 0xff);
        let ink = Color::Rgb(0x1a, 0x1a, 0x1a);
        let muted = Color::Rgb(0x70, 0x70, 0x70);
        let rule = Color::Rgb(0xc0, 0xc0, 0xc0);
        let panel = Color::Rgb(0xee, 0xee, 0xee);
        let red = Color::Rgb(0xc0, 0x39, 0x2b);
        let green = Color::Rgb(0x1d, 0x7a, 0x2c);
        let yellow = Color::Rgb(0xa6, 0x7c, 0x00);
        let orange = Color::Rgb(0xc7, 0x53, 0x00);
        let blue = Color::Rgb(0x18, 0x4a, 0xa6);
        let cyan = Color::Rgb(0x0e, 0x77, 0x90);
        let magenta = Color::Rgb(0xa6, 0x2c, 0x82);
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(green).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(blue).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(yellow).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(red).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(ink).bg(bg));
        m.insert("MATCH_FWD".into(), Style::default().fg(blue).bg(bg));
        m.insert("MATCH_REV".into(), Style::default().fg(magenta).bg(bg));
        m.insert("MISMATCH".into(), Style::default().fg(bg).bg(red).add_modifier(Modifier::BOLD));
        m.insert("DELETION".into(), Style::default().fg(magenta).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("INSERTION".into(), Style::default().fg(green).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("VARIANT".into(), Style::default().fg(bg).bg(green).add_modifier(Modifier::BOLD));
        m.insert("HEADER".into(), Style::default().fg(ink).bg(panel).add_modifier(Modifier::BOLD));
        m.insert("FOOTER".into(), Style::default().fg(ink).bg(panel));
        m.insert("OVERVIEW".into(), Style::default().fg(orange).bg(bg));
        m.insert("BORDER".into(), Style::default().fg(rule).bg(bg));
        m.insert("COVERAGE".into(), Style::default().fg(blue).bg(bg));
        m.insert("SIGNAL".into(), Style::default().fg(cyan).bg(bg));
        m.insert("WARNING".into(), Style::default().fg(orange).bg(bg));
        m.insert("ERROR".into(), Style::default().fg(red).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(green).bg(bg));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(green).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(green).bg(bg));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(muted).bg(bg));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(blue).bg(bg).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(blue).bg(bg).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    /// Solarized Dark — Ethan Schoonover's classic dark palette.
    /// base03 #002b36 / base02 #073642 / base0 #839496.
    pub fn solarized_dark() -> Self {
        let base03 = Color::Rgb(0x00, 0x2b, 0x36);
        let base02 = Color::Rgb(0x07, 0x36, 0x42);
        let base01 = Color::Rgb(0x58, 0x6e, 0x75);
        let base0 = Color::Rgb(0x83, 0x94, 0x96);
        let yellow = Color::Rgb(0xb5, 0x89, 0x00);
        let orange = Color::Rgb(0xcb, 0x4b, 0x16);
        let red = Color::Rgb(0xdc, 0x32, 0x2f);
        let magenta = Color::Rgb(0xd3, 0x36, 0x82);
        let blue = Color::Rgb(0x26, 0x8b, 0xd2);
        let cyan = Color::Rgb(0x2a, 0xa1, 0x98);
        let green = Color::Rgb(0x85, 0x99, 0x00);
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(cyan).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(yellow).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(base0));
        m.insert("MATCH_FWD".into(), Style::default().fg(cyan));
        m.insert("MATCH_REV".into(), Style::default().fg(magenta));
        m.insert("MISMATCH".into(), Style::default().fg(base03).bg(red).add_modifier(Modifier::BOLD));
        m.insert("DELETION".into(), Style::default().fg(magenta).add_modifier(Modifier::BOLD));
        m.insert("INSERTION".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("VARIANT".into(), Style::default().fg(base03).bg(green).add_modifier(Modifier::BOLD));
        m.insert("HEADER".into(), Style::default().fg(base0).bg(base02).add_modifier(Modifier::BOLD));
        m.insert("FOOTER".into(), Style::default().fg(base0).bg(base02));
        m.insert("OVERVIEW".into(), Style::default().fg(yellow));
        m.insert("BORDER".into(), Style::default().fg(base01));
        m.insert("COVERAGE".into(), Style::default().fg(blue));
        m.insert("SIGNAL".into(), Style::default().fg(cyan));
        m.insert("WARNING".into(), Style::default().fg(orange));
        m.insert("ERROR".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(green));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(green));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(base01));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(blue).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(blue).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    /// Solarized Light — same palette, light backgrounds.
    /// base3 #fdf6e3 / base2 #eee8d5 / base00 #657b83.
    pub fn solarized_light() -> Self {
        let base2 = Color::Rgb(0xee, 0xe8, 0xd5);
        let base1 = Color::Rgb(0x93, 0xa1, 0xa1);
        let base00 = Color::Rgb(0x65, 0x7b, 0x83);
        let base01 = Color::Rgb(0x58, 0x6e, 0x75);
        let yellow = Color::Rgb(0xb5, 0x89, 0x00);
        let orange = Color::Rgb(0xcb, 0x4b, 0x16);
        let red = Color::Rgb(0xdc, 0x32, 0x2f);
        let magenta = Color::Rgb(0xd3, 0x36, 0x82);
        let blue = Color::Rgb(0x26, 0x8b, 0xd2);
        let cyan = Color::Rgb(0x2a, 0xa1, 0x98);
        let green = Color::Rgb(0x85, 0x99, 0x00);
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(cyan).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(yellow).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(base00));
        m.insert("MATCH_FWD".into(), Style::default().fg(blue));
        m.insert("MATCH_REV".into(), Style::default().fg(magenta));
        m.insert("MISMATCH".into(), Style::default().fg(Color::White).bg(red).add_modifier(Modifier::BOLD));
        m.insert("DELETION".into(), Style::default().fg(magenta).add_modifier(Modifier::BOLD));
        m.insert("INSERTION".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("VARIANT".into(), Style::default().fg(base01).bg(yellow).add_modifier(Modifier::BOLD));
        m.insert("HEADER".into(), Style::default().fg(base01).bg(base2).add_modifier(Modifier::BOLD));
        m.insert("FOOTER".into(), Style::default().fg(base01).bg(base2));
        m.insert("OVERVIEW".into(), Style::default().fg(orange));
        m.insert("BORDER".into(), Style::default().fg(base1));
        m.insert("COVERAGE".into(), Style::default().fg(blue));
        m.insert("SIGNAL".into(), Style::default().fg(blue));
        m.insert("WARNING".into(), Style::default().fg(orange));
        m.insert("ERROR".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(green));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(green));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(base1));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(blue).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(blue).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    /// Dracula — high-contrast dark palette with vivid pink/cyan accents.
    /// bg #282a36 / fg #f8f8f2.
    pub fn dracula() -> Self {
        let bg = Color::Rgb(0x28, 0x2a, 0x36);
        let current_line = Color::Rgb(0x44, 0x47, 0x5a);
        let fg = Color::Rgb(0xf8, 0xf8, 0xf2);
        let comment = Color::Rgb(0x62, 0x72, 0xa4);
        let cyan = Color::Rgb(0x8b, 0xe9, 0xfd);
        let green = Color::Rgb(0x50, 0xfa, 0x7b);
        let orange = Color::Rgb(0xff, 0xb8, 0x6c);
        let pink = Color::Rgb(0xff, 0x79, 0xc6);
        let purple = Color::Rgb(0xbd, 0x93, 0xf9);
        let red = Color::Rgb(0xff, 0x55, 0x55);
        let yellow = Color::Rgb(0xf1, 0xfa, 0x8c);
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(cyan).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(yellow).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(fg));
        m.insert("MATCH_FWD".into(), Style::default().fg(cyan));
        m.insert("MATCH_REV".into(), Style::default().fg(pink));
        m.insert("MISMATCH".into(), Style::default().fg(bg).bg(red).add_modifier(Modifier::BOLD));
        m.insert("DELETION".into(), Style::default().fg(pink).add_modifier(Modifier::BOLD));
        m.insert("INSERTION".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("VARIANT".into(), Style::default().fg(bg).bg(green).add_modifier(Modifier::BOLD));
        m.insert("HEADER".into(), Style::default().fg(bg).bg(purple).add_modifier(Modifier::BOLD));
        m.insert("FOOTER".into(), Style::default().fg(fg).bg(current_line));
        m.insert("OVERVIEW".into(), Style::default().fg(orange));
        m.insert("BORDER".into(), Style::default().fg(comment));
        m.insert("COVERAGE".into(), Style::default().fg(cyan));
        m.insert("SIGNAL".into(), Style::default().fg(purple));
        m.insert("WARNING".into(), Style::default().fg(yellow));
        m.insert("ERROR".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(green));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(green));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(comment));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(pink).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(pink).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    /// Gruvbox Dark — warm, retro palette popular for editors.
    /// bg0 #282828 / fg1 #ebdbb2.
    pub fn gruvbox_dark() -> Self {
        let bg0 = Color::Rgb(0x28, 0x28, 0x28);
        let bg2 = Color::Rgb(0x50, 0x49, 0x45);
        let fg1 = Color::Rgb(0xeb, 0xdb, 0xb2);
        let gray = Color::Rgb(0x92, 0x83, 0x74);
        let red = Color::Rgb(0xfb, 0x49, 0x34);
        let green = Color::Rgb(0xb8, 0xbb, 0x26);
        let yellow = Color::Rgb(0xfa, 0xbd, 0x2f);
        let blue = Color::Rgb(0x83, 0xa5, 0x98);
        let purple = Color::Rgb(0xd3, 0x86, 0x9b);
        let aqua = Color::Rgb(0x8e, 0xc0, 0x7c);
        let orange = Color::Rgb(0xfe, 0x80, 0x19);
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(aqua).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(yellow).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(fg1));
        m.insert("MATCH_FWD".into(), Style::default().fg(blue));
        m.insert("MATCH_REV".into(), Style::default().fg(purple));
        m.insert("MISMATCH".into(), Style::default().fg(bg0).bg(red).add_modifier(Modifier::BOLD));
        m.insert("DELETION".into(), Style::default().fg(purple).add_modifier(Modifier::BOLD));
        m.insert("INSERTION".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("VARIANT".into(), Style::default().fg(bg0).bg(yellow).add_modifier(Modifier::BOLD));
        m.insert("HEADER".into(), Style::default().fg(bg0).bg(orange).add_modifier(Modifier::BOLD));
        m.insert("FOOTER".into(), Style::default().fg(fg1).bg(bg2));
        m.insert("OVERVIEW".into(), Style::default().fg(orange));
        m.insert("BORDER".into(), Style::default().fg(gray));
        m.insert("COVERAGE".into(), Style::default().fg(aqua));
        m.insert("SIGNAL".into(), Style::default().fg(orange));
        m.insert("WARNING".into(), Style::default().fg(yellow));
        m.insert("ERROR".into(), Style::default().fg(red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(green));
        m.insert("ANNOTATION_EXON".into(), Style::default().fg(green).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_UTR".into(), Style::default().fg(green));
        m.insert("ANNOTATION_INTRON".into(), Style::default().fg(gray));
        m.insert("ANNOTATION_NAME".into(), Style::default().fg(orange).add_modifier(Modifier::BOLD));
        m.insert("ANNOTATION_STRAND".into(), Style::default().fg(orange).add_modifier(Modifier::BOLD));
        Self { map: m }
    }

    pub fn get(&self, key: &str) -> Style {
        self.map.get(key).copied().unwrap_or_default()
    }

    pub fn merge_overrides(&mut self, overrides: &HashMap<String, String>) {
        for (k, v) in overrides {
            if let Some(style) = parse_style(v) {
                self.map.insert(k.clone(), style);
            }
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

fn default_preset() -> String {
    "dark".into()
}

/// Load the initial theme: CLI `--light` flag wins; otherwise the config's
/// `[theme] preset = "..."` is consulted; otherwise dark. Returns both the
/// chosen preset (so `t` can cycle from a known starting point) and the
/// theme with custom overrides applied.
pub fn load_theme(
    preset_override: Option<bool>,
    config_path: Option<&Path>,
) -> (ThemePreset, Theme) {
    // CLI flag takes precedence. `Some(true)` ⇒ light, `Some(false)` ⇒ dark.
    let config: Option<ThemeConfig> = config_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str::<HashMap<String, toml::Value>>(&s).ok())
        .and_then(|m| m.get("theme").cloned())
        .and_then(|v| v.try_into().ok());

    let preset = match preset_override {
        Some(true) => ThemePreset::Light,
        Some(false) => ThemePreset::Dark,
        None => config
            .as_ref()
            .and_then(|c| ThemePreset::parse(&c.preset))
            .unwrap_or(ThemePreset::Dark),
    };

    let mut theme = Theme::for_preset(preset);
    if let Some(cfg) = config {
        theme.merge_overrides(&cfg.custom);
    }
    (preset, theme)
}

fn parse_style(s: &str) -> Option<Style> {
    // Minimal parser: tokens separated by spaces. Recognized tokens:
    //   "bold", "dim", "italic", "underline"
    //   "<color>"               → fg
    //   "on <color>"            → bg
    //   colors: black, red, green, yellow, blue, magenta, cyan, white, gray
    let mut style = Style::default();
    let mut tokens = s.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        match tok {
            "bold" => style = style.add_modifier(Modifier::BOLD),
            "dim" => style = style.add_modifier(Modifier::DIM),
            "italic" => style = style.add_modifier(Modifier::ITALIC),
            "underline" => style = style.add_modifier(Modifier::UNDERLINED),
            "on" => {
                if let Some(c) = tokens.next() {
                    style = style.bg(parse_color(c)?);
                }
            }
            other => {
                style = style.fg(parse_color(other)?);
            }
        }
    }
    Some(style)
}

fn parse_color(s: &str) -> Option<Color> {
    Some(match s {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_has_nucleotide_styles() {
        let t = Theme::dark();
        assert_ne!(t.get("A"), Style::default());
        assert_ne!(t.get("C"), Style::default());
    }

    #[test]
    fn parse_style_handles_bold_fg() {
        let s = parse_style("bold red").unwrap();
        assert!(s.add_modifier.contains(Modifier::BOLD));
        assert_eq!(s.fg, Some(Color::Red));
    }

    #[test]
    fn parse_style_handles_fg_on_bg() {
        let s = parse_style("white on red").unwrap();
        assert_eq!(s.fg, Some(Color::White));
        assert_eq!(s.bg, Some(Color::Red));
    }

    #[test]
    fn theme_preset_cycles_through_all_variants() {
        // Every preset must lead to a different next() value (cycle never
        // gets stuck) and the cycle must close after `ALL.len()` steps.
        let mut p = ThemePreset::Dark;
        let mut seen = vec![p];
        for _ in 0..ThemePreset::ALL.len() - 1 {
            p = p.next();
            assert!(!seen.contains(&p), "cycle revisits {:?}", p);
            seen.push(p);
        }
        assert_eq!(p.next(), ThemePreset::Dark, "cycle should wrap to Dark");
    }

    #[test]
    fn every_preset_defines_required_keys() {
        // Widgets read these keys by name; a missing key would show up as the
        // default style. Guard all presets against accidentally dropping one.
        const REQUIRED: &[&str] = &[
            "A", "C", "G", "T", "N",
            "MATCH_FWD", "MATCH_REV", "MISMATCH",
            "DELETION", "INSERTION", "VARIANT",
            "HEADER", "FOOTER", "OVERVIEW", "BORDER",
            "COVERAGE", "SIGNAL",
            "WARNING", "ERROR", "SUCCESS",
            "ANNOTATION_EXON", "ANNOTATION_UTR", "ANNOTATION_INTRON",
            "ANNOTATION_NAME", "ANNOTATION_STRAND",
        ];
        for &p in &ThemePreset::ALL {
            let theme = Theme::for_preset(p);
            for &k in REQUIRED {
                assert_ne!(
                    theme.get(k),
                    Style::default(),
                    "{} missing key {k}",
                    p.name()
                );
            }
        }
    }

    #[test]
    fn theme_preset_parse_accepts_aliases() {
        assert_eq!(ThemePreset::parse("dark"), Some(ThemePreset::Dark));
        assert_eq!(
            ThemePreset::parse("solarized-dark"),
            Some(ThemePreset::SolarizedDark)
        );
        assert_eq!(
            ThemePreset::parse("solarized_dark"),
            Some(ThemePreset::SolarizedDark)
        );
        assert_eq!(ThemePreset::parse("gruvbox"), Some(ThemePreset::GruvboxDark));
        assert_eq!(ThemePreset::parse("DRACULA"), Some(ThemePreset::Dracula));
        assert_eq!(ThemePreset::parse("nope"), None);
    }
}
