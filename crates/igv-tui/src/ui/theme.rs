//! Color theme: built-in dark/light presets + user overrides via TOML.

use std::collections::HashMap;
use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Dark,
    Light,
    Custom,
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

pub fn load_theme(preset_override: Option<bool>, config_path: Option<&Path>) -> Theme {
    // CLI flag takes precedence. `Some(true)` ⇒ light, `Some(false)` ⇒ dark.
    let cli_pref = preset_override;
    let config: Option<ThemeConfig> = config_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str::<HashMap<String, toml::Value>>(&s).ok())
        .and_then(|m| m.get("theme").cloned())
        .and_then(|v| v.try_into().ok());

    let preset = match cli_pref {
        Some(true) => Preset::Light,
        Some(false) => Preset::Dark,
        None => match config.as_ref().map(|c| c.preset.as_str()) {
            Some("light") => Preset::Light,
            Some("dark") | None => Preset::Dark,
            Some(_) => Preset::Custom,
        },
    };

    let mut theme = match preset {
        Preset::Light => Theme::light(),
        _ => Theme::dark(),
    };

    if let Some(cfg) = config {
        theme.merge_overrides(&cfg.custom);
    }
    theme
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
}
