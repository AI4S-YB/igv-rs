//! Renderer-tunable parameters.

use crate::theme::GraphicalTheme;

#[derive(Debug, Clone)]
pub struct SvgOptions {
    pub width_px: u32,
    pub track_heights: TrackHeights,
    pub theme: GraphicalTheme,
    /// Optional title text for the header band. None → use `region` formatting.
    pub title: Option<String>,
    /// Honor a per-track signal max (interactive snapshots can pipe in
    /// the current `signal_shared_scale` toggle's max). `None` → per-track.
    pub signal_shared_max: Option<f32>,
    /// Opt-in chromosome cytoband ideogram below the header.
    pub show_ideogram: bool,
    /// Opt-in sequence / translation / motif track when zoomed in (`<= 240 bp`).
    pub show_sequence: bool,
    /// Opt-in SNV allele-fraction stacked bars overlaid on coverage depth.
    pub show_coverage_snv: bool,
    /// Opt-in RNA-seq Sashimi splice-junction arc tracks for BAMs with junctions.
    pub show_sashimi: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            width_px: 1200,
            track_heights: TrackHeights::default(),
            theme: GraphicalTheme::igv_light(),
            title: None,
            signal_shared_max: None,
            show_ideogram: false,
            show_sequence: false,
            show_coverage_snv: false,
            show_sashimi: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TrackHeights {
    pub header: u32,
    pub ideogram: u32,
    pub ruler: u32,
    pub sequence: u32,
    pub annotation_each: u32,
    pub variants: u32,
    pub coverage: u32,
    pub sashimi_each: u32,
    pub signal_each: u32,
    pub link_each: u32,
    pub seg_each: u32,
    pub gwas_each: u32,
    pub alignments_each: u32,
    pub lane_height: u32,
    pub gutter: u32,
    /// Left margin reserved for track labels (px).
    pub margin_left: u32,
    /// Right margin (px).
    pub margin_right: u32,
}

impl Default for TrackHeights {
    fn default() -> Self {
        Self {
            header: 40,
            ideogram: 18,
            ruler: 28,
            sequence: 44,
            annotation_each: 36,
            variants: 24,
            coverage: 80,
            sashimi_each: 70,
            signal_each: 80,
            link_each: 100,
            seg_each: 90,
            gwas_each: 100,
            alignments_each: 160,
            lane_height: 12,
            gutter: 4,
            margin_left: 80,
            margin_right: 12,
        }
    }
}
