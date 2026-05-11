use clap::Parser;
use igv_rs::cli::Cli;

#[test]
fn defaults_are_sensible() {
    let cli = Cli::parse_from(["igv-rs", "ref.fa"]);
    assert_eq!(cli.snapshot_format, "svg");
    assert_eq!(cli.snapshot_width, 1200);
    assert!((cli.snapshot_flank - 0.1).abs() < 1e-9);
    assert_eq!(cli.snapshot_theme, "igv");
    assert!(cli.snapshot_bed.is_none());
    assert!(cli.snapshot_genes.is_none());
}

#[test]
fn batch_flags_parse() {
    let cli = Cli::parse_from([
        "igv-rs",
        "ref.fa",
        "--snapshot-bed",
        "regions.bed",
        "--snapshot-out",
        "out/",
        "--snapshot-format",
        "png",
        "--snapshot-width",
        "1600",
        "--snapshot-flank",
        "0.2",
    ]);
    assert_eq!(cli.snapshot_bed.unwrap().to_str().unwrap(), "regions.bed");
    assert_eq!(cli.snapshot_out.unwrap().to_str().unwrap(), "out/");
    assert_eq!(cli.snapshot_format, "png");
    assert_eq!(cli.snapshot_width, 1600);
    assert!((cli.snapshot_flank - 0.2).abs() < 1e-9);
}
