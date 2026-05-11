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
        let height = inner.height as f32;
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
            // Bar height in fractional rows. Lower bar_eighths = "1/8 of a row";
            // we paint full `█` blocks first, then a partial block on top using
            // ▁▂▃▄▅▆▇█ so a column can resolve up to 8× the row count.
            let frac = (col_max / scale_max).clamp(0.0, 1.0) * height;
            let bar_eighths = (frac * 8.0).round() as u32;
            if bar_eighths == 0 {
                continue;
            }
            let full_rows = (bar_eighths / 8) as u16;
            let partial = (bar_eighths % 8) as u8;
            let x = inner.x + col as u16;
            if x >= inner.x + inner.width {
                continue;
            }
            for row in 0..full_rows.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                buf[(x, y)].set_char('█').set_style(style);
            }
            if partial > 0 && full_rows < inner.height {
                let y = inner.y + inner.height.saturating_sub(1) - full_rows;
                buf[(x, y)]
                    .set_char(LOWER_EIGHTHS[partial as usize])
                    .set_style(style);
            }
        }
    }
}

/// Bottom-anchored partial blocks: index = number of eighths filled (0–8).
/// Index 0 (' ') is unused — we early-return when there's no partial.
const LOWER_EIGHTHS: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}',
    '\u{2587}', '\u{2588}',
];

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(bins: &[SignalBin], width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::dark();
        let region = igv_core::region::Region::new("chr1", 1, 100).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(
                    SignalWidget {
                        display_name: "x",
                        bins,
                        region: &region,
                        theme: &theme,
                        shared_max: None,
                    },
                    f.area(),
                );
            })
            .unwrap();
        // Snapshot terminal buffer as Vec<String>, one row per line.
        let buf = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn full_height_bar_is_full_block() {
        // value 1.0 against scale_max 1.0 with 4 inner rows → 4 full `█`s.
        let bins = vec![SignalBin { start: 1, end: 100, value: 1.0 }];
        // height=6 → 1 top border + 4 inner + 1 bottom border.
        let rows = render(&bins, 4, 6);
        // Inner rows are y=1..=4. The single column should be all `█`.
        #[allow(clippy::needless_range_loop)]
        for y in 1..=4 {
            assert!(rows[y].contains('\u{2588}'), "row {y}: {:?}", rows[y]);
        }
    }

    #[test]
    fn fractional_bar_uses_partial_block() {
        // Two bins: one drives the scale, the other should land below the
        // first row threshold so it renders as a partial block.
        let bins = vec![
            SignalBin { start: 1, end: 50, value: 1.0 },     // drives scale
            SignalBin { start: 51, end: 100, value: 0.05 },  // tiny bar
        ];
        let rows = render(&bins, 8, 10); // 8 inner rows
        // 0.05 * 8 = 0.4 row → round(0.4*8)=3 eighths → `▃` at the bottom row.
        // The right half of the canvas is the small bin; bottom inner row = y=8.
        assert!(
            rows[8].chars().any(|c| matches!(c, '\u{2581}'..='\u{2587}')),
            "expected a partial block in bottom row, got: {:?}",
            rows[8]
        );
    }
}
