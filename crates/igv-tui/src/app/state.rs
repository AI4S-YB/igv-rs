use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use igv_core::region::{Region, MAX_REGION_WIDTH};
use igv_core::render::Thresholds;
use igv_core::source::{BamSource, FastaSource, FetchOpts, RefMeta, VcfSource};
use igv_core::source::bam::AlignmentRow;
use igv_core::source::vcf::VariantRecord;

use crate::ui::theme::Theme;

/// Single owner of all UI-relevant mutable state.
pub struct AppState {
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub bams: Vec<BamTrack>,
    pub references: Vec<RefMeta>,

    pub region: Region,
    pub reference_seq: Vec<u8>,

    pub variants: Vec<VariantRecord>,
    /// Per-BAM rows, parallel to `bams` indices.
    pub bam_rows: Vec<Vec<AlignmentRow>>,

    pub theme: Theme,
    pub light_mode: bool,
    pub thresholds: Thresholds,

    pub bookmarks: HashMap<char, Region>,
    pub status: Option<StatusMessage>,

    pub command_open: bool,
    pub command_buffer: String,

    pub generation: u64,
    pub loading: bool,
    pub should_quit: bool,
}

// Note: cannot derive `Debug` because `BamSource: Send + Sync` does not require
// `Debug`. Plan adaptation: drop the `Debug` derive on `BamTrack`.
#[derive(Clone)]
pub struct BamTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn BamSource>,
    pub fetch_opts: FetchOpts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub kind: StatusKind,
    pub text: String,
    pub set_at: std::time::Instant,
}

impl AppState {
    /// Move by the nav_overlap fraction (default 50%).
    pub fn nav_step(&self) -> u64 {
        let w = self.region.width();
        ((w as f64) * 0.5) as u64
    }

    /// Compute new region for forward/backward navigation.
    pub fn next_navigation(&self, forward: bool) -> Region {
        let step = self.nav_step().max(1);
        let width = self.region.width();
        let new_start = if forward {
            self.region.start.saturating_add(step).max(1)
        } else {
            self.region.start.saturating_sub(step).max(1)
        };
        let new_end = new_start + width - 1;
        Region {
            chrom: self.region.chrom.clone(),
            start: new_start,
            end: new_end,
        }
    }

    /// Compute new region for zoom in/out around the current center.
    pub fn next_zoom(&self, zoom_in: bool, factor: f64) -> Region {
        let width = self.region.width();
        let new_width: u64 = if zoom_in {
            ((width as f64) / factor).round() as u64
        } else {
            ((width as f64) * factor).round() as u64
        };
        let new_width = new_width.clamp(10, MAX_REGION_WIDTH);
        let center = self.region.start + width / 2;
        let new_start = center.saturating_sub(new_width / 2).max(1);
        let new_end = new_start + new_width - 1;
        Region {
            chrom: self.region.chrom.clone(),
            start: new_start,
            end: new_end,
        }
    }
}
