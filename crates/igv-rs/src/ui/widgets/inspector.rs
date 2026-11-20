//! Modal feature inspector popup (`InspectorPopup`) opened by clicking a read,
//! variant, gene, splice junction, CNV segment, link, or GWAS point.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app::state::InspectorPopup;
use crate::ui::theme::Theme;

pub struct InspectorWidget<'a> {
    pub popup: &'a InspectorPopup,
    pub theme: &'a Theme,
}

impl Widget for InspectorWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let key_w = self
            .popup
            .rows
            .iter()
            .map(|(k, _)| k.chars().count())
            .max()
            .unwrap_or(6);
        let val_w = self
            .popup
            .rows
            .iter()
            .map(|(_, v)| v.chars().count())
            .max()
            .unwrap_or(10);

        let popup_w = ((key_w + val_w + 8) as u16).min(area.width.saturating_sub(4)).max(30);
        let popup_h = ((self.popup.rows.len() + 3) as u16)
            .min(area.height.saturating_sub(4))
            .max(5);
        let x = area.x + area.width.saturating_sub(popup_w) / 2;
        let y = area.y + area.height.saturating_sub(popup_h) / 2;
        let rect = Rect::new(x, y, popup_w, popup_h);

        Clear.render(rect, buf);

        let key_style = Style::default().add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line<'static>> = self
            .popup
            .rows
            .iter()
            .map(|(k, v)| {
                let pad = " ".repeat(key_w.saturating_sub(k.chars().count()));
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(k.clone(), key_style),
                    Span::raw(pad),
                    Span::raw(" : "),
                    Span::raw(v.clone()),
                ])
            })
            .collect();
        lines.push(Line::from(Span::styled(
            " [Esc/Enter/q or click to close]",
            self.theme.get("WARNING"),
        )));

        let block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                format!(" {} ", self.popup.title),
                self.theme.get("HEADER"),
            ))
            .style(self.theme.get("BORDER"));
        Paragraph::new(lines).block(block).render(rect, buf);
    }
}
