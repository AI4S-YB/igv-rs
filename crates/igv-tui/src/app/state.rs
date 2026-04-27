use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use igv_core::region::{Region, MAX_REGION_WIDTH};
use igv_core::render::Thresholds;
use igv_core::source::{BamSource, FastaSource, FetchOpts, RefMeta, VcfSource};
use igv_core::source::bam::AlignmentRow;
use igv_core::source::vcf::VariantRecord;

use crate::app::action::Action;
use crate::app::loader::LoadRequest;
use crate::ui::theme::Theme;

/// Minimum / maximum sizes for user-resizable tracks.
pub const ALIGNMENT_MIN_HEIGHT: u16 = 4;
pub const ALIGNMENT_MAX_HEIGHT: u16 = 60;
pub const ALIGNMENT_DEFAULT_HEIGHT: u16 = 6;
pub const COVERAGE_MIN_HEIGHT: u16 = 3;
pub const COVERAGE_MAX_HEIGHT: u16 = 20;
pub const COVERAGE_DEFAULT_HEIGHT: u16 = 5;
pub const SIGNAL_MIN_HEIGHT: u16 = 2;
pub const SIGNAL_MAX_HEIGHT: u16 = 12;
pub const SIGNAL_DEFAULT_HEIGHT: u16 = 4;

/// Single owner of all UI-relevant mutable state.
pub struct AppState {
    /// Held to keep the `Arc<dyn FastaSource>` alive alongside the loader's
    /// own clone; not read directly through state.
    #[allow(dead_code)]
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub bams: Vec<BamTrack>,
    pub references: Vec<RefMeta>,

    pub region: Region,
    pub reference_seq: Vec<u8>,

    pub variants: Vec<VariantRecord>,
    /// Per-BAM rows, parallel to `bams` indices.
    pub bam_rows: Vec<Vec<AlignmentRow>>,
    /// Per-BAM lane assignment for each row, parallel to `bam_rows`.
    pub bam_lanes: Vec<Vec<u32>>,
    /// Total lane count per BAM track.
    pub bam_total_lanes: Vec<u16>,
    /// Vertical scroll applied to all alignment tracks.
    pub bam_scroll: u16,

    pub annotations: Vec<AnnotationTrack>,
    pub annotation_rows: Vec<Vec<igv_core::source::AnnotationTranscript>>,

    pub signals: Vec<SignalTrack>,
    pub signal_bins: Vec<Vec<igv_core::source::SignalBin>>,
    pub signal_shared_scale: bool,
    pub signal_track_height: u16,

    /// User-controlled track heights.
    pub alignment_height: u16,
    pub coverage_height: u16,

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
#[allow(dead_code)]
pub struct BamTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn BamSource>,
    pub fetch_opts: FetchOpts,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AnnotationTrack {
    pub path: std::path::PathBuf,
    pub display: String,
    pub source: std::sync::Arc<dyn igv_core::source::AnnotationSource>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct SignalTrack {
    pub path: std::path::PathBuf,
    pub display: String,
    pub source: std::sync::Arc<dyn igv_core::source::SignalSource>,
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
    /// Length of the current chromosome from the loaded references.
    fn current_chrom_len(&self) -> Option<u64> {
        self.references
            .iter()
            .find(|r| r.name == self.region.chrom)
            .map(|r| r.length)
    }

    /// Compute new region for forward/backward navigation, clamped to the
    /// chromosome bounds. `large=false` shifts by 1/10 of the window
    /// (fine), `large=true` by a full window (page).
    pub fn next_navigation(&self, forward: bool, large: bool) -> Region {
        let width = self.region.width();
        let step = if large {
            width.max(1)
        } else {
            (width / 10).max(1)
        };
        let chrom_len = self.current_chrom_len().unwrap_or(u64::MAX);
        let max_start = chrom_len.saturating_sub(width).max(1);
        let new_start = if forward {
            self.region.start.saturating_add(step).min(max_start).max(1)
        } else {
            self.region.start.saturating_sub(step).max(1)
        };
        let new_end = (new_start + width - 1).min(chrom_len.max(1));
        Region {
            chrom: self.region.chrom.clone(),
            start: new_start,
            end: new_end,
        }
    }

    /// Compute new region for zoom in/out around the current center, clamped
    /// to chromosome bounds.
    pub fn next_zoom(&self, zoom_in: bool, factor: f64) -> Region {
        let width = self.region.width();
        let new_width: u64 = if zoom_in {
            ((width as f64) / factor).round() as u64
        } else {
            ((width as f64) * factor).round() as u64
        };
        let new_width = new_width.clamp(10, MAX_REGION_WIDTH);
        let center = self.region.start + width / 2;
        let chrom_len = self.current_chrom_len().unwrap_or(u64::MAX);
        let new_start = center.saturating_sub(new_width / 2).max(1);
        let new_end = (new_start + new_width - 1).min(chrom_len.max(1));
        Region {
            chrom: self.region.chrom.clone(),
            start: new_start,
            end: new_end,
        }
    }
}

impl AppState {
    /// Apply an `Action`, optionally producing a new `LoadRequest` for the
    /// loader. Returns `None` when no fetch is needed (e.g. theme toggle).
    pub fn apply(&mut self, action: Action) -> Option<LoadRequest> {
        match action {
            Action::Quit => {
                self.should_quit = true;
                None
            }
            Action::ToggleTheme => {
                self.light_mode = !self.light_mode;
                self.theme = if self.light_mode {
                    Theme::light()
                } else {
                    Theme::dark()
                };
                None
            }
            Action::Move { forward, large } => {
                let r = self.next_navigation(forward, large);
                self.set_region_pending(r)
            }
            Action::Zoom { zoom_in } => {
                let r = self.next_zoom(zoom_in, 1.5);
                self.set_region_pending(r)
            }
            Action::Goto(r) => self.set_region_pending(r),
            Action::ScrollAlignments(delta) => {
                let cap = self
                    .bam_total_lanes
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0)
                    .saturating_sub(1);
                if delta > 0 {
                    self.bam_scroll =
                        self.bam_scroll.saturating_add(delta as u16).min(cap);
                } else {
                    self.bam_scroll =
                        self.bam_scroll.saturating_sub((-delta) as u16);
                }
                None
            }
            Action::ResizeAlignments(delta) => {
                self.alignment_height = if delta > 0 {
                    self.alignment_height
                        .saturating_add(delta as u16)
                        .min(ALIGNMENT_MAX_HEIGHT)
                } else {
                    self.alignment_height
                        .saturating_sub((-delta) as u16)
                        .max(ALIGNMENT_MIN_HEIGHT)
                };
                self.set_status(
                    StatusKind::Info,
                    format!("alignment height: {}", self.alignment_height),
                );
                None
            }
            Action::ResizeCoverage(delta) => {
                self.coverage_height = if delta > 0 {
                    self.coverage_height
                        .saturating_add(delta as u16)
                        .min(COVERAGE_MAX_HEIGHT)
                } else {
                    self.coverage_height
                        .saturating_sub((-delta) as u16)
                        .max(COVERAGE_MIN_HEIGHT)
                };
                self.set_status(
                    StatusKind::Info,
                    format!("coverage height: {}", self.coverage_height),
                );
                None
            }
            Action::OpenCommand => {
                self.command_open = true;
                self.command_buffer.clear();
                None
            }
            Action::CommandSubmit(buf) => {
                self.command_open = false;
                self.command_buffer.clear();
                match Region::parse(&buf) {
                    Ok(r) => self.set_region_pending(r),
                    Err(e) => {
                        self.set_status(StatusKind::Error, format!("parse: {e}"));
                        None
                    }
                }
            }
            Action::CommandCancel => {
                self.command_open = false;
                self.command_buffer.clear();
                None
            }
            Action::SetBookmark(c) => {
                self.bookmarks.insert(c, self.region.clone());
                self.set_status(StatusKind::Info, format!("bookmark '{}' set", c));
                None
            }
            Action::JumpBookmark(c) => match self.bookmarks.get(&c).cloned() {
                Some(r) => self.set_region_pending(r),
                None => {
                    self.set_status(StatusKind::Warning, format!("no bookmark '{}'", c));
                    None
                }
            },
            Action::ToggleSignalSharedScale => {
                self.signal_shared_scale = !self.signal_shared_scale;
                let mode = if self.signal_shared_scale { "shared" } else { "per-track" };
                self.set_status(StatusKind::Info, format!("signal scale: {mode}"));
                None
            }
            Action::ResizeSignal(delta) => {
                self.signal_track_height = if delta > 0 {
                    self.signal_track_height
                        .saturating_add(delta as u16)
                        .min(SIGNAL_MAX_HEIGHT)
                } else {
                    self.signal_track_height
                        .saturating_sub((-delta) as u16)
                        .max(SIGNAL_MIN_HEIGHT)
                };
                self.set_status(
                    StatusKind::Info,
                    format!("signal height: {}", self.signal_track_height),
                );
                None
            }
            Action::None => None,
        }
    }

    fn set_region_pending(&mut self, region: Region) -> Option<LoadRequest> {
        self.region = region;
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        // Clear stale data so widgets don't render new reads against the old
        // reference window (causing transient phantom mismatches) until the
        // new fetches land.
        self.reference_seq.clear();
        for rows in &mut self.bam_rows {
            rows.clear();
        }
        for lanes in &mut self.bam_lanes {
            lanes.clear();
        }
        for total in &mut self.bam_total_lanes {
            *total = 0;
        }
        self.bam_scroll = 0;
        for rows in &mut self.annotation_rows {
            rows.clear();
        }
        for bins in &mut self.signal_bins {
            bins.clear();
        }
        self.variants.clear();
        Some(LoadRequest {
            generation: self.generation,
            region: self.region.clone(),
            fetch_opts: FetchOpts::default(),
        })
    }

    pub fn set_status(&mut self, kind: StatusKind, text: impl Into<String>) {
        self.status = Some(StatusMessage {
            kind,
            text: text.into(),
            set_at: std::time::Instant::now(),
        });
    }
}
