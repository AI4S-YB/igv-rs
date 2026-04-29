//! SVG painter for link tracks (BEDPE). Mirrors LinkWidget's mode
//! selection but emits Bézier arcs in arc mode and per-pixel strips
//! in heatmap mode, with continuous color from `theme.link_gradient`.

use igv_core::render_inputs::LinkTrackSnapshot;
use igv_core::source::link::{LinkScope, VisibleLink};

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &LinkTrackSnapshot,
    theme: &GraphicalTheme,
) {
    let label_y = (area.y + area.h / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    if track.visible.is_empty() {
        return;
    }

    // Mode selection: arc if number of arc-eligible records fits the
    // pixel-row budget. ~8 px per arc lane.
    let arc_lane_h: u32 = 8;
    let arc_count_estimate = track
        .visible
        .iter()
        .filter(|v| matches!(v.scope, LinkScope::BothIn | LinkScope::PartialCis { .. }))
        .count() as u32;
    let arc_budget = area.h / arc_lane_h.max(1);
    if arc_count_estimate <= arc_budget {
        paint_arc(doc, area, plot, &track.visible, theme);
    } else {
        paint_heatmap(doc, area, plot, &track.visible, theme);
    }
}

fn paint_arc(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    visible: &[VisibleLink],
    theme: &GraphicalTheme,
) {
    let region_start = plot.region_start;
    let region_end = plot.region_start + plot.region_width_bp;

    let anchor_y = area.y + area.h.saturating_sub(8);
    let arc_top = (area.y + 4) as f64;
    let arc_bot = anchor_y as f64;

    // Per-window score normalization.
    let scored: Vec<f64> = visible.iter().filter_map(|v| v.record.score).collect();
    let (s_min, s_max) = if scored.is_empty() {
        (0.0, 1.0)
    } else {
        (
            scored.iter().cloned().fold(f64::INFINITY, f64::min),
            scored.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let normalize = |s: Option<f64>| -> f64 {
        match s {
            Some(v) if (s_max - s_min).abs() > f64::EPSILON => {
                (v - s_min) / (s_max - s_min)
            }
            _ => 0.5,
        }
    };

    // BothIn arcs.
    for v in visible {
        if !matches!(v.scope, LinkScope::BothIn) {
            continue;
        }
        let mid_a = midpoint_u64(v.record.start_a, v.record.end_a);
        let mid_b = midpoint_u64(v.record.start_b, v.record.end_b);
        let xa = plot.bp_to_px(mid_a);
        let xb = plot.bp_to_px(mid_b);
        let lo_x = xa.min(xb);
        let hi_x = xa.max(xb);
        let span = hi_x - lo_x;
        let lift = (span * 0.5).min(arc_bot - arc_top);
        let cy = (arc_bot - lift).max(arc_top);
        let color = theme.link_color_at(normalize(v.record.score));
        let d = format!(
            "M {:.2} {:.2} C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
            lo_x, arc_bot,
            lo_x, cy,
            hi_x, cy,
            hi_x, arc_bot,
        );
        doc.path(&d, color, 1.5, None);

        anchor_rect(doc, plot, v.record.start_a, v.record.end_a, anchor_y, color);
        anchor_rect(doc, plot, v.record.start_b, v.record.end_b, anchor_y, color);
    }

    // PartialCis half-arcs with arrowheads.
    for v in visible {
        if let LinkScope::PartialCis { off_anchor_mid: _, off_to_left } = v.scope {
            let in_anchor = if v.record.end_a >= region_start && v.record.start_a <= region_end {
                (v.record.start_a, v.record.end_a)
            } else {
                (v.record.start_b, v.record.end_b)
            };
            let mid_in = midpoint_u64(in_anchor.0, in_anchor.1);
            let x_in = plot.bp_to_px(mid_in);
            let x_edge = if off_to_left {
                plot.plot_x0 as f64
            } else {
                plot.plot_x1 as f64
            };
            let lift = ((x_edge - x_in).abs() * 0.4).min(arc_bot - arc_top);
            let cy = (arc_bot - lift).max(arc_top);
            let color = theme.link_color_at(normalize(v.record.score));
            let d = format!(
                "M {:.2} {:.2} C {:.2} {:.2}, {:.2} {:.2}, {:.2} {:.2}",
                x_in, arc_bot,
                x_in, cy,
                x_edge, cy,
                x_edge, arc_bot,
            );
            doc.path(&d, color, 1.5, None);
            anchor_rect(doc, plot, in_anchor.0, in_anchor.1, anchor_y, color);
            // Arrowhead triangle at the edge.
            let arrow_h = 4.0;
            let dir: f64 = if off_to_left { -1.0 } else { 1.0 };
            doc.polygon(
                &[
                    (x_edge, arc_bot - arrow_h / 2.0),
                    (x_edge, arc_bot + arrow_h / 2.0),
                    (x_edge + dir * arrow_h, arc_bot),
                ],
                color,
            );
        }
    }

    // Trans markers.
    for v in visible {
        if let LinkScope::Trans { off_chrom, off_anchor_mid } = &v.scope {
            // Determine the in-window anchor (one anchor's chrom matches plot's chrom — but
            // PlotMetrics doesn't carry the chrom name. Instead, use the anchor whose range
            // overlaps [region_start, region_end].)
            let (in_s, in_e) = if v.record.end_a >= region_start && v.record.start_a <= region_end {
                (v.record.start_a, v.record.end_a)
            } else {
                (v.record.start_b, v.record.end_b)
            };
            let mid_in = midpoint_u64(in_s.max(region_start), in_e.min(region_end));
            let x = plot.bp_to_px(mid_in);
            let color = theme.link_color_at(0.5);
            anchor_rect(doc, plot, in_s, in_e, anchor_y, color);
            let label = format!("⤴ {}:{}", off_chrom, *off_anchor_mid / 1_000_000);
            doc.text(
                x,
                (anchor_y as f64) - 4.0,
                &label,
                color,
                theme.font_px_small,
                TextAnchor::Middle,
            );
        }
    }
}

fn anchor_rect(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    s: u64,
    e: u64,
    y: u32,
    fill: crate::theme::Rgb,
) {
    let x0 = plot.bp_to_px(s);
    let x1 = plot.bp_to_px(e + 1);
    let w = (x1 - x0).max(2.0);
    doc.rect(x0, y as f64, w, 6.0, fill);
}

fn midpoint_u64(s: u64, e: u64) -> u64 {
    s + (e - s) / 2
}

fn paint_heatmap(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    visible: &[VisibleLink],
    theme: &GraphicalTheme,
) {
    let plot_w = plot.plot_width.max(1) as usize;
    let mut col_score: Vec<f64> = vec![0.0; plot_w];
    let scored: Vec<f64> = visible.iter().filter_map(|v| v.record.score).collect();
    let q25 = if scored.len() >= 4 {
        let mut s = scored.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        s[s.len() / 4]
    } else {
        0.0
    };
    for v in visible {
        let score = v.record.score.unwrap_or(q25);
        for (s, e) in [(v.record.start_a, v.record.end_a), (v.record.start_b, v.record.end_b)] {
            let x0 = plot.bp_to_px(s) as usize;
            let x1 = plot.bp_to_px(e + 1) as usize;
            let lo = x0.saturating_sub(plot.plot_x0 as usize);
            let hi = x1.saturating_sub(plot.plot_x0 as usize).min(plot_w);
            for val in &mut col_score[lo..hi] {
                if score > *val {
                    *val = score;
                }
            }
        }
    }
    let max = col_score.iter().cloned().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        return;
    }
    let strip_h = (area.h.saturating_sub(8)) as f64;
    let strip_y = (area.y + 4) as f64;
    for (c, &s) in col_score.iter().enumerate() {
        if s <= 0.0 {
            continue;
        }
        let t = (s / max).clamp(0.0, 1.0);
        let color = theme.link_color_at(t);
        let x = plot.plot_x0 as f64 + c as f64;
        doc.rect(x, strip_y, 1.0, strip_h, color);
    }
}
