//! SVG render entry point. Per-track functions live in submodules and
//! all draw into a shared `SvgDoc`.

pub mod alignments;
pub mod annotations;
pub mod coverage;
pub mod doc;
pub mod header;
pub mod link;
pub mod ruler;
pub mod signal;
pub mod variants;

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
    if let Some(rect) = layout.variants {
        variants::draw(&mut doc, rect, &layout.plot, inputs, &opts.theme);
    }
    if let Some(rect) = layout.coverage {
        coverage::draw(&mut doc, rect, &layout.plot, inputs, &opts.theme);
    }
    for (rect, track) in layout.signals.iter().zip(inputs.signals.iter()) {
        signal::draw(&mut doc, *rect, &layout.plot, track, opts.signal_shared_max, &opts.theme);
    }
    for (rect, track) in layout.links.iter().zip(inputs.links.iter()) {
        link::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }
    for (rect, track) in layout.alignments.iter().zip(inputs.bams.iter()) {
        alignments::draw(
            &mut doc,
            *rect,
            &layout.plot,
            track,
            &opts.track_heights,
            &inputs.reference_seq,
            inputs.region.start,
            &opts.theme,
        );
    }

    doc.finish()
}
