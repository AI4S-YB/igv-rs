//! Integration test: command-palette input that doesn't parse as a region
//! is matched against loaded annotations by gene_name / gene_id /
//! transcript_id, and the resulting region is the union span of all matched
//! transcripts on the same chromosome.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::render::Thresholds;
use igv_core::source::annotation::{
    AnnotationBlock, AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};
use igv_core::source::{FastaSource, RefMeta};
use igv_tui::app::action::Action;
use igv_tui::app::state::{
    AnnotationTrack, AppState, ALIGNMENT_DEFAULT_HEIGHT, COVERAGE_DEFAULT_HEIGHT,
    SIGNAL_DEFAULT_HEIGHT,
};
use igv_tui::ui::theme::Theme;

struct MockFasta;

#[async_trait]
impl FastaSource for MockFasta {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 100_000 }])
    }
    async fn fetch(&self, _r: &Region) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

struct MockAnnotation {
    name: String,
    by_chrom: Vec<(String, AnnotationTranscript)>,
}

#[async_trait]
impl AnnotationSource for MockAnnotation {
    async fn fetch(&self, _r: &Region) -> Result<Vec<AnnotationTranscript>> {
        Ok(Vec::new())
    }
    fn display_name(&self) -> &str {
        &self.name
    }
    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        let q = query.trim();
        self.by_chrom
            .iter()
            .filter(|(_, tx)| {
                tx.name.eq_ignore_ascii_case(q)
                    || tx.id.eq_ignore_ascii_case(q)
                    || tx.gene_id.as_deref().is_some_and(|g| g.eq_ignore_ascii_case(q))
            })
            .cloned()
            .collect()
    }
}

fn make_tx(name: &str, id: &str, gene_id: Option<&str>, start: u64, end: u64) -> AnnotationTranscript {
    AnnotationTranscript {
        name: name.into(),
        id: id.into(),
        gene_id: gene_id.map(str::to_string),
        strand: Strand::Forward,
        blocks: vec![AnnotationBlock { start, end, kind: BlockKind::Cds }],
        kind: TranscriptKind::Mrna,
    }
}

fn make_state(annotation: Arc<dyn AnnotationSource>) -> AppState {
    AppState {
        fasta: Arc::new(MockFasta),
        vcf: None,
        bams: Vec::new(),
        references: vec![RefMeta { name: "chr1".into(), length: 100_000 }],
        region: Region::new("chr1", 1, 250).unwrap(),
        reference_seq: Vec::new(),
        variants: Vec::new(),
        bam_rows: Vec::new(),
        bam_lanes: Vec::new(),
        bam_total_lanes: Vec::new(),
        bam_scroll: 0,
        annotations: vec![AnnotationTrack {
            path: std::path::PathBuf::from("mock.gff3"),
            display: "mock".into(),
            source: annotation,
        }],
        annotation_rows: vec![Vec::new()],
        signals: Vec::new(),
        signal_bins: Vec::new(),
        signal_shared_scale: false,
        signal_track_height: SIGNAL_DEFAULT_HEIGHT,
        alignment_height: ALIGNMENT_DEFAULT_HEIGHT,
        coverage_height: COVERAGE_DEFAULT_HEIGHT,
        theme: Theme::dark(),
        theme_preset: igv_tui::ui::theme::ThemePreset::Dark,
        thresholds: Thresholds::default(),
        bookmarks: HashMap::new(),
        status: None,
        command_open: false,
        command_buffer: String::new(),
        help_open: false,
        terminal_width: 0,
        generation: 0,
        loading: false,
        should_quit: false,
    }
}

#[test]
fn command_submit_jumps_to_gene_by_name() {
    let mock = Arc::new(MockAnnotation {
        name: "mock".into(),
        by_chrom: vec![
            ("chr1".into(), make_tx("HER2", "tx_a", Some("ENSG_HER2"), 100, 500)),
            ("chr1".into(), make_tx("HER2", "tx_b", Some("ENSG_HER2"), 200, 600)),
            ("chr1".into(), make_tx("BRCA1", "tx_c", Some("ENSG_BRCA1"), 50_000, 60_000)),
        ],
    });
    let mut state = make_state(mock);
    let req = state.apply(Action::CommandSubmit("HER2".into()));
    assert!(req.is_some(), "expected a LoadRequest");
    assert_eq!(state.region.chrom, "chr1");
    // Union of (100..=500) and (200..=600) → (100..=600).
    assert_eq!(state.region.start, 100);
    assert_eq!(state.region.end, 600);
}

#[test]
fn command_submit_jumps_by_gene_id_when_only_id_matches() {
    let mock = Arc::new(MockAnnotation {
        name: "mock".into(),
        by_chrom: vec![
            ("chr1".into(), make_tx("HER2", "tx_a", Some("ENSG_HER2"), 100, 500)),
        ],
    });
    let mut state = make_state(mock);
    let req = state.apply(Action::CommandSubmit("ENSG_HER2".into()));
    assert!(req.is_some());
    assert_eq!(state.region.start, 100);
    assert_eq!(state.region.end, 500);
}

#[test]
fn command_submit_jumps_by_transcript_id() {
    let mock = Arc::new(MockAnnotation {
        name: "mock".into(),
        by_chrom: vec![
            ("chr1".into(), make_tx("HER2", "tx_a", Some("ENSG_HER2"), 100, 500)),
            ("chr1".into(), make_tx("HER2", "tx_b", Some("ENSG_HER2"), 200, 600)),
        ],
    });
    let mut state = make_state(mock);
    let req = state.apply(Action::CommandSubmit("tx_b".into()));
    assert!(req.is_some());
    assert_eq!(state.region.start, 200);
    assert_eq!(state.region.end, 600);
}

#[test]
fn command_submit_unknown_gene_keeps_region_and_sets_error() {
    let mock = Arc::new(MockAnnotation {
        name: "mock".into(),
        by_chrom: vec![("chr1".into(), make_tx("HER2", "tx_a", None, 100, 500))],
    });
    let mut state = make_state(mock);
    let original = state.region.clone();
    let req = state.apply(Action::CommandSubmit("UNKNOWN_GENE".into()));
    assert!(req.is_none());
    assert_eq!(state.region, original);
    assert!(state.status.is_some());
}

#[test]
fn command_submit_region_form_still_works() {
    let mock = Arc::new(MockAnnotation { name: "mock".into(), by_chrom: vec![] });
    let mut state = make_state(mock);
    let req = state.apply(Action::CommandSubmit("chr1:1000-2000".into()));
    assert!(req.is_some());
    assert_eq!(state.region.start, 1000);
    assert_eq!(state.region.end, 2000);
}
