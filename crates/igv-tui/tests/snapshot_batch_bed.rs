//! End-to-end batch BED snapshot test. Calls the batch entry directly
//! (not the binary) so we can run with no FASTA index handling.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::region::Region;
use igv_core::source::{FastaSource, RefMeta};
use igv_render::GraphicalTheme;
use igv_tui::app::action::SnapshotFormat;
use igv_tui::snapshot::batch::{run, BatchOpts};
use igv_tui::snapshot::regions::LabeledRegion;

struct StubFasta;

#[async_trait]
impl FastaSource for StubFasta {
    async fn references(&self) -> igv_core::error::Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 1000 }])
    }
    async fn fetch(&self, _r: &Region) -> igv_core::error::Result<Vec<u8>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn batch_bed_emits_one_svg_per_region() {
    let dir = tempfile::tempdir().unwrap();
    let opts = BatchOpts {
        out_dir: dir.path().to_path_buf(),
        format: SnapshotFormat::Svg,
        width_px: 800,
        flank: 0.0,
        theme: GraphicalTheme::igv_light(),
    };
    let regions = vec![
        LabeledRegion {
            region: Region::new("chr1", 100, 200).unwrap(),
            label: Some("A".into()),
        },
        LabeledRegion {
            region: Region::new("chr1", 500, 600).unwrap(),
            label: None,
        },
    ];
    run(
        Arc::new(StubFasta) as Arc<dyn FastaSource>,
        None,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![RefMeta { name: "chr1".into(), length: 1000 }],
        regions,
        opts,
    )
    .await
    .unwrap();

    let out_a = dir.path().join("A_chr1_100_200.svg");
    let out_b = dir.path().join("chr1_500_600.svg");
    assert!(out_a.exists(), "missing {}", out_a.display());
    assert!(out_b.exists(), "missing {}", out_b.display());
    let body = std::fs::read_to_string(&out_a).unwrap();
    assert!(body.starts_with("<?xml"), "not SVG-shaped");
}
