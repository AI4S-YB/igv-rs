use igv_core::render_inputs::{RenderInputs, VcfTrackSnapshot};
use igv_core::source::{GenotypeCall, VariantRecord};

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::{GraphicalTheme, Rgb};

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    let cy = (area.y + area.h / 2) as f64;
    for v in &inputs.variants {
        let x = plot.bp_to_px(v.pos);
        let color = if is_indel(v) { theme.variant_indel } else { theme.variant_snv };
        let r = 3.0;
        doc.polygon(&[(x - r, cy + r), (x + r, cy + r), (x, cy - r)], color);
    }
}

pub fn draw_track(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &VcfTrackSnapshot,
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

    let header_h = 20.0;
    let cy = area.y as f64 + header_h / 2.0;
    for v in &track.records {
        let x = plot.bp_to_px(v.pos);
        let color = if is_indel(v) { theme.variant_indel } else { theme.variant_snv };
        let r = 3.0;
        doc.polygon(&[(x - r, cy + r), (x + r, cy + r), (x, cy - r)], color);
    }

    let max_samples = track.sample_names.len().min(12);
    let row_h = 8.0;
    for s_idx in 0..max_samples {
        let sy = area.y as f64 + header_h + (s_idx as f64) * 10.0;
        doc.text(
            (plot.margin_left - 6) as f64,
            sy + 7.0,
            &track.sample_names[s_idx],
            theme.muted,
            8,
            TextAnchor::End,
        );
        for v in &track.records {
            let Some(sg) = v.genotypes.get(s_idx) else {
                continue;
            };
            let fill = match sg.call {
                GenotypeCall::HomRef => Rgb(0xe5, 0xe7, 0xeb),
                GenotypeCall::Het => Rgb(0x3b, 0x82, 0xf6),
                GenotypeCall::HomAlt => Rgb(0xef, 0x44, 0x44),
                GenotypeCall::OtherAlt => Rgb(0xa8, 0x55, 0xf7),
                GenotypeCall::NoCall => Rgb(0xf8, 0xfa, 0xfc),
            };
            let x = plot.bp_to_px(v.pos) - 2.0;
            doc.rect(x, sy, 4.0, row_h, fill);
            if sg.mendelian_violation {
                doc.rect_outline(x - 0.5, sy - 0.5, 5.0, row_h + 1.0, Rgb(0xec, 0x48, 0x99), 1.2);
            }
        }
    }
}

fn is_indel(v: &VariantRecord) -> bool {
    let ref_len = v.reference_allele.len();
    v.alternate_alleles.iter().any(|a| a.len() != ref_len)
}
