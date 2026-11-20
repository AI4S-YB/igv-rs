//! Multi-sample SEG / CNV diverging copy-ratio heatmap (`▀` half-blocks packing
//! 2 samples per terminal row: fg = top sample, bg = bottom sample).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::{genomic_to_screen, Region};
use igv_core::source::seg::SegInterval;

use crate::ui::theme::Theme;

pub struct SegWidget<'a> {
    pub display_name: &'a str,
    pub samples: &'a [String],
    pub segments: &'a [SegInterval],
    pub region: &'a Region,
    pub theme: &'a Theme,
}

impl Widget for SegWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(format!(
                "seg[{}] ({} samples)",
                self.display_name,
                self.samples.len()
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || self.samples.is_empty() {
            return;
        }

        let view_start = self.region.start - 1;
        let vw = self.region.width();
        let w = inner.width as usize;

        // Pre-fill grid [sample][col] -> Option<Color>
        let mut grid: Vec<Vec<Option<Color>>> = vec![vec![None; w]; self.samples.len()];
        for seg in self.segments {
            let Some(si) = self.samples.iter().position(|s| s == &seg.sample) else {
                continue;
            };
            let s0 = seg.start.max(self.region.start).saturating_sub(1);
            let e0 = seg.end.min(self.region.end).saturating_sub(1);
            if e0 < s0 {
                continue;
            }
            let c0 = genomic_to_screen(s0, view_start, vw, inner.width as u32).unwrap_or(0) as usize;
            let c1 = genomic_to_screen(e0, view_start, vw, inner.width as u32)
                .unwrap_or(inner.width.saturating_sub(1) as u32) as usize;
            let color = diverging_cnv_color(seg.value);
            let hi = c1.min(w.saturating_sub(1));
            if c0 <= hi {
                for slot in &mut grid[si][c0..=hi] {
                    *slot = Some(color);
                }
            }
        }

        let pairs = self.samples.len().div_ceil(2);
        for pair_row in 0..pairs.min(inner.height as usize) {
            let top_i = pair_row * 2;
            let bot_i = top_i + 1;
            let y = inner.y + pair_row as u16;
            for cx in 0..w {
                let top_c = grid[top_i][cx].unwrap_or(Color::Rgb(240, 240, 240));
                let bot_c = grid
                    .get(bot_i)
                    .and_then(|row| row[cx])
                    .unwrap_or(Color::Rgb(240, 240, 240));
                buf[(inner.x + cx as u16, y)]
                    .set_char('▀')
                    .set_style(Style::default().fg(top_c).bg(bot_c));
            }
        }
    }
}

pub fn diverging_cnv_color(log2_ratio: f32) -> Color {
    let t = (log2_ratio / 1.5).clamp(-1.0, 1.0);
    if t < 0.0 {
        let a = -t;
        let r = (240.0 + (33.0 - 240.0) * a).round() as u8;
        let g = (240.0 + (102.0 - 240.0) * a).round() as u8;
        let b = (240.0 + (172.0 - 240.0) * a).round() as u8;
        Color::Rgb(r, g, b)
    } else {
        let r = (240.0 + (178.0 - 240.0) * t).round() as u8;
        let g = (240.0 + (24.0 - 240.0) * t).round() as u8;
        let b = (240.0 + (43.0 - 240.0) * t).round() as u8;
        Color::Rgb(r, g, b)
    }
}
