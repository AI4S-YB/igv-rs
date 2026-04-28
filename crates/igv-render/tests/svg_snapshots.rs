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

use igv_core::render_inputs::AnnotationTrackSnapshot;
use igv_core::source::{AnnotationBlock, AnnotationTranscript, BlockKind, Strand, TranscriptKind};

#[test]
fn annotations_only_two_transcripts() {
    let mut inputs = empty_inputs(1, 1000);
    inputs.annotations.push(AnnotationTrackSnapshot {
        display: "genes.gtf".into(),
        transcripts: vec![
            AnnotationTranscript {
                name: "GENE1".into(),
                id: "tx1".into(),
                gene_id: Some("g1".into()),
                strand: Strand::Forward,
                blocks: vec![
                    AnnotationBlock { start: 100, end: 200, kind: BlockKind::Exon },
                    AnnotationBlock { start: 400, end: 500, kind: BlockKind::Exon },
                ],
                kind: TranscriptKind::Mrna,
            },
            AnnotationTranscript {
                name: "GENE2".into(),
                id: "tx2".into(),
                gene_id: Some("g2".into()),
                strand: Strand::Reverse,
                blocks: vec![AnnotationBlock { start: 600, end: 800, kind: BlockKind::Exon }],
                kind: TranscriptKind::Mrna,
            },
        ],
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("annotations_only_two_transcripts", svg);
}
