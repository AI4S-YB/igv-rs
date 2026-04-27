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
        let region = &self.state.region;
        let bams_present = !self.state.bams.is_empty();

        // Compute summed depth across BAM tracks up-front so the title can
        // include the observed max ("[0-N]"), matching IGV's y-axis label.
        let mut summed: Vec<u32> = Vec::new();
        let mut max_depth: u32 = 0;
        if bams_present {
            summed = vec![0u32; region.width() as usize];
            for rows in &self.state.bam_rows {
                let cov = coverage::compute(rows, region.start, region.end);
                for (i, d) in cov.depths.iter().enumerate() {
                    summed[i] = summed[i].saturating_add(*d);
                }
            }
            max_depth = summed.iter().copied().max().unwrap_or(0);
        }

        let title = if bams_present {
            format!("coverage [0-{}]", max_depth)
        } else {
            "coverage".to_string()
        };
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || !bams_present {
            return;
        }

        let scale = max_depth.max(1) as f32;
        let style = self.theme.get("COVERAGE");
        let height = inner.height as usize;
        for (i, &d) in summed.iter().enumerate() {
            let g = (region.start - 1) + i as u64;
            let col = match genomic_to_screen(g, region.start - 1, region.width(), inner.width as u32) {
                Some(c) => c,
                None => continue,
            };
            let bar_h = ((d as f32 / scale) * height as f32).ceil() as u16;
            for row in 0..bar_h.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                buf.get_mut(inner.x + col as u16, y)
                    .set_char('█')
                    .set_style(style);
            }
        }
    }
}
