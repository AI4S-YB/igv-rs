//! Coverage track: aggregate depth across all loaded BAMs in the current
//! region into one bar chart. Sums per-bp coverage from each BAM's
//! alignment rows.

use igv_core::render_inputs::RenderInputs;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    let label_y = (area.y + area.h / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        "coverage",
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let cols = plot.plot_width.max(1);
    let mut depth = vec![0u32; cols as usize];
    let region = &inputs.region;
    let region_width = region.width().max(1);
    for bam in &inputs.bams {
        for row in &bam.rows {
            let lo = row.ref_start.max(region.start);
            let hi = row.ref_end.min(region.end);
            if hi < lo {
                continue;
            }
            for bp in lo..=hi {
                let off = (bp - region.start) as f64;
                let frac = (off / region_width as f64).clamp(0.0, 0.999_999);
                let col = (frac * cols as f64) as usize;
                if col < depth.len() {
                    depth[col] += 1;
                }
            }
        }
    }

    let max_depth = depth.iter().copied().max().unwrap_or(0);
    if max_depth == 0 {
        return;
    }
    let baseline_y = (area.y + area.h - 2) as f64;
    let usable_h = area.h as f64 - 4.0;
    for (i, &d) in depth.iter().enumerate() {
        if d == 0 {
            continue;
        }
        let h = (d as f64 / max_depth as f64) * usable_h;
        let x = plot.plot_x0 as f64 + i as f64;
        doc.rect(x, baseline_y - h, 1.0, h, theme.coverage_bar);
    }

    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + theme.font_px_small) as f64,
        &format!("max {}", max_depth),
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );
}
