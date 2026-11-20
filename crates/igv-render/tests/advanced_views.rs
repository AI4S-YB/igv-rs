use std::sync::Arc;

use igv_core::alignment::mods::BaseModCall;
use igv_core::alignment::{AlignmentColorMode, AlignmentRenderOpts, MateLink};
use igv_core::genome::{CytoStain, Cytoband};
use igv_core::region::Region;
use igv_core::render_inputs::{
    BamTrackSnapshot, GwasTrackSnapshot, RenderInputs, SegTrackSnapshot, SequenceRenderOpts,
    VcfTrackSnapshot,
};
use igv_core::sashimi::SpliceJunction;
use igv_core::sequence::MotifHit;
use igv_core::source::annotation::Strand;
use igv_core::source::bam::{AlignmentRow, CigarKind, CigarOp};
use igv_core::source::gwas::GwasPoint;
use igv_core::source::link::LinkRecord;
use igv_core::source::seg::SegInterval;
use igv_core::source::{GenotypeCall, RefMeta, SampleGenotype, VariantRecord};
use igv_render::{
    encode_iterm2_png, encode_kitty_png, encode_sixel_rgba, render_circular_png,
    render_circular_svg, render_png, render_svg, render_terminal_escape, CircularOptions,
    SvgOptions, TerminalGraphicsProtocol,
};

#[test]
fn renders_sequence_ideogram_sashimi_seg_gwas_vcf_and_mods_svg() {
    let inputs = RenderInputs {
        region: Region::new("chr1", 1, 60).unwrap(),
        references: vec![RefMeta { name: "chr1".into(), length: 120 }],
        cytobands: vec![
            Cytoband {
                chrom: "chr1".into(),
                start: 1,
                end: 60,
                name: "p1".into(),
                stain: CytoStain::Gpos50,
            },
            Cytoband {
                chrom: "chr1".into(),
                start: 61,
                end: 120,
                name: "q1".into(),
                stain: CytoStain::Acen,
            },
        ],
        reference_seq: b"ATGCCATGGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTAGCTA".to_vec(),
        sequence_opts: SequenceRenderOpts {
            show_complement: true,
            show_translation: true,
            reverse_translation: false,
            motif_pattern: Some("ATGG".into()),
            motif_hits: vec![MotifHit {
                start: 5,
                end: 8,
                strand: Strand::Forward,
                matched: "ATGG".into(),
            }],
        },
        vcf_tracks: vec![VcfTrackSnapshot {
            display: "trio.vcf".into(),
            sample_names: vec!["Child".into(), "Father".into(), "Mother".into()],
            records: vec![VariantRecord {
                chrom: "chr1".into(),
                pos: 25,
                id: Some("rs9".into()),
                reference_allele: "A".into(),
                alternate_alleles: vec!["G".into()],
                quality: Some(90.0),
                passes_filter: true,
                allele_freq: Some(0.2),
                info_raw: String::new(),
                genotypes: vec![
                    SampleGenotype {
                        sample: "Child".into(),
                        call: GenotypeCall::Het,
                        mendelian_violation: true,
                        ..Default::default()
                    },
                    SampleGenotype {
                        sample: "Father".into(),
                        call: GenotypeCall::HomRef,
                        ..Default::default()
                    },
                    SampleGenotype {
                        sample: "Mother".into(),
                        call: GenotypeCall::HomRef,
                        ..Default::default()
                    },
                ],
            }],
        }],
        alignment_opts: AlignmentRenderOpts {
            color_mode: AlignmentColorMode::BaseModification,
            view_as_pairs: true,
            show_soft_clips: true,
            ..Default::default()
        },
        bams: vec![BamTrackSnapshot {
            display: "rna.bam".into(),
            rows: vec![AlignmentRow {
                query_name: "r1".into(),
                ref_start: 10,
                ref_end: 49,
                mapq: 60,
                query_sequence: vec![b'C'; 40],
                cigar: vec![
                    CigarOp { kind: CigarKind::SoftClip, len: 2 },
                    CigarOp { kind: CigarKind::Match, len: 10 },
                    CigarOp { kind: CigarKind::Skip, len: 20 },
                    CigarOp { kind: CigarKind::Match, len: 10 },
                ],
                base_mods: vec![BaseModCall {
                    q_idx: 4,
                    code: 'm',
                    opposite_strand: false,
                    prob: 0.95,
                }],
                ..Default::default()
            }],
            lanes: vec![0],
            total_lanes: 1,
            mate_links: vec![MateLink {
                lane: 0,
                left_end: 20,
                right_start: 35,
            }],
            junctions: vec![SpliceJunction {
                start: 20,
                end: 39,
                count: 14,
                strand: Strand::Forward,
            }],
            ..Default::default()
        }],
        segs: vec![SegTrackSnapshot {
            display: "cnv.seg".into(),
            samples: vec!["Tumor".into(), "Normal".into()],
            segments: vec![
                SegInterval {
                    sample: "Tumor".into(),
                    chrom: "chr1".into(),
                    start: 10,
                    end: 50,
                    num_marks: Some(12),
                    value: 1.4,
                },
                SegInterval {
                    sample: "Normal".into(),
                    chrom: "chr1".into(),
                    start: 10,
                    end: 50,
                    num_marks: Some(12),
                    value: -1.1,
                },
            ],
        }],
        gwas: vec![GwasTrackSnapshot {
            display: "trait.gwas".into(),
            points: vec![GwasPoint {
                pos: 30,
                neg_log10_p: 9.2,
                p_raw: 6.3e-10,
                snp_id: "rsLead".into(),
            }],
        }],
        ..Default::default()
    };

    let opts = SvgOptions {
        show_ideogram: true,
        show_sequence: true,
        show_coverage_snv: true,
        show_sashimi: true,
        ..Default::default()
    };

    let svg = render_svg(&inputs, &opts);
    assert!(svg.contains("rna.bam (sashimi)"));
    assert!(svg.contains(">14<"));
    assert!(svg.contains("rsLead (9.2)"));
    assert!(svg.contains("Tumor"));
    assert!(svg.contains("trio.vcf"));
    assert!(svg.contains("F1"));

    let png = render_png(&inputs, &opts).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");

    // Verify Kitty / iTerm2 / Sixel escape sequences
    let kitty = encode_kitty_png(&png);
    assert!(kitty.starts_with("\x1b_Ga=T,f=100,"));
    assert!(kitty.ends_with("\x1b\\"));

    let iterm = encode_iterm2_png(&png, 80, 24);
    assert!(iterm.starts_with("\x1b]1337;File=inline=1;"));
    assert!(iterm.ends_with('\x07'));

    let sixel_opt =
        render_terminal_escape(&inputs, &opts, TerminalGraphicsProtocol::Sixel, 80, 24).unwrap();
    let sixel = sixel_opt.expect("sixel sequence");
    assert!(sixel.starts_with("\x1bPq"));
    assert!(sixel.ends_with("\x1b\\"));

    let tiny = encode_sixel_rgba(&[255, 0, 0, 255, 0, 255, 0, 255], 2, 1);
    assert!(tiny.starts_with("\x1bPq"));
}

#[test]
fn renders_circular_svg_and_png() {
    let refs = vec![
        RefMeta { name: "chr1".into(), length: 1_000_000 },
        RefMeta { name: "chr2".into(), length: 800_000 },
    ];
    let links = vec![LinkRecord {
        chrom_a: Arc::from("chr1"),
        start_a: 200_000,
        end_a: 205_000,
        chrom_b: Arc::from("chr2"),
        start_b: 500_000,
        end_b: 505_000,
        name: Some("fusion".into()),
        score: Some(25.0),
        strand_a: Strand::Forward,
        strand_b: Strand::Reverse,
    }];
    let segs = vec![SegInterval {
        sample: "S1".into(),
        chrom: "chr1".into(),
        start: 100_000,
        end: 600_000,
        num_marks: None,
        value: 1.2,
    }];
    let svg = render_circular_svg(&refs, &links, &segs, &CircularOptions::default());
    assert!(svg.contains("<path d=\"M "));
    assert!(svg.contains(">chr1<"));
    assert!(svg.contains(">chr2<"));
    let png = render_circular_png(&refs, &links, &segs, &CircularOptions::default()).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}
