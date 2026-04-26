use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::coverage;
use igv_core::region::genomic_to_screen;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct CoverageWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for CoverageWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title("coverage");
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || self.state.bams.is_empty() {
            return;
        }

        let region = &self.state.region;
        // Sum coverage across all BAM tracks for the summary band.
        let mut summed = vec![0u32; region.width() as usize];
        for rows in &self.state.bam_rows {
            let cov = coverage::compute(rows, region.start, region.end);
            for (i, d) in cov.depths.iter().enumerate() {
                summed[i] = summed[i].saturating_add(*d);
            }
        }
        let max = *summed.iter().max().unwrap_or(&0).max(&1) as f32;

        let style = self.theme.get("COVERAGE");
        let height = inner.height as usize;
        for (i, &d) in summed.iter().enumerate() {
            let g = (region.start - 1) + i as u64;
            let col = match genomic_to_screen(g, region.start - 1, region.width(), inner.width as u32) {
                Some(c) => c,
                None => continue,
            };
            let bar_h = ((d as f32 / max) * height as f32).ceil() as u16;
            for row in 0..bar_h.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                buf.get_mut(inner.x + col as u16, y)
                    .set_char('█')
                    .set_style(style);
            }
        }
    }
}
