use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct HeaderWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for HeaderWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let region_text = self.state.region.to_string();
        let line = Line::from(vec![
            Span::styled(" igv-rs ", self.theme.get("HEADER")),
            Span::raw("  "),
            Span::styled(region_text, self.theme.get("OVERVIEW")),
            Span::raw("  "),
            Span::styled(
                format!("({} bp)", self.state.region.width()),
                self.theme.get("WARNING"),
            ),
            Span::raw("  "),
            Span::styled(
                if self.state.loading { "loading…" } else { "" },
                self.theme.get("WARNING"),
            ),
        ]);
        Paragraph::new(line)
            .block(Block::default().borders(Borders::BOTTOM).style(self.theme.get("BORDER")))
            .render(area, buf);
    }
}
