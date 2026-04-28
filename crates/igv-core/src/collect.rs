//! Synchronous-await collector for `RenderInputs`.
//!
//! Used by the headless snapshot batch path. Issues each source's
//! `fetch` in sequence and assembles the result. Does **not** use the
//! TUI loader's mpsc/generation machinery — that exists for
//! cancellation during interactive use, which the batch path doesn't
//! need.

use std::sync::Arc;

use crate::alignment::assign_lanes;
use crate::region::Region;
use crate::render::RenderMode;
use crate::render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, RenderInputs, SignalTrackSnapshot,
};
use crate::source::{
    AnnotationSource, BamSource, FastaSource, FetchOpts, FetchSignalOpts, RefMeta,
    SignalSource, VcfSource,
};

#[derive(Clone)]
pub struct Sources {
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub bams: Vec<(String, Arc<dyn BamSource>)>,
    pub annotations: Vec<(String, Arc<dyn AnnotationSource>)>,
    pub signals: Vec<(String, Arc<dyn SignalSource>)>,
    pub references: Vec<RefMeta>,
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources")
            .field("vcf", &self.vcf.is_some())
            .field("bams", &self.bams.len())
            .field("annotations", &self.annotations.len())
            .field("signals", &self.signals.len())
            .field("references", &self.references.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CollectOpts {
    pub fetch_opts: FetchOpts,
    pub signal_opts: FetchSignalOpts,
    pub render_mode: RenderMode,
}

impl Default for CollectOpts {
    fn default() -> Self {
        Self {
            fetch_opts: FetchOpts::default(),
            signal_opts: FetchSignalOpts::default(),
            render_mode: RenderMode::DetailedReads,
        }
    }
}

/// Collect all data needed to render one region. Skips heavy fetches
/// at wide zoom levels (matches the loader's gating policy):
///
/// * Reference sequence: only `PerBase` and `DetailedReads`.
/// * Variants: skipped at `OverviewOnly`.
/// * BAM rows: only `PerBase` and `DetailedReads`.
/// * Annotations: always fetched.
/// * Signals: always fetched (bigWig zoom-pyramid handles it).
pub async fn collect_render_inputs(
    sources: &Sources,
    region: &Region,
    opts: &CollectOpts,
) -> crate::error::Result<RenderInputs> {
    let mode = opts.render_mode;

    let reference_seq = if matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
        sources.fasta.fetch(region).await?
    } else {
        Vec::new()
    };

    let variants = if let Some(vcf) = &sources.vcf {
        if !matches!(mode, RenderMode::OverviewOnly) {
            vcf.fetch(region).await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut bams = Vec::with_capacity(sources.bams.len());
    for (display, src) in &sources.bams {
        let rows = if matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            src.fetch(region, &opts.fetch_opts).await?
        } else {
            Vec::new()
        };
        let lanes = assign_lanes(&rows);
        let total_lanes_u32 = lanes.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        let total_lanes = total_lanes_u32.min(u16::MAX as u32) as u16;
        bams.push(BamTrackSnapshot {
            display: display.clone(),
            rows,
            lanes,
            total_lanes,
        });
    }

    let mut annotations = Vec::with_capacity(sources.annotations.len());
    for (display, src) in &sources.annotations {
        let transcripts = src.fetch(region).await?;
        annotations.push(AnnotationTrackSnapshot {
            display: display.clone(),
            transcripts,
        });
    }

    let mut signals = Vec::with_capacity(sources.signals.len());
    for (display, src) in &sources.signals {
        let bins = src.fetch(region, &opts.signal_opts).await?;
        signals.push(SignalTrackSnapshot {
            display: display.clone(),
            bins,
        });
    }

    Ok(RenderInputs {
        region: region.clone(),
        references: sources.references.clone(),
        reference_seq,
        variants,
        bams,
        annotations,
        signals,
        render_mode: mode,
    })
}
