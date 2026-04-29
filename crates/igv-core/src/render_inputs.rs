//! `RenderInputs` — single bag of data needed to render one snapshot
//! (one frame's worth of all loaded tracks for the current region).
//!
//! Both the TUI interactive snapshot path (filling from `AppState`) and
//! the headless batch path (filling from `collect_render_inputs`) build
//! one of these and hand it to `igv-render`.

use crate::region::Region;
use crate::render::RenderMode;
use crate::source::{
    AlignmentRow, AnnotationTranscript, RefMeta, SignalBin, VariantRecord,
};
use crate::source::link::VisibleLink;

#[derive(Debug, Clone)]
pub struct BamTrackSnapshot {
    pub display: String,
    pub rows: Vec<AlignmentRow>,
    /// Per-row lane index (parallel to `rows`).
    pub lanes: Vec<u32>,
    /// Total lane count (max lane index + 1, or 0 if empty).
    pub total_lanes: u16,
}

#[derive(Debug, Clone)]
pub struct AnnotationTrackSnapshot {
    pub display: String,
    pub transcripts: Vec<AnnotationTranscript>,
}

#[derive(Debug, Clone)]
pub struct SignalTrackSnapshot {
    pub display: String,
    pub bins: Vec<SignalBin>,
}

#[derive(Debug, Clone)]
pub struct LinkTrackSnapshot {
    pub display: String,
    pub visible: Vec<VisibleLink>,
    pub total_record_count: usize,
}

#[derive(Debug, Clone)]
pub struct RenderInputs {
    pub region: Region,
    pub references: Vec<RefMeta>,
    pub reference_seq: Vec<u8>,
    pub variants: Vec<VariantRecord>,
    pub bams: Vec<BamTrackSnapshot>,
    pub annotations: Vec<AnnotationTrackSnapshot>,
    pub signals: Vec<SignalTrackSnapshot>,
    pub links: Vec<LinkTrackSnapshot>,
    pub render_mode: RenderMode,
}

impl RenderInputs {
    /// True iff every track-vec is empty (no data to render).
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
            && self.bams.iter().all(|t| t.rows.is_empty())
            && self.annotations.iter().all(|t| t.transcripts.is_empty())
            && self.signals.iter().all(|t| t.bins.is_empty())
            && self.links.iter().all(|t| t.visible.is_empty())
            && self.reference_seq.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Region;
    use crate::render::RenderMode;

    #[test]
    fn empty_inputs_reports_empty() {
        let inputs = RenderInputs {
            region: Region::new("chr1", 1, 100).unwrap(),
            references: vec![],
            reference_seq: vec![],
            variants: vec![],
            bams: vec![],
            annotations: vec![],
            signals: vec![],
            links: vec![],
            render_mode: RenderMode::DetailedReads,
        };
        assert!(inputs.is_empty());
    }

    #[test]
    fn empty_inputs_reports_empty_with_links() {
        let inputs = RenderInputs {
            region: Region::new("chr1", 1, 100).unwrap(),
            references: vec![],
            reference_seq: vec![],
            variants: vec![],
            bams: vec![],
            annotations: vec![],
            signals: vec![],
            links: vec![],
            render_mode: RenderMode::DetailedReads,
        };
        assert!(inputs.is_empty());
    }
}
