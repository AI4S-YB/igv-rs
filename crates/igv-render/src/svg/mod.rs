//! SVG render entry point. Per-track functions live in submodules and
//! all draw into a shared `SvgDoc`.

pub mod annotations;
pub mod doc;
pub mod header;
pub mod ruler;

use igv_core::render_inputs::RenderInputs;

use crate::layout;
use crate::options::SvgOptions;
use crate::svg::doc::SvgDoc;

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> String {
    let layout = layout::compute(inputs, opts.width_px, &opts.track_heights);
    let mut doc = SvgDoc::new(
        layout.total_width,
        layout.total_height,
        opts.theme.bg,
        opts.theme.font_family,
    );

    header::draw(&mut doc, layout.header, inputs, opts.title.as_deref(), &opts.theme);
    ruler::draw(&mut doc, layout.ruler, &layout.plot, inputs, &opts.theme);
    for (rect, track) in layout.annotations.iter().zip(inputs.annotations.iter()) {
        annotations::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }

    doc.finish()
}
