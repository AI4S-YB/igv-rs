//! Smoke-test the synchronous collector: an in-memory mock-source
//! triple (FastaSource / no VCF / no BAM), one region, returns the
//! expected RenderInputs.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::source::{FastaSource, RefMeta};
use igv_core::{collect_render_inputs, CollectOpts, Sources};

struct MockFasta;

#[async_trait]
impl FastaSource for MockFasta {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 1000 }])
    }
    async fn fetch(&self, _region: &Region) -> Result<Vec<u8>> {
        Ok(b"ACGTACGT".to_vec())
    }
}

#[tokio::test]
async fn collect_minimal_inputs() {
    let sources = Sources {
        fasta: Arc::new(MockFasta) as Arc<dyn FastaSource>,
        vcf: None,
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        references: vec![RefMeta { name: "chr1".into(), length: 1000 }],
    };
    let region = Region::new("chr1", 1, 8).unwrap();
    let opts = CollectOpts {
        render_mode: RenderMode::DetailedReads,
        ..CollectOpts::default()
    };
    let out = collect_render_inputs(&sources, &region, &opts).await.unwrap();
    assert_eq!(out.region, region);
    assert_eq!(out.reference_seq, b"ACGTACGT".to_vec());
    assert!(out.bams.is_empty());
    assert_eq!(out.render_mode, RenderMode::DetailedReads);
}

#[tokio::test]
async fn collect_skips_reference_at_wide_zoom() {
    let sources = Sources {
        fasta: Arc::new(MockFasta) as Arc<dyn FastaSource>,
        vcf: None,
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        references: vec![RefMeta { name: "chr1".into(), length: 1000 }],
    };
    let region = Region::new("chr1", 1, 1000).unwrap();
    let opts = CollectOpts {
        render_mode: RenderMode::OverviewOnly,
        ..CollectOpts::default()
    };
    let out = collect_render_inputs(&sources, &region, &opts).await.unwrap();
    assert!(out.reference_seq.is_empty(), "reference should be gated at OverviewOnly");
}
