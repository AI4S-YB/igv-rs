//! Multi-sample SEG / CNV diverging copy-ratio heatmap track.

use igv_core::render_inputs::SegTrackSnapshot;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::{GraphicalTheme, Rgb};

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &SegTrackSnapshot,
    theme: &GraphicalTheme,
) {
    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + 12) as f64,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    if track.samples.is_empty() {
        return;
    }

    let usable_top = area.y as f64 + 14.0;
    let usable_h = (area.h as f64 - 16.0).max(8.0);
    let n = track.samples.len();
    let row_h = (usable_h / n as f64).max(2.0);

    for (idx, sample) in track.samples.iter().enumerate() {
        let sy = usable_top + idx as f64 * row_h;
        if row_h >= 7.0 {
            doc.text(
                (plot.margin_left - 6) as f64,
                sy + row_h * 0.8,
                sample,
                theme.muted,
                8,
                TextAnchor::End,
            );
        }
        doc.rect(
            plot.plot_x0 as f64,
            sy,
            plot.plot_width as f64,
            (row_h - 0.5).max(1.0),
            Rgb(0xf8, 0xfa, 0xfc),
        );
    }

    for seg in &track.segments {
        let Some(s_idx) = track.samples.iter().position(|s| s == &seg.sample) else {
            continue;
        };
        let x0 = plot.bp_to_px(seg.start);
        let x1 = plot.bp_to_px(seg.end + 1);
        let w = (x1 - x0).max(1.0);
        let sy = usable_top + s_idx as f64 * row_h;
        doc.rect(x0, sy, w, (row_h - 0.5).max(1.0), cnv_color(seg.value));
    }
}

pub fn cnv_color(log2_ratio: f32) -> Rgb {
    let t = (log2_ratio / 1.5).clamp(-1.0, 1.0);
    if t < 0.0 {
        let a = -t;
        lerp_rgb(Rgb(245, 245, 245), Rgb(33, 102, 172), a)
    } else {
        lerp_rgb(Rgb(245, 245, 245), Rgb(178, 24, 43), t)
    }
}

fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let r = (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t).round() as u8;
    let g = (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t).round() as u8;
    let bl = (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t).round() as u8;
    Rgb(r, g, bl)
}
