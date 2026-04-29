use std::path::PathBuf;

use igv_core::source::LinkFormat;

#[test]
fn format_dispatch_by_extension() {
    let cases = [
        ("a.bedpe", Some(LinkFormat::Bedpe)),
        ("a.bedpe.gz", Some(LinkFormat::Bedpe)),
        ("a.BEDPE", Some(LinkFormat::Bedpe)),
        ("a.BedPE.GZ", Some(LinkFormat::Bedpe)),
        ("a.bedpe.bak", None),
        ("a.bw", None),
        ("plain", None),
    ];
    for (name, expected) in cases {
        let got = LinkFormat::from_path(&PathBuf::from(name));
        assert_eq!(got, expected, "case {name}");
    }
}

#[test]
fn format_parse_string() {
    assert_eq!(LinkFormat::parse("bedpe"), Some(LinkFormat::Bedpe));
    assert_eq!(LinkFormat::parse("BEDPE"), Some(LinkFormat::Bedpe));
    assert_eq!(LinkFormat::parse("interact"), None);
    assert_eq!(LinkFormat::parse(""), None);
}

#[tokio::test]
async fn open_link_unknown_extension_errors_with_hint() {
    let err = igv_core::source::open_link(
        std::path::Path::new("/nope.unknown"),
        None,
    )
    .await
    .err()
    .expect("expected an error for unknown extension");
    let msg = err.to_string();
    assert!(msg.contains("--link-format"), "msg: {msg}");
}
