use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::source::signal::SignalBin;

use crate::ui::theme::Theme;

pub struct SignalWidget<'a> {
    pub display_name: &'a str,
    pub bins: &'a [SignalBin],
    pub region: &'a igv_core::region::Region,
    pub theme: &'a Theme,
    /// `Some(g)` when shared-scale is on; widget uses `g` instead of its
    /// own max so different tracks become visually comparable.
    pub shared_max: Option<f32>,
}

impl Widget for SignalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let self_max = self
            .bins
            .iter()
            .map(|b| b.value)
            .fold(0.0_f32, f32::max);
        let scale_max = self.shared_max.unwrap_or(self_max);
        let suffix = if self.shared_max.is_some() { "*" } else { "" };
        let title = format!(
            "signal[{}] [0-{:.1}{}]",
            self.display_name, scale_max, suffix
        );
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || self.bins.is_empty() || scale_max <= 0.0 {
            return;
        }

        let style = self.theme.get("SIGNAL");
        let height = inner.height as usize;
        let region = self.region;

        // For each terminal column, take the max value among bins whose
        // [start, end] overlap the genomic range that maps to this column.
        let cols = inner.width as u32;
        for col in 0..cols {
            // Inverse map column → genomic range
            let col_start = region.start
                + (col as u64 * region.width()) / cols.max(1) as u64;
            let col_end = region.start
                + ((col + 1) as u64 * region.width()) / cols.max(1) as u64;
            let mut col_max = 0.0_f32;
            for b in self.bins {
                if b.end >= col_start && b.start < col_end && b.value > col_max {
                    col_max = b.value;
                }
            }
            if col_max <= 0.0 {
                continue;
            }
            let bar_h =
                ((col_max / scale_max) * height as f32).ceil() as u16;
            for row in 0..bar_h.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                let x = inner.x + col as u16;
                if x < inner.x + inner.width {
                    buf[(x, y)].set_char('█').set_style(style);
                }
            }
        }
    }
}
