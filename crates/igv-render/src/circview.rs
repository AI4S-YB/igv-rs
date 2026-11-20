//! Whole-genome Circular SV / Interaction Chord + CNV Ring Renderer (Circos-style SVG/PNG).

use std::collections::HashMap;
use std::f64::consts::PI;

use igv_core::source::link::LinkRecord;
use igv_core::source::seg::SegInterval;
use igv_core::source::RefMeta;

use crate::error::RenderError;
use crate::png::svg_to_png;
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::svg::seg::cnv_color;
use crate::theme::{GraphicalTheme, Rgb};

#[derive(Debug, Clone)]
pub struct CircularOptions {
    pub size_px: u32,
    pub min_sv_len: u64,
    pub title: Option<String>,
    pub theme: GraphicalTheme,
}

impl Default for CircularOptions {
    fn default() -> Self {
        Self {
            size_px: 900,
            min_sv_len: 1_000,
            title: None,
            theme: GraphicalTheme::igv_light(),
        }
    }
}

pub fn render_circular_svg(
    refs: &[RefMeta],
    links: &[LinkRecord],
    segs: &[SegInterval],
    opts: &CircularOptions,
) -> String {
    let size = opts.size_px.max(300);
    let theme = &opts.theme;
    let mut doc = SvgDoc::new(size, size, theme.bg, theme.font_family);

    let title = opts
        .title
        .as_deref()
        .unwrap_or("Whole-Genome Structural View");
    doc.text(
        (size as f64) / 2.0,
        24.0,
        title,
        theme.fg,
        theme.font_px_label,
        TextAnchor::Middle,
    );

    if refs.is_empty() {
        return doc.finish();
    }

    let cx = size as f64 / 2.0;
    let cy = size as f64 / 2.0 + 8.0;
    let r_outer = (size as f64 * 0.40).max(80.0);
    let r_cnv_outer = r_outer - 14.0;
    let r_cnv_inner = r_cnv_outer - 16.0;
    let r_chord = if segs.is_empty() {
        r_outer - 12.0
    } else {
        r_cnv_inner - 6.0
    };

    let total_bp: u64 = refs.iter().map(|r| r.length.max(1)).sum();
    let gap_rad = if refs.len() > 1 { 0.02 } else { 0.0 };
    let total_gap = gap_rad * refs.len() as f64;
    let avail_rad = (2.0 * PI - total_gap).max(0.5);

    struct ArcSpan {
        a0: f64,
        span: f64,
        length: u64,
    }

    let mut spans: HashMap<String, ArcSpan> = HashMap::new();
    let mut cursor = -PI / 2.0;
    let palette = [
        Rgb(0x3b, 0x82, 0xf6),
        Rgb(0x10, 0xb9, 0x81),
        Rgb(0xf5, 0x9e, 0x0b),
        Rgb(0x8b, 0x5c, 0xf6),
        Rgb(0xec, 0x48, 0x99),
        Rgb(0x06, 0xb6, 0xd4),
    ];

    for (idx, rf) in refs.iter().enumerate() {
        let len = rf.length.max(1);
        let span = avail_rad * (len as f64 / total_bp.max(1) as f64);
        let a0 = cursor;
        let a1 = cursor + span;
        let col = palette[idx % palette.len()];

        // Outer chromosome arc
        draw_arc_band(&mut doc, cx, cy, r_outer - 8.0, r_outer, a0, a1, col);

        // Label
        let mid_a = (a0 + a1) / 2.0;
        let lx = cx + (r_outer + 16.0) * mid_a.cos();
        let ly = cy + (r_outer + 16.0) * mid_a.sin() + 4.0;
        doc.text(lx, ly, &rf.name, theme.fg, 10, TextAnchor::Middle);

        spans.insert(rf.name.clone(), ArcSpan { a0, span, length: len });
        cursor = a1 + gap_rad;
    }

    let bp_to_angle = |chrom: &str, bp: u64| -> Option<f64> {
        let s = spans.get(chrom).or_else(|| {
            let alt = if let Some(rest) = chrom.strip_prefix("chr") {
                rest.to_string()
            } else {
                format!("chr{chrom}")
            };
            spans.get(&alt)
        })?;
        let frac = (bp.saturating_sub(1) as f64 / s.length.max(1) as f64).clamp(0.0, 1.0);
        Some(s.a0 + frac * s.span)
    };

    // Inner CNV ring segments
    for seg in segs {
        let Some(a0) = bp_to_angle(&seg.chrom, seg.start) else {
            continue;
        };
        let Some(a1) = bp_to_angle(&seg.chrom, seg.end) else {
            continue;
        };
        if a1 > a0 {
            draw_arc_band(&mut doc, cx, cy, r_cnv_inner, r_cnv_outer, a0, a1, cnv_color(seg.value));
        }
    }

    // Center-curving SV / chromatin interaction chords
    for lk in links {
        if lk.chrom_a == lk.chrom_b && lk.start_b.abs_diff(lk.start_a) < opts.min_sv_len {
            continue;
        }
        let Some(aa) = bp_to_angle(&lk.chrom_a, (lk.start_a + lk.end_a) / 2) else {
            continue;
        };
        let Some(ab) = bp_to_angle(&lk.chrom_b, (lk.start_b + lk.end_b) / 2) else {
            continue;
        };
        let x1 = cx + r_chord * aa.cos();
        let y1 = cy + r_chord * aa.sin();
        let x2 = cx + r_chord * ab.cos();
        let y2 = cy + r_chord * ab.sin();
        let d = format!("M {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2}", x1, y1, cx, cy, x2, y2);
        let stroke = if lk.chrom_a == lk.chrom_b {
            Rgb(0x25, 0x63, 0xeb)
        } else {
            Rgb(0xdc, 0x26, 0x26)
        };
        let w = lk
            .score
            .map(|s| (1.0 + s.max(1.0).log10()).min(3.5))
            .unwrap_or(1.2);
        doc.path(&d, stroke, w, None);
    }

    doc.finish()
}

pub fn render_circular_png(
    refs: &[RefMeta],
    links: &[LinkRecord],
    segs: &[SegInterval],
    opts: &CircularOptions,
) -> Result<Vec<u8>, RenderError> {
    let svg = render_circular_svg(refs, links, segs, opts);
    svg_to_png(&svg)
}

#[allow(clippy::too_many_arguments)]
fn draw_arc_band(
    doc: &mut SvgDoc,
    cx: f64,
    cy: f64,
    r_in: f64,
    r_out: f64,
    a0: f64,
    a1: f64,
    fill: Rgb,
) {
    let steps = (((a1 - a0).abs() * r_out) / 8.0).ceil().clamp(4.0, 64.0) as usize;
    let mut pts = Vec::with_capacity((steps + 1) * 2);
    for i in 0..=steps {
        let a = a0 + (a1 - a0) * (i as f64 / steps as f64);
        pts.push((cx + r_out * a.cos(), cy + r_out * a.sin()));
    }
    for i in (0..=steps).rev() {
        let a = a0 + (a1 - a0) * (i as f64 / steps as f64);
        pts.push((cx + r_in * a.cos(), cy + r_in * a.sin()));
    }
    doc.polygon(&pts, fill);
}
