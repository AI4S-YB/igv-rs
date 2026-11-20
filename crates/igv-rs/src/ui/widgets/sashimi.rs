//! RNA-seq Sashimi splice-junction arc + exon depth TUI widget.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::{genomic_to_screen, Region};
use igv_core::sashimi::SpliceJunction;
use igv_core::source::annotation::Strand;

use crate::ui::theme::Theme;

pub struct SashimiWidget<'a> {
    pub display_name: &'a str,
    pub junctions: &'a [SpliceJunction],
    pub region: &'a Region,
    pub theme: &'a Theme,
}

impl Widget for SashimiWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(format!(
                "sashimi[{}] ({} junctions)",
                self.display_name,
                self.junctions.len()
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || inner.height < 2 {
            return;
        }

        let base_y = inner.y + inner.height - 1;
        let view_start = self.region.start - 1;
        let vw = self.region.width();
        let max_count = self.junctions.iter().map(|j| j.count).max().unwrap_or(1).max(1);
        let max_arc_rows = inner.height.saturating_sub(1).max(1);

        for j in self.junctions {
            let x0 = genomic_to_screen(j.start.saturating_sub(1), view_start, vw, inner.width as u32);
            let x1 = genomic_to_screen(j.end.saturating_sub(1), view_start, vw, inner.width as u32);
            let (c0, c1) = match (x0, x1) {
                (Some(a), Some(b)) => (a.min(b) as u16, a.max(b) as u16),
                _ => continue,
            };
            let span = c1.saturating_sub(c0);
            if span < 2 {
                continue;
            }
            let frac = ((j.count as f64 + 1.0).log2() / (max_count as f64 + 1.0).log2()).clamp(0.25, 1.0);
            let lift = ((frac * max_arc_rows as f64).round() as u16).clamp(1, max_arc_rows);
            let top_y = base_y.saturating_sub(lift);
            let col = match j.strand {
                Strand::Forward => Color::Cyan,
                Strand::Reverse => Color::LightRed,
                Strand::Unknown => Color::Magenta,
            };
            let st = Style::default().fg(col);

            for y in top_y + 1..=base_y {
                buf[(inner.x + c0, y)].set_char('│').set_style(st);
                buf[(inner.x + c1, y)].set_char('│').set_style(st);
            }
            buf[(inner.x + c0, top_y)].set_char('╭').set_style(st);
            buf[(inner.x + c1, top_y)].set_char('╮').set_style(st);
            for cx in c0 + 1..c1 {
                buf[(inner.x + cx, top_y)].set_char('─').set_style(st);
            }

            let label = format!("{}", j.count);
            if span as usize >= label.len() + 2 {
                let lx = inner.x + c0 + (span - label.len() as u16) / 2;
                for (i, ch) in label.chars().enumerate() {
                    buf[(lx + i as u16, top_y)].set_char(ch).set_style(st);
                }
            }
        }
    }
}
