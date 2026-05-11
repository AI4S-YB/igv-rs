use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct VariantsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for VariantsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title("variants");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.area() == 0 || self.state.variants.is_empty() {
            return;
        }
        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if matches!(mode, RenderMode::OverviewOnly) {
            return;
        }

        let view_start_0 = region.start - 1;
        let style = self.theme.get("VARIANT");

        for v in &self.state.variants {
            let pos_0 = v.pos.saturating_sub(1);
            let col = match genomic_to_screen(pos_0, view_start_0, region.width(), inner.width as u32) {
                Some(c) => c,
                None => continue,
            };
            // Choose glyph: ALT base if room, else `▼`.
            let glyph: char = match mode {
                RenderMode::PerBase => v
                    .alternate_alleles
                    .first()
                    .and_then(|a| a.chars().next())
                    .unwrap_or('▼'),
                _ => '▼',
            };
            buf[(inner.x + col as u16, inner.y)]
                .set_char(glyph)
                .set_style(style);
        }
    }
}
