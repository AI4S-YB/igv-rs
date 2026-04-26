use std::path::Path;

use igv_core::region::Region;
use igv_core::source::bam::{BamSource, FetchOpts, NoodlesBamSource};

#[tokio::test]
async fn fetches_reads_overlapping_region() {
    let source = NoodlesBamSource::open(Path::new("tests/data/sample.bam"), None)
        .await
        .unwrap();
    let region = Region::new("chr1", 1, 100).unwrap();
    let reads = source.fetch(&region, &FetchOpts::default()).await.unwrap();
    assert_eq!(reads.len(), 3);
    let names: Vec<_> = reads.iter().map(|r| r.query_name.as_str()).collect();
    assert!(names.contains(&"read1"));
    assert!(names.contains(&"read2"));
    assert!(names.contains(&"read3"));
}

#[tokio::test]
async fn cigar_is_parsed() {
    let source = NoodlesBamSource::open(Path::new("tests/data/sample.bam"), None)
        .await
        .unwrap();
    let region = Region::new("chr1", 20, 30).unwrap();
    let reads = source.fetch(&region, &FetchOpts::default()).await.unwrap();
    let read2 = reads
        .iter()
        .find(|r| r.query_name == "read2")
        .expect("read2 in region");
    assert_eq!(read2.cigar.len(), 3);
}
