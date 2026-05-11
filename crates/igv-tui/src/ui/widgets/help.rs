//! Modal keybinding help overlay (`?` toggles, any key dismisses).
//!
//! Drawn on top of the main UI as a centered, bordered popup. Sized to its
//! content; if the terminal is too small, the popup is clipped to the available
//! area without crashing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::ui::theme::Theme;

pub struct HelpWidget<'a> {
    pub theme: &'a Theme,
}

/// (section title, [(key, description)]).
type Section = (&'static str, &'static [(&'static str, &'static str)]);

const SECTIONS: &[Section] = &[
    (
        "Navigation",
        &[
            ("a / ←", "page backward (full window)"),
            ("d / →", "page forward (full window)"),
            ("h", "fine pan backward (1/10 window)"),
            ("l", "fine pan forward (1/10 window)"),
            ("w / ↑", "zoom in"),
            ("s / ↓", "zoom out"),
        ],
    ),
    (
        "Tracks",
        &[
            ("j / k", "scroll alignment lanes down / up"),
            ("+ / -", "grow / shrink alignment-track height"),
            ("] / [", "grow / shrink coverage-track height"),
            ("} / {", "grow / shrink signal-track height"),
            ("< / >", "shrink / grow link-track height"),
            ("\\", "toggle signal shared / per-track Y-scale"),
        ],
    ),
    (
        "Jump & bookmarks",
        &[
            (": / g", "open command palette (region or gene)"),
            ("m<c>", "set bookmark to letter <c>"),
            ("'<c>", "jump to bookmark <c>"),
        ],
    ),
    (
        "Misc",
        &[
            ("t", "cycle theme (dark / light / paper / solarized / dracula / gruvbox)"),
            ("S", "save SVG snapshot of current view"),
            ("B", "open browser view (igv.js)"),
            ("?", "toggle this help"),
            ("q / Ctrl-C", "quit"),
        ],
    ),
];

impl Widget for HelpWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = build_lines(self.theme);
        let inner_w = content_width();
        let inner_h = lines.len() as u16;
        // +2 for top/bottom borders, +2 for left/right padding columns.
        let popup_w = (inner_w + 4).min(area.width);
        let popup_h = (inner_h + 2).min(area.height);
        let popup = centered(area, popup_w, popup_h);

        Clear.render(popup, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                " Keybindings — press any key to close ",
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
        lines.push(Line::from(Span::styled(
            format!(" {title} "),
            title_style,
        )));
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
    // 2 (left margin) + key_w + 2 (gap) + desc_w
    (2 + key_w + 2 + desc_w) as u16
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect { x, y, width: w, height: h }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn renders_without_panic_in_small_area() {
        let backend = TestBackend::new(20, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|f| {
                f.render_widget(HelpWidget { theme: &theme }, f.area());
            })
            .unwrap();
    }

    #[test]
    fn renders_in_normal_area() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        terminal
            .draw(|f| {
                f.render_widget(HelpWidget { theme: &theme }, f.area());
            })
            .unwrap();
    }
}
