use igv_core::render_inputs::RenderInputs;

use crate::layout::Rect;
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

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
