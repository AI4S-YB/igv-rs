use std::path::Path;

use igv_core::region::Region;
use igv_core::source::vcf::{NoodlesVcfSource, VcfSource};

#[tokio::test]
async fn fetches_three_variants_in_range() {
    let path = Path::new("tests/data/sample.vcf.gz");
    let source = NoodlesVcfSource::open(path).await.unwrap();
    let region = Region::new("chr1", 1, 100).unwrap();
    let variants = source.fetch(&region).await.unwrap();
    assert_eq!(variants.len(), 3);
    assert_eq!(variants[0].pos, 10);
    assert_eq!(variants[0].reference_allele, "A");
    assert_eq!(variants[0].alternate_alleles, vec!["G".to_string()]);
}

#[tokio::test]
async fn returns_empty_outside_range() {
    let path = Path::new("tests/data/sample.vcf.gz");
    let source = NoodlesVcfSource::open(path).await.unwrap();
    let region = Region::new("chr1", 60, 100).unwrap();
    let variants = source.fetch(&region).await.unwrap();
    assert!(variants.is_empty());
}
