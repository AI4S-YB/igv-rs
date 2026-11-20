//! GWAS Manhattan scatter track (`-log10(p)` vs genomic position).

use igv_core::render_inputs::GwasTrackSnapshot;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::{GraphicalTheme, Rgb};

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &GwasTrackSnapshot,
    theme: &GraphicalTheme,
) {
    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + area.h / 2) as f64,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let y_bot = (area.y + area.h - 6) as f64;
    let y_top = (area.y + 8) as f64;
    let usable_h = (y_bot - y_top).max(10.0);

    let max_y = track
        .points
        .iter()
        .map(|p| p.neg_log10_p)
        .fold(8.5_f32, f32::max) as f64;

    let map_y = |val: f64| -> f64 { y_bot - (val / max_y).clamp(0.0, 1.0) * usable_h };

    // Suggestive (1e-5) and Genome-wide (5e-8 -> 7.301) lines
    let y_sug = map_y(5.0);
    let y_gw = map_y(7.301);
    doc.line(
        plot.plot_x0 as f64,
        y_sug,
        plot.plot_x1 as f64,
        y_sug,
        Rgb(0x93, 0xc5, 0xfd),
        0.8,
    );
    doc.line(
        plot.plot_x0 as f64,
        y_gw,
        plot.plot_x1 as f64,
        y_gw,
        Rgb(0xfc, 0xa5, 0xa5),
        1.0,
    );
    doc.line(
        plot.plot_x0 as f64,
        y_bot,
        plot.plot_x1 as f64,
        y_bot,
        theme.muted,
        0.8,
    );

    let mut top_point: Option<(f64, f64, f32, &str)> = None;
    for pt in &track.points {
        let px = plot.bp_to_px(pt.pos);
        let py = map_y(pt.neg_log10_p as f64);
        let sig = pt.neg_log10_p >= 7.301;
        let fill = if sig {
            Rgb(0xdc, 0x26, 0x26)
        } else if pt.neg_log10_p >= 5.0 {
            Rgb(0xd9, 0x77, 0x06)
        } else {
            Rgb(0x3b, 0x82, 0xf6)
        };
        doc.circle(px, py, if sig { 3.0 } else { 2.0 }, fill);
        if top_point.map_or(true, |t| pt.neg_log10_p > t.2) {
            top_point = Some((px, py, pt.neg_log10_p, &pt.snp_id));
        }
    }

    if let Some((px, py, val, label)) = top_point {
        if val >= 5.0 {
            doc.text(
                px,
                (py - 5.0).max(y_top + 6.0),
                &format!("{label} ({val:.1})"),
                Rgb(0x99, 0x1b, 0x1b),
                9,
                TextAnchor::Middle,
            );
        }
    }
}
