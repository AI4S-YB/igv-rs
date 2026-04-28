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
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            width_px: 1200,
            track_heights: TrackHeights::default(),
            theme: GraphicalTheme::igv_light(),
            title: None,
            signal_shared_max: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TrackHeights {
    pub header: u32,
    pub ruler: u32,
    pub annotation_each: u32,
    pub variants: u32,
    pub coverage: u32,
    pub signal_each: u32,
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
            ruler: 28,
            annotation_each: 36,
            variants: 24,
            coverage: 80,
            signal_each: 80,
            alignments_each: 160,
            lane_height: 12,
            gutter: 4,
            margin_left: 80,
            margin_right: 12,
        }
    }
}
