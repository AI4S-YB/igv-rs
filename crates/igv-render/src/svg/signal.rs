//! Signal track (bigWig). Bar chart with optional shared-scale max
//! supplied via SvgOptions.signal_shared_max.

use igv_core::render_inputs::SignalTrackSnapshot;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &SignalTrackSnapshot,
    shared_max: Option<f32>,
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

    if track.bins.is_empty() {
        return;
    }
    let local_max = track.bins.iter().map(|b| b.value).fold(0.0_f32, f32::max);
    let max = shared_max.unwrap_or(local_max);
    if max <= 0.0 {
        return;
    }
    let baseline_y = (area.y + area.h - 2) as f64;
    let usable_h = area.h as f64 - 4.0;
    for bin in &track.bins {
        if bin.value <= 0.0 {
            continue;
        }
        let x0 = plot.bp_to_px(bin.start);
        let x1 = plot.bp_to_px(bin.end + 1);
        let w = (x1 - x0).max(1.0);
        let h = (bin.value as f64 / max as f64) * usable_h;
        doc.rect(x0, baseline_y - h, w, h, theme.signal_bar);
    }

    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + theme.font_px_small) as f64,
        &format!("max {:.2}", max),
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );
}
