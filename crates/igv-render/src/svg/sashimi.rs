//! Sashimi plot: exon coverage histogram + splice-junction quadratic Bézier arcs.

use igv_core::region::Region;
use igv_core::render_inputs::BamTrackSnapshot;
use igv_core::source::annotation::Strand;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::{GraphicalTheme, Rgb};

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &BamTrackSnapshot,
    region: &Region,
    theme: &GraphicalTheme,
) {
    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + area.h / 2) as f64,
        &format!("{} (sashimi)", track.display),
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let baseline_y = (area.y + area.h - 4) as f64;
    let max_arc_h = (area.h as f64 - 16.0).max(16.0);

    // Draw subtle exon coverage bars at bottom third
    let cols = plot.plot_width.max(1) as usize;
    let rw = region.width().max(1);
    let mut events = vec![0i32; cols + 1];
    for row in &track.rows {
        let lo = row.ref_start.max(region.start);
        let hi = row.ref_end.min(region.end);
        if hi < lo {
            continue;
        }
        let lc = (((lo - region.start) as f64 / rw as f64).clamp(0.0, 0.999_999) * cols as f64)
            as usize;
        let hc = (((hi - region.start) as f64 / rw as f64).clamp(0.0, 0.999_999) * cols as f64)
            as usize;
        events[lc] += 1;
        events[hc + 1] -= 1;
    }
    let mut running = 0i32;
    let mut depth = vec![0u32; cols];
    for (i, d) in depth.iter_mut().enumerate() {
        running += events[i];
        *d = running.max(0) as u32;
    }
    let max_d = depth.iter().copied().max().unwrap_or(0);
    if max_d > 0 {
        let cov_h = max_arc_h * 0.35;
        for (i, &d) in depth.iter().enumerate() {
            if d == 0 {
                continue;
            }
            let h = (d as f64 / max_d as f64) * cov_h;
            doc.rect(
                plot.plot_x0 as f64 + i as f64,
                baseline_y - h,
                1.0,
                h,
                Rgb(0xcb, 0xd5, 0xe1),
            );
        }
    }

    doc.line(
        plot.plot_x0 as f64,
        baseline_y,
        plot.plot_x1 as f64,
        baseline_y,
        theme.muted,
        0.8,
    );

    let max_count = track.junctions.iter().map(|j| j.count).max().unwrap_or(1).max(1);
    for j in &track.junctions {
        let x0 = plot.bp_to_px(j.start);
        let x1 = plot.bp_to_px(j.end + 1);
        let span = (x1 - x0).abs();
        if span < 2.0 {
            continue;
        }
        let mid_x = (x0 + x1) / 2.0;
        let frac = ((j.count as f64 + 1.0).log2() / (max_count as f64 + 1.0).log2()).clamp(0.2, 1.0);
        let apex_y = (baseline_y - frac * max_arc_h).max(area.y as f64 + 4.0);
        let ctrl_y = baseline_y - 2.0 * (baseline_y - apex_y);
        let d = format!(
            "M {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2}",
            x0, baseline_y, mid_x, ctrl_y, x1, baseline_y
        );
        let stroke = match j.strand {
            Strand::Forward => Rgb(0x1d, 0x4e, 0xd8),
            Strand::Reverse => Rgb(0xb9, 0x1c, 0x1c),
            Strand::Unknown => theme.link_color,
        };
        let stroke_w = 1.0 + 2.5 * frac;
        doc.path(&d, stroke, stroke_w, None);
        doc.text(
            mid_x,
            apex_y + 10.0,
            &j.count.to_string(),
            stroke,
            9,
            TextAnchor::Middle,
        );
    }
}
