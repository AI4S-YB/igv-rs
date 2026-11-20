use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "igv-rs",
    version,
    about = "Interactive terminal & headless genome viewer"
)]
pub struct Cli {
    /// Reference genome FASTA/.2bit (local path or http(s)/s3/gs URL).
    /// Optional when --genome, --hub, --session, or --batch is supplied.
    pub fasta: Option<String>,

    /// Built-in genome ID (`hg38`, `hg19`, `mm39`, `mm10`, `danRer11`, `dm6`, `ce11`, `sacCer3`)
    /// or path/URL to a `.json` descriptor or `.2bit`/FASTA reference.
    #[arg(long = "genome")]
    pub genome: Option<String>,

    /// Variant VCF/BCF file or URL (`.vcf`, `.vcf.gz`, `.bcf`). May be repeated.
    #[arg(short = 'v', long = "vcf")]
    pub vcf: Vec<String>,

    /// PED pedigree file or URL for trio ordering & Mendelian violation detection.
    #[arg(long = "ped")]
    pub ped: Option<PathBuf>,

    /// Alignment file or URL (`.bam`, `.cram`, `.sam`, `.paf`). May be repeated.
    #[arg(short = 'b', long = "bam")]
    pub bam: Vec<String>,

    /// Initial region(s), e.g. "chr1:1000-2000". Repeat up to 4 times for split panes.
    #[arg(short = 'r', long = "region")]
    pub region: Vec<String>,

    /// Alignment tag name to inspect/color by (two-character tag, e.g. "HP", "ha").
    #[arg(short = 't', long = "tag")]
    pub tag: Option<String>,

    /// Annotation track file or URL (`.gff`, `.gff3`, `.gtf`, `.bed`, `.bb`, `.bigBed`).
    #[arg(short = 'g', long = "annotation")]
    pub annotations: Vec<String>,

    /// Override annotation format auto-detection (`gff`, `gff3`, `gtf`, `bed`, `bigbed`, ...).
    #[arg(long = "annotation-format")]
    pub annotation_format: Option<String>,

    /// Signal track file or URL (`.bw`, `.bigwig`, `.wig`, `.bedGraph`). May be repeated.
    #[arg(short = 's', long = "signal")]
    pub signals: Vec<String>,

    /// Override signal format auto-detection (`bigwig`, `wig`, `bedgraph`).
    #[arg(long = "signal-format")]
    pub signal_format: Option<String>,

    /// Signal aggregation summary across windows (`max`, `mean`, `min`, `sum`).
    #[arg(long = "signal-summary", default_value = "max")]
    pub signal_summary: String,

    /// Pairwise interaction link file or URL (`.bedpe`, `.bedpe.gz`, `.inter.bb`). May be repeated.
    #[arg(short = 'l', long = "link")]
    pub links: Vec<String>,

    /// Override link format auto-detection (`bedpe`, `biginteract`).
    #[arg(long = "link-format")]
    pub link_format: Option<String>,

    /// Drop links whose score column is below this value.
    #[arg(long = "link-min-score")]
    pub link_min_score: Option<f64>,

    /// Copy-number segmentation file or URL (`.seg`, `.cbs`, `.cn`). May be repeated.
    #[arg(long = "seg")]
    pub seg: Vec<String>,

    /// GWAS Manhattan summary stats file or URL (`.gwas`, `.assoc`). May be repeated.
    #[arg(long = "gwas")]
    pub gwas: Vec<String>,

    /// UCSC Track Hub `hub.txt` path or URL. May be repeated.
    #[arg(long = "hub")]
    pub hub: Vec<String>,

    /// Alignment coloring mode (`strand`, `insert`, `pair`, `mate`, `tag`, `mapq`, `mod`, `bisulfite`).
    #[arg(long = "color-by")]
    pub color_by: Option<String>,

    /// Alignment sorting mode (`none`, `start`, `base`, `strand`, `mapq`, `insert`, `mate`, `tag`, `hp`).
    #[arg(long = "sort-by")]
    pub sort_by: Option<String>,

    /// Alignment grouping mode (`none`, `strand`, `rg`, `hp`, `pair`, `mate`, `tag`, `chimeric`).
    #[arg(long = "group-by")]
    pub group_by: Option<String>,

    /// Group mate reads onto the same lane with connecting lines (View as Pairs).
    #[arg(long = "view-as-pairs")]
    pub view_as_pairs: bool,

    /// Show soft-clipped overhang bases in alignment tracks.
    #[arg(long = "show-soft-clips")]
    pub show_soft_clips: bool,

    /// Per-bin reservoir downsampling read cap (`0` = unlimited).
    #[arg(long = "max-depth", default_value_t = 200)]
    pub max_depth: usize,

    /// Display RNA-seq Sashimi splice-junction arc plots alongside coverage/alignments.
    #[arg(long = "sashimi")]
    pub sashimi: bool,

    /// Export a whole-genome circular SV / CNV chord plot (`.svg` or `.png`).
    #[arg(long = "circular-out")]
    pub circular_out: Option<PathBuf>,

    /// Load a saved `.toml` or IGV `.json` session file.
    #[arg(long = "session")]
    pub session: Option<PathBuf>,

    /// Run an IGV `.txt`/`.igv` batch script headlessly (no TUI) and exit.
    #[arg(long = "batch")]
    pub batch: Option<PathBuf>,

    /// Enable an IGV socket listener on `127.0.0.1:<PORT>` for live external commands.
    #[arg(long = "port")]
    pub port: Option<u16>,

    /// Optional bearer auth token sent with remote HTTP(S) track/genome requests.
    #[arg(long = "auth-token")]
    pub auth_token: Option<String>,

    /// Inline terminal pixel graphics protocol (`auto`, `none`, `kitty`, `iterm2`, `sixel`).
    #[arg(long = "graphics", default_value = "auto")]
    pub graphics: String,

    /// Use light theme (for light-background terminals).
    #[arg(long = "light-mode")]
    pub light_mode: bool,

    /// Logging level filter.
    #[arg(long = "log-level", default_value = "info")]
    pub log_level: String,

    /// Optional override config path. Defaults to `$XDG_CONFIG_HOME/igv-rs/config.toml`.
    #[arg(long = "config")]
    pub config: Option<PathBuf>,

    /// Render snapshots for every region in this BED file (no TUI).
    #[arg(long = "snapshot-bed")]
    pub snapshot_bed: Option<PathBuf>,

    /// Render snapshots for every gene name in this newline-separated file (no TUI).
    #[arg(long = "snapshot-genes")]
    pub snapshot_genes: Option<PathBuf>,

    /// Output directory for batch snapshots.
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
        let cli = Cli::parse_from(["igv-rs", "ref.fa", "--no-browser", "--serve-port", "9001"]);
        assert!(cli.no_browser);
        assert_eq!(cli.serve_port, 9001);
        assert_eq!(cli.fasta.as_deref(), Some("ref.fa"));
    }

    #[test]
    fn serve_port_defaults_to_zero() {
        let cli = Cli::parse_from(["igv-rs", "ref.fa"]);
        assert_eq!(cli.serve_port, 0);
        assert!(!cli.no_browser);
    }

    #[test]
    fn genome_and_multi_locus_parse_without_positional_fasta() {
        let cli = Cli::parse_from([
            "igv-rs",
            "--genome",
            "hg38",
            "-r",
            "chr7:55000000-55200000",
            "-r",
            "chr17:7668402-7687538",
            "--color-by",
            "mod",
            "--sashimi",
            "--port",
            "60151",
        ]);
        assert!(cli.fasta.is_none());
        assert_eq!(cli.genome.as_deref(), Some("hg38"));
        assert_eq!(cli.region.len(), 2);
        assert_eq!(cli.color_by.as_deref(), Some("mod"));
        assert!(cli.sashimi);
        assert_eq!(cli.port, Some(60151));
    }
}
