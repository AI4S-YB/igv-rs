use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use igv_core::alignment::{
    downsample_reads, group_and_pack, sort_rows, AlignmentBand, AlignmentColorMode,
    AlignmentRenderOpts, MateLink,
};
use igv_core::genome::{ChromAliases, Cytoband};
use igv_core::pedigree::Pedigree;
use igv_core::region::Region;
use igv_core::render::{RenderMode, Thresholds};
use igv_core::render_inputs::{RenderInputs, SequenceRenderOpts};
use igv_core::sashimi::{extract_splice_junctions, SpliceJunction};
use igv_core::sequence::search_iupac_motif;
use igv_core::source::bam::AlignmentRow;
use igv_core::source::gwas::{GwasPoint, GwasSource};
use igv_core::source::seg::{SegSource, SegTrackData};
use igv_core::source::vcf::VariantRecord;
use igv_core::source::{BamSource, FastaSource, FetchOpts, RefMeta, SignalSummary, VcfSource};
use igv_render::TerminalGraphicsProtocol;

use crate::app::action::{Action, SnapshotFormat};
use crate::app::loader::LoadRequest;
use crate::ui::theme::{Theme, ThemePreset};

#[derive(Debug, Clone)]
pub struct SnapshotJob {
    pub path: Option<PathBuf>,
    pub format: SnapshotFormat,
}

#[derive(Debug, Clone)]
pub enum AsyncCommand {
    LoadTrack(String),
    LoadGenome(String),
    LoadSession(PathBuf),
    SaveSession(PathBuf),
    BlatQuery(String),
}

#[derive(Debug, Clone)]
pub struct InspectorPopup {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

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
pub const LINK_MIN_HEIGHT: u16 = 3;
pub const LINK_MAX_HEIGHT: u16 = 16;
pub const LINK_DEFAULT_HEIGHT: u16 = 6;

pub fn signal_bins_for_width(terminal_width: u16) -> u32 {
    ((terminal_width as u32).saturating_mul(2)).clamp(64, 4096)
}

#[derive(Debug)]
struct EmptyFasta;

#[async_trait]
impl FastaSource for EmptyFasta {
    async fn references(&self) -> igv_core::error::Result<Vec<RefMeta>> {
        Ok(Vec::new())
    }
    async fn fetch(&self, _region: &Region) -> igv_core::error::Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

/// Single owner of all UI-relevant mutable state.
pub struct AppState {
    pub fasta: Arc<dyn FastaSource>,
    pub genome_id: Option<String>,
    pub cytobands: Vec<Cytoband>,
    pub chrom_aliases: ChromAliases,
    pub pedigree: Option<Arc<Pedigree>>,
    pub auth_token: Option<String>,

    /// Legacy first-VCF handle retained alongside multi-VCF `vcf_tracks`.
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub vcf_tracks: Vec<VcfTrack>,
    pub vcf_track_rows: Vec<Vec<VariantRecord>>,

    pub bams: Vec<BamTrack>,
    pub references: Vec<RefMeta>,

    pub region: Region,
    pub panes: Vec<Region>,
    pub active_pane: usize,
    pub pane_snapshots: Vec<Option<RenderInputs>>,

    pub reference_seq: Vec<u8>,
    pub sequence_opts: SequenceRenderOpts,

    pub variants: Vec<VariantRecord>,
    pub bam_raw_rows: Vec<Vec<AlignmentRow>>,
    pub bam_rows: Vec<Vec<AlignmentRow>>,
    pub bam_lanes: Vec<Vec<u32>>,
    pub bam_total_lanes: Vec<u16>,
    pub bam_bands: Vec<Vec<AlignmentBand>>,
    pub bam_mate_links: Vec<Vec<MateLink>>,
    pub bam_downsampled: Vec<usize>,
    pub bam_junctions: Vec<Vec<SpliceJunction>>,
    pub bam_scroll: u16,
    pub alignment_opts: AlignmentRenderOpts,
    pub bam_tag: Option<String>,

    pub annotations: Vec<AnnotationTrack>,
    pub annotation_rows: Vec<Vec<igv_core::source::AnnotationTranscript>>,

    pub signals: Vec<SignalTrack>,
    pub signal_bins: Vec<Vec<igv_core::source::SignalBin>>,
    pub signal_shared_scale: bool,
    pub signal_summary: SignalSummary,
    pub signal_track_height: u16,

    pub links: Vec<LinkTrack>,
    pub link_records: Vec<Vec<igv_core::source::link::VisibleLink>>,
    pub link_track_height: u16,
    pub link_min_score: Option<f64>,

    pub segs: Vec<SegTrack>,
    pub seg_data: Vec<SegTrackData>,
    pub gwas: Vec<GwasTrack>,
    pub gwas_points: Vec<Vec<GwasPoint>>,

    pub show_sashimi: bool,
    pub alignment_height: u16,
    pub coverage_height: u16,

    pub theme: Theme,
    pub theme_preset: ThemePreset,
    pub thresholds: Thresholds,

    pub bookmarks: HashMap<char, Region>,
    pub status: Option<StatusMessage>,
    pub inspector: Option<InspectorPopup>,

    pub command_open: bool,
    pub command_buffer: String,
    pub help_open: bool,
    pub terminal_width: u16,

    pub graphics_protocol: TerminalGraphicsProtocol,
    pub graphics_overlay: bool,

    pub pending_snapshot: Option<SnapshotJob>,
    pub pending_circular: Option<PathBuf>,
    pub pending_async: Option<AsyncCommand>,

    pub generation: u64,
    pub loading: bool,
    pub loaded_count: usize,
    pub should_quit: bool,
}

impl Default for AppState {
    fn default() -> Self {
        let region = Region::new("chr1", 1, 1000).expect("valid default region");
        Self {
            fasta: Arc::new(EmptyFasta),
            genome_id: None,
            cytobands: Vec::new(),
            chrom_aliases: ChromAliases::default(),
            pedigree: None,
            auth_token: None,
            vcf: None,
            vcf_tracks: Vec::new(),
            vcf_track_rows: Vec::new(),
            bams: Vec::new(),
            references: Vec::new(),
            region: region.clone(),
            panes: vec![region],
            active_pane: 0,
            pane_snapshots: vec![None],
            reference_seq: Vec::new(),
            sequence_opts: SequenceRenderOpts::default(),
            variants: Vec::new(),
            bam_raw_rows: Vec::new(),
            bam_rows: Vec::new(),
            bam_lanes: Vec::new(),
            bam_total_lanes: Vec::new(),
            bam_bands: Vec::new(),
            bam_mate_links: Vec::new(),
            bam_downsampled: Vec::new(),
            bam_junctions: Vec::new(),
            bam_scroll: 0,
            alignment_opts: AlignmentRenderOpts::default(),
            bam_tag: None,
            annotations: Vec::new(),
            annotation_rows: Vec::new(),
            signals: Vec::new(),
            signal_bins: Vec::new(),
            signal_shared_scale: false,
            signal_summary: SignalSummary::Max,
            signal_track_height: SIGNAL_DEFAULT_HEIGHT,
            links: Vec::new(),
            link_records: Vec::new(),
            link_track_height: LINK_DEFAULT_HEIGHT,
            link_min_score: None,
            segs: Vec::new(),
            seg_data: Vec::new(),
            gwas: Vec::new(),
            gwas_points: Vec::new(),
            show_sashimi: false,
            alignment_height: ALIGNMENT_DEFAULT_HEIGHT,
            coverage_height: COVERAGE_DEFAULT_HEIGHT,
            theme: Theme::dark(),
            theme_preset: ThemePreset::Dark,
            thresholds: Thresholds::default(),
            bookmarks: HashMap::new(),
            status: None,
            inspector: None,
            command_open: false,
            command_buffer: String::new(),
            help_open: false,
            terminal_width: 0,
            graphics_protocol: TerminalGraphicsProtocol::Auto,
            graphics_overlay: false,
            pending_snapshot: None,
            pending_circular: None,
            pending_async: None,
            generation: 0,
            loading: false,
            loaded_count: 0,
            should_quit: false,
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct BamTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn BamSource>,
    pub fetch_opts: FetchOpts,
}

#[derive(Clone)]
pub struct VcfTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn VcfSource>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct AnnotationTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn igv_core::source::AnnotationSource>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct SignalTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn igv_core::source::SignalSource>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct LinkTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn igv_core::source::LinkSource>,
}

#[derive(Clone)]
pub struct SegTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn SegSource>,
}

#[derive(Clone)]
pub struct GwasTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn GwasSource>,
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

pub(crate) fn parse_snapshot_command(trimmed: &str) -> Option<(PathBuf, SnapshotFormat)> {
    let body = trimmed.strip_prefix(':').unwrap_or(trimmed);
    let rest = body
        .strip_prefix("snapshot ")
        .or_else(|| body.strip_prefix("snap "))?;
    let path = PathBuf::from(rest.trim());
    if path.as_os_str().is_empty() {
        return None;
    }
    let format = SnapshotFormat::from_path(&path);
    Some((path, format))
}

impl AppState {
    fn current_chrom_len(&self) -> Option<u64> {
        let avail: Vec<String> = self.references.iter().map(|r| r.name.clone()).collect();
        let target = self
            .chrom_aliases
            .resolve(&self.region.chrom, &avail)
            .unwrap_or(&self.region.chrom);
        self.references
            .iter()
            .find(|r| r.name == target)
            .map(|r| r.length)
    }

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

    pub fn render_mode(&self) -> RenderMode {
        self.thresholds.classify(self.region.width())
    }

    pub fn expected_loads(&self) -> usize {
        let suppress_overview = matches!(self.render_mode(), RenderMode::OverviewOnly);
        let extra_vcfs = if !suppress_overview {
            self.vcf_tracks.len()
        } else {
            0
        };
        expected_loads_for(
            self.render_mode(),
            self.bams.len(),
            self.vcf.is_some(),
            self.annotations.len(),
            self.signals.len(),
            self.links.len(),
        ) + extra_vcfs
            + self.segs.len()
            + self.gwas.len()
    }

    pub fn recompute_bam_layout(&mut self, bam_index: usize) {
        let Some(raw) = self.bam_raw_rows.get(bam_index).cloned() else {
            return;
        };
        let junctions = extract_splice_junctions(&raw, 1);
        let max_depth = self.alignment_opts.max_depth.unwrap_or(0);
        let (mut rows, downsampled) = downsample_reads(raw, max_depth, 500, 42);
        let center = self.region.start + self.region.width() / 2;
        sort_rows(
            &mut rows,
            self.alignment_opts.sort_mode,
            center,
            &self.reference_seq,
            self.region.start,
        );
        let packed = group_and_pack(
            &rows,
            self.alignment_opts.group_mode,
            self.alignment_opts.view_as_pairs,
        );
        if let Some(s) = self.bam_rows.get_mut(bam_index) {
            *s = rows;
        }
        if let Some(s) = self.bam_lanes.get_mut(bam_index) {
            *s = packed.lanes;
        }
        if let Some(s) = self.bam_total_lanes.get_mut(bam_index) {
            *s = packed.total_lanes;
        }
        if let Some(s) = self.bam_bands.get_mut(bam_index) {
            *s = packed.bands;
        }
        if let Some(s) = self.bam_mate_links.get_mut(bam_index) {
            *s = packed.mate_links;
        }
        if let Some(s) = self.bam_downsampled.get_mut(bam_index) {
            *s = downsampled;
        }
        if let Some(s) = self.bam_junctions.get_mut(bam_index) {
            *s = junctions;
        }
    }

    pub fn recompute_all_bams(&mut self) {
        let n = self.bams.len();
        for i in 0..n {
            self.recompute_bam_layout(i);
        }
    }

    pub fn recompute_motif_hits(&mut self) {
        self.sequence_opts.motif_hits = match &self.sequence_opts.motif_pattern {
            Some(pat) if !self.reference_seq.is_empty() => {
                search_iupac_motif(pat, &self.reference_seq, self.region.start).unwrap_or_default()
            }
            _ => Vec::new(),
        };
    }

    pub fn ensure_track_vecs(&mut self) {
        let b = self.bams.len();
        self.bam_raw_rows.resize_with(b, Vec::new);
        self.bam_rows.resize_with(b, Vec::new);
        self.bam_lanes.resize_with(b, Vec::new);
        self.bam_total_lanes.resize(b, 0);
        self.bam_bands.resize_with(b, Vec::new);
        self.bam_mate_links.resize_with(b, Vec::new);
        self.bam_downsampled.resize(b, 0);
        self.bam_junctions.resize_with(b, Vec::new);
        self.vcf_track_rows.resize_with(self.vcf_tracks.len(), Vec::new);
        self.annotation_rows
            .resize_with(self.annotations.len(), Vec::new);
        self.signal_bins.resize_with(self.signals.len(), Vec::new);
        self.link_records.resize_with(self.links.len(), Vec::new);
        self.seg_data
            .resize_with(self.segs.len(), SegTrackData::default);
        self.gwas_points.resize_with(self.gwas.len(), Vec::new);
        if self.panes.is_empty() {
            self.panes.push(self.region.clone());
            self.active_pane = 0;
        }
        self.pane_snapshots.resize_with(self.panes.len(), || None);
    }
}

pub fn expected_loads_for(
    mode: RenderMode,
    n_bams: usize,
    has_vcf: bool,
    n_annotations: usize,
    n_signals: usize,
    n_links: usize,
) -> usize {
    let suppress_overview = matches!(mode, RenderMode::OverviewOnly);
    let vcf = if has_vcf && !suppress_overview { 1 } else { 0 };
    1 + n_bams + vcf + n_annotations + n_signals + n_links
}

impl AppState {
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
            Action::NextPane => {
                if self.panes.len() > 1 {
                    let prev_snap = crate::snapshot::writer::inputs_from_state(self);
                    if let Some(slot) = self.pane_snapshots.get_mut(self.active_pane) {
                        *slot = Some(prev_snap);
                    }
                    self.active_pane = (self.active_pane + 1) % self.panes.len();
                    let next = self.panes[self.active_pane].clone();
                    self.set_status(
                        StatusKind::Info,
                        format!("pane {}/{} → {next}", self.active_pane + 1, self.panes.len()),
                    );
                    self.set_region_pending(next)
                } else {
                    None
                }
            }
            Action::ToggleSashimi => {
                self.show_sashimi = !self.show_sashimi;
                self.set_status(
                    StatusKind::Info,
                    format!("sashimi: {}", if self.show_sashimi { "on" } else { "off" }),
                );
                None
            }
            Action::TogglePairs => {
                self.alignment_opts.view_as_pairs = !self.alignment_opts.view_as_pairs;
                self.recompute_all_bams();
                self.set_status(
                    StatusKind::Info,
                    format!(
                        "view-as-pairs: {}",
                        if self.alignment_opts.view_as_pairs { "on" } else { "off" }
                    ),
                );
                None
            }
            Action::CycleColorMode => {
                self.alignment_opts.color_mode = self.alignment_opts.color_mode.next();
                if self.alignment_opts.color_mode == AlignmentColorMode::Tag
                    && self.bam_tag.is_none()
                {
                    self.bam_tag = Some("HP".to_string());
                }
                self.set_status(
                    StatusKind::Info,
                    format!("color-by: {}", self.alignment_opts.color_mode.name()),
                );
                None
            }
            Action::ToggleTranslation => {
                self.sequence_opts.show_translation = !self.sequence_opts.show_translation;
                self.set_status(
                    StatusKind::Info,
                    format!(
                        "3-frame translation: {}",
                        if self.sequence_opts.show_translation {
                            "on"
                        } else {
                            "off"
                        }
                    ),
                );
                None
            }
            Action::ToggleGraphics => {
                self.graphics_overlay = !self.graphics_overlay;
                self.set_status(
                    StatusKind::Info,
                    format!(
                        "inline graphics ({}): {}",
                        self.graphics_protocol.name(),
                        if self.graphics_overlay { "on" } else { "off" }
                    ),
                );
                None
            }
            Action::CloseInspector => {
                self.inspector = None;
                None
            }
            Action::MouseClick { .. } | Action::MouseDrag { .. } => None,
            Action::ScrollAlignments(delta) => {
                let cap = self
                    .bam_total_lanes
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0)
                    .saturating_sub(1);
                if delta > 0 {
                    self.bam_scroll = self.bam_scroll.saturating_add(delta as u16).min(cap);
                } else {
                    self.bam_scroll = self.bam_scroll.saturating_sub((-delta) as u16);
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
                crate::app::commands::execute_command(self, &buf)
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
                let mode = if self.signal_shared_scale {
                    "shared"
                } else {
                    "per-track"
                };
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
            Action::ResizeLink(delta) => {
                self.link_track_height = if delta > 0 {
                    self.link_track_height
                        .saturating_add(delta as u16)
                        .min(LINK_MAX_HEIGHT)
                } else {
                    self.link_track_height
                        .saturating_sub((-delta) as u16)
                        .max(LINK_MIN_HEIGHT)
                };
                self.set_status(
                    StatusKind::Info,
                    format!("link height: {}", self.link_track_height),
                );
                None
            }
            Action::SaveSnapshot { path, format } => {
                if self.loading {
                    self.set_status(StatusKind::Warning, "snapshot: still loading, try again");
                } else {
                    self.pending_snapshot = Some(SnapshotJob { path, format });
                }
                None
            }
            Action::OpenBrowser | Action::None => None,
        }
    }

    pub fn find_gene_region(&self, query: &str) -> Option<(Region, String)> {
        let sources: Vec<_> = self.annotations.iter().map(|t| t.source.clone()).collect();
        let (region, label) = igv_core::source::annotation::find_by_name_union(&sources, query)?;
        let status = format!("{label} ({region})");
        Some((region, status))
    }

    pub fn set_region_pending(&mut self, region: Region) -> Option<LoadRequest> {
        let mut resolved = region;
        let avail: Vec<String> = self.references.iter().map(|r| r.name.clone()).collect();
        if let Some(canon) = self.chrom_aliases.resolve(&resolved.chrom, &avail) {
            resolved.chrom = canon.to_string();
        }
        self.region = resolved.clone();
        if self.panes.is_empty() {
            self.panes.push(resolved);
            self.active_pane = 0;
        } else if let Some(slot) = self.panes.get_mut(self.active_pane) {
            *slot = resolved;
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.loaded_count = 0;
        self.reference_seq.clear();
        for rows in &mut self.bam_raw_rows {
            rows.clear();
        }
        for rows in &mut self.bam_rows {
            rows.clear();
        }
        for lanes in &mut self.bam_lanes {
            lanes.clear();
        }
        for total in &mut self.bam_total_lanes {
            *total = 0;
        }
        for b in &mut self.bam_bands {
            b.clear();
        }
        for l in &mut self.bam_mate_links {
            l.clear();
        }
        for d in &mut self.bam_downsampled {
            *d = 0;
        }
        for j in &mut self.bam_junctions {
            j.clear();
        }
        self.bam_scroll = 0;
        for rows in &mut self.annotation_rows {
            rows.clear();
        }
        for bins in &mut self.signal_bins {
            bins.clear();
        }
        for v in &mut self.link_records {
            v.clear();
        }
        for r in &mut self.vcf_track_rows {
            r.clear();
        }
        for s in &mut self.seg_data {
            *s = SegTrackData::default();
        }
        for g in &mut self.gwas_points {
            g.clear();
        }
        self.variants.clear();

        let render_mode = self.render_mode();
        Some(LoadRequest {
            generation: self.generation,
            region: self.region.clone(),
            fetch_opts: FetchOpts::default(),
            signal_max_bins: signal_bins_for_width(self.terminal_width),
            link_min_score: self.link_min_score,
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
        assert_eq!(signal_bins_for_width(20), 64);
        assert_eq!(signal_bins_for_width(80), 160);
        assert_eq!(signal_bins_for_width(200), 400);
        assert_eq!(signal_bins_for_width(4000), 4096);
    }

    #[test]
    fn classify_chromosome_scale_is_overview_only() {
        let t = Thresholds::default();
        assert_eq!(t.classify(248_000_000), RenderMode::OverviewOnly);
    }

    #[test]
    fn expected_loads_per_base_counts_all_sources() {
        let n = expected_loads_for(RenderMode::PerBase, 2, true, 1, 1, 0);
        assert_eq!(n, 6);
    }

    #[test]
    fn expected_loads_overview_drops_vcf() {
        let n = expected_loads_for(RenderMode::OverviewOnly, 1, true, 1, 1, 0);
        assert_eq!(n, 4);
    }

    #[test]
    fn expected_loads_wide_zoom_still_counts_stubs() {
        let n = expected_loads_for(RenderMode::CoverageDense, 2, true, 1, 1, 0);
        assert_eq!(n, 6);
    }

    #[test]
    fn expected_loads_minimal_view_is_one() {
        let n = expected_loads_for(RenderMode::DetailedReads, 0, false, 0, 0, 0);
        assert_eq!(n, 1);
    }

    #[test]
    fn parse_snapshot_with_path() {
        let (p, f) = parse_snapshot_command("snapshot foo.svg").unwrap();
        assert_eq!(p.to_str().unwrap(), "foo.svg");
        assert!(matches!(f, SnapshotFormat::Svg));
    }

    #[test]
    fn parse_snap_alias_with_png() {
        let (p, f) = parse_snapshot_command("snap out/x.png").unwrap();
        assert_eq!(p.to_str().unwrap(), "out/x.png");
        assert!(matches!(f, SnapshotFormat::Png));
    }

    #[test]
    fn parse_snapshot_ignores_leading_colon() {
        assert!(parse_snapshot_command(":snapshot foo.svg").is_some());
    }

    #[test]
    fn parse_snapshot_rejects_empty_path() {
        assert!(parse_snapshot_command("snapshot ").is_none());
    }

    #[test]
    fn parse_snapshot_rejects_other_commands() {
        assert!(parse_snapshot_command("HER2").is_none());
        assert!(parse_snapshot_command("chr1:1000-2000").is_none());
    }

    fn test_state_with_links(n_links: usize) -> AppState {
        use async_trait::async_trait;
        use igv_core::source::link::{FetchLinkOpts, LinkSource};
        use std::sync::Arc;

        #[derive(Debug)]
        struct StubLink;
        #[async_trait]
        impl LinkSource for StubLink {
            async fn query(
                &self,
                _r: &Region,
                _o: &FetchLinkOpts,
            ) -> igv_core::error::Result<Vec<igv_core::source::link::VisibleLink>> {
                Ok(Vec::new())
            }
            fn display_name(&self) -> &str {
                "stub"
            }
            fn record_count(&self) -> usize {
                0
            }
        }

        let mut links = Vec::new();
        for _ in 0..n_links {
            links.push(LinkTrack {
                path: "stub.bedpe".into(),
                display: "stub".into(),
                source: Arc::new(StubLink),
            });
        }
        AppState {
            links,
            link_records: vec![Vec::new(); n_links],
            references: vec![RefMeta {
                name: "chr1".into(),
                length: 1_000_000,
            }],
            terminal_width: 80,
            ..AppState::default()
        }
    }

    #[test]
    fn link_height_clamps() {
        let mut s = test_state_with_links(2);
        for _ in 0..20 {
            let _ = s.apply(Action::ResizeLink(1));
        }
        assert_eq!(s.link_track_height, LINK_MAX_HEIGHT);
        for _ in 0..30 {
            let _ = s.apply(Action::ResizeLink(-1));
        }
        assert_eq!(s.link_track_height, LINK_MIN_HEIGHT);
    }

    #[test]
    fn expected_loads_includes_links() {
        let n = expected_loads_for(RenderMode::DetailedReads, 0, false, 0, 0, 2);
        assert_eq!(n, 3);
    }
}
