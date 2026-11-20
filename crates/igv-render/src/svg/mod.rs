//! SVG render entry point. Per-track functions live in submodules and
//! all draw into a shared `SvgDoc`.

pub mod alignments;
pub mod annotations;
pub mod coverage;
pub mod doc;
pub mod gwas;
pub mod header;
pub mod link;
pub mod ruler;
pub mod sashimi;
pub mod seg;
pub mod sequence;
pub mod signal;
pub mod variants;

use igv_core::render_inputs::RenderInputs;

use crate::layout;
use crate::options::SvgOptions;
use crate::svg::doc::SvgDoc;

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> String {
    let layout = layout::compute_with_options(inputs, opts);
    let mut doc = SvgDoc::new(
        layout.total_width,
        layout.total_height,
        opts.theme.bg,
        opts.theme.font_family,
    );

    header::draw(&mut doc, layout.header, inputs, opts.title.as_deref(), &opts.theme);
    if let Some(rect) = layout.ideogram {
        header::draw_ideogram(&mut doc, rect, &layout.plot, inputs, &opts.theme);
    }
    ruler::draw(&mut doc, layout.ruler, &layout.plot, inputs, &opts.theme);
    if let Some(rect) = layout.sequence {
        sequence::draw(&mut doc, rect, &layout.plot, inputs, &opts.theme);
    }
    for (rect, track) in layout.annotations.iter().zip(inputs.annotations.iter()) {
        annotations::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }
    for (rect, track) in layout.vcf_tracks.iter().zip(inputs.vcf_tracks.iter()) {
        variants::draw_track(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }
    if let Some(rect) = layout.variants {
        variants::draw(&mut doc, rect, &layout.plot, inputs, &opts.theme);
    }
    if let Some(rect) = layout.coverage {
        coverage::draw_with_snv(
            &mut doc,
            rect,
            &layout.plot,
            inputs,
            opts.show_coverage_snv,
            &opts.theme,
        );
    }
    let sashimi_bams: Vec<_> = inputs
        .bams
        .iter()
        .filter(|b| !b.junctions.is_empty())
        .collect();
    for (rect, bam) in layout.sashimi.iter().zip(sashimi_bams.iter()) {
        sashimi::draw(&mut doc, *rect, &layout.plot, bam, &inputs.region, &opts.theme);
    }
    for (rect, track) in layout.signals.iter().zip(inputs.signals.iter()) {
        signal::draw(&mut doc, *rect, &layout.plot, track, opts.signal_shared_max, &opts.theme);
    }
    for (rect, track) in layout.links.iter().zip(inputs.links.iter()) {
        link::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }
    for (rect, track) in layout.segs.iter().zip(inputs.segs.iter()) {
        seg::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }
    for (rect, track) in layout.gwas.iter().zip(inputs.gwas.iter()) {
        gwas::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }
    for (rect, track) in layout.alignments.iter().zip(inputs.bams.iter()) {
        alignments::draw_with_opts(
            &mut doc,
            *rect,
            &layout.plot,
            track,
            &opts.track_heights,
            &inputs.reference_seq,
            inputs.region.start,
            &inputs.alignment_opts,
            &opts.theme,
        );
    }

    doc.finish()
}
