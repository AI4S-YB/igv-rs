use std::path::PathBuf;
use std::sync::Arc;

use igv_core::collect_render_inputs;
use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::source::fasta::NoodlesFastaSource;
use igv_core::source::link::open_link;
use igv_core::{CollectOpts, Sources};

fn fixture_bedpe() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/sample.bedpe")
}

fn fixture_fasta() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/sample.fa")
}

#[tokio::test]
async fn collect_includes_link_track() {
    let fasta = Arc::new(NoodlesFastaSource::open(&fixture_fasta()).await.unwrap())
        as Arc<dyn igv_core::source::FastaSource>;
    let refs = fasta.references().await.unwrap();
    let link = open_link(&fixture_bedpe(), None).await.unwrap();
    let sources = Sources {
        fasta,
        vcf: None,
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        links: vec![("sample.bedpe".into(), link)],
        references: refs,
    };
    // chr1:1_500_000-1_650_000 → loop2 BothIn + spanning_loop PartialCis.
    // Use OverviewOnly to skip reference seq fetch (fasta fixture is only 100 bp).
    let region = Region::new("chr1", 1_500_000, 1_650_000).unwrap();
    let opts = CollectOpts {
        render_mode: RenderMode::OverviewOnly,
        ..CollectOpts::default()
    };
    let inputs = collect_render_inputs(&sources, &region, &opts).await.unwrap();
    assert_eq!(inputs.links.len(), 1);
    assert!(
        inputs.links[0].visible.len() >= 2,
        "expected ≥2 visible links, got {}",
        inputs.links[0].visible.len()
    );
    assert_eq!(inputs.links[0].total_record_count, 7);
    assert_eq!(inputs.links[0].display, "sample.bedpe");
}
