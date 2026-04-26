use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct OverviewWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for OverviewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .style(self.theme.get("BORDER"))
            .title(format!("chromosome {}", self.state.region.chrom));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 {
            return;
        }

        let chrom_len = self
            .state
            .references
            .iter()
            .find(|r| r.name == self.state.region.chrom)
            .map(|r| r.length)
            .unwrap_or(self.state.region.end);
        if chrom_len == 0 {
            return;
        }

        let bar_y = inner.y;
        let style = self.theme.get("OVERVIEW");
        for x in 0..inner.width {
            buf.get_mut(inner.x + x, bar_y)
                .set_char('─')
                .set_style(style);
        }

        let start_col = ((self.state.region.start as u128 * inner.width as u128 / chrom_len as u128)
            .min(inner.width as u128 - 1)) as u16;
        let end_col = ((self.state.region.end as u128 * inner.width as u128 / chrom_len as u128)
            .min(inner.width as u128 - 1)) as u16;
        for x in start_col..=end_col {
            buf.get_mut(inner.x + x, bar_y)
                .set_char('█')
                .set_style(style);
        }
    }
}
