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
            Action::MoveForward | Action::MoveBackward => {
                let r = self.next_navigation(matches!(action, Action::MoveForward));
                self.set_region_pending(r)
            }
            Action::Zoom { zoom_in } => {
                let r = self.next_zoom(zoom_in, 1.5);
                self.set_region_pending(r)
            }
            Action::Goto(r) => self.set_region_pending(r),
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
            Action::None => None,
        }
    }

    fn set_region_pending(&mut self, region: Region) -> Option<LoadRequest> {
        self.region = region;
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
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
