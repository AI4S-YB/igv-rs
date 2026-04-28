use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use igv_core::region::Region;
use igv_core::render::{RenderMode, Thresholds};
use igv_core::source::{BamSource, FastaSource, FetchOpts, RefMeta, VcfSource};
use igv_core::source::bam::AlignmentRow;
use igv_core::source::vcf::VariantRecord;

use crate::app::action::Action;
use crate::app::loader::LoadRequest;
use crate::ui::theme::{Theme, ThemePreset};

/// Minimum / maximum sizes for user-resizable tracks.
pub const ALIGNMENT_MIN_HEIGHT: u16 = 4;
pub const ALIGNMENT_MAX_HEIGHT: u16 = 60;
pub const ALIGNMENT_DEFAULT_HEIGHT: u16 = 6;
pub const COVERAGE_MIN_HEIGHT: u16 = 3;
pub const COVERAGE_MAX_HEIGHT: u16 = 20;
pub const COVERAGE_DEFAULT_HEIGHT: u16 = 5;
pub const SIGNAL_MIN_HEIGHT: u16 = 2;
pub const SIGNAL_MAX_HEIGHT: u16 = 12;
pub const SIGNAL_DEFAULT_HEIGHT: u16 = 6;

/// Translate terminal column count into a target bin count for signal fetches.
///
/// The widget aggregates with **max** when re-binning to terminal columns, so
/// 2× oversampling preserves spike detail without thrashing the bigWig zoom
/// pyramid. Clamped so very narrow terminals still get useful resolution and
/// very wide ones don't request absurd amounts of data.
pub fn signal_bins_for_width(terminal_width: u16) -> u32 {
    ((terminal_width as u32).saturating_mul(2)).clamp(64, 4096)
}

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
    pub theme_preset: ThemePreset,
    pub thresholds: Thresholds,

    pub bookmarks: HashMap<char, Region>,
    pub status: Option<StatusMessage>,

    pub command_open: bool,
    pub command_buffer: String,
    pub help_open: bool,
    /// Last terminal width seen by `draw()`. Used to size signal-track fetches
    /// so the bigWig zoom level roughly matches the column count we'll render
    /// into. Updated on every frame; resize events trigger a re-fetch.
    pub terminal_width: u16,

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
    /// to chromosome bounds. There is no fixed maximum width — the upper limit
    /// is the length of the current chromosome, so users can zoom all the way
    /// out to whole-chromosome view (loader gates heavy fetches by render mode
    /// at wide zoom — see `Loader::dispatch`).
    pub fn next_zoom(&self, zoom_in: bool, factor: f64) -> Region {
        let width = self.region.width();
        let new_width: u64 = if zoom_in {
            ((width as f64) / factor).round() as u64
        } else {
            ((width as f64) * factor).round() as u64
        };
        let chrom_len = self.current_chrom_len().unwrap_or(u64::MAX);
        let new_width = new_width.clamp(10, chrom_len.max(10));
        let center = self.region.start + width / 2;
        let new_start = center.saturating_sub(new_width / 2).max(1);
        let new_end = (new_start + new_width - 1).min(chrom_len.max(1));
        Region {
            chrom: self.region.chrom.clone(),
            start: new_start,
            end: new_end,
        }
    }

    /// Render-mode classification for the current view, used by the loader to
    /// skip heavy fetches at wide zoom and by widgets to choose presentation.
    pub fn render_mode(&self) -> RenderMode {
        self.thresholds.classify(self.region.width())
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
                self.theme_preset = self.theme_preset.next();
                self.theme = Theme::for_preset(self.theme_preset);
                self.set_status(
                    StatusKind::Info,
                    format!("theme: {}", self.theme_preset.name()),
                );
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
                let trimmed = buf.trim();
                // A bareword like "HER2" parses as a chromosome-only region,
                // but it's almost certainly a gene name. So accept the parse
                // only when the chromosome actually exists in the loaded
                // references; otherwise fall through to gene-name lookup.
                let parse = Region::parse(trimmed);
                let parsed_known = parse.as_ref().ok().map(|r| {
                    self.references.iter().any(|m| m.name == r.chrom)
                });
                match (parse, parsed_known) {
                    (Ok(r), Some(true)) => self.set_region_pending(r),
                    (parse_result, _) => match self.find_gene_region(trimmed) {
                        Some((region, label)) => {
                            self.set_status(StatusKind::Info, format!("jump → {label}"));
                            self.set_region_pending(region)
                        }
                        None => {
                            let msg = match parse_result {
                                Ok(r) => format!("unknown chrom or gene: {}", r.chrom),
                                Err(_) if self.annotations.is_empty() => {
                                    format!("invalid region: {trimmed}")
                                }
                                Err(_) => format!("not a region or gene: {trimmed}"),
                            };
                            self.set_status(StatusKind::Error, msg);
                            None
                        }
                    },
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
            Action::ToggleHelp => {
                self.help_open = !self.help_open;
                None
            }
            Action::CloseHelp => {
                self.help_open = false;
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

    /// Search loaded annotation tracks for a transcript whose gene_name,
    /// gene_id, or transcript_id matches `query` case-insensitively. On a
    /// hit, returns a region spanning the **union** of all matched
    /// transcripts on the same chromosome (so multi-isoform genes show all
    /// isoforms at once), plus a label suitable for the status line.
    fn find_gene_region(&self, query: &str) -> Option<(Region, String)> {
        if query.is_empty() {
            return None;
        }
        // First match wins; subsequent matches on the same chrom expand the
        // span (matches across chromosomes are kept on the first chrom seen).
        let mut chrom: Option<String> = None;
        let mut span: Option<(u64, u64)> = None;
        let mut label: Option<String> = None;
        for track in &self.annotations {
            for (c, tx) in track.source.find_by_name(query) {
                let Some((s, e)) = tx.span() else { continue };
                match &chrom {
                    None => {
                        chrom = Some(c);
                        span = Some((s, e));
                        label = Some(tx.name.clone());
                    }
                    Some(existing) if existing == &c => {
                        let (cs, ce) = span.unwrap();
                        span = Some((cs.min(s), ce.max(e)));
                    }
                    Some(_) => {}
                }
            }
        }
        let chrom = chrom?;
        let (s, e) = span?;
        let region = Region::new(chrom, s, e).ok()?;
        let label = label.unwrap_or_else(|| query.to_string());
        let status = format!("{label} ({region})");
        Some((region, status))
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
        let render_mode = self.render_mode();
        Some(LoadRequest {
            generation: self.generation,
            region: self.region.clone(),
            fetch_opts: FetchOpts::default(),
            signal_max_bins: signal_bins_for_width(self.terminal_width),
            render_mode,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_bins_for_width_clamps_low_and_high() {
        assert_eq!(signal_bins_for_width(0), 64);
        assert_eq!(signal_bins_for_width(20), 64);     // 20*2=40 → clamped up
        assert_eq!(signal_bins_for_width(80), 160);    // 80*2 sits in range
        assert_eq!(signal_bins_for_width(200), 400);
        assert_eq!(signal_bins_for_width(4000), 4096); // 4000*2=8000 → clamped down
    }

    #[test]
    fn classify_chromosome_scale_is_overview_only() {
        // Sanity check: the constant change in `region.rs` (no MAX_REGION_WIDTH)
        // means we can construct a chr-scale view; classification at that size
        // must land in OverviewOnly so the loader knows to skip heavy fetches.
        let t = Thresholds::default();
        assert_eq!(t.classify(248_000_000), RenderMode::OverviewOnly);
    }
}
