use std::path::PathBuf;

use igv_core::region::Region;
use igv_core::source::link::{open_link, FetchLinkOpts, LinkScope, LinkSource};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/sample.bedpe")
}

async fn open() -> std::sync::Arc<dyn LinkSource> {
    open_link(&fixture(), None).await.unwrap()
}

#[tokio::test]
async fn query_returns_both_in_and_partial_cis() {
    let src = open().await;
    let region = Region::new("chr1", 1_500_000, 1_650_000).unwrap();
    let visible = src.query(&region, &FetchLinkOpts::default()).await.unwrap();
    let names: Vec<_> = visible
        .iter()
        .map(|v| v.record.name.as_deref().unwrap_or("(unnamed)"))
        .collect();
    assert!(names.contains(&"loop2"), "got names: {names:?}");
    assert!(
        names.contains(&"spanning_loop"),
        "spanning_loop should be PartialCis (one anchor in window): {names:?}"
    );

    // Verify loop2 is BothIn
    let loop2 = visible
        .iter()
        .find(|v| v.record.name.as_deref() == Some("loop2"))
        .expect("loop2 must be in result");
    assert!(
        matches!(loop2.scope, LinkScope::BothIn),
        "loop2 should be BothIn, got: {:?}",
        loop2.scope
    );

    // Verify spanning_loop is PartialCis with correct off-anchor details
    let spanning = visible
        .iter()
        .find(|v| v.record.name.as_deref() == Some("spanning_loop"))
        .expect("spanning_loop must be in result");
    assert!(
        matches!(spanning.scope, LinkScope::PartialCis { .. }),
        "spanning_loop should be PartialCis, got: {:?}",
        spanning.scope
    );
    match &spanning.scope {
        LinkScope::PartialCis { off_anchor_mid, off_to_left } => {
            assert!(
                !*off_to_left,
                "spanning_loop's off-anchor (anchor B at chr1:5M) should be to the RIGHT"
            );
            assert_eq!(
                *off_anchor_mid, 5_000_500,
                "off_anchor_mid should be midpoint of anchor B [5_000_000, 5_001_000]"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn query_returns_trans_when_one_anchor_in_window() {
    let src = open().await;
    let region = Region::new("chr2", 4_999_500, 5_000_500).unwrap();
    let visible = src.query(&region, &FetchLinkOpts::default()).await.unwrap();
    let trans: Vec<_> = visible
        .iter()
        .filter(|v| matches!(v.scope, LinkScope::Trans { .. }))
        .collect();
    assert_eq!(trans.len(), 1, "expected exactly one trans hit");
    assert_eq!(trans[0].record.name.as_deref(), Some("trans_link"));

    // Verify off_chrom and off_anchor_mid for trans_link
    // trans_link: chr1 [1004001, 1005000] ↔ chr2 [4999001, 5000000] (1-based inclusive)
    // Query on chr2, so off anchor is on chr1 with midpoint = (1004001 + 1005000) / 2 = 1004500
    match &trans[0].scope {
        LinkScope::Trans { off_chrom, off_anchor_mid } => {
            assert_eq!(
                off_chrom.as_ref(),
                "chr1",
                "off_chrom should be the chrom of the OFF anchor (chr1, since query was on chr2)"
            );
            assert_eq!(
                *off_anchor_mid, 1_004_500,
                "off_anchor_mid should be midpoint of the chr1 anchor [1_004_001, 1_005_000]"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn query_drops_spanning_links_with_no_anchor_overlap() {
    let src = open().await;
    let region = Region::new("chr1", 2_500_000, 2_600_000).unwrap();
    let visible = src.query(&region, &FetchLinkOpts::default()).await.unwrap();
    assert!(
        visible.iter().all(|v| v.record.name.as_deref() != Some("spanning_loop")),
        "spanning_loop should not be returned: {:?}",
        visible.iter().map(|v| &v.record.name).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn query_returns_empty_for_unknown_chromosome() {
    let src = open().await;
    let region = Region::new("chrZZZ", 1, 1000).unwrap();
    let visible = src.query(&region, &FetchLinkOpts::default()).await.unwrap();
    assert!(visible.is_empty());
}

#[tokio::test]
async fn query_filters_low_scores_when_min_score_set() {
    let src = open().await;
    let region = Region::new("chr1", 2_999_500, 3_051_500).unwrap();
    let unfiltered = src
        .query(&region, &FetchLinkOpts { min_score: None })
        .await
        .unwrap();
    assert!(unfiltered
        .iter()
        .any(|v| v.record.name.as_deref() == Some("low_score_loop")));
    let filtered = src
        .query(&region, &FetchLinkOpts { min_score: Some(1.0) })
        .await
        .unwrap();
    assert!(!filtered
        .iter()
        .any(|v| v.record.name.as_deref() == Some("low_score_loop")));
}

#[tokio::test]
async fn query_keeps_unscored_records_under_min_score() {
    let src = open().await;
    let region = Region::new("chr1", 2_000_000, 2_012_000).unwrap();
    let visible = src
        .query(&region, &FetchLinkOpts { min_score: Some(99.0) })
        .await
        .unwrap();
    assert!(
        visible.iter().any(|v| v.record.score.is_none()),
        "unscored record should survive --link-min-score: {visible:?}"
    );
}

#[tokio::test]
async fn deduplicates_when_both_anchors_overlap_window() {
    let src = open().await;
    let region = Region::new("chr1", 1_000_000, 1_010_000).unwrap();
    let visible = src.query(&region, &FetchLinkOpts::default()).await.unwrap();
    let loop1_count = visible
        .iter()
        .filter(|v| v.record.name.as_deref() == Some("loop1"))
        .count();
    assert_eq!(loop1_count, 1, "loop1 must not be returned twice");
}
