use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::render_inputs::RenderInputs;
use igv_render::{render_png, SvgOptions};

#[test]
fn png_smoke_empty_view() {
    let inputs = RenderInputs {
        region: Region::new("chr1", 1, 1000).unwrap(),
        references: vec![],
        reference_seq: vec![],
        variants: vec![],
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        links: vec![],
        render_mode: RenderMode::DetailedReads,
    };
    let bytes = render_png(&inputs, &SvgOptions::default()).expect("render_png");
    assert!(bytes.len() > 100, "png output too small");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}
