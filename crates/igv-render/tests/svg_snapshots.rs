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
        links: vec![],
        render_mode: RenderMode::DetailedReads,
    }
}

#[test]
fn empty_view_renders_header_and_ruler() {
    let inputs = empty_inputs(1, 1000);
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("empty_view_header_ruler", svg);
}

use igv_core::render_inputs::{AnnotationTrackSnapshot, BamTrackSnapshot, SignalTrackSnapshot};
use igv_core::source::bam::{CigarKind, CigarOp};
use igv_core::source::{
    AlignmentRow, AnnotationBlock, AnnotationTranscript, BlockKind, SignalBin, Strand,
    TranscriptKind, VariantRecord,
};

fn fake_read(start: u64, end: u64) -> AlignmentRow {
    AlignmentRow {
        query_name: "r".into(),
        flag: 0,
        ref_start: start,
        ref_end: end,
        mapq: 60,
        is_reverse: false,
        query_sequence: vec![],
        cigar: vec![],
        tag: None,
    }
}

fn cigar_match(len: u32) -> CigarOp {
    CigarOp { kind: CigarKind::Match, len }
}

#[test]
fn variants_only_three_records() {
    let mut inputs = empty_inputs(1, 1000);
    inputs.variants = vec![
        VariantRecord {
            chrom: "chr1".into(),
            pos: 250,
            reference_allele: "A".into(),
            alternate_alleles: vec!["T".into()],
            quality: Some(60.0),
            passes_filter: true,
        },
        VariantRecord {
            chrom: "chr1".into(),
            pos: 600,
            reference_allele: "G".into(),
            alternate_alleles: vec!["GA".into()],
            quality: Some(50.0),
            passes_filter: true,
        },
        VariantRecord {
            chrom: "chr1".into(),
            pos: 900,
            reference_allele: "C".into(),
            alternate_alleles: vec!["A".into()],
            quality: Some(40.0),
            passes_filter: true,
        },
    ];
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("variants_only_three_records", svg);
}

#[test]
fn coverage_only_one_bam_two_reads() {
    let mut inputs = empty_inputs(1, 100);
    inputs.bams.push(BamTrackSnapshot {
        display: "sample.bam".into(),
        rows: vec![fake_read(10, 60), fake_read(30, 80)],
        lanes: vec![0, 1],
        total_lanes: 2,
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("coverage_only_one_bam_two_reads", svg);
}

#[test]
fn signal_only_one_bigwig() {
    let mut inputs = empty_inputs(1, 1000);
    inputs.signals.push(SignalTrackSnapshot {
        display: "chip.bw".into(),
        bins: vec![
            SignalBin { start: 1, end: 200, value: 5.0 },
            SignalBin { start: 201, end: 400, value: 12.0 },
            SignalBin { start: 401, end: 600, value: 3.0 },
            SignalBin { start: 601, end: 800, value: 8.5 },
            SignalBin { start: 801, end: 1000, value: 1.5 },
        ],
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("signal_only_one_bigwig", svg);
}

#[test]
fn alignments_only_two_reads() {
    let mut inputs = empty_inputs(1, 200);
    inputs.reference_seq = vec![b'A'; 200];
    let mut row1 = fake_read(20, 60);
    row1.cigar = vec![cigar_match(41)];
    row1.query_sequence = vec![b'A'; 41];
    let mut row2 = fake_read(80, 150);
    row2.is_reverse = true;
    row2.cigar = vec![cigar_match(71)];
    row2.query_sequence = vec![b'T'; 71];
    inputs.bams.push(BamTrackSnapshot {
        display: "sample.bam".into(),
        rows: vec![row1, row2],
        lanes: vec![0, 0],
        total_lanes: 1,
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("alignments_only_two_reads", svg);
}

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
