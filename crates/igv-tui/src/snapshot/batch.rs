//! Headless batch entry: render every region in a list to its own
//! file. No TUI, no raw mode.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use igv_core::collect_render_inputs;
use igv_core::source::{
    AnnotationSource, BamSource, FastaSource, FetchOpts, FetchSignalOpts, RefMeta, SignalSource,
    VcfSource,
};
use igv_core::{CollectOpts, Sources};
use igv_render::{render_png, render_svg, GraphicalTheme, SvgOptions};
use tracing::{info, warn};

use crate::app::action::SnapshotFormat;
use crate::snapshot::naming::batch_name;
use crate::snapshot::regions::{apply_flank, clamp_to_chrom, LabeledRegion};

#[derive(Debug, Clone)]
pub struct BatchOpts {
    pub out_dir: PathBuf,
    pub format: SnapshotFormat,
    pub width_px: u32,
    pub flank: f64,
    pub theme: GraphicalTheme,
}

pub async fn run(
    fasta: Arc<dyn FastaSource>,
    vcf: Option<Arc<dyn VcfSource>>,
    bams: Vec<(String, Arc<dyn BamSource>)>,
    annotations: Vec<(String, Arc<dyn AnnotationSource>)>,
    signals: Vec<(String, Arc<dyn SignalSource>)>,
    references: Vec<RefMeta>,
    regions: Vec<LabeledRegion>,
    batch: BatchOpts,
) -> Result<()> {
    std::fs::create_dir_all(&batch.out_dir)
        .with_context(|| format!("create {}", batch.out_dir.display()))?;

    let sources = Sources {
        fasta,
        vcf,
        bams,
        annotations,
        signals,
        links: vec![],
        references: references.clone(),
    };

    let mut rendered = 0usize;
    let mut skipped = 0usize;
    let total = regions.len();
    let thresholds = igv_core::render::Thresholds::default();

    for (i, lr) in regions.iter().enumerate() {
        let chrom_len = references
            .iter()
            .find(|m| m.name == lr.region.chrom)
            .map(|m| m.length);
        let padded = clamp_to_chrom(&apply_flank(&lr.region, batch.flank), chrom_len);
        let mode = thresholds.classify(padded.width());
        let collect_opts = CollectOpts {
            fetch_opts: FetchOpts::default(),
            signal_opts: FetchSignalOpts::default(),
            link_opts: igv_core::source::link::FetchLinkOpts::default(),
            render_mode: mode,
        };
        let inputs = match collect_render_inputs(&sources, &padded, &collect_opts).await {
            Ok(v) => v,
            Err(e) => {
                warn!("[{}/{}] {}: collect failed: {}", i + 1, total, padded, e);
                eprintln!("[{}/{}] {}: collect failed: {}", i + 1, total, padded, e);
                skipped += 1;
                continue;
            }
        };
        let opts = SvgOptions {
            width_px: batch.width_px,
            theme: batch.theme.clone(),
            signal_shared_max: None,
            ..SvgOptions::default()
        };
        let path = batch_name(&batch.out_dir, lr.label.as_deref(), &padded, batch.format);
        let result = match batch.format {
            SnapshotFormat::Svg => std::fs::write(&path, render_svg(&inputs, &opts))
                .map_err(anyhow::Error::from),
            SnapshotFormat::Png => match render_png(&inputs, &opts) {
                Ok(b) => std::fs::write(&path, b).map_err(anyhow::Error::from),
                Err(e) => Err(anyhow::anyhow!("png render: {}", e)),
            },
        };
        match result {
            Ok(()) => {
                info!("[{}/{}] {} → {}", i + 1, total, padded, path.display());
                eprintln!("[{}/{}] {} → {}", i + 1, total, padded, path.display());
                rendered += 1;
            }
            Err(e) => {
                warn!("[{}/{}] {}: write failed: {}", i + 1, total, padded, e);
                eprintln!("[{}/{}] {}: write failed: {}", i + 1, total, padded, e);
                skipped += 1;
            }
        }
    }

    eprintln!(
        "snapshot: rendered {}, skipped {} (total {})",
        rendered, skipped, total
    );
    if rendered == 0 && total > 0 {
        anyhow::bail!("no snapshots rendered");
    }
    Ok(())
}

pub fn parse_format(s: &str) -> Result<SnapshotFormat> {
    match s.to_ascii_lowercase().as_str() {
        "svg" => Ok(SnapshotFormat::Svg),
        "png" => Ok(SnapshotFormat::Png),
        _ => Err(anyhow::anyhow!("unknown snapshot format '{}' (svg|png)", s)),
    }
}

pub fn parse_theme(s: &str) -> Result<GraphicalTheme> {
    match s.to_ascii_lowercase().as_str() {
        "igv" | "tui" => Ok(GraphicalTheme::igv_light()),
        _ => Err(anyhow::anyhow!("unknown snapshot theme '{}' (igv|tui)", s)),
    }
}
