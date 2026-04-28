use igv_core::render_inputs::RenderInputs;
use igv_core::source::VariantRecord;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::SvgDoc;
use crate::theme::GraphicalTheme;

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

fn is_indel(v: &VariantRecord) -> bool {
    let ref_len = v.reference_allele.len();
    v.alternate_alleles.iter().any(|a| a.len() != ref_len)
}
