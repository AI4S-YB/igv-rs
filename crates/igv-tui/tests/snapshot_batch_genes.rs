//! Integration test for the gene resolver. Uses a stub
//! AnnotationSource so we don't need a real GFF on disk.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::source::{
    AnnotationBlock, AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};
use igv_tui::snapshot::genes::resolve;

struct StubAnno;

#[async_trait]
impl AnnotationSource for StubAnno {
    async fn fetch(&self, _r: &Region) -> Result<Vec<AnnotationTranscript>> {
        Ok(vec![])
    }
    fn display_name(&self) -> &str {
        "stub"
    }
    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        if query.eq_ignore_ascii_case("gene1") {
            vec![(
                "chr1".into(),
                AnnotationTranscript {
                    name: "GENE1".into(),
                    id: "tx1".into(),
                    gene_id: Some("g1".into()),
                    strand: Strand::Forward,
                    blocks: vec![AnnotationBlock {
                        start: 100,
                        end: 500,
                        kind: BlockKind::Exon,
                    }],
                    kind: TranscriptKind::Mrna,
                },
            )]
        } else {
            vec![]
        }
    }
}

#[test]
fn resolve_known_gene_returns_region() {
    let sources: Vec<Arc<dyn AnnotationSource>> = vec![Arc::new(StubAnno)];
    let names = vec!["gene1".to_string()];
    let v = resolve(&names, &sources);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].region.chrom, "chr1");
    assert_eq!(v[0].region.start, 100);
    assert_eq!(v[0].region.end, 500);
    assert_eq!(v[0].label.as_deref(), Some("gene1"));
}

#[test]
fn resolve_unknown_gene_skipped() {
    let sources: Vec<Arc<dyn AnnotationSource>> = vec![Arc::new(StubAnno)];
    let names = vec!["nope".to_string()];
    let v = resolve(&names, &sources);
    assert!(v.is_empty());
}
