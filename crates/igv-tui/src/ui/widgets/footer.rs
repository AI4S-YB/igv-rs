use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::state::{AppState, StatusKind};
use crate::ui::theme::Theme;

const KEYS: &str = "a/d:nav  w/s:zoom  g/::goto  m<c>:mark  '<c>:jump  t:theme  q:quit";

pub struct FooterWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for FooterWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans: Vec<Span<'_>> = Vec::new();

        if let Some(msg) = &self.state.status {
            let style = match msg.kind {
                StatusKind::Info => self.theme.get("SUCCESS"),
                StatusKind::Warning => self.theme.get("WARNING"),
                StatusKind::Error => self.theme.get("ERROR"),
            };
            spans.push(Span::styled(format!(" {} ", msg.text), style));
            spans.push(Span::raw("  "));
        }

        if self.state.command_open {
            spans.push(Span::styled(":", self.theme.get("HEADER")));
            spans.push(Span::raw(self.state.command_buffer.clone()));
            spans.push(Span::styled("█", self.theme.get("HEADER")));
        } else {
            spans.push(Span::styled(KEYS, self.theme.get("FOOTER")));
        }

        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::TOP).style(self.theme.get("BORDER")))
            .render(area, buf);
    }
}
