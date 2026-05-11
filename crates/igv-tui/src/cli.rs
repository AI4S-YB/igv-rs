use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "igv-rs",
    version,
    about = "Interactive terminal genome viewer (Rust rewrite of cligv)"
)]
pub struct Cli {
    /// Path to the reference genome FASTA file (must have a .fai index).
    pub fasta: PathBuf,

    /// Path to a VCF file (must have a .tbi index). May be repeated in a
    /// future iteration; today only the first is honored.
    #[arg(short = 'v', long = "vcf")]
    pub vcf: Option<PathBuf>,

    /// Path to a BAM file (must have a .bai or .csi index). May be repeated
    /// to display multiple alignment tracks.
    #[arg(short = 'b', long = "bam")]
    pub bam: Vec<PathBuf>,

    /// Initial region (e.g. "chr1:1000-2000", "chr1:1000", "chr1").
    #[arg(short = 'r', long = "region")]
    pub region: Option<String>,

    /// BAM tag to color reads by (two-character tag, e.g. "ha").
    #[arg(short = 't', long = "tag")]
    pub tag: Option<String>,

    /// Path to a GFF3, GTF, BED, or narrowPeak/broadPeak annotation file.
    /// Format auto-detected by extension. May be repeated.
    #[arg(short = 'g', long = "annotation")]
    pub annotations: Vec<std::path::PathBuf>,

    /// Override annotation format auto-detection
    /// (`gff`, `gff3`, `gtf`, `bed`, `narrowpeak`, or `broadpeak`).
    /// Applies to all `-g` files.
    #[arg(long = "annotation-format")]
    pub annotation_format: Option<String>,

    /// Path to a bigWig signal file (.bw / .bigwig). May be repeated.
    #[arg(short = 's', long = "signal")]
    pub signals: Vec<PathBuf>,

    /// Override signal format auto-detection (currently only `bigwig`).
    /// Applies to all `-s` files.
    #[arg(long = "signal-format")]
    pub signal_format: Option<String>,

    /// Path to a BEDPE link file (.bedpe / .bedpe.gz). May be repeated.
    /// Each file becomes its own track showing pairwise interactions
    /// (chromatin loops, enhancer-promoter, ChIA-PET, etc.).
    #[arg(short = 'l', long = "link")]
    pub links: Vec<PathBuf>,

    /// Override link format auto-detection (currently only `bedpe`).
    /// Applies to all `-l` files.
    #[arg(long = "link-format")]
    pub link_format: Option<String>,

    /// Drop links whose score column is below this value.
    /// Records without a score are unaffected.
    #[arg(long = "link-min-score")]
    pub link_min_score: Option<f64>,

    /// Use light theme (for light-background terminals).
    #[arg(long = "light-mode")]
    pub light_mode: bool,

    /// Logging level filter.
    #[arg(long = "log-level", default_value = "info")]
    pub log_level: String,

    /// Optional override config path. Defaults to
    /// `$XDG_CONFIG_HOME/igv-rs/config.toml`.
    #[arg(long = "config")]
    pub config: Option<PathBuf>,

    /// Render snapshots for every region in this BED file (no TUI).
    /// Mutually exclusive with --snapshot-genes.
    #[arg(long = "snapshot-bed")]
    pub snapshot_bed: Option<PathBuf>,

    /// Render snapshots for every gene name in this newline-separated
    /// file (no TUI). Requires at least one -g/--annotation. Mutually
    /// exclusive with --snapshot-bed.
    #[arg(long = "snapshot-genes")]
    pub snapshot_genes: Option<PathBuf>,

    /// Output directory for batch snapshots. Required when
    /// --snapshot-bed or --snapshot-genes is set.
    #[arg(long = "snapshot-out")]
    pub snapshot_out: Option<PathBuf>,

    /// Output format for snapshots: `svg` (default) or `png`.
    #[arg(long = "snapshot-format", default_value = "svg")]
    pub snapshot_format: String,

    /// Image width in px for snapshots.
    #[arg(long = "snapshot-width", default_value_t = 1200)]
    pub snapshot_width: u32,

    /// Padding fraction added to each side of every batch region.
    #[arg(long = "snapshot-flank", default_value_t = 0.1)]
    pub snapshot_flank: f64,

    /// Snapshot color theme: `igv` (default) or `tui`.
    #[arg(long = "snapshot-theme", default_value = "igv")]
    pub snapshot_theme: String,

    /// Disable the `B` keystroke / browser launch (CI, headless servers).
    #[arg(long = "no-browser")]
    pub no_browser: bool,

    /// TCP port for the browser-view HTTP server. 0 picks any free port.
    #[arg(long = "serve-port", default_value_t = 0)]
    pub serve_port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_browser_and_serve_port_parse() {
        let cli = Cli::parse_from([
            "igv-rs",
            "ref.fa",
            "--no-browser",
            "--serve-port", "9001",
        ]);
        assert!(cli.no_browser);
        assert_eq!(cli.serve_port, 9001);
    }

    #[test]
    fn serve_port_defaults_to_zero() {
        let cli = Cli::parse_from(["igv-rs", "ref.fa"]);
        assert_eq!(cli.serve_port, 0);
        assert!(!cli.no_browser);
    }
}
