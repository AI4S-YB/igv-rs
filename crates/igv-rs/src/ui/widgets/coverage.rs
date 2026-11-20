use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::coverage::{self, BaseCounts};
use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;

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
        let mode = self.state.thresholds.classify(region.width());
        let coverage_active =
            bams_present && matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads);

        let mut summed: Vec<u32> = Vec::new();
        let mut counts: Vec<BaseCounts> = Vec::new();
        let mut max_depth: u32 = 0;
        if coverage_active {
            let n = region.width() as usize;
            summed = vec![0u32; n];
            counts = vec![BaseCounts::default(); n];
            for rows in &self.state.bam_rows {
                let cov = coverage::compute(rows, region.start, region.end);
                for (i, d) in cov.depths.iter().enumerate() {
                    summed[i] = summed[i].saturating_add(*d);
                    if let (Some(dst), Some(src)) = (counts.get_mut(i), cov.counts.get(i)) {
                        dst.a = dst.a.saturating_add(src.a);
                        dst.c = dst.c.saturating_add(src.c);
                        dst.g = dst.g.saturating_add(src.g);
                        dst.t = dst.t.saturating_add(src.t);
                        dst.n = dst.n.saturating_add(src.n);
                    }
                }
            }
            max_depth = summed.iter().copied().max().unwrap_or(0);
        }

        let title = if coverage_active {
            format!("coverage [0-{}]", max_depth)
        } else if bams_present {
            "coverage (zoomed out)".to_string()
        } else {
            "coverage".to_string()
        };
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || !coverage_active {
            return;
        }

        let scale = max_depth.max(1) as f32;
        let default_style = self.theme.get("COVERAGE");
        let height = inner.height as usize;

        for (i, &d) in summed.iter().enumerate() {
            let g = (region.start - 1) + i as u64;
            let Some(col) = genomic_to_screen(g, region.start - 1, region.width(), inner.width as u32)
            else {
                continue;
            };
            let bar_h = ((d as f32 / scale) * height as f32).ceil() as u16;
            if bar_h == 0 {
                continue;
            }

            // Detect SNV allele fraction >= 20% against reference
            let mut snv_dominant: Option<Style> = None;
            if let (Some(&ref_b), Some(bc)) = (self.state.reference_seq.get(i), counts.get(i)) {
                let tot = bc.total();
                if tot >= 2 {
                    let ref_cnt = match ref_b.to_ascii_uppercase() {
                        b'A' => bc.a,
                        b'C' => bc.c,
                        b'G' => bc.g,
                        b'T' => bc.t,
                        _ => tot,
                    };
                    if (tot.saturating_sub(ref_cnt) as f32 / tot as f32) >= 0.20 {
                        let (dom_key, _) = [
                            ("A", if ref_b.eq_ignore_ascii_case(&b'A') { 0 } else { bc.a }),
                            ("C", if ref_b.eq_ignore_ascii_case(&b'C') { 0 } else { bc.c }),
                            ("G", if ref_b.eq_ignore_ascii_case(&b'G') { 0 } else { bc.g }),
                            ("T", if ref_b.eq_ignore_ascii_case(&b'T') { 0 } else { bc.t }),
                        ]
                        .into_iter()
                        .max_by_key(|&(_, c)| c)
                        .unwrap_or(("A", 0));
                        snv_dominant = Some(self.theme.get(dom_key));
                    }
                }
            }

            let cell_style = snv_dominant.unwrap_or(default_style);
            for row in 0..bar_h.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                buf[(inner.x + col as u16, y)]
                    .set_char('█')
                    .set_style(cell_style);
            }
        }
    }
}
