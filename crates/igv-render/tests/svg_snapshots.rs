//! Per-track insta snapshots. Each test renders a focused fixture and
//! pins the resulting SVG. SVG output uses {:.2} formatting throughout
//! for cross-machine determinism.

use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::render_inputs::RenderInputs;
use igv_render::{render_svg, SvgOptions};

fn empty_inputs(start: u64, end: u64) -> RenderInputs {
    RenderInputs {
        region: Region::new("chr1", start, end).unwrap(),
        references: vec![],
        reference_seq: vec![],
        variants: vec![],
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        render_mode: RenderMode::DetailedReads,
    }
}

#[test]
fn empty_view_renders_header_and_ruler() {
    let inputs = empty_inputs(1, 1000);
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("empty_view_header_ruler", svg);
}
