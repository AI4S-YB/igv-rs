use std::path::Path;

use igv_core::region::Region;
use igv_core::source::annotation::bed::NoodlesBedSource;
use igv_core::source::{AnnotationSource, BlockKind, Strand, TranscriptKind};

#[tokio::test]
async fn bed_loads_simple_features_with_strand() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(txs.iter().any(|t| t.name == "feat1" && t.strand == Strand::Forward));
    assert!(txs.iter().any(|t| t.name == "feat2" && t.strand == Strand::Reverse));
    let feat1 = txs.iter().find(|t| t.name == "feat1").unwrap();
    assert_eq!(feat1.blocks.len(), 1);
    assert_eq!(feat1.blocks[0].kind, BlockKind::BedSegment);
    assert_eq!(feat1.kind, TranscriptKind::BedFeature);
}

#[tokio::test]
async fn bed12_decomposes_into_blocks() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let big = txs.iter().find(|t| t.name == "bigblock").expect("bigblock missing");
    assert_eq!(big.blocks.len(), 3);
    // BED is 0-based half-open; we store 1-based inclusive.
    // Source range: chromStart=699, blockStarts=0,150,251 blockSizes=100,80,50
    // Block 1: 700..=799, Block 2: 850..=929, Block 3: 951..=1000
    let starts: Vec<u64> = big.blocks.iter().map(|b| b.start).collect();
    let ends: Vec<u64> = big.blocks.iter().map(|b| b.end).collect();
    assert_eq!(starts, vec![700, 850, 951]);
    assert_eq!(ends, vec![799, 929, 1000]);
}

#[tokio::test]
async fn bed_returns_only_overlapping_features() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let region = Region::new("chr1", 100, 250).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(txs.iter().any(|t| t.name == "feat1"));
    assert!(!txs.iter().any(|t| t.name == "feat2"));
    assert!(!txs.iter().any(|t| t.name == "bigblock"));
}

#[tokio::test]
async fn bed_find_by_name_matches_column4() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let hits = src.find_by_name("feat1");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, "chr1");
    assert_eq!(hits[0].1.name, "feat1");
    // Case-insensitive.
    assert_eq!(src.find_by_name("FEAT1").len(), 1);
    // gene_id is None for BED.
    assert!(hits[0].1.gene_id.is_none());
    // Misses.
    assert!(src.find_by_name("missing").is_empty());
}
