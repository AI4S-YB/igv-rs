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
    let scopes: Vec<_> = visible.iter().map(|v| &v.scope).collect();
    assert!(
        scopes.iter().any(|s| matches!(s, LinkScope::BothIn)),
        "scopes: {scopes:?}"
    );
    assert!(
        scopes
            .iter()
            .any(|s| matches!(s, LinkScope::PartialCis { .. })),
        "scopes: {scopes:?}"
    );
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
