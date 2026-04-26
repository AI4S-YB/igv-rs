use std::path::Path;

use igv_core::region::Region;
use igv_core::source::fasta::NoodlesFastaSource;
use igv_core::source::FastaSource;

#[tokio::test]
async fn lists_references_with_lengths() {
    let path = Path::new("tests/data/sample.fa");
    let source = NoodlesFastaSource::open(path).await.unwrap();
    let refs = source.references().await.unwrap();
    let chr1 = refs.iter().find(|r| r.name == "chr1").unwrap();
    assert_eq!(chr1.length, 100);
    let chr2 = refs.iter().find(|r| r.name == "chr2").unwrap();
    assert_eq!(chr2.length, 50);
}

#[tokio::test]
async fn fetches_substring_for_region() {
    let path = Path::new("tests/data/sample.fa");
    let source = NoodlesFastaSource::open(path).await.unwrap();
    let region = Region::new("chr1", 1, 4).unwrap();
    let bytes = source.fetch(&region).await.unwrap();
    assert_eq!(bytes, b"ACGT");
}

#[tokio::test]
async fn fetch_errors_on_unknown_chrom() {
    let path = Path::new("tests/data/sample.fa");
    let source = NoodlesFastaSource::open(path).await.unwrap();
    let region = Region::new("chrZ", 1, 4).unwrap();
    assert!(source.fetch(&region).await.is_err());
}
