//! GWAS Manhattan `-log10(p)` Braille subpixel scatter plot + 5e-8 threshold.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::{genomic_to_screen, Region};
use igv_core::source::gwas::GwasPoint;

use crate::ui::theme::Theme;

const BRAILLE_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

pub struct GwasWidget<'a> {
    pub display_name: &'a str,
    pub points: &'a [GwasPoint],
    pub region: &'a Region,
    pub theme: &'a Theme,
}

impl Widget for GwasWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let max_y = self
            .points
            .iter()
            .map(|p| p.neg_log10_p)
            .fold(8.5_f32, f32::max);
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(format!(
                "gwas[{}] ({} SNPs, max -log10p={:.1})",
                self.display_name,
                self.points.len(),
                max_y
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 {
            return;
        }

        let w = inner.width as usize;
        let h = inner.height as usize;
        let dot_h = (h * 4).max(1);
        let thresh_dot = ((1.0 - (7.301 / max_y).clamp(0.0, 1.0)) * (dot_h - 1) as f32) as usize;
        let thresh_row = (thresh_dot / 4).min(h - 1);
        for cx in 0..w {
            buf[(inner.x + cx as u16, inner.y + thresh_row as u16)]
                .set_char('┈')
                .set_style(Style::default().fg(Color::Red));
        }

        let mut masks = vec![vec![0u8; w]; h];
        let mut sig_cell = vec![vec![false; w]; h];
        let view_start = self.region.start - 1;
        let vw = self.region.width();

        let mut lead: Option<(usize, usize, &GwasPoint)> = None;
        for pt in self.points {
            let dot_x = match genomic_to_screen(pt.pos.saturating_sub(1), view_start, vw, (w * 2) as u32) {
                Some(dx) => dx as usize,
                None => continue,
            };
            let frac = (pt.neg_log10_p / max_y).clamp(0.0, 1.0);
            let dot_y = ((1.0 - frac) * (dot_h - 1) as f32).round() as usize;
            let cx = (dot_x / 2).min(w - 1);
            let cy = (dot_y / 4).min(h - 1);
            let sub_x = dot_x % 2;
            let sub_y = dot_y % 4;
            masks[cy][cx] |= BRAILLE_BITS[sub_x][sub_y];
            if pt.neg_log10_p >= 7.301 {
                sig_cell[cy][cx] = true;
            }
            if lead.map_or(true, |l| pt.neg_log10_p > l.2.neg_log10_p) {
                lead = Some((cx, cy, pt));
            }
        }

        for cy in 0..h {
            for cx in 0..w {
                let m = masks[cy][cx];
                if m == 0 {
                    continue;
                }
                let ch = char::from_u32(0x2800 + m as u32).unwrap_or('⣿');
                let color = if sig_cell[cy][cx] {
                    Color::Red
                } else {
                    Color::Cyan
                };
                buf[(inner.x + cx as u16, inner.y + cy as u16)]
                    .set_char(ch)
                    .set_style(Style::default().fg(color));
            }
        }

        if let Some((cx, cy, pt)) = lead {
            if pt.neg_log10_p >= 5.0 {
                let label = format!(" {}({:.1})", pt.snp_id, pt.neg_log10_p);
                let start_x = (cx + 1).min(w.saturating_sub(label.len()));
                for (i, ch) in label.chars().take(w.saturating_sub(start_x)).enumerate() {
                    buf[(inner.x + (start_x + i) as u16, inner.y + cy as u16)]
                        .set_char(ch)
                        .set_style(Style::default().fg(Color::Yellow));
                }
            }
        }
    }
}
