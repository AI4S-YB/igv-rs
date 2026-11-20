//! Modal keybinding & runtime command help overlay (`?` toggles, any key dismisses).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::ui::theme::Theme;

pub struct HelpWidget<'a> {
    pub theme: &'a Theme,
}

type Section = (&'static str, &'static [(&'static str, &'static str)]);

const SECTIONS: &[Section] = &[
    (
        "Navigation & Panes",
        &[
            ("a / ← / d / →", "page backward / forward (full window)"),
            ("h / l", "fine pan backward / forward (1/10 window)"),
            ("w / ↑ / s / ↓", "zoom in / out"),
            ("Tab", "cycle active split locus pane (1..=4)"),
            ("Mouse", "click/drag ruler to jump/zoom, click feature to inspect"),
        ],
    ),
    (
        "Alignments & Views",
        &[
            ("j / k / Wheel", "scroll alignment lanes down / up"),
            ("+ / -", "grow / shrink alignment-track height"),
            ("P / C / J / T", "toggle Pairs / cycle Color / Sashimi / 3-frame AA"),
            ("G / B / S", "inline Kitty/iTerm2/Sixel / igv.js Browser / SVG snapshot"),
            ("] [ / } { / < >", "resize coverage / signal / link tracks"),
            ("\\", "toggle signal shared / per-track Y-scale"),
        ],
    ),
    (
        "Runtime : Commands",
        &[
            (":sort / :group", "sort (base|mapq|hp|start...) / group (strand|rg|hp...)"),
            (":color / :pairs", "color (strand|mod|bisulfite|pair|insert|tag|mapq)"),
            (":motif / :strand", "highlight IUPAC motif / toggle complement strand"),
            (":split / :load", "add split pane / load track URL or path"),
            (":blat / :circular", "query UCSC hgBlat / export Circos chord SVG/PNG"),
            (":save-session", "save/load .toml session or import IGV .json"),
        ],
    ),
];

impl Widget for HelpWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = build_lines(self.theme);
        let inner_w = content_width();
        let inner_h = lines.len() as u16;
        let popup_w = (inner_w + 4).min(area.width);
        let popup_h = (inner_h + 2).min(area.height);
        let popup = centered(area, popup_w, popup_h);

        Clear.render(popup, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                " Keybindings & Commands — press any key to close ",
                self.theme.get("HEADER"),
            ))
            .style(self.theme.get("BORDER"));
        Paragraph::new(lines).block(block).render(popup, buf);
    }
}

fn build_lines(theme: &Theme) -> Vec<Line<'static>> {
    let key_w = max_key_width();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let key_style = Style::default().add_modifier(Modifier::BOLD);
    let title_style = theme.get("HEADER");

    for (i, (title, entries)) in SECTIONS.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(format!(" {title} "), title_style)));
        for (key, desc) in *entries {
            let pad = " ".repeat(key_w.saturating_sub(key.chars().count()));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled((*key).to_string(), key_style),
                Span::raw(pad),
                Span::raw("  "),
                Span::raw((*desc).to_string()),
            ]));
        }
    }
    lines
}

fn max_key_width() -> usize {
    SECTIONS
        .iter()
        .flat_map(|(_, e)| e.iter())
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0)
}

fn content_width() -> u16 {
    let key_w = max_key_width();
    let desc_w = SECTIONS
        .iter()
        .flat_map(|(_, e)| e.iter())
        .map(|(_, d)| d.chars().count())
        .max()
        .unwrap_or(0);
    (2 + key_w + 2 + desc_w) as u16
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lines_fit_expected_dimensions() {
        let t = Theme::dark();
        let lines = build_lines(&t);
        assert!(lines.len() >= 10);
    }
}
