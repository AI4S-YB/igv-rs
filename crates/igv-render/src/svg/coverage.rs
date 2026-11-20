//! Coverage track: aggregate depth across all loaded BAMs in the current
//! region into one bar chart, with optional SNV allele-fraction stacked bars.

use igv_core::coverage::compute;
use igv_core::render_inputs::RenderInputs;
use igv_core::source::AlignmentRow;

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
    draw_with_snv(doc, area, plot, inputs, false, theme);
}

pub fn draw_with_snv(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    highlight_snv: bool,
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

    let cols = plot.plot_width.max(1) as usize;
    let region = &inputs.region;
    let region_width = region.width().max(1);
    let mut events = vec![0i32; cols + 1];
    for bam in &inputs.bams {
        for row in &bam.rows {
            let lo = row.ref_start.max(region.start);
            let hi = row.ref_end.min(region.end);
            if hi < lo {
                continue;
            }
            let lo_col = bp_to_col(lo, region.start, region_width, cols);
            let hi_col = bp_to_col(hi, region.start, region_width, cols);
            events[lo_col] += 1;
            events[hi_col + 1] -= 1;
        }
    }
    let mut depth = vec![0u32; cols];
    let mut running: i32 = 0;
    for (i, slot) in depth.iter_mut().enumerate() {
        running += events[i];
        *slot = running.max(0) as u32;
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

    if highlight_snv && !inputs.reference_seq.is_empty() {
        let all_rows: Vec<AlignmentRow> = inputs
            .bams
            .iter()
            .flat_map(|b| b.rows.iter().cloned())
            .collect();
        let cov = compute(&all_rows, region.start, region.end);
        let col_w = (plot.plot_width as f64 / region_width as f64).max(1.5);
        for (bp_idx, &ref_b) in inputs.reference_seq.iter().enumerate() {
            if let Some(fracs) = cov.mismatch_fractions(bp_idx, ref_b, 0.20) {
                let d = cov.depths.get(bp_idx).copied().unwrap_or(0);
                if d == 0 {
                    continue;
                }
                let total_bar_h = (d as f64 / max_depth.max(1) as f64) * usable_h;
                let x = plot.bp_to_px(region.start + bp_idx as u64);
                let mut cur_y = baseline_y;
                for (base, frac) in fracs {
                    if frac <= 0.0 {
                        continue;
                    }
                    let seg_h = total_bar_h * frac as f64;
                    doc.rect(x, cur_y - seg_h, col_w, seg_h, theme.mismatch_color(base));
                    cur_y -= seg_h;
                }
            }
        }
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

fn bp_to_col(bp: u64, region_start: u64, region_width: u64, cols: usize) -> usize {
    let off = bp.saturating_sub(region_start) as f64;
    let frac = (off / region_width as f64).clamp(0.0, 0.999_999);
    (frac * cols as f64) as usize
}
