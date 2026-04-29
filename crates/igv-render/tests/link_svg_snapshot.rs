use std::sync::Arc;

use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::render_inputs::{LinkTrackSnapshot, RenderInputs};
use igv_core::source::annotation::Strand;
use igv_core::source::link::{LinkRecord, LinkScope, VisibleLink};
use igv_render::{render_svg, SvgOptions};

fn link(s_a: u64, e_a: u64, s_b: u64, e_b: u64, score: Option<f64>) -> VisibleLink {
    VisibleLink {
        record: LinkRecord {
            chrom_a: Arc::from("chr1"),
            start_a: s_a,
            end_a: e_a,
            chrom_b: Arc::from("chr1"),
            start_b: s_b,
            end_b: e_b,
            name: None,
            score,
            strand_a: Strand::Forward,
            strand_b: Strand::Reverse,
        },
        scope: LinkScope::BothIn,
    }
}

#[test]
fn link_arc_emits_bezier_path_and_anchor_rects() {
    let inputs = RenderInputs {
        region: Region::new("chr1", 1_000_000, 1_010_000).unwrap(),
        references: vec![],
        reference_seq: vec![],
        variants: vec![],
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        links: vec![LinkTrackSnapshot {
            display: "loops.bedpe".into(),
            visible: vec![
                link(1_001_000, 1_002_000, 1_008_000, 1_009_000, Some(5.0)),
                link(1_003_000, 1_004_000, 1_006_000, 1_007_000, Some(2.0)),
            ],
            total_record_count: 2,
        }],
        render_mode: RenderMode::DetailedReads,
    };
    let svg = render_svg(&inputs, &SvgOptions::default());
    let bezier_count = svg.matches(r#"<path d="M "#).count();
    assert!(bezier_count >= 2, "expected ≥2 Bézier paths, got {bezier_count} in:\n{svg}");
    assert!(svg.contains("<rect "));
    assert!(svg.contains("loops.bedpe"));
}

#[test]
fn link_heatmap_emits_per_column_strip() {
    let mut visible = Vec::new();
    for i in 0..200 {
        let off = 1_000_000 + i * 50;
        visible.push(link(off, off + 20, off + 30, off + 40, Some(i as f64)));
    }
    let inputs = RenderInputs {
        region: Region::new("chr1", 1_000_000, 1_011_000).unwrap(),
        references: vec![],
        reference_seq: vec![],
        variants: vec![],
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        links: vec![LinkTrackSnapshot {
            display: "dense.bedpe".into(),
            visible,
            total_record_count: 200,
        }],
        render_mode: RenderMode::DetailedReads,
    };
    let svg = render_svg(&inputs, &SvgOptions::default());
    let rect_count = svg.matches("<rect ").count();
    assert!(rect_count > 50, "heatmap should emit many <rect> strips, got {rect_count}");
}
