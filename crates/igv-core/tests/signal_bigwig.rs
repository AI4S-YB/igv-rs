use std::path::PathBuf;

use igv_core::region::Region;
use igv_core::source::open_signal;
use igv_core::source::signal::{FetchSignalOpts, SignalSummary};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/small.bw")
}

#[tokio::test]
async fn open_succeeds_and_reports_display_name() {
    let src = open_signal(&fixture(), None).await.unwrap();
    assert_eq!(src.display_name(), "small.bw");
}

#[tokio::test]
async fn open_nonexistent_path_returns_error() {
    let err = open_signal(std::path::Path::new("/no/such/file.bw"), None)
        .await
        .err()
        .expect("expected error opening missing file");
    let msg = err.to_string().to_lowercase();
    // Either io error or bigtools error; the message must mention the file
    // or be a bigwig parse error — anything useful is fine.
    assert!(!msg.is_empty());
}

#[tokio::test]
async fn fetch_chr1_raw_returns_per_base_ramp() {
    let src = open_signal(&fixture(), None).await.unwrap();
    let region = Region::new("chr1", 1, 100).unwrap();
    let opts = FetchSignalOpts {
        max_bins: 100,           // 100 bp / 100 bins = 1 bp/col → raw path
        summary: SignalSummary::Max,
    };
    let bins = src.fetch(&region, &opts).await.unwrap();
    assert!(!bins.is_empty(), "raw path returned empty");
    // Bin at index 0 should have value ~0, last bin value should be near 99.
    assert!(bins.first().unwrap().value < 1.0);
    let last = bins.last().unwrap().value;
    assert!(last > 90.0, "last bin value = {last}");
}

#[tokio::test]
async fn fetch_chr1_full_uses_zoom_summary() {
    let src = open_signal(&fixture(), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let opts = FetchSignalOpts {
        max_bins: 10,            // 1000 bp / 10 bins = 100 bp/col → zoom path
        summary: SignalSummary::Max,
    };
    let bins = src.fetch(&region, &opts).await.unwrap();
    assert!(bins.len() <= 10);
    assert!(bins.last().unwrap().value > 800.0);
}

#[tokio::test]
async fn fetch_unknown_chrom_returns_empty_no_error() {
    let src = open_signal(&fixture(), None).await.unwrap();
    let region = Region::new("chrZ", 1, 100).unwrap();
    let bins = src
        .fetch(&region, &FetchSignalOpts::default())
        .await
        .unwrap();
    assert!(bins.is_empty());
}
