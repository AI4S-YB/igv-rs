//! Synchronous-await collector for `RenderInputs` (used by headless batch
//! and inline terminal pixel snapshots).

use std::sync::Arc;

use crate::alignment::{downsample_reads, group_and_pack, sort_rows, AlignmentRenderOpts};
use crate::genome::Cytoband;
use crate::region::Region;
use crate::render::RenderMode;
use crate::render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, GwasTrackSnapshot, LinkTrackSnapshot, RenderInputs,
    SegTrackSnapshot, SequenceRenderOpts, SignalTrackSnapshot, VcfTrackSnapshot,
};
use crate::sashimi::extract_splice_junctions;
use crate::sequence::search_iupac_motif;
use crate::source::link::{FetchLinkOpts, LinkSource};
use crate::source::{
    AnnotationSource, BamSource, FastaSource, FetchOpts, FetchSignalOpts, GwasSource, RefMeta,
    SegSource, SignalSource, VcfSource,
};

#[derive(Clone)]
pub struct Sources {
    pub fasta: Arc<dyn FastaSource>,
    /// Legacy single-VCF field; also combined with `vcfs`.
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub vcfs: Vec<(String, Arc<dyn VcfSource>)>,
    pub bams: Vec<(String, Arc<dyn BamSource>)>,
    pub annotations: Vec<(String, Arc<dyn AnnotationSource>)>,
    pub signals: Vec<(String, Arc<dyn SignalSource>)>,
    pub links: Vec<(String, Arc<dyn LinkSource>)>,
    pub segs: Vec<(String, Arc<dyn SegSource>)>,
    pub gwas: Vec<(String, Arc<dyn GwasSource>)>,
    pub references: Vec<RefMeta>,
    pub cytobands: Vec<Cytoband>,
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources")
            .field("vcf", &self.vcf.is_some())
            .field("vcfs", &self.vcfs.len())
            .field("bams", &self.bams.len())
            .field("annotations", &self.annotations.len())
            .field("signals", &self.signals.len())
            .field("links", &self.links.len())
            .field("segs", &self.segs.len())
            .field("gwas", &self.gwas.len())
            .field("references", &self.references.len())
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct CollectOpts {
    pub fetch_opts: FetchOpts,
    pub signal_opts: FetchSignalOpts,
    pub link_opts: FetchLinkOpts,
    pub alignment_opts: AlignmentRenderOpts,
    pub sequence_opts: SequenceRenderOpts,
    pub render_mode: RenderMode,
}

impl Default for CollectOpts {
    fn default() -> Self {
        Self {
            fetch_opts: FetchOpts::default(),
            signal_opts: FetchSignalOpts::default(),
            link_opts: FetchLinkOpts::default(),
            alignment_opts: AlignmentRenderOpts::default(),
            sequence_opts: SequenceRenderOpts::default(),
            render_mode: RenderMode::DetailedReads,
        }
    }
}

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

    let mut sequence_opts = opts.sequence_opts.clone();
    if let Some(pat) = &sequence_opts.motif_pattern {
        if !reference_seq.is_empty() {
            sequence_opts.motif_hits = search_iupac_motif(pat, &reference_seq, region.start)
                .unwrap_or_default();
        }
    }

    let mut vcf_tracks = Vec::new();
    let mut legacy_variants = Vec::new();
    if !matches!(mode, RenderMode::OverviewOnly) {
        if let Some(vcf) = &sources.vcf {
            let recs = vcf.fetch(region).await?;
            legacy_variants = recs.clone();
            if sources.vcfs.is_empty() {
                vcf_tracks.push(VcfTrackSnapshot {
                    display: vcf.display_name().to_string(),
                    sample_names: vcf.sample_names(),
                    records: recs,
                });
            }
        }
        for (display, vcf) in &sources.vcfs {
            let recs = vcf.fetch(region).await?;
            if legacy_variants.is_empty() {
                legacy_variants = recs.clone();
            }
            vcf_tracks.push(VcfTrackSnapshot {
                display: display.clone(),
                sample_names: vcf.sample_names(),
                records: recs,
            });
        }
    }

    let center_bp = region.start + region.width() / 2;
    let mut bams = Vec::with_capacity(sources.bams.len());
    for (display, src) in &sources.bams {
        let mut rows = if matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            src.fetch(region, &opts.fetch_opts).await?
        } else {
            Vec::new()
        };
        let mut downsampled = 0usize;
        if let Some(cap) = opts.alignment_opts.max_depth {
            if cap > 0 {
                let (kept, dropped) = downsample_reads(rows, cap, 200, 1);
                rows = kept;
                downsampled = dropped;
            }
        }
        sort_rows(
            &mut rows,
            opts.alignment_opts.sort_mode,
            center_bp,
            &reference_seq,
            region.start,
        );
        let junctions = extract_splice_junctions(&rows, 1);
        let packed = group_and_pack(
            &rows,
            opts.alignment_opts.group_mode,
            opts.alignment_opts.view_as_pairs,
        );
        bams.push(BamTrackSnapshot {
            display: display.clone(),
            rows,
            lanes: packed.lanes,
            total_lanes: packed.total_lanes,
            bands: packed.bands,
            mate_links: packed.mate_links,
            downsampled,
            junctions,
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

    let mut links = Vec::with_capacity(sources.links.len());
    for (display, src) in &sources.links {
        let visible = src.query(region, &opts.link_opts).await?;
        links.push(LinkTrackSnapshot {
            display: display.clone(),
            visible,
            total_record_count: src.record_count(),
        });
    }

    let mut segs = Vec::with_capacity(sources.segs.len());
    for (display, src) in &sources.segs {
        let data = src.fetch(region).await?;
        segs.push(SegTrackSnapshot {
            display: display.clone(),
            samples: data.samples,
            segments: data.segments,
        });
    }

    let mut gwas = Vec::with_capacity(sources.gwas.len());
    for (display, src) in &sources.gwas {
        let points = src.fetch(region).await?;
        gwas.push(GwasTrackSnapshot {
            display: display.clone(),
            points,
        });
    }

    Ok(RenderInputs {
        region: region.clone(),
        references: sources.references.clone(),
        cytobands: sources.cytobands.clone(),
        reference_seq,
        sequence_opts,
        variants: legacy_variants,
        vcf_tracks,
        bams,
        alignment_opts: opts.alignment_opts.clone(),
        annotations,
        signals,
        links,
        segs,
        gwas,
        render_mode: mode,
    })
}
