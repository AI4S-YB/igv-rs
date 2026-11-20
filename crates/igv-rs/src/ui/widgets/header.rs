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
        let mut spans = vec![
            Span::styled(" igv-rs ", self.theme.get("HEADER")),
            Span::raw("  "),
        ];
        if let Some(g) = &self.state.genome_id {
            spans.push(Span::styled(format!("[{g}]"), self.theme.get("SUCCESS")));
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(region_text, self.theme.get("OVERVIEW")));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("({} bp)", self.state.region.width()),
            self.theme.get("WARNING"),
        ));
        if self.state.panes.len() > 1 {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("pane {}/{}", self.state.active_pane + 1, self.state.panes.len()),
                self.theme.get("SUCCESS"),
            ));
        }
        if self.state.show_sashimi {
            spans.push(Span::styled("  [sashimi]", self.theme.get("HEADER")));
        }
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            if self.state.loading { "loading…" } else { "" },
            self.theme.get("WARNING"),
        ));

        Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .style(self.theme.get("BORDER")),
            )
            .render(area, buf);
    }
}
