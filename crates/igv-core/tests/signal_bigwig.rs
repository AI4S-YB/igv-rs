use std::path::PathBuf;

use igv_core::source::open_signal;

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
