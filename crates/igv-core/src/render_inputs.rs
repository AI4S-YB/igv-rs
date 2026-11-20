//! `RenderInputs` — single bag of data needed to render one snapshot
//! (one frame's worth of all loaded tracks for the current region).

use crate::alignment::{
    AlignmentBand, AlignmentRenderOpts, MateLink,
};
use crate::genome::Cytoband;
use crate::region::Region;
use crate::render::RenderMode;
use crate::sashimi::SpliceJunction;
use crate::sequence::MotifHit;
use crate::source::gwas::GwasPoint;
use crate::source::link::VisibleLink;
use crate::source::seg::SegInterval;
use crate::source::{
    AlignmentRow, AnnotationTranscript, RefMeta, SignalBin, VariantRecord,
};

#[derive(Debug, Clone, Default)]
pub struct BamTrackSnapshot {
    pub display: String,
    pub rows: Vec<AlignmentRow>,
    /// Per-row lane index (parallel to `rows`).
    pub lanes: Vec<u32>,
    /// Total lane count (max lane index + 1, or 0 if empty).
    pub total_lanes: u16,
    pub bands: Vec<AlignmentBand>,
    pub mate_links: Vec<MateLink>,
    pub downsampled: usize,
    pub junctions: Vec<SpliceJunction>,
}

#[derive(Debug, Clone, Default)]
pub struct VcfTrackSnapshot {
    pub display: String,
    pub sample_names: Vec<String>,
    pub records: Vec<VariantRecord>,
}

#[derive(Debug, Clone, Default)]
pub struct AnnotationTrackSnapshot {
    pub display: String,
    pub transcripts: Vec<AnnotationTranscript>,
}

#[derive(Debug, Clone, Default)]
pub struct SignalTrackSnapshot {
    pub display: String,
    pub bins: Vec<SignalBin>,
}

#[derive(Debug, Clone, Default)]
pub struct LinkTrackSnapshot {
    pub display: String,
    pub visible: Vec<VisibleLink>,
    pub total_record_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SegTrackSnapshot {
    pub display: String,
    pub samples: Vec<String>,
    pub segments: Vec<SegInterval>,
}

#[derive(Debug, Clone, Default)]
pub struct GwasTrackSnapshot {
    pub display: String,
    pub points: Vec<GwasPoint>,
}

#[derive(Debug, Clone, Default)]
pub struct SequenceRenderOpts {
    pub show_complement: bool,
    pub show_translation: bool,
    pub reverse_translation: bool,
    pub motif_pattern: Option<String>,
    pub motif_hits: Vec<MotifHit>,
}

#[derive(Debug, Clone)]
pub struct RenderInputs {
    pub region: Region,
    pub references: Vec<RefMeta>,
    pub cytobands: Vec<Cytoband>,
    pub reference_seq: Vec<u8>,
    pub sequence_opts: SequenceRenderOpts,
    /// Legacy single-VCF field retained for existing callers/snapshots.
    pub variants: Vec<VariantRecord>,
    pub vcf_tracks: Vec<VcfTrackSnapshot>,
    pub bams: Vec<BamTrackSnapshot>,
    pub alignment_opts: AlignmentRenderOpts,
    pub annotations: Vec<AnnotationTrackSnapshot>,
    pub signals: Vec<SignalTrackSnapshot>,
    pub links: Vec<LinkTrackSnapshot>,
    pub segs: Vec<SegTrackSnapshot>,
    pub gwas: Vec<GwasTrackSnapshot>,
    pub render_mode: RenderMode,
}

impl Default for RenderInputs {
    fn default() -> Self {
        Self {
            region: Region::new("chr1", 1, 100).expect("valid default region"),
            references: Vec::new(),
            cytobands: Vec::new(),
            reference_seq: Vec::new(),
            sequence_opts: SequenceRenderOpts::default(),
            variants: Vec::new(),
            vcf_tracks: Vec::new(),
            bams: Vec::new(),
            alignment_opts: AlignmentRenderOpts::default(),
            annotations: Vec::new(),
            signals: Vec::new(),
            links: Vec::new(),
            segs: Vec::new(),
            gwas: Vec::new(),
            render_mode: RenderMode::DetailedReads,
        }
    }
}

impl RenderInputs {
    /// True iff every track-vec is empty (no data to render).
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
            && self.vcf_tracks.iter().all(|t| t.records.is_empty())
            && self.bams.iter().all(|t| t.rows.is_empty())
            && self.annotations.iter().all(|t| t.transcripts.is_empty())
            && self.signals.iter().all(|t| t.bins.is_empty())
            && self.links.iter().all(|t| t.visible.is_empty())
            && self.segs.iter().all(|t| t.segments.is_empty())
            && self.gwas.iter().all(|t| t.points.is_empty())
            && self.reference_seq.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_reports_empty() {
        let inputs = RenderInputs::default();
        assert!(inputs.is_empty());
    }
}
