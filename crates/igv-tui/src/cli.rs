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
}
