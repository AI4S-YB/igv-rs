//! Reference DNA sequence, optional complement strand, 3-frame amino-acid
//! translation, and IUPAC motif search highlights.

use igv_core::render_inputs::RenderInputs;
use igv_core::sequence::{complement_base, translate_three_frames};

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
    if inputs.reference_seq.is_empty() {
        return;
    }

    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + 12) as f64,
        "sequence",
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let seq = &inputs.reference_seq;
    let base_w = (plot.plot_width as f64 / seq.len().max(1) as f64).max(1.0);
    let show_letters = base_w >= 7.0;

    // Motif hit highlights behind bases
    for hit in &inputs.sequence_opts.motif_hits {
        let hx0 = plot.bp_to_px(hit.start);
        let hx1 = plot.bp_to_px(hit.end + 1);
        doc.rect(
            hx0,
            area.y as f64 + 1.0,
            (hx1 - hx0).max(2.0),
            area.h as f64 - 2.0,
            Rgb(0xfe, 0xf3, 0xc7),
        );
    }

    let fwd_y = (area.y + 2) as f64;
    for (i, &b) in seq.iter().enumerate() {
        let x = plot.bp_to_px(inputs.region.start + i as u64);
        let fill = base_bg(b);
        doc.rect(x, fwd_y, base_w.max(1.0), 12.0, fill);
        if show_letters {
            let ch = (b.to_ascii_uppercase() as char).to_string();
            doc.text(
                x + base_w / 2.0,
                fwd_y + 10.0,
                &ch,
                theme.fg,
                9,
                TextAnchor::Middle,
            );
        }
    }

    let mut next_y = fwd_y + 14.0;
    if inputs.sequence_opts.show_complement {
        for (i, &b) in seq.iter().enumerate() {
            let cb = complement_base(b);
            let x = plot.bp_to_px(inputs.region.start + i as u64);
            doc.rect(x, next_y, base_w.max(1.0), 11.0, base_bg(cb));
            if show_letters {
                let ch = (cb.to_ascii_uppercase() as char).to_string();
                doc.text(
                    x + base_w / 2.0,
                    next_y + 9.0,
                    &ch,
                    theme.fg,
                    9,
                    TextAnchor::Middle,
                );
            }
        }
        next_y += 13.0;
    }

    if inputs.sequence_opts.show_translation {
        let frames = translate_three_frames(
            seq,
            inputs.region.start,
            inputs.sequence_opts.reverse_translation,
        );
        for f in &frames {
            doc.text(
                (plot.margin_left - 6) as f64,
                next_y + 9.0,
                &format!("F{}", f.frame + 1),
                theme.muted,
                8,
                TextAnchor::End,
            );
            for codon in &f.codons {
                let x0 = plot.bp_to_px(codon.start_bp);
                let x1 = plot.bp_to_px(codon.end_bp + 1);
                let cw = (x1 - x0 - 0.5).max(1.0);
                let bg = if codon.is_start {
                    Rgb(0xbb, 0xf7, 0xd0)
                } else if codon.is_stop {
                    Rgb(0xfe, 0xca, 0xca)
                } else {
                    Rgb(0xe2, 0xe8, 0xf0)
                };
                doc.rect(x0, next_y, cw, 10.0, bg);
                if cw >= 6.0 {
                    let s = codon.aa.to_string();
                    doc.text(
                        x0 + cw / 2.0,
                        next_y + 8.5,
                        &s,
                        theme.fg,
                        8,
                        TextAnchor::Middle,
                    );
                }
            }
            next_y += 12.0;
        }
    }
}

fn base_bg(b: u8) -> Rgb {
    match b.to_ascii_uppercase() {
        b'A' => Rgb(0xdc, 0xfc, 0xe7),
        b'C' => Rgb(0xdb, 0xea, 0xfe),
        b'G' => Rgb(0xff, 0xed, 0xd5),
        b'T' | b'U' => Rgb(0xfe, 0xe2, 0xe2),
        _ => Rgb(0xf1, 0xf5, 0xf9),
    }
}
