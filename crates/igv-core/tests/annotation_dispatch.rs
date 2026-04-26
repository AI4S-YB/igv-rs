use std::path::Path;

use igv_core::region::Region;
use igv_core::source::annotation::open_annotation;

#[tokio::test]
async fn dispatcher_opens_gff3() {
    let src = open_annotation(Path::new("tests/data/sample.gff3"), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(!txs.is_empty());
    assert!(src.display_name().contains("sample.gff3"));
}

#[tokio::test]
async fn dispatcher_opens_gtf() {
    let src = open_annotation(Path::new("tests/data/sample.gtf"), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert_eq!(txs.len(), 1);
}

#[tokio::test]
async fn dispatcher_opens_bed() {
    let src = open_annotation(Path::new("tests/data/sample.bed"), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert_eq!(txs.len(), 4);
}

#[tokio::test]
async fn dispatcher_errors_on_unknown_extension() {
    let result = open_annotation(Path::new("tests/data/sample.fa"), None).await;
    assert!(result.is_err());
}
