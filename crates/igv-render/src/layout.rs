//! Pixel-based layout for one snapshot. Mirrors igv-rs's track order
//! (ruler → annotations → variants → coverage → signal → alignments)
//! but in px units rather than terminal cells.

use igv_core::render_inputs::RenderInputs;

use crate::options::TrackHeights;

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub total_width: u32,
    pub total_height: u32,
    pub plot: PlotMetrics,
    pub header: Rect,
    pub ruler: Rect,
    pub annotations: Vec<Rect>,
    pub links: Vec<Rect>,
    pub variants: Option<Rect>,
    pub coverage: Option<Rect>,
    pub signals: Vec<Rect>,
    pub alignments: Vec<Rect>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlotMetrics {
    pub margin_left: u32,
    pub margin_right: u32,
    pub plot_x0: u32,
    pub plot_x1: u32,
    pub plot_width: u32,
    pub region_start: u64,
    /// `region.end - region.start` (one less than width). Makes
    /// `bp_to_px` map `region.start → plot_x0` and `region.end → plot_x1`.
    /// Zero when the region is a single base — guarded in `bp_to_px`.
    pub region_width_bp: u64,
}

impl PlotMetrics {
    /// Map a 1-based bp position to an x px coordinate within the plot
    /// area. Positions before `region_start` clamp to `plot_x0`; positions
    /// past the right edge clamp to `plot_x1`.
    pub fn bp_to_px(&self, bp: u64) -> f64 {
        if self.region_width_bp == 0 {
            return self.plot_x0 as f64;
        }
        let off = bp.saturating_sub(self.region_start) as f64;
        let frac = (off / self.region_width_bp as f64).clamp(0.0, 1.0);
        self.plot_x0 as f64 + frac * self.plot_width as f64
    }
}

pub fn compute(inputs: &RenderInputs, width_px: u32, h: &TrackHeights) -> Layout {
    let margin_left = h.margin_left;
    let margin_right = h.margin_right;
    let plot_x0 = margin_left;
    let plot_x1 = width_px.saturating_sub(margin_right);
    let plot_width = plot_x1.saturating_sub(plot_x0);
    let plot = PlotMetrics {
        margin_left,
        margin_right,
        plot_x0,
        plot_x1,
        plot_width,
        region_start: inputs.region.start,
        region_width_bp: inputs.region.end - inputs.region.start,
    };

    let mut y: u32 = 0;
    let header = Rect { x: 0, y, w: width_px, h: h.header };
    y += h.header + h.gutter;
    let ruler = Rect { x: 0, y, w: width_px, h: h.ruler };
    y += h.ruler + h.gutter;

    let mut annotations = Vec::with_capacity(inputs.annotations.len());
    for _ in &inputs.annotations {
        annotations.push(Rect { x: 0, y, w: width_px, h: h.annotation_each });
        y += h.annotation_each + h.gutter;
    }

    let mut links = Vec::with_capacity(inputs.links.len());
    for _ in &inputs.links {
        links.push(Rect { x: 0, y, w: width_px, h: h.link_each });
        y += h.link_each + h.gutter;
    }

    let variants = if !inputs.variants.is_empty() {
        let r = Rect { x: 0, y, w: width_px, h: h.variants };
        y += h.variants + h.gutter;
        Some(r)
    } else {
        None
    };

    let coverage = if !inputs.bams.is_empty() {
        let r = Rect { x: 0, y, w: width_px, h: h.coverage };
        y += h.coverage + h.gutter;
        Some(r)
    } else {
        None
    };

    let mut signals = Vec::with_capacity(inputs.signals.len());
    for _ in &inputs.signals {
        signals.push(Rect { x: 0, y, w: width_px, h: h.signal_each });
        y += h.signal_each + h.gutter;
    }

    let mut alignments = Vec::with_capacity(inputs.bams.len());
    for _ in &inputs.bams {
        alignments.push(Rect { x: 0, y, w: width_px, h: h.alignments_each });
        y += h.alignments_each + h.gutter;
    }

    let total_height = y;

    Layout {
        total_width: width_px,
        total_height,
        plot,
        header,
        ruler,
        annotations,
        links,
        variants,
        coverage,
        signals,
        alignments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use igv_core::region::Region;
    use igv_core::render::RenderMode;
    use igv_core::render_inputs::RenderInputs;

    fn empty_inputs() -> RenderInputs {
        RenderInputs {
            region: Region::new("chr1", 1, 100).unwrap(),
            references: vec![],
            reference_seq: vec![],
            variants: vec![],
            bams: vec![],
            annotations: vec![],
            signals: vec![],
            links: vec![],
            render_mode: RenderMode::DetailedReads,
        }
    }

    #[test]
    fn empty_layout_has_only_header_and_ruler() {
        let l = compute(&empty_inputs(), 1200, &TrackHeights::default());
        assert!(l.annotations.is_empty());
        assert!(l.coverage.is_none());
        assert!(l.signals.is_empty());
        assert!(l.alignments.is_empty());
        // header(40) + gutter(4) + ruler(28) + gutter(4) = 76
        assert_eq!(l.total_height, 76);
    }

    #[test]
    fn bp_to_px_maps_endpoints() {
        let l = compute(&empty_inputs(), 1200, &TrackHeights::default());
        // region 1..=100, divisor = end - start = 99, plot covers x=80..1188 (1108 px)
        assert!((l.plot.bp_to_px(1) - 80.0).abs() < 1e-6);
        assert!((l.plot.bp_to_px(100) - 1188.0).abs() < 1e-6);
    }

    #[test]
    fn bp_to_px_clamps_oob() {
        let l = compute(&empty_inputs(), 1200, &TrackHeights::default());
        assert!((l.plot.bp_to_px(0) - 80.0).abs() < 1e-6);
        assert!((l.plot.bp_to_px(10_000) - 1188.0).abs() < 1e-6);
    }

    #[test]
    fn link_layout_sits_between_annotations_and_variants() {
        use igv_core::render_inputs::{AnnotationTrackSnapshot, LinkTrackSnapshot, RenderInputs};
        use igv_core::source::link::{LinkRecord, LinkScope, VisibleLink};
        use igv_core::source::annotation::{
            AnnotationBlock, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
        };
        use std::sync::Arc;

        let inputs = RenderInputs {
            region: igv_core::region::Region::new("chr1", 1, 1000).unwrap(),
            references: vec![],
            reference_seq: vec![],
            variants: vec![],
            bams: vec![],
            annotations: vec![AnnotationTrackSnapshot {
                display: "g.gff".into(),
                transcripts: vec![AnnotationTranscript {
                    name: "g".into(),
                    id: "t".into(),
                    gene_id: None,
                    strand: Strand::Forward,
                    blocks: vec![AnnotationBlock {
                        start: 100,
                        end: 200,
                        kind: BlockKind::Exon,
                    }],
                    kind: TranscriptKind::Mrna,
                }],
            }],
            signals: vec![],
            links: vec![LinkTrackSnapshot {
                display: "l.bedpe".into(),
                visible: vec![VisibleLink {
                    record: LinkRecord {
                        chrom_a: Arc::from("chr1"),
                        start_a: 100,
                        end_a: 200,
                        chrom_b: Arc::from("chr1"),
                        start_b: 700,
                        end_b: 800,
                        name: None,
                        score: Some(1.0),
                        strand_a: Strand::Forward,
                        strand_b: Strand::Reverse,
                    },
                    scope: LinkScope::BothIn,
                }],
                total_record_count: 1,
            }],
            render_mode: igv_core::render::RenderMode::DetailedReads,
        };
        let l = compute(&inputs, 1200, &TrackHeights::default());
        assert_eq!(l.links.len(), 1);
        assert_eq!(l.links[0].h, TrackHeights::default().link_each);
        assert!(l.links[0].y > l.annotations[0].y);
        assert!(l.links[0].y >= l.annotations[0].y + l.annotations[0].h);
    }
}
