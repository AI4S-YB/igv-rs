use std::path::PathBuf;

use igv_core::source::SignalFormat;

#[test]
fn format_dispatch_by_extension() {
    let cases = [
        ("a.bw", Some(SignalFormat::BigWig)),
        ("a.bigwig", Some(SignalFormat::BigWig)),
        ("a.bigWig", Some(SignalFormat::BigWig)),
        ("a.BW", Some(SignalFormat::BigWig)),
        ("a.bw.gz", None),
        ("a.bam", None),
        ("plain", None),
    ];
    for (name, expected) in cases {
        let got = SignalFormat::from_path(&PathBuf::from(name));
        assert_eq!(got, expected, "case {name}");
    }
}

#[test]
fn format_parse_string() {
    assert_eq!(SignalFormat::parse("bw"), Some(SignalFormat::BigWig));
    assert_eq!(SignalFormat::parse("BIGWIG"), Some(SignalFormat::BigWig));
    assert_eq!(SignalFormat::parse("BigWig"), Some(SignalFormat::BigWig));
    assert_eq!(SignalFormat::parse("bigbed"), None);
    assert_eq!(SignalFormat::parse(""), None);
}

#[tokio::test]
async fn open_signal_unknown_extension_errors_with_hint() {
    let err = igv_core::source::open_signal(
        std::path::Path::new("/nope.unknown"),
        None,
    )
    .await
    .err()
    .expect("expected an error for unknown extension");
    let msg = err.to_string();
    assert!(msg.contains("--signal-format"), "msg: {msg}");
}
