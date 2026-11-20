use igv_core::genome::CytoStain;
use igv_core::render_inputs::RenderInputs;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::{GraphicalTheme, Rgb};

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    inputs: &RenderInputs,
    title: Option<&str>,
    theme: &GraphicalTheme,
) {
    let title = title.map(str::to_string).unwrap_or_else(|| "igv-rs snapshot".into());
    let region_str = format!(
        "{}:{}-{}",
        inputs.region.chrom, inputs.region.start, inputs.region.end
    );
    let baseline_y = (area.y + area.h * 2 / 3) as f64;
    doc.text(
        12.0,
        baseline_y,
        &title,
        theme.fg,
        theme.font_px_label,
        TextAnchor::Start,
    );
    doc.text(
        (area.w - 12) as f64,
        baseline_y,
        &region_str,
        theme.muted,
        theme.font_px_normal,
        TextAnchor::End,
    );
}

pub fn draw_ideogram(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    let bands: Vec<_> = inputs
        .cytobands
        .iter()
        .filter(|b| b.chrom == inputs.region.chrom)
        .collect();
    if bands.is_empty() {
        return;
    }

    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + area.h * 2 / 3) as f64,
        &inputs.region.chrom,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let chrom_len = inputs
        .references
        .iter()
        .find(|r| r.name == inputs.region.chrom)
        .map(|r| r.length)
        .unwrap_or_else(|| bands.iter().map(|b| b.end).max().unwrap_or(1))
        .max(1) as f64;

    let y = (area.y + 2) as f64;
    let bh = (area.h.saturating_sub(4)).max(6) as f64;
    let pw = plot.plot_width as f64;
    let x0_base = plot.plot_x0 as f64;

    let mut acen_left = true;
    for band in &bands {
        let bx0 = x0_base + ((band.start.saturating_sub(1)) as f64 / chrom_len) * pw;
        let bx1 = x0_base + (band.end as f64 / chrom_len) * pw;
        let bw = (bx1 - bx0).max(0.5);
        match band.stain {
            CytoStain::Acen => {
                let fill = Rgb(0xc0, 0x39, 0x2b);
                if acen_left {
                    doc.polygon(&[(bx0, y), (bx1, y + bh / 2.0), (bx0, y + bh)], fill);
                } else {
                    doc.polygon(&[(bx1, y), (bx1, y + bh), (bx0, y + bh / 2.0)], fill);
                }
                acen_left = false;
            }
            _ => {
                let fill = match band.stain {
                    CytoStain::Gneg => Rgb(0xf5, 0xf5, 0xf5),
                    CytoStain::Gpos25 => Rgb(0xc8, 0xc8, 0xc8),
                    CytoStain::Gpos50 => Rgb(0x90, 0x90, 0x90),
                    CytoStain::Gpos75 => Rgb(0x58, 0x58, 0x58),
                    CytoStain::Gpos100 => Rgb(0x22, 0x22, 0x22),
                    CytoStain::Gvar => Rgb(0xd6, 0xe4, 0xf0),
                    CytoStain::Stalk => Rgb(0x9a, 0xb8, 0xd3),
                    CytoStain::Acen => unreachable!(),
                };
                doc.rect(bx0, y, bw, bh, fill);
            }
        }
    }

    doc.rect_outline(x0_base, y, pw, bh, theme.muted, 0.8);

    // Viewport indicator box
    let vx0 = x0_base + ((inputs.region.start.saturating_sub(1)) as f64 / chrom_len) * pw;
    let vx1 = x0_base + (inputs.region.end as f64 / chrom_len) * pw;
    let vw = (vx1 - vx0).max(3.0);
    doc.rect_outline(vx0, y - 1.0, vw, bh + 2.0, Rgb(0xd6, 0x27, 0x28), 1.8);
}
