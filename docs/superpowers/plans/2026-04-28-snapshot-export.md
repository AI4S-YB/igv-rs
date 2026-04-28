# Snapshot export (SVG / PNG) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SVG-primary, PNG-secondary snapshot export to `igv-rs`, supporting interactive single-view export, BED-batch, and gene-list batch.

**Architecture:** New `igv-render` crate owns graphical rendering. `igv-core` gains `RenderInputs` plus a synchronous `collect_render_inputs` helper for the headless batch path. `igv-tui` adds key `S`, palette `:snapshot`, and CLI flags `--snapshot-bed` / `--snapshot-genes`.

**Tech Stack:** Rust 2021 (workspace at 1.75), `usvg` 0.42 + `resvg` 0.42 + `tiny-skia` 0.11 for PNG, `insta` for SVG snapshot tests, existing `igv-core` data sources.

**Spec:** `docs/superpowers/specs/2026-04-28-snapshot-export-design.md`

---

## Notes for the implementer

- All file paths are absolute project-relative paths.
- Rust formatting: run `cargo fmt --all` before committing each task.
- Lints: `cargo clippy --workspace --all-targets -- -D warnings` should stay clean. Fix issues at the end of each task.
- The project uses `cargo nextest` is **not** required; standard `cargo test` works.
- Commit message convention from `git log`: `feat(scope): summary`, `docs(scope): summary`, `test(scope): summary`, `chore(scope): summary`. Wrap commit body at 72 chars. End each commit with a blank line then `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- Region coordinates are 1-based inclusive. `Region::width()` returns `end - start + 1`.
- SVG floats: format with `{:.2}` everywhere to keep `insta` snapshots stable across machines.
- Don't add `pub use` re-exports unless the next task needs them.

---

## Task 1: Workspace plumbing — create `igv-render` crate skeleton

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/igv-render/Cargo.toml`
- Create: `crates/igv-render/src/lib.rs`

- [ ] **Step 1: Add `igv-render` to workspace members**

Edit `Cargo.toml` (workspace root). Change:
```toml
members = ["crates/igv-core", "crates/igv-tui"]
```
to:
```toml
members = ["crates/igv-core", "crates/igv-render", "crates/igv-tui"]
```

- [ ] **Step 2: Write `crates/igv-render/Cargo.toml`**

```toml
[package]
name = "igv-render"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
igv-core = { path = "../igv-core" }
thiserror.workspace = true

# PNG pipeline. Pure-Rust, no system deps.
usvg = { version = "0.42", default-features = false }
resvg = { version = "0.42", default-features = false }
tiny-skia = { version = "0.11", default-features = false, features = ["std", "png-format"] }

[dev-dependencies]
insta.workspace = true
```

- [ ] **Step 3: Write `crates/igv-render/src/lib.rs`**

```rust
//! Graphical (SVG / PNG) renderer for igv-rs snapshots.
//!
//! Consumes `igv_core::render::RenderInputs` and emits an SVG string or a
//! PNG byte buffer. The same data shape feeds both the interactive
//! snapshot key (`S`) and the headless batch CLI (`--snapshot-bed`,
//! `--snapshot-genes`).

#![forbid(unsafe_code)]
```

- [ ] **Step 4: Verify the workspace builds**

Run: `cargo check --workspace`
Expected: PASS (no warnings about unused deps yet — `usvg`, `resvg`, `tiny-skia` are flagged as unused; that's fine for now and will resolve in Task 13).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/igv-render
git commit -m "$(cat <<'EOF'
feat(render): scaffold igv-render crate

Empty crate with deps on igv-core, usvg, resvg, tiny-skia. Will
host SVG + PNG snapshot rendering — see snapshot-export design.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: `igv-core` — `RenderInputs` data structure

**Files:**
- Create: `crates/igv-core/src/render_inputs.rs`
- Modify: `crates/igv-core/src/lib.rs`

- [ ] **Step 1: Write `crates/igv-core/src/render_inputs.rs`**

```rust
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
pub struct RenderInputs {
    pub region: Region,
    pub references: Vec<RefMeta>,
    pub reference_seq: Vec<u8>,
    pub variants: Vec<VariantRecord>,
    pub bams: Vec<BamTrackSnapshot>,
    pub annotations: Vec<AnnotationTrackSnapshot>,
    pub signals: Vec<SignalTrackSnapshot>,
    pub render_mode: RenderMode,
}

impl RenderInputs {
    /// True iff every track-vec is empty (no data to render).
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
            && self.bams.iter().all(|t| t.rows.is_empty())
            && self.annotations.iter().all(|t| t.transcripts.is_empty())
            && self.signals.iter().all(|t| t.bins.is_empty())
            && self.reference_seq.is_empty()
    }
}
```

- [ ] **Step 2: Wire module into `crates/igv-core/src/lib.rs`**

Read the file first. Currently it's a tiny mod-list. Add `pub mod render_inputs;` and a re-export:

```rust
pub mod render_inputs;

pub use render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, RenderInputs, SignalTrackSnapshot,
};
```

- [ ] **Step 3: Add a smoke unit test inside `render_inputs.rs`**

Append:

```rust
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
            render_mode: RenderMode::DetailedReads,
        };
        assert!(inputs.is_empty());
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p igv-core render_inputs`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/render_inputs.rs crates/igv-core/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(core): add RenderInputs for snapshot rendering

Single bag of per-region data (refs, variants, bams, annotations,
signals) shared by the interactive snapshot path and the headless
batch path. No fetching here — that comes in collect_render_inputs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `igv-core` — `collect_render_inputs` helper

**Files:**
- Create: `crates/igv-core/src/collect.rs`
- Modify: `crates/igv-core/src/lib.rs`

- [ ] **Step 1: Write `crates/igv-core/src/collect.rs`**

```rust
//! Synchronous-await collector for `RenderInputs`.
//!
//! Used by the headless snapshot batch path. Issues each source's
//! `fetch` in sequence and assembles the result. Does **not** use the
//! TUI loader's mpsc/generation machinery — that exists for
//! cancellation during interactive use, which the batch path doesn't
//! need.

use std::sync::Arc;

use crate::alignment::assign_lanes;
use crate::region::Region;
use crate::render::RenderMode;
use crate::render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, RenderInputs, SignalTrackSnapshot,
};
use crate::source::{
    AnnotationSource, BamSource, FastaSource, FetchOpts, FetchSignalOpts, RefMeta,
    SignalSource, VcfSource,
};

#[derive(Clone)]
pub struct Sources {
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub bams: Vec<(String, Arc<dyn BamSource>)>,
    pub annotations: Vec<(String, Arc<dyn AnnotationSource>)>,
    pub signals: Vec<(String, Arc<dyn SignalSource>)>,
    pub references: Vec<RefMeta>,
}

#[derive(Clone, Copy)]
pub struct CollectOpts {
    pub fetch_opts: FetchOpts,
    pub signal_opts: FetchSignalOpts,
    pub render_mode: RenderMode,
}

impl Default for CollectOpts {
    fn default() -> Self {
        Self {
            fetch_opts: FetchOpts::default(),
            signal_opts: FetchSignalOpts::default(),
            render_mode: RenderMode::DetailedReads,
        }
    }
}

/// Collect all data needed to render one region. Skips heavy fetches
/// at wide zoom levels (matches the loader's gating policy):
///
/// * Reference sequence: only `PerBase` and `DetailedReads`.
/// * Variants: skipped at `OverviewOnly`.
/// * BAM rows: only `PerBase` and `DetailedReads`.
/// * Annotations: always fetched.
/// * Signals: always fetched (bigWig zoom-pyramid handles it).
pub async fn collect_render_inputs(
    sources: &Sources,
    region: &Region,
    opts: &CollectOpts,
) -> crate::error::Result<RenderInputs> {
    let mode = opts.render_mode;

    let reference_seq = if matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
        sources.fasta.fetch(region).await?
    } else {
        Vec::new()
    };

    let variants = if let Some(vcf) = &sources.vcf {
        if !matches!(mode, RenderMode::OverviewOnly) {
            vcf.fetch(region).await?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut bams = Vec::with_capacity(sources.bams.len());
    for (display, src) in &sources.bams {
        let rows = if matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            src.fetch(region, &opts.fetch_opts).await?
        } else {
            Vec::new()
        };
        let lanes = assign_lanes(&rows);
        let total_lanes_u32 = lanes.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        let total_lanes = total_lanes_u32.min(u16::MAX as u32) as u16;
        bams.push(BamTrackSnapshot {
            display: display.clone(),
            rows,
            lanes,
            total_lanes,
        });
    }

    let mut annotations = Vec::with_capacity(sources.annotations.len());
    for (display, src) in &sources.annotations {
        let transcripts = src.fetch(region).await?;
        annotations.push(AnnotationTrackSnapshot {
            display: display.clone(),
            transcripts,
        });
    }

    let mut signals = Vec::with_capacity(sources.signals.len());
    for (display, src) in &sources.signals {
        let bins = src.fetch(region, &opts.signal_opts).await?;
        signals.push(SignalTrackSnapshot {
            display: display.clone(),
            bins,
        });
    }

    Ok(RenderInputs {
        region: region.clone(),
        references: sources.references.clone(),
        reference_seq,
        variants,
        bams,
        annotations,
        signals,
        render_mode: mode,
    })
}
```

- [ ] **Step 2: Wire module into `crates/igv-core/src/lib.rs`**

Add `pub mod collect;` and re-export:

```rust
pub use collect::{collect_render_inputs, CollectOpts, Sources};
```

- [ ] **Step 3: Build to verify types resolve**

Run: `cargo check -p igv-core`
Expected: PASS.

- [ ] **Step 4: Add an integration test fixture**

Create: `crates/igv-core/tests/collect_render_inputs.rs`

```rust
//! Smoke-test the synchronous collector: an in-memory mock-source
//! triple (FastaSource / no VCF / no BAM), one region, returns the
//! expected RenderInputs.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::source::{FastaSource, RefMeta};
use igv_core::{collect_render_inputs, CollectOpts, Sources};

struct MockFasta;

#[async_trait]
impl FastaSource for MockFasta {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 1000 }])
    }
    async fn fetch(&self, _region: &Region) -> Result<Vec<u8>> {
        Ok(b"ACGTACGT".to_vec())
    }
}

#[tokio::test]
async fn collect_minimal_inputs() {
    let sources = Sources {
        fasta: Arc::new(MockFasta) as Arc<dyn FastaSource>,
        vcf: None,
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        references: vec![RefMeta { name: "chr1".into(), length: 1000 }],
    };
    let region = Region::new("chr1", 1, 8).unwrap();
    let opts = CollectOpts {
        render_mode: RenderMode::DetailedReads,
        ..CollectOpts::default()
    };
    let out = collect_render_inputs(&sources, &region, &opts).await.unwrap();
    assert_eq!(out.region, region);
    assert_eq!(out.reference_seq, b"ACGTACGT".to_vec());
    assert!(out.bams.is_empty());
    assert_eq!(out.render_mode, RenderMode::DetailedReads);
}

#[tokio::test]
async fn collect_skips_reference_at_wide_zoom() {
    let sources = Sources {
        fasta: Arc::new(MockFasta) as Arc<dyn FastaSource>,
        vcf: None,
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        references: vec![RefMeta { name: "chr1".into(), length: 1000 }],
    };
    let region = Region::new("chr1", 1, 1000).unwrap();
    let opts = CollectOpts {
        render_mode: RenderMode::OverviewOnly,
        ..CollectOpts::default()
    };
    let out = collect_render_inputs(&sources, &region, &opts).await.unwrap();
    assert!(out.reference_seq.is_empty(), "reference should be gated at OverviewOnly");
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p igv-core --test collect_render_inputs`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/igv-core/src/collect.rs crates/igv-core/src/lib.rs crates/igv-core/tests/collect_render_inputs.rs
git commit -m "$(cat <<'EOF'
feat(core): collect_render_inputs for batch snapshot path

Sequential async helper that gathers refs/variants/bam/annotation/
signal data for one region into a RenderInputs. Mirrors the loader's
zoom-mode gating so wide views don't issue heavy BAM/reference
fetches. Used by the headless batch path; interactive snapshots
build RenderInputs directly from AppState.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `igv-render` — public types (`SvgOptions`, `TrackHeights`, `RenderError`)

**Files:**
- Create: `crates/igv-render/src/options.rs`
- Create: `crates/igv-render/src/error.rs`
- Modify: `crates/igv-render/src/lib.rs`

- [ ] **Step 1: Write `crates/igv-render/src/error.rs`**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("usvg parse: {0}")]
    UsvgParse(String),
    #[error("png encode: {0}")]
    PngEncode(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
```

- [ ] **Step 2: Write `crates/igv-render/src/options.rs`**

```rust
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
```

- [ ] **Step 3: Update `crates/igv-render/src/lib.rs`**

Replace contents:

```rust
//! Graphical (SVG / PNG) renderer for igv-rs snapshots.

#![forbid(unsafe_code)]

pub mod error;
pub mod options;
pub mod theme;

pub use error::RenderError;
pub use options::{SvgOptions, TrackHeights};
pub use theme::GraphicalTheme;
```

- [ ] **Step 4: Build (will fail until Task 5 adds `theme` module)**

This task ends with a known-broken build because `theme` is referenced. Task 5 closes the gap. **Do not commit yet** — bundle Task 4 + Task 5 into one commit at the end of Task 5. Continue to Task 5.

---

## Task 5: `igv-render` — `GraphicalTheme`

**Files:**
- Create: `crates/igv-render/src/theme.rs`

- [ ] **Step 1: Write `crates/igv-render/src/theme.rs`**

```rust
//! RGB-based theme. Independent of crossterm `Style` — the SVG world
//! does not have ANSI/named colors, only hex.

#[derive(Debug, Clone, Copy)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone)]
pub struct GraphicalTheme {
    pub bg: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub ruler_text: Rgb,
    pub transcript_exon: Rgb,
    pub transcript_intron: Rgb,
    pub transcript_label: Rgb,
    pub variant_snv: Rgb,
    pub variant_indel: Rgb,
    pub coverage_bar: Rgb,
    pub signal_bar: Rgb,
    pub read_forward: Rgb,
    pub read_reverse: Rgb,
    pub mismatch_a: Rgb,
    pub mismatch_c: Rgb,
    pub mismatch_g: Rgb,
    pub mismatch_t: Rgb,
    pub mismatch_n: Rgb,
    pub font_family: &'static str,
    pub font_px_small: u32,
    pub font_px_normal: u32,
    pub font_px_label: u32,
}

impl GraphicalTheme {
    pub fn igv_light() -> Self {
        Self {
            bg: Rgb(0xff, 0xff, 0xff),
            fg: Rgb(0x1a, 0x1a, 0x1a),
            muted: Rgb(0x88, 0x88, 0x88),
            ruler_text: Rgb(0x44, 0x44, 0x44),
            transcript_exon: Rgb(0x1f, 0x3b, 0x73),
            transcript_intron: Rgb(0x77, 0x77, 0x77),
            transcript_label: Rgb(0x1a, 0x1a, 0x1a),
            variant_snv: Rgb(0xc0, 0x39, 0x2b),
            variant_indel: Rgb(0x7d, 0x3c, 0x98),
            coverage_bar: Rgb(0x88, 0x88, 0x88),
            signal_bar: Rgb(0x1f, 0x4e, 0x79),
            read_forward: Rgb(0x9e, 0xc3, 0xe0),
            read_reverse: Rgb(0xe8, 0xb6, 0xb6),
            mismatch_a: Rgb(0x2c, 0xa0, 0x2c),
            mismatch_c: Rgb(0x1f, 0x77, 0xb4),
            mismatch_g: Rgb(0xff, 0x7f, 0x0e),
            mismatch_t: Rgb(0xd6, 0x27, 0x28),
            mismatch_n: Rgb(0x88, 0x88, 0x88),
            font_family: "DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif",
            font_px_small: 10,
            font_px_normal: 12,
            font_px_label: 14,
        }
    }

    /// Color for a mismatch base. Returns `mismatch_n` for unknown bases.
    pub fn mismatch_color(&self, base: u8) -> Rgb {
        match base.to_ascii_uppercase() {
            b'A' => self.mismatch_a,
            b'C' => self.mismatch_c,
            b'G' => self.mismatch_g,
            b'T' => self.mismatch_t,
            _ => self.mismatch_n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_pads_short_components() {
        assert_eq!(Rgb(0, 1, 254).hex(), "#0001fe");
    }

    #[test]
    fn igv_light_returns_white_bg() {
        let t = GraphicalTheme::igv_light();
        assert_eq!(t.bg.hex(), "#ffffff");
    }

    #[test]
    fn mismatch_color_falls_back_to_n_for_unknown() {
        let t = GraphicalTheme::igv_light();
        assert_eq!(t.mismatch_color(b'X').hex(), t.mismatch_n.hex());
        assert_eq!(t.mismatch_color(b'a').hex(), t.mismatch_a.hex());
    }
}
```

- [ ] **Step 2: Build**

Run: `cargo check -p igv-render`
Expected: PASS.

- [ ] **Step 3: Test**

Run: `cargo test -p igv-render`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit (covers Tasks 4 + 5)**

```bash
git add crates/igv-render/src
git commit -m "$(cat <<'EOF'
feat(render): SvgOptions, TrackHeights, GraphicalTheme types

Public API surface for snapshot rendering. igv_light() ships the
default IGV-style RGB palette; bold/italic and font sizing are spec'd
here so per-track renderers can stay theme-agnostic.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `igv-render` — px-based layout & `bp_to_px`

**Files:**
- Create: `crates/igv-render/src/layout.rs`
- Modify: `crates/igv-render/src/lib.rs`

- [ ] **Step 1: Write `crates/igv-render/src/layout.rs`**

```rust
//! Pixel-based layout for one snapshot. Mirrors igv-tui's track order
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
        region_width_bp: inputs.region.width(),
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
        // region 1..=100, plot covers x=80..1188 (1108 px)
        assert!((l.plot.bp_to_px(1) - 80.0).abs() < 1e-6);
        assert!((l.plot.bp_to_px(100) - 1188.0).abs() < 1e-6);
    }

    #[test]
    fn bp_to_px_clamps_oob() {
        let l = compute(&empty_inputs(), 1200, &TrackHeights::default());
        assert!((l.plot.bp_to_px(0) - 80.0).abs() < 1e-6);
        assert!((l.plot.bp_to_px(10_000) - 1188.0).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Wire into `lib.rs`**

Add to `crates/igv-render/src/lib.rs`:

```rust
pub mod layout;
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p igv-render layout`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/igv-render/src/layout.rs crates/igv-render/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(render): px-based snapshot layout + bp_to_px

Mirrors igv-tui's track order in pixel coordinates. PlotMetrics owns
the bp→px transform and clamps OOB positions to the plot edges so
edge cases (partially-visible reads) render predictably.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `igv-render` — SVG document scaffolding + header & ruler

**Files:**
- Create: `crates/igv-render/src/svg/mod.rs`
- Create: `crates/igv-render/src/svg/doc.rs`
- Create: `crates/igv-render/src/svg/header.rs`
- Create: `crates/igv-render/src/svg/ruler.rs`
- Create: `crates/igv-render/tests/svg_snapshots.rs`
- Modify: `crates/igv-render/src/lib.rs`

- [ ] **Step 1: Write `crates/igv-render/src/svg/doc.rs`**

```rust
//! SVG document builder. Lives behind a small typed API so we can swap
//! the backing string concatenation for the `svg` crate later without
//! touching per-track code.

use std::fmt::Write;

use crate::theme::Rgb;

pub struct SvgDoc {
    body: String,
    width: u32,
    height: u32,
}

impl SvgDoc {
    pub fn new(width: u32, height: u32, bg: Rgb, font_family: &str) -> Self {
        let mut body = String::new();
        writeln!(
            body,
            r#"<?xml version="1.0" encoding="UTF-8"?>"#
        ).unwrap();
        writeln!(
            body,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="{}">"#,
            width, height, width, height, font_family
        ).unwrap();
        writeln!(
            body,
            r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
            width, height, bg.hex()
        ).unwrap();
        Self { body, width, height }
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: Rgb) {
        writeln!(
            self.body,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"/>"#,
            x, y, w, h, fill.hex()
        ).unwrap();
    }

    pub fn rect_stroke(&mut self, x: f64, y: f64, w: f64, h: f64, stroke: Rgb, stroke_w: f64) {
        writeln!(
            self.body,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="{}" stroke-width="{:.2}"/>"#,
            x, y, w, h, stroke.hex(), stroke_w
        ).unwrap();
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: Rgb, stroke_w: f64) {
        writeln!(
            self.body,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{:.2}"/>"#,
            x1, y1, x2, y2, stroke.hex(), stroke_w
        ).unwrap();
    }

    pub fn text(
        &mut self,
        x: f64,
        y: f64,
        text: &str,
        fill: Rgb,
        font_px: u32,
        anchor: TextAnchor,
    ) {
        writeln!(
            self.body,
            r#"<text x="{:.2}" y="{:.2}" fill="{}" font-size="{}" text-anchor="{}">{}</text>"#,
            x, y, fill.hex(), font_px, anchor.as_str(), escape_xml(text)
        ).unwrap();
    }

    pub fn polygon(&mut self, points: &[(f64, f64)], fill: Rgb) {
        let mut buf = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i > 0 {
                buf.push(' ');
            }
            write!(buf, "{:.2},{:.2}", x, y).unwrap();
        }
        writeln!(
            self.body,
            r#"<polygon points="{}" fill="{}"/>"#,
            buf, fill.hex()
        ).unwrap();
    }

    pub fn finish(mut self) -> String {
        self.body.push_str("</svg>\n");
        self.body
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
}

#[derive(Clone, Copy)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

impl TextAnchor {
    fn as_str(self) -> &'static str {
        match self {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        }
    }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}
```

- [ ] **Step 2: Write `crates/igv-render/src/svg/header.rs`**

```rust
use igv_core::render_inputs::RenderInputs;

use crate::layout::Rect;
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    inputs: &RenderInputs,
    title: Option<&str>,
    theme: &GraphicalTheme,
) {
    let title = title.map(str::to_string).unwrap_or_else(|| "igv-rs snapshot".into());
    let region_str = format!(
        "{}:{}-{}",
        inputs.region.chrom, inputs.region.start, inputs.region.end
    );
    let baseline_y = (area.y + area.h * 2 / 3) as f64;
    doc.text(
        12.0,
        baseline_y,
        &title,
        theme.fg,
        theme.font_px_label,
        TextAnchor::Start,
    );
    doc.text(
        (area.w - 12) as f64,
        baseline_y,
        &region_str,
        theme.muted,
        theme.font_px_normal,
        TextAnchor::End,
    );
}
```

- [ ] **Step 3: Write `crates/igv-render/src/svg/ruler.rs`**

```rust
use igv_core::render_inputs::RenderInputs;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    let baseline_y = (area.y + area.h - 6) as f64;
    doc.line(
        plot.plot_x0 as f64,
        baseline_y,
        plot.plot_x1 as f64,
        baseline_y,
        theme.muted,
        1.0,
    );
    let region = &inputs.region;
    let step = nice_step_bp(region.width());
    let first = ((region.start + step - 1) / step) * step;
    let mut tick = first;
    while tick <= region.end {
        let x = plot.bp_to_px(tick);
        doc.line(x, baseline_y - 4.0, x, baseline_y, theme.muted, 1.0);
        let label = format_bp(tick);
        doc.text(
            x,
            baseline_y - 6.0,
            &label,
            theme.ruler_text,
            theme.font_px_small,
            TextAnchor::Middle,
        );
        tick = tick.saturating_add(step);
        if tick == 0 { break; }
    }
}

/// Pick a "nice" tick interval (1, 2, 5 × 10^k) for a given region width
/// such that we get roughly 6–10 ticks across the plot.
pub fn nice_step_bp(region_width: u64) -> u64 {
    if region_width == 0 { return 1; }
    let target = (region_width / 8).max(1);
    let mag = 10u64.pow((target as f64).log10().floor() as u32);
    for &m in &[1u64, 2, 5, 10] {
        if mag * m >= target { return mag * m; }
    }
    mag * 10
}

fn format_bp(bp: u64) -> String {
    // 1,234,567 style.
    let s = bp.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_step_for_typical_widths() {
        assert!(nice_step_bp(100) <= 25);
        assert!(nice_step_bp(1_000) <= 250);
        assert!(nice_step_bp(50_000) >= 5_000);
        assert!(nice_step_bp(50_000) <= 10_000);
    }

    #[test]
    fn format_bp_inserts_thousand_separators() {
        assert_eq!(format_bp(0), "0");
        assert_eq!(format_bp(123), "123");
        assert_eq!(format_bp(1_234), "1,234");
        assert_eq!(format_bp(1_234_567), "1,234,567");
    }
}
```

- [ ] **Step 4: Write `crates/igv-render/src/svg/mod.rs`**

```rust
//! SVG render entry point. Per-track functions live in submodules and
//! all draw into a shared `SvgDoc`.

pub mod doc;
pub mod header;
pub mod ruler;

use igv_core::render_inputs::RenderInputs;

use crate::layout;
use crate::options::SvgOptions;
use crate::svg::doc::SvgDoc;

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> String {
    let layout = layout::compute(inputs, opts.width_px, &opts.track_heights);
    let mut doc = SvgDoc::new(
        layout.total_width,
        layout.total_height,
        opts.theme.bg,
        opts.theme.font_family,
    );

    header::draw(&mut doc, layout.header, inputs, opts.title.as_deref(), &opts.theme);
    ruler::draw(&mut doc, layout.ruler, &layout.plot, inputs, &opts.theme);

    doc.finish()
}
```

- [ ] **Step 5: Wire `lib.rs`**

Update `crates/igv-render/src/lib.rs` to add public `render_svg`:

```rust
pub mod error;
pub mod layout;
pub mod options;
pub mod svg;
pub mod theme;

pub use error::RenderError;
pub use options::{SvgOptions, TrackHeights};
pub use theme::GraphicalTheme;

pub fn render_svg(inputs: &igv_core::render_inputs::RenderInputs, opts: &SvgOptions) -> String {
    svg::render(inputs, opts)
}
```

- [ ] **Step 6: Write the first insta snapshot test**

Create `crates/igv-render/tests/svg_snapshots.rs`:

```rust
//! Per-track insta snapshots. Each test renders a focused fixture and
//! pins the resulting SVG. SVG output uses {:.2} formatting throughout
//! for cross-machine determinism.

use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::render_inputs::RenderInputs;
use igv_render::{render_svg, SvgOptions};

fn empty_inputs(start: u64, end: u64) -> RenderInputs {
    RenderInputs {
        region: Region::new("chr1", start, end).unwrap(),
        references: vec![],
        reference_seq: vec![],
        variants: vec![],
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        render_mode: RenderMode::DetailedReads,
    }
}

#[test]
fn empty_view_renders_header_and_ruler() {
    let inputs = empty_inputs(1, 1000);
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("empty_view_header_ruler", svg);
}
```

- [ ] **Step 7: Run the test (will fail by design — first run accepts snapshot)**

Run: `cargo test -p igv-render --test svg_snapshots`
Expected: FAIL with insta diff message ("no current snapshot found").

- [ ] **Step 8: Accept the snapshot**

Run: `INSTA_UPDATE=always cargo test -p igv-render --test svg_snapshots`
Expected: PASS (1 test). A new file `crates/igv-render/tests/snapshots/svg_snapshots__empty_view_header_ruler.snap` is created.

- [ ] **Step 9: Re-run normally to confirm stability**

Run: `cargo test -p igv-render --test svg_snapshots`
Expected: PASS (no diffs).

- [ ] **Step 10: Run unit tests too**

Run: `cargo test -p igv-render`
Expected: PASS (all tests including ruler unit tests).

- [ ] **Step 11: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): SVG scaffolding with header + ruler tracks

SvgDoc gives a small typed builder over a String body. render_svg
walks the layout and dispatches to per-track draw fns. First insta
snapshot pins an empty-view (header + ruler only) figure for
regression coverage.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `igv-render` — annotations track

**Files:**
- Create: `crates/igv-render/src/svg/annotations.rs`
- Modify: `crates/igv-render/src/svg/mod.rs`
- Modify: `crates/igv-render/tests/svg_snapshots.rs`

- [ ] **Step 1: Write `crates/igv-render/src/svg/annotations.rs`**

```rust
//! Annotations track: one row per loaded GFF/BED track.
//!
//! Within the row each visible transcript is laid out one per "lane"
//! (simple greedy non-overlap). For v1 we render at most `max_lanes` per
//! track and append a "+N more" label if truncated.

use igv_core::render_inputs::AnnotationTrackSnapshot;
use igv_core::source::{AnnotationTranscript, BlockKind, Strand};

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

const MAX_LANES_PER_TRACK: usize = 4;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &AnnotationTrackSnapshot,
    theme: &GraphicalTheme,
) {
    // Track label on the left margin.
    let label_y = (area.y + area.h / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    if track.transcripts.is_empty() {
        return;
    }

    let lanes = assign_lanes(&track.transcripts);
    let lane_count = lanes.iter().copied().max().map(|m| m + 1).unwrap_or(0).min(MAX_LANES_PER_TRACK);
    let usable = (lane_count.max(1)) as f64;
    let lane_h = (area.h as f64 - 4.0) / usable;
    let exon_h = lane_h * 0.6;
    let intron_y_offset = lane_h / 2.0;

    let mut drawn = 0usize;
    for (lane, tx) in lanes.iter().zip(track.transcripts.iter()) {
        let lane = *lane as usize;
        if lane >= MAX_LANES_PER_TRACK {
            continue;
        }
        let lane_top = area.y as f64 + 2.0 + (lane as f64) * lane_h;
        draw_transcript(doc, plot, tx, lane_top, lane_h, exon_h, intron_y_offset, theme);
        drawn += 1;
    }

    let truncated = track.transcripts.len().saturating_sub(drawn);
    if truncated > 0 {
        doc.text(
            (plot.margin_left + 6) as f64,
            (area.y + area.h - 4) as f64,
            &format!("+{} more", truncated),
            theme.muted,
            theme.font_px_small,
            TextAnchor::Start,
        );
    }
}

fn draw_transcript(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    tx: &AnnotationTranscript,
    lane_top: f64,
    lane_h: f64,
    exon_h: f64,
    intron_y_offset: f64,
    theme: &GraphicalTheme,
) {
    let Some((s, e)) = tx.span() else { return };
    let x0 = plot.bp_to_px(s);
    let x1 = plot.bp_to_px(e);

    // Intron line.
    let intron_y = lane_top + intron_y_offset;
    doc.line(x0, intron_y, x1, intron_y, theme.transcript_intron, 1.0);
    draw_strand_chevrons(doc, x0, x1, intron_y, tx.strand, theme);

    // Exon / UTR / CDS blocks.
    let exon_y = lane_top + (lane_h - exon_h) / 2.0;
    for block in &tx.blocks {
        let bx0 = plot.bp_to_px(block.start);
        let bx1 = plot.bp_to_px(block.end);
        let w = (bx1 - bx0).max(1.0);
        let h = match block.kind {
            BlockKind::Utr5 | BlockKind::Utr3 => exon_h * 0.5,
            _ => exon_h,
        };
        let y = exon_y + (exon_h - h) / 2.0;
        doc.rect(bx0, y, w, h, theme.transcript_exon);
    }

    // Label centred above the transcript span (or to the left if too wide).
    let label_x = (x0 + x1) / 2.0;
    doc.text(
        label_x,
        lane_top + theme.font_px_small as f64,
        &tx.name,
        theme.transcript_label,
        theme.font_px_small,
        TextAnchor::Middle,
    );
}

fn draw_strand_chevrons(
    doc: &mut SvgDoc,
    x0: f64,
    x1: f64,
    y: f64,
    strand: Strand,
    theme: &GraphicalTheme,
) {
    let direction: i32 = match strand {
        Strand::Forward => 1,
        Strand::Reverse => -1,
        Strand::Unknown => return,
    };
    let span = x1 - x0;
    if span < 12.0 { return; }
    let step = 30.0;
    let mut x = x0 + step;
    while x < x1 - 4.0 {
        let dx = 4.0 * direction as f64;
        let dy = 3.0;
        doc.line(x - dx, y - dy, x, y, theme.transcript_intron, 1.0);
        doc.line(x - dx, y + dy, x, y, theme.transcript_intron, 1.0);
        x += step;
    }
}

fn assign_lanes(transcripts: &[AnnotationTranscript]) -> Vec<u32> {
    // Greedy lane packing by leftmost block start.
    let mut lane_ends: Vec<u64> = Vec::new();
    let mut lanes = Vec::with_capacity(transcripts.len());
    for tx in transcripts {
        let Some((s, e)) = tx.span() else {
            lanes.push(0);
            continue;
        };
        let mut placed = None;
        for (i, end) in lane_ends.iter_mut().enumerate() {
            if s > *end {
                *end = e;
                placed = Some(i as u32);
                break;
            }
        }
        let lane = placed.unwrap_or_else(|| {
            lane_ends.push(e);
            (lane_ends.len() - 1) as u32
        });
        lanes.push(lane);
    }
    lanes
}
```

- [ ] **Step 2: Wire annotations into `crates/igv-render/src/svg/mod.rs`**

Replace `render`:

```rust
pub mod doc;
pub mod header;
pub mod ruler;
pub mod annotations;

use igv_core::render_inputs::RenderInputs;

use crate::layout;
use crate::options::SvgOptions;
use crate::svg::doc::SvgDoc;

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> String {
    let layout = layout::compute(inputs, opts.width_px, &opts.track_heights);
    let mut doc = SvgDoc::new(
        layout.total_width,
        layout.total_height,
        opts.theme.bg,
        opts.theme.font_family,
    );

    header::draw(&mut doc, layout.header, inputs, opts.title.as_deref(), &opts.theme);
    ruler::draw(&mut doc, layout.ruler, &layout.plot, inputs, &opts.theme);
    for (rect, track) in layout.annotations.iter().zip(inputs.annotations.iter()) {
        annotations::draw(&mut doc, *rect, &layout.plot, track, &opts.theme);
    }

    doc.finish()
}
```

- [ ] **Step 3: Add an annotations-only fixture test**

Append to `crates/igv-render/tests/svg_snapshots.rs`:

```rust
use igv_core::render_inputs::AnnotationTrackSnapshot;
use igv_core::source::{AnnotationBlock, AnnotationTranscript, BlockKind, Strand, TranscriptKind};

#[test]
fn annotations_only_two_transcripts() {
    let mut inputs = empty_inputs(1, 1000);
    inputs.annotations.push(AnnotationTrackSnapshot {
        display: "genes.gtf".into(),
        transcripts: vec![
            AnnotationTranscript {
                name: "GENE1".into(),
                id: "tx1".into(),
                gene_id: Some("g1".into()),
                strand: Strand::Forward,
                blocks: vec![
                    AnnotationBlock { start: 100, end: 200, kind: BlockKind::Exon },
                    AnnotationBlock { start: 400, end: 500, kind: BlockKind::Exon },
                ],
                kind: TranscriptKind::Mrna,
            },
            AnnotationTranscript {
                name: "GENE2".into(),
                id: "tx2".into(),
                gene_id: Some("g2".into()),
                strand: Strand::Reverse,
                blocks: vec![
                    AnnotationBlock { start: 600, end: 800, kind: BlockKind::Exon },
                ],
                kind: TranscriptKind::Mrna,
            },
        ],
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("annotations_only_two_transcripts", svg);
}
```

- [ ] **Step 4: Run, accept snapshot, re-run**

```bash
cargo test -p igv-render --test svg_snapshots annotations_only_two_transcripts
INSTA_UPDATE=always cargo test -p igv-render --test svg_snapshots annotations_only_two_transcripts
cargo test -p igv-render
```

Expected on the final command: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): annotations track

Greedy lane packing by leftmost span; renders intron lines with
strand chevrons and exon/UTR boxes (UTRs at half exon height).
Truncates >MAX_LANES_PER_TRACK with a "+N more" label.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: `igv-render` — variants track

**Files:**
- Create: `crates/igv-render/src/svg/variants.rs`
- Modify: `crates/igv-render/src/svg/mod.rs`
- Modify: `crates/igv-render/tests/svg_snapshots.rs`

- [ ] **Step 1: Write `crates/igv-render/src/svg/variants.rs`**

```rust
use igv_core::render_inputs::RenderInputs;
use igv_core::source::VariantRecord;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::SvgDoc;
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    let cy = (area.y + area.h / 2) as f64;
    for v in &inputs.variants {
        let x = plot.bp_to_px(v.pos);
        let color = if is_indel(v) { theme.variant_indel } else { theme.variant_snv };
        let r = 3.0;
        doc.polygon(
            &[(x - r, cy + r), (x + r, cy + r), (x, cy - r)],
            color,
        );
    }
}

fn is_indel(v: &VariantRecord) -> bool {
    let ref_len = v.reference_allele.len();
    v.alternate_alleles.iter().any(|a| a.len() != ref_len)
}
```

- [ ] **Step 2: Wire into `crates/igv-render/src/svg/mod.rs`**

Add `pub mod variants;` and inside `render` (after annotations loop):

```rust
if let Some(rect) = layout.variants {
    variants::draw(&mut doc, rect, &layout.plot, inputs, &opts.theme);
}
```

- [ ] **Step 3: Add fixture**

Append to `crates/igv-render/tests/svg_snapshots.rs`:

```rust
use igv_core::source::VariantRecord;

#[test]
fn variants_only_three_records() {
    let mut inputs = empty_inputs(1, 1000);
    inputs.variants = vec![
        VariantRecord {
            chrom: "chr1".into(), pos: 250,
            reference_allele: "A".into(),
            alternate_alleles: vec!["T".into()],
            quality: Some(60.0), passes_filter: true,
        },
        VariantRecord {
            chrom: "chr1".into(), pos: 600,
            reference_allele: "G".into(),
            alternate_alleles: vec!["GA".into()],
            quality: Some(50.0), passes_filter: true,
        },
        VariantRecord {
            chrom: "chr1".into(), pos: 900,
            reference_allele: "C".into(),
            alternate_alleles: vec!["A".into()],
            quality: Some(40.0), passes_filter: true,
        },
    ];
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("variants_only_three_records", svg);
}
```

- [ ] **Step 4: Run, accept, re-run**

```bash
cargo test -p igv-render --test svg_snapshots variants_only_three_records
INSTA_UPDATE=always cargo test -p igv-render --test svg_snapshots variants_only_three_records
cargo test -p igv-render
```

Expected on final: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): variants track (small triangles, indel vs SNV)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: `igv-render` — coverage track

**Files:**
- Create: `crates/igv-render/src/svg/coverage.rs`
- Modify: `crates/igv-render/src/svg/mod.rs`
- Modify: `crates/igv-render/tests/svg_snapshots.rs`

- [ ] **Step 1: Write `crates/igv-render/src/svg/coverage.rs`**

```rust
//! Coverage track: aggregate depth across all loaded BAMs in the current
//! region into one bar chart. Sums per-bp coverage from each BAM's
//! alignment rows.

use igv_core::render_inputs::RenderInputs;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    // Track label.
    let label_y = (area.y + area.h / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        "coverage",
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    // Compute depth per pixel column (sum across BAMs).
    let cols = plot.plot_width.max(1);
    let mut depth = vec![0u32; cols as usize];
    let region = &inputs.region;
    let region_width = region.width().max(1);
    for bam in &inputs.bams {
        for row in &bam.rows {
            let lo = row.ref_start.max(region.start);
            let hi = row.ref_end.min(region.end);
            if hi < lo { continue; }
            for bp in lo..=hi {
                let off = (bp - region.start) as f64;
                let frac = (off / region_width as f64).clamp(0.0, 0.999_999);
                let col = (frac * cols as f64) as usize;
                if col < depth.len() { depth[col] += 1; }
            }
        }
    }

    let max_depth = depth.iter().copied().max().unwrap_or(0);
    if max_depth == 0 {
        return;
    }
    let baseline_y = (area.y + area.h - 2) as f64;
    let usable_h = area.h as f64 - 4.0;
    for (i, &d) in depth.iter().enumerate() {
        if d == 0 { continue; }
        let h = (d as f64 / max_depth as f64) * usable_h;
        let x = plot.plot_x0 as f64 + i as f64;
        doc.rect(x, baseline_y - h, 1.0, h, theme.coverage_bar);
    }

    // Max-depth label.
    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + theme.font_px_small) as f64,
        &format!("max {}", max_depth),
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );
}
```

- [ ] **Step 2: Wire into `mod.rs`**

Add `pub mod coverage;` and after variants:

```rust
if let Some(rect) = layout.coverage {
    coverage::draw(&mut doc, rect, &layout.plot, inputs, &opts.theme);
}
```

- [ ] **Step 3: Add fixture (need a BAM track with one read)**

Append to `crates/igv-render/tests/svg_snapshots.rs`:

```rust
use igv_core::render_inputs::BamTrackSnapshot;
use igv_core::source::AlignmentRow;

fn fake_read(start: u64, end: u64) -> AlignmentRow {
    AlignmentRow {
        query_name: "r".into(),
        flag: 0,
        ref_start: start,
        ref_end: end,
        mapq: 60,
        is_reverse: false,
        query_sequence: vec![],
        cigar: vec![],
        tag: None,
    }
}

#[test]
fn coverage_only_one_bam_two_reads() {
    let mut inputs = empty_inputs(1, 100);
    inputs.bams.push(BamTrackSnapshot {
        display: "sample.bam".into(),
        rows: vec![fake_read(10, 60), fake_read(30, 80)],
        lanes: vec![0, 1],
        total_lanes: 2,
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("coverage_only_one_bam_two_reads", svg);
}
```

- [ ] **Step 4: Run, accept, re-run**

```bash
cargo test -p igv-render --test svg_snapshots coverage_only_one_bam_two_reads
INSTA_UPDATE=always cargo test -p igv-render --test svg_snapshots coverage_only_one_bam_two_reads
cargo test -p igv-render
```

- [ ] **Step 5: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): coverage track

Per-pixel-column depth from summed BAM rows. Bar height scaled to the
per-track max; "max N" label at top-left of the track band.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: `igv-render` — signal track

**Files:**
- Create: `crates/igv-render/src/svg/signal.rs`
- Modify: `crates/igv-render/src/svg/mod.rs`
- Modify: `crates/igv-render/tests/svg_snapshots.rs`

- [ ] **Step 1: Write `crates/igv-render/src/svg/signal.rs`**

```rust
//! Signal track (bigWig). Bar chart with optional shared-scale max
//! supplied via SvgOptions.signal_shared_max.

use igv_core::render_inputs::SignalTrackSnapshot;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &SignalTrackSnapshot,
    shared_max: Option<f32>,
    theme: &GraphicalTheme,
) {
    let label_y = (area.y + area.h / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    if track.bins.is_empty() {
        return;
    }
    let local_max = track.bins.iter().map(|b| b.value).fold(0.0_f32, f32::max);
    let max = shared_max.unwrap_or(local_max);
    if max <= 0.0 {
        return;
    }
    let baseline_y = (area.y + area.h - 2) as f64;
    let usable_h = area.h as f64 - 4.0;
    for bin in &track.bins {
        if bin.value <= 0.0 { continue; }
        let x0 = plot.bp_to_px(bin.start);
        let x1 = plot.bp_to_px(bin.end + 1);
        let w = (x1 - x0).max(1.0);
        let h = (bin.value as f64 / max as f64) * usable_h;
        doc.rect(x0, baseline_y - h, w, h, theme.signal_bar);
    }

    doc.text(
        (plot.margin_left - 6) as f64,
        (area.y + theme.font_px_small) as f64,
        &format!("max {:.2}", max),
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );
}
```

- [ ] **Step 2: Wire into `mod.rs`**

Add `pub mod signal;` and inside `render`, replacing the trailing portion to insert the signal loop after coverage:

```rust
for (rect, track) in layout.signals.iter().zip(inputs.signals.iter()) {
    signal::draw(&mut doc, *rect, &layout.plot, track, opts.signal_shared_max, &opts.theme);
}
```

- [ ] **Step 3: Add fixture**

Append to `crates/igv-render/tests/svg_snapshots.rs`:

```rust
use igv_core::render_inputs::SignalTrackSnapshot;
use igv_core::source::SignalBin;

#[test]
fn signal_only_one_bigwig() {
    let mut inputs = empty_inputs(1, 1000);
    inputs.signals.push(SignalTrackSnapshot {
        display: "chip.bw".into(),
        bins: vec![
            SignalBin { start: 1,    end: 200,  value: 5.0 },
            SignalBin { start: 201,  end: 400,  value: 12.0 },
            SignalBin { start: 401,  end: 600,  value: 3.0 },
            SignalBin { start: 601,  end: 800,  value: 8.5 },
            SignalBin { start: 801,  end: 1000, value: 1.5 },
        ],
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("signal_only_one_bigwig", svg);
}
```

- [ ] **Step 4: Run, accept, re-run**

```bash
cargo test -p igv-render --test svg_snapshots signal_only_one_bigwig
INSTA_UPDATE=always cargo test -p igv-render --test svg_snapshots signal_only_one_bigwig
cargo test -p igv-render
```

- [ ] **Step 5: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): signal track

Bar chart per bin; honors SvgOptions.signal_shared_max for the
shared-scale toggle interactive snapshots inherit from AppState.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: `igv-render` — alignments track

**Files:**
- Create: `crates/igv-render/src/svg/alignments.rs`
- Modify: `crates/igv-render/src/svg/mod.rs`
- Modify: `crates/igv-render/tests/svg_snapshots.rs`

- [ ] **Step 1: Write `crates/igv-render/src/svg/alignments.rs`**

```rust
//! Alignments track: one rect per read, lane-packed using the per-row
//! lane indices already computed by collect_render_inputs / AppState.
//! Mismatches are 1-px ticks; soft-clip handling is left to a follow-up.

use igv_core::render_inputs::BamTrackSnapshot;
use igv_core::source::AlignmentRow;

use crate::layout::{PlotMetrics, Rect};
use crate::options::TrackHeights;
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &BamTrackSnapshot,
    h: &TrackHeights,
    reference_seq: &[u8],
    region_start: u64,
    theme: &GraphicalTheme,
) {
    let label_y = (area.y + h.lane_height / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let max_lanes_visible = (area.h / h.lane_height).max(1);
    let mut shown = 0u32;
    for (row, lane) in track.rows.iter().zip(track.lanes.iter()) {
        if *lane >= max_lanes_visible {
            continue;
        }
        let y = area.y as f64 + (*lane as f64) * h.lane_height as f64 + 1.0;
        let body_h = h.lane_height as f64 - 2.0;
        let fill = if row.is_reverse { theme.read_reverse } else { theme.read_forward };
        draw_read(doc, plot, row, y, body_h, fill, theme);
        if !reference_seq.is_empty() {
            draw_mismatches(doc, plot, row, y, body_h, reference_seq, region_start, theme);
        }
        shown += 1;
    }
    let truncated = track.rows.len().saturating_sub(shown as usize);
    if truncated > 0 {
        doc.text(
            (plot.margin_left + 6) as f64,
            (area.y + area.h - 4) as f64,
            &format!("+{} reads not shown", truncated),
            theme.muted,
            theme.font_px_small,
            TextAnchor::Start,
        );
    }
}

fn draw_read(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
    fill: crate::theme::Rgb,
    _theme: &GraphicalTheme,
) {
    let x0 = plot.bp_to_px(row.ref_start);
    let x1 = plot.bp_to_px(row.ref_end + 1);
    let tip = (body_h / 2.0).min(4.0);
    if x1 - x0 < tip * 2.0 {
        doc.rect(x0, y, (x1 - x0).max(1.0), body_h, fill);
        return;
    }
    if row.is_reverse {
        // Tip on the left.
        doc.polygon(
            &[
                (x0 + tip, y),
                (x1, y),
                (x1, y + body_h),
                (x0 + tip, y + body_h),
                (x0, y + body_h / 2.0),
            ],
            fill,
        );
    } else {
        // Tip on the right.
        doc.polygon(
            &[
                (x0, y),
                (x1 - tip, y),
                (x1, y + body_h / 2.0),
                (x1 - tip, y + body_h),
                (x0, y + body_h),
            ],
            fill,
        );
    }
}

fn draw_mismatches(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
    reference_seq: &[u8],
    region_start: u64,
    theme: &GraphicalTheme,
) {
    // Walk the CIGAR and compare bases to the reference window.
    use igv_core::source::bam::CigarKind;
    let mut ref_pos: u64 = row.ref_start;
    let mut q_pos: usize = 0;
    for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                for _ in 0..op.len {
                    let qbase = row.query_sequence.get(q_pos).copied().unwrap_or(b'N');
                    let ref_idx = (ref_pos as i64 - region_start as i64) as isize;
                    if ref_idx >= 0 && (ref_idx as usize) < reference_seq.len() {
                        let rbase = reference_seq[ref_idx as usize];
                        if !bases_match(qbase, rbase) {
                            let x = plot.bp_to_px(ref_pos);
                            doc.rect(x, y, 1.0, body_h, theme.mismatch_color(qbase));
                        }
                    }
                    ref_pos += 1;
                    q_pos += 1;
                }
            }
            CigarKind::Insertion | CigarKind::SoftClip => { q_pos += op.len as usize; }
            CigarKind::Deletion | CigarKind::Skip => { ref_pos += op.len as u64; }
            CigarKind::HardClip | CigarKind::Padding => {}
        }
    }
}

fn bases_match(a: u8, b: u8) -> bool {
    a.eq_ignore_ascii_case(&b) || a == b'N' || b == b'N'
}
```

- [ ] **Step 2: Wire into `mod.rs`**

Add `pub mod alignments;` and in `render` after the signal loop:

```rust
for (rect, track) in layout.alignments.iter().zip(inputs.bams.iter()) {
    alignments::draw(
        &mut doc,
        *rect,
        &layout.plot,
        track,
        &opts.track_heights,
        &inputs.reference_seq,
        inputs.region.start,
        &opts.theme,
    );
}
```

- [ ] **Step 3: Add fixture**

Append to `crates/igv-render/tests/svg_snapshots.rs`:

```rust
use igv_core::source::bam::{CigarKind, CigarOp};

fn cigar_match(len: u32) -> CigarOp {
    CigarOp { kind: CigarKind::Match, len }
}

#[test]
fn alignments_only_two_reads() {
    let mut inputs = empty_inputs(1, 200);
    inputs.reference_seq = vec![b'A'; 200];
    let mut row1 = fake_read(20, 60);
    row1.cigar = vec![cigar_match(41)];
    row1.query_sequence = vec![b'A'; 41];
    let mut row2 = fake_read(80, 150);
    row2.is_reverse = true;
    row2.cigar = vec![cigar_match(71)];
    row2.query_sequence = vec![b'T'; 71]; // mismatches every column
    inputs.bams.push(BamTrackSnapshot {
        display: "sample.bam".into(),
        rows: vec![row1, row2],
        lanes: vec![0, 0],
        total_lanes: 1,
    });
    let svg = render_svg(&inputs, &SvgOptions::default());
    insta::assert_snapshot!("alignments_only_two_reads", svg);
}
```

- [ ] **Step 4: Run, accept, re-run**

```bash
cargo test -p igv-render --test svg_snapshots alignments_only_two_reads
INSTA_UPDATE=always cargo test -p igv-render --test svg_snapshots alignments_only_two_reads
cargo test -p igv-render
```

- [ ] **Step 5: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): alignments track

Reads as strand-tipped polygons; per-base mismatches drawn as 1-px
ticks coloured by query base. Truncates with "+K reads not shown"
when total_lanes exceeds the band height.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: `igv-render` — `render_png` via resvg

**Files:**
- Create: `crates/igv-render/src/png.rs`
- Modify: `crates/igv-render/src/lib.rs`
- Create: `crates/igv-render/tests/png_smoke.rs`

- [ ] **Step 1: Write `crates/igv-render/src/png.rs`**

```rust
//! PNG output: render the SVG, parse with usvg, raster with resvg into
//! a tiny-skia Pixmap, encode PNG.

use igv_core::render_inputs::RenderInputs;

use crate::error::RenderError;
use crate::options::SvgOptions;

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> Result<Vec<u8>, RenderError> {
    let svg = crate::svg::render(inputs, opts);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .map_err(|e| RenderError::UsvgParse(e.to_string()))?;
    let size = tree.size();
    let w = size.width().ceil() as u32;
    let h = size.height().ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| RenderError::PngEncode("pixmap alloc failed".into()))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    pixmap.encode_png()
        .map_err(|e| RenderError::PngEncode(e.to_string()))
}
```

- [ ] **Step 2: Update `crates/igv-render/src/lib.rs`**

Replace lib.rs with:

```rust
//! Graphical (SVG / PNG) renderer for igv-rs snapshots.

#![forbid(unsafe_code)]

pub mod error;
pub mod layout;
pub mod options;
pub mod png;
pub mod svg;
pub mod theme;

pub use error::RenderError;
pub use options::{SvgOptions, TrackHeights};
pub use theme::GraphicalTheme;

pub fn render_svg(
    inputs: &igv_core::render_inputs::RenderInputs,
    opts: &SvgOptions,
) -> String {
    svg::render(inputs, opts)
}

pub fn render_png(
    inputs: &igv_core::render_inputs::RenderInputs,
    opts: &SvgOptions,
) -> Result<Vec<u8>, RenderError> {
    png::render(inputs, opts)
}
```

- [ ] **Step 3: Write smoke test**

Create `crates/igv-render/tests/png_smoke.rs`:

```rust
use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::render_inputs::RenderInputs;
use igv_render::{render_png, SvgOptions};

#[test]
fn png_smoke_empty_view() {
    let inputs = RenderInputs {
        region: Region::new("chr1", 1, 1000).unwrap(),
        references: vec![],
        reference_seq: vec![],
        variants: vec![],
        bams: vec![],
        annotations: vec![],
        signals: vec![],
        render_mode: RenderMode::DetailedReads,
    };
    let bytes = render_png(&inputs, &SvgOptions::default()).expect("render_png");
    assert!(bytes.len() > 100, "png output too small");
    // PNG magic.
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}
```

- [ ] **Step 4: Build and run**

Run: `cargo test -p igv-render --test png_smoke`
Expected: PASS.

- [ ] **Step 5: Run full igv-render test suite**

Run: `cargo test -p igv-render`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-render/src crates/igv-render/tests
git commit -m "$(cat <<'EOF'
feat(render): render_png via usvg + resvg + tiny-skia

Pure-Rust PNG path. SVG path stays the source of truth — render_png
parses what render_svg produces. Smoke test asserts PNG magic bytes.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: `igv-tui` — `Action::SaveSnapshot`, `pending_snapshot`, the `S` key

**Files:**
- Modify: `crates/igv-tui/Cargo.toml`
- Modify: `crates/igv-tui/src/app/action.rs`
- Modify: `crates/igv-tui/src/app/state.rs`
- Modify: `crates/igv-tui/src/input.rs`

- [ ] **Step 1: Add dep on `igv-render` to `crates/igv-tui/Cargo.toml`**

Replace the `igv-core` dep line area:
```toml
igv-core = { path = "../igv-core" }
igv-render = { path = "../igv-render" }
```

- [ ] **Step 2: Add `Action::SaveSnapshot` and the `SnapshotFormat` enum**

In `crates/igv-tui/src/app/action.rs`, add at top of the enum:

```rust
/// Save the current view to disk. `path = None` → auto-named in cwd.
SaveSnapshot { path: Option<std::path::PathBuf>, format: SnapshotFormat },
```

And below the enum:

```rust
#[derive(Debug, Clone, Copy)]
pub enum SnapshotFormat {
    Svg,
    Png,
}

impl SnapshotFormat {
    pub fn from_path(p: &std::path::Path) -> Self {
        match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) {
            Some(ref s) if s == "png" => Self::Png,
            _ => Self::Svg,
        }
    }
}
```

- [ ] **Step 3: Add `pending_snapshot` field to `AppState`**

In `crates/igv-tui/src/app/state.rs`, near the other UI state fields (above `pub generation: u64,`):

```rust
pub pending_snapshot: Option<SnapshotJob>,
```

And add at module top (after the existing `use` block):

```rust
use crate::app::action::SnapshotFormat;

#[derive(Debug, Clone)]
pub struct SnapshotJob {
    pub path: Option<std::path::PathBuf>,
    pub format: SnapshotFormat,
}
```

- [ ] **Step 4: Initialise `pending_snapshot: None` in `main.rs`**

In `crates/igv-tui/src/main.rs`, in the `AppState { ... }` literal, add `pending_snapshot: None,` next to `should_quit: false,`.

- [ ] **Step 5: Handle `SaveSnapshot` in `AppState::apply`**

In `crates/igv-tui/src/app/state.rs`, add a new arm to the `match action` in `apply`:

```rust
Action::SaveSnapshot { path, format } => {
    if self.loading {
        self.set_status(StatusKind::Warning, "snapshot: still loading, try again");
    } else {
        self.pending_snapshot = Some(SnapshotJob { path, format });
    }
    None
}
```

- [ ] **Step 6: Map `S` to `SaveSnapshot` in `crates/igv-tui/src/input.rs`**

Inside the `match code` block, add a new arm:

```rust
KeyCode::Char('S') => Action::SaveSnapshot { path: None, format: crate::app::action::SnapshotFormat::Svg },
```

- [ ] **Step 7: Add a unit test for the key binding**

In the test module of `input.rs`:

```rust
#[test]
fn capital_s_saves_svg_snapshot() {
    let mut s = InputState::default();
    let act = s.map(&key('S'), false);
    assert!(matches!(
        act,
        Action::SaveSnapshot { path: None, format: crate::app::action::SnapshotFormat::Svg }
    ));
}
```

- [ ] **Step 8: Build and test**

Run: `cargo test -p igv-tui input::tests::capital_s_saves_svg_snapshot`
Expected: PASS.

- [ ] **Step 9: Confirm full crate still tests**

Run: `cargo test -p igv-tui`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/igv-tui
git commit -m "$(cat <<'EOF'
feat(tui): S key sets a pending snapshot job

Action::SaveSnapshot is pure state: it sets state.pending_snapshot.
The main loop drains the pending job after each apply (next task),
keeping IO out of the state machine.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 15: `igv-tui` — palette `:snapshot <path>`

**Files:**
- Modify: `crates/igv-tui/src/app/state.rs`

- [ ] **Step 1: Intercept `snapshot`/`snap` in `CommandSubmit`**

In `crates/igv-tui/src/app/state.rs`, find the `Action::CommandSubmit(buf) => { ... }` arm. At the top of that arm (right after `let trimmed = buf.trim();`) add:

```rust
// `snapshot path` / `snap path`, optionally with leading ':'.
if let Some(rest) = trimmed
    .strip_prefix(':').unwrap_or(trimmed)
    .strip_prefix("snapshot ")
    .or_else(|| trimmed.strip_prefix(':').unwrap_or(trimmed).strip_prefix("snap "))
{
    let path = std::path::PathBuf::from(rest.trim());
    if path.as_os_str().is_empty() {
        self.set_status(StatusKind::Error, "snapshot: missing path");
        return None;
    }
    let format = crate::app::action::SnapshotFormat::from_path(&path);
    if self.loading {
        self.set_status(StatusKind::Warning, "snapshot: still loading, try again");
    } else {
        self.pending_snapshot = Some(SnapshotJob { path: Some(path), format });
    }
    return None;
}
```

- [ ] **Step 2: Refactor parsing into a pure function**

Constructing `AppState` in a unit test is awkward (it owns several `Arc<dyn ...>` handles). Instead, factor the parsing into a free function that the apply arm calls.

In `state.rs` add a free function above `impl AppState`:

```rust
/// Try to parse the palette buffer as a `snapshot`/`snap` command.
/// Returns `Some((path, format))` when matched, `None` otherwise.
pub(crate) fn parse_snapshot_command(
    trimmed: &str,
) -> Option<(std::path::PathBuf, crate::app::action::SnapshotFormat)> {
    let body = trimmed.strip_prefix(':').unwrap_or(trimmed);
    let rest = body.strip_prefix("snapshot ").or_else(|| body.strip_prefix("snap "))?;
    let path = std::path::PathBuf::from(rest.trim());
    if path.as_os_str().is_empty() { return None; }
    let format = crate::app::action::SnapshotFormat::from_path(&path);
    Some((path, format))
}
```

Then change Step 1's snippet to use this helper:

```rust
if let Some((path, format)) = parse_snapshot_command(trimmed) {
    if self.loading {
        self.set_status(StatusKind::Warning, "snapshot: still loading, try again");
    } else {
        self.pending_snapshot = Some(SnapshotJob { path: Some(path), format });
    }
    return None;
}
```

And add tests right next to the helper:

```rust
#[cfg(test)]
mod snapshot_cmd_tests {
    use super::parse_snapshot_command;
    use crate::app::action::SnapshotFormat;

    #[test]
    fn parses_snapshot_with_path() {
        let (p, f) = parse_snapshot_command("snapshot foo.svg").unwrap();
        assert_eq!(p.to_str().unwrap(), "foo.svg");
        assert!(matches!(f, SnapshotFormat::Svg));
    }

    #[test]
    fn parses_snap_alias_with_png() {
        let (p, f) = parse_snapshot_command("snap out/x.png").unwrap();
        assert_eq!(p.to_str().unwrap(), "out/x.png");
        assert!(matches!(f, SnapshotFormat::Png));
    }

    #[test]
    fn ignores_leading_colon() {
        assert!(parse_snapshot_command(":snapshot foo.svg").is_some());
    }

    #[test]
    fn rejects_empty_path() {
        assert!(parse_snapshot_command("snapshot ").is_none());
    }

    #[test]
    fn rejects_other_commands() {
        assert!(parse_snapshot_command("HER2").is_none());
        assert!(parse_snapshot_command("chr1:1000-2000").is_none());
    }
}
```

- [ ] **Step 3: Test**

Run: `cargo test -p igv-tui snapshot_cmd_tests`
Expected: PASS (5 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/igv-tui/src/app/state.rs
git commit -m "$(cat <<'EOF'
feat(tui): :snapshot palette command

`snapshot <path>` and `snap <path>` (optional leading colon) parse to
SaveSnapshot, with the file extension picking SVG vs PNG. Falls
through to the existing region/gene parser when not matched.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 16: `igv-tui` — main-loop snapshot writer + auto-naming

**Files:**
- Create: `crates/igv-tui/src/snapshot/mod.rs`
- Create: `crates/igv-tui/src/snapshot/naming.rs`
- Create: `crates/igv-tui/src/snapshot/writer.rs`
- Modify: `crates/igv-tui/src/lib.rs`
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Write `crates/igv-tui/src/snapshot/naming.rs`**

```rust
//! Filename builders shared by interactive auto-naming and batch.

use std::path::{Path, PathBuf};

use igv_core::region::Region;

use crate::app::action::SnapshotFormat;

pub fn ext(format: SnapshotFormat) -> &'static str {
    match format {
        SnapshotFormat::Svg => "svg",
        SnapshotFormat::Png => "png",
    }
}

/// Default name for the `S`-key snapshot in cwd:
/// `snapshot_<chrom>_<start>_<end>.<ext>`.
pub fn auto_name(region: &Region, format: SnapshotFormat) -> PathBuf {
    PathBuf::from(format!(
        "snapshot_{}_{}_{}.{}",
        sanitize(&region.chrom), region.start, region.end, ext(format),
    ))
}

/// Name for batch outputs: `<label>_<chrom>_<start>_<end>.<ext>`.
/// `label = None` → `<chrom>_<start>_<end>.<ext>`.
pub fn batch_name(
    out_dir: &Path,
    label: Option<&str>,
    region: &Region,
    format: SnapshotFormat,
) -> PathBuf {
    let stem = match label {
        Some(l) if !l.trim().is_empty() => format!(
            "{}_{}_{}_{}",
            sanitize(l), sanitize(&region.chrom), region.start, region.end
        ),
        _ => format!(
            "{}_{}_{}",
            sanitize(&region.chrom), region.start, region.end
        ),
    };
    out_dir.join(format!("{}.{}", stem, ext(format)))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_name_default() {
        let r = Region::new("chr1", 1000, 2000).unwrap();
        let p = auto_name(&r, SnapshotFormat::Svg);
        assert_eq!(p.to_str().unwrap(), "snapshot_chr1_1000_2000.svg");
    }

    #[test]
    fn batch_name_with_label() {
        let r = Region::new("chr2", 5, 10).unwrap();
        let p = batch_name(Path::new("out"), Some("BRCA1"), &r, SnapshotFormat::Png);
        assert_eq!(p.to_str().unwrap(), "out/BRCA1_chr2_5_10.png");
    }

    #[test]
    fn batch_name_without_label() {
        let r = Region::new("chr2", 5, 10).unwrap();
        let p = batch_name(Path::new("out"), None, &r, SnapshotFormat::Svg);
        assert_eq!(p.to_str().unwrap(), "out/chr2_5_10.svg");
    }

    #[test]
    fn sanitize_strips_path_separators() {
        let r = Region::new("chr1", 1, 2).unwrap();
        let p = batch_name(Path::new("out"), Some("a/b\\c"), &r, SnapshotFormat::Svg);
        assert_eq!(p.to_str().unwrap(), "out/a_b_c_chr1_1_2.svg");
    }
}
```

- [ ] **Step 2: Write `crates/igv-tui/src/snapshot/writer.rs`**

```rust
//! Build a RenderInputs from AppState and write SVG/PNG.

use std::path::Path;

use anyhow::{Context, Result};
use igv_core::render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, RenderInputs, SignalTrackSnapshot,
};
use igv_render::{render_png, render_svg, SvgOptions};

use crate::app::action::SnapshotFormat;
use crate::app::state::AppState;

/// Build a `RenderInputs` snapshot from the current TUI state.
pub fn inputs_from_state(state: &AppState) -> RenderInputs {
    let bams = state
        .bams
        .iter()
        .enumerate()
        .map(|(i, t)| BamTrackSnapshot {
            display: t.display.clone(),
            rows: state.bam_rows.get(i).cloned().unwrap_or_default(),
            lanes: state.bam_lanes.get(i).cloned().unwrap_or_default(),
            total_lanes: state.bam_total_lanes.get(i).copied().unwrap_or(0),
        })
        .collect();
    let annotations = state
        .annotations
        .iter()
        .enumerate()
        .map(|(i, t)| AnnotationTrackSnapshot {
            display: t.display.clone(),
            transcripts: state.annotation_rows.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    let signals = state
        .signals
        .iter()
        .enumerate()
        .map(|(i, t)| SignalTrackSnapshot {
            display: t.display.clone(),
            bins: state.signal_bins.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    RenderInputs {
        region: state.region.clone(),
        references: state.references.clone(),
        reference_seq: state.reference_seq.clone(),
        variants: state.variants.clone(),
        bams,
        annotations,
        signals,
        render_mode: state.render_mode(),
    }
}

/// Compute the shared-scale max if AppState's signal_shared_scale is on.
fn signal_shared_max(state: &AppState) -> Option<f32> {
    if !state.signal_shared_scale {
        return None;
    }
    let m = state
        .signal_bins
        .iter()
        .flatten()
        .map(|b| b.value)
        .fold(0.0_f32, f32::max);
    Some(m)
}

pub fn write_snapshot(
    state: &AppState,
    path: &Path,
    format: SnapshotFormat,
) -> Result<()> {
    let inputs = inputs_from_state(state);
    let mut opts = SvgOptions::default();
    opts.signal_shared_max = signal_shared_max(state);
    match format {
        SnapshotFormat::Svg => {
            let svg = render_svg(&inputs, &opts);
            std::fs::write(path, svg).with_context(|| format!("write {}", path.display()))?;
        }
        SnapshotFormat::Png => {
            let bytes = render_png(&inputs, &opts).with_context(|| "render PNG")?;
            std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Write `crates/igv-tui/src/snapshot/mod.rs`**

```rust
pub mod naming;
pub mod writer;
```

- [ ] **Step 4: Wire into `crates/igv-tui/src/lib.rs`**

Read current contents of `crates/igv-tui/src/lib.rs`. Append `pub mod snapshot;` to the existing module list.

- [ ] **Step 5: Drain `pending_snapshot` in the main loop**

In `crates/igv-tui/src/main.rs`, find the part of `run_loop` that calls `state.apply(action)`. Right after that block, add a draining helper. Easiest: extract a small helper function below `run_loop`:

```rust
fn drain_snapshot(state: &mut AppState) {
    let Some(job) = state.pending_snapshot.take() else { return };
    let path = job
        .path
        .clone()
        .unwrap_or_else(|| igv_tui::snapshot::naming::auto_name(&state.region, job.format));
    match igv_tui::snapshot::writer::write_snapshot(state, &path, job.format) {
        Ok(()) => state.set_status(
            igv_tui::app::state::StatusKind::Info,
            format!("snapshot → {}", path.display()),
        ),
        Err(e) => state.set_status(
            igv_tui::app::state::StatusKind::Error,
            format!("snapshot failed: {}", e),
        ),
    }
}
```

And call `drain_snapshot(state);` at the start of each loop iteration after `state.apply`. Concretely, change the existing dispatch block:

```rust
if let Some(req) = state.apply(action) {
    loader.dispatch(req);
}
```

to:

```rust
if let Some(req) = state.apply(action) {
    loader.dispatch(req);
}
drain_snapshot(state);
```

(There are two such call sites — `events.next()` arm and the initial `state.apply(Action::Goto(...))` setup. The setup site does not need draining since no SaveSnapshot is possible there.)

- [ ] **Step 6: Build**

Run: `cargo build -p igv-tui`
Expected: PASS (warnings about unused are OK if any).

- [ ] **Step 7: Run snapshot tests**

Run: `cargo test -p igv-tui snapshot::naming`
Expected: PASS (4 tests).

- [ ] **Step 8: Manual smoke test (optional, recommended)**

```bash
cargo run -p igv-tui -- crates/igv-core/tests/data/test.fa  # any test FASTA you have
# Inside TUI: press S
# Expect: status line "snapshot → snapshot_<chrom>_<s>_<e>.svg"
```

If you don't have test data handy, skip this step.

- [ ] **Step 9: Commit**

```bash
git add crates/igv-tui
git commit -m "$(cat <<'EOF'
feat(tui): write snapshot file when S/`:snapshot` triggers

Adds snapshot::{naming, writer}. inputs_from_state clones the
AppState track buffers into a RenderInputs (no source refetch — the
TUI already has the data on screen). Main loop drains
state.pending_snapshot after every apply().

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 17: `igv-tui` — CLI flags for batch + headless detection

**Files:**
- Modify: `crates/igv-tui/src/cli.rs`
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Add batch flags to `cli.rs`**

Append before the closing brace of `pub struct Cli`:

```rust
    /// Render snapshots for every region in this BED file (no TUI).
    /// Mutually exclusive with --snapshot-genes.
    #[arg(long = "snapshot-bed")]
    pub snapshot_bed: Option<PathBuf>,

    /// Render snapshots for every gene name in this newline-separated
    /// file (no TUI). Requires at least one -g/--annotation. Mutually
    /// exclusive with --snapshot-bed.
    #[arg(long = "snapshot-genes")]
    pub snapshot_genes: Option<PathBuf>,

    /// Output directory for batch snapshots. Required when
    /// --snapshot-bed or --snapshot-genes is set.
    #[arg(long = "snapshot-out")]
    pub snapshot_out: Option<PathBuf>,

    /// Output format for snapshots: `svg` (default) or `png`.
    #[arg(long = "snapshot-format", default_value = "svg")]
    pub snapshot_format: String,

    /// Image width in px for snapshots.
    #[arg(long = "snapshot-width", default_value_t = 1200)]
    pub snapshot_width: u32,

    /// Padding fraction added to each side of every batch region.
    #[arg(long = "snapshot-flank", default_value_t = 0.1)]
    pub snapshot_flank: f64,

    /// Snapshot color theme: `igv` (default) or `tui`.
    #[arg(long = "snapshot-theme", default_value = "igv")]
    pub snapshot_theme: String,
```

- [ ] **Step 2: Detect headless mode in `main.rs`**

In `main.rs`, near the top of `main()` (after `let args = cli::Cli::parse();`), insert the headless branch *before* `enable_raw_mode` etc.:

```rust
// Headless batch path. Implementation lands in Task 18 / 19; for now
// just bail if conflicting flags are set.
if args.snapshot_bed.is_some() && args.snapshot_genes.is_some() {
    return Err(anyhow!("--snapshot-bed and --snapshot-genes are mutually exclusive"));
}
if (args.snapshot_bed.is_some() || args.snapshot_genes.is_some())
    && args.snapshot_out.is_none()
{
    return Err(anyhow!("--snapshot-out is required with batch flags"));
}
```

- [ ] **Step 3: Build**

Run: `cargo build -p igv-tui`
Expected: PASS.

- [ ] **Step 4: Argument parsing test**

Add a small test `crates/igv-tui/tests/cli_snapshot_args.rs`:

```rust
use clap::Parser;
use igv_tui::cli::Cli;

#[test]
fn defaults_are_sensible() {
    // The fasta arg is positional; supply a placeholder path.
    let cli = Cli::parse_from(["igv-rs", "ref.fa"]);
    assert_eq!(cli.snapshot_format, "svg");
    assert_eq!(cli.snapshot_width, 1200);
    assert!((cli.snapshot_flank - 0.1).abs() < 1e-9);
    assert_eq!(cli.snapshot_theme, "igv");
    assert!(cli.snapshot_bed.is_none());
    assert!(cli.snapshot_genes.is_none());
}

#[test]
fn batch_flags_parse() {
    let cli = Cli::parse_from([
        "igv-rs", "ref.fa",
        "--snapshot-bed", "regions.bed",
        "--snapshot-out", "out/",
        "--snapshot-format", "png",
        "--snapshot-width", "1600",
        "--snapshot-flank", "0.2",
    ]);
    assert_eq!(cli.snapshot_bed.unwrap().to_str().unwrap(), "regions.bed");
    assert_eq!(cli.snapshot_out.unwrap().to_str().unwrap(), "out/");
    assert_eq!(cli.snapshot_format, "png");
    assert_eq!(cli.snapshot_width, 1600);
    assert!((cli.snapshot_flank - 0.2).abs() < 1e-9);
}
```

(May need `pub use cli::Cli;` re-export in `crates/igv-tui/src/lib.rs`. Check `lib.rs` — if `cli` is already a public module, this works; otherwise add `pub mod cli;`.)

- [ ] **Step 5: Run**

Run: `cargo test -p igv-tui --test cli_snapshot_args`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/igv-tui
git commit -m "$(cat <<'EOF'
feat(tui): batch snapshot CLI flags + mutual-exclusion guard

--snapshot-bed / --snapshot-genes / --snapshot-out gate the headless
path. Conflicting combos error before raw mode is enabled. Default
format svg, width 1200, flank 0.1, theme igv.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 18: `igv-tui` — `--snapshot-bed` batch path

**Files:**
- Create: `crates/igv-tui/src/snapshot/batch.rs`
- Create: `crates/igv-tui/src/snapshot/regions.rs`
- Modify: `crates/igv-tui/src/snapshot/mod.rs`
- Modify: `crates/igv-tui/src/main.rs`
- Create: `crates/igv-tui/tests/snapshot_batch_bed.rs`

- [ ] **Step 1: Write `crates/igv-tui/src/snapshot/regions.rs`**

```rust
//! Parse a region list (BED or gene-name file) into a vector of
//! (region, optional label) pairs. Pure / sync — no IO beyond reading
//! the input file.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use igv_core::region::Region;

#[derive(Debug, Clone)]
pub struct LabeledRegion {
    pub region: Region,
    pub label: Option<String>,
}

pub fn parse_bed(path: &Path) -> Result<Vec<LabeledRegion>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("track") || trimmed.starts_with("browser") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 3 {
            return Err(anyhow!(
                "{}: line {}: BED needs ≥3 tab-separated columns",
                path.display(), lineno + 1
            ));
        }
        let chrom = cols[0].to_string();
        let start: u64 = cols[1].parse().with_context(|| format!(
            "{}: line {}: bad start", path.display(), lineno + 1
        ))?;
        let end: u64 = cols[2].parse().with_context(|| format!(
            "{}: line {}: bad end", path.display(), lineno + 1
        ))?;
        // BED is 0-based half-open. Convert to igv-core's 1-based inclusive.
        if end == 0 || end <= start {
            return Err(anyhow!(
                "{}: line {}: end {} <= start {}", path.display(), lineno + 1, end, start
            ));
        }
        let region = Region::new(chrom, start + 1, end)?;
        let label = cols.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty());
        out.push(LabeledRegion { region, label });
    }
    Ok(out)
}

/// Apply a flank fraction symmetrically. Output region width is
/// `floor(input * (1 + 2f))`. Clamps `start` to ≥1; chromosome-end
/// clamping happens later when `references` is available.
pub fn apply_flank(region: &Region, flank: f64) -> Region {
    let w = region.width();
    let pad = (w as f64 * flank).floor() as u64;
    let new_start = region.start.saturating_sub(pad).max(1);
    let new_end = region.end.saturating_add(pad);
    Region::new(region.chrom.clone(), new_start, new_end)
        .unwrap_or_else(|_| region.clone())
}

/// Final clamp using a chromosome length lookup.
pub fn clamp_to_chrom(region: &Region, chrom_len: Option<u64>) -> Region {
    let Some(chrom_len) = chrom_len else { return region.clone() };
    region.clamp_to(chrom_len).unwrap_or_else(|_| region.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flank_zero_is_identity() {
        let r = Region::new("chr1", 100, 200).unwrap();
        let f = apply_flank(&r, 0.0);
        assert_eq!(f.start, 100);
        assert_eq!(f.end, 200);
    }

    #[test]
    fn flank_ten_percent_pads_each_side() {
        let r = Region::new("chr1", 100, 200).unwrap();
        let f = apply_flank(&r, 0.1);
        // width = 101, pad = floor(10.1) = 10
        assert_eq!(f.start, 90);
        assert_eq!(f.end, 210);
    }

    #[test]
    fn flank_clamps_start_to_one() {
        let r = Region::new("chr1", 5, 10).unwrap();
        let f = apply_flank(&r, 1.0); // pad = 6
        assert_eq!(f.start, 1);
        assert_eq!(f.end, 16);
    }

    #[test]
    fn parse_bed_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("r.bed");
        std::fs::write(&p, "chr1\t99\t200\tBRCA1\nchr2\t499\t600\n").unwrap();
        let v = parse_bed(&p).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].region.start, 100);
        assert_eq!(v[0].region.end, 200);
        assert_eq!(v[0].label.as_deref(), Some("BRCA1"));
        assert_eq!(v[1].label, None);
    }
}
```

- [ ] **Step 2: Write `crates/igv-tui/src/snapshot/batch.rs`**

```rust
//! Headless batch entry: render every region in a list to its own
//! file. No TUI, no raw mode.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use igv_core::collect_render_inputs;
use igv_core::region::Region;
use igv_core::source::{
    AnnotationSource, BamSource, FastaSource, FetchOpts, FetchSignalOpts, RefMeta,
    SignalSource, VcfSource,
};
use igv_core::{CollectOpts, Sources};
use igv_render::{render_png, render_svg, GraphicalTheme, SvgOptions};
use tracing::{info, warn};

use crate::app::action::SnapshotFormat;
use crate::snapshot::naming::batch_name;
use crate::snapshot::regions::{apply_flank, clamp_to_chrom, LabeledRegion};

#[derive(Debug, Clone)]
pub struct BatchOpts {
    pub out_dir: PathBuf,
    pub format: SnapshotFormat,
    pub width_px: u32,
    pub flank: f64,
    pub theme: GraphicalTheme,
}

pub async fn run(
    fasta: Arc<dyn FastaSource>,
    vcf: Option<Arc<dyn VcfSource>>,
    bams: Vec<(String, Arc<dyn BamSource>)>,
    annotations: Vec<(String, Arc<dyn AnnotationSource>)>,
    signals: Vec<(String, Arc<dyn SignalSource>)>,
    references: Vec<RefMeta>,
    regions: Vec<LabeledRegion>,
    batch: BatchOpts,
) -> Result<()> {
    std::fs::create_dir_all(&batch.out_dir)
        .with_context(|| format!("create {}", batch.out_dir.display()))?;

    let sources = Sources {
        fasta,
        vcf,
        bams,
        annotations,
        signals,
        references: references.clone(),
    };

    let mut rendered = 0usize;
    let mut skipped = 0usize;
    let total = regions.len();
    let thresholds = igv_core::render::Thresholds::default();

    for (i, lr) in regions.iter().enumerate() {
        let chrom_len = references
            .iter()
            .find(|m| m.name == lr.region.chrom)
            .map(|m| m.length);
        let padded = clamp_to_chrom(&apply_flank(&lr.region, batch.flank), chrom_len);
        let mode = thresholds.classify(padded.width());
        let collect_opts = CollectOpts {
            fetch_opts: FetchOpts::default(),
            signal_opts: FetchSignalOpts::default(),
            render_mode: mode,
        };
        let inputs = match collect_render_inputs(&sources, &padded, &collect_opts).await {
            Ok(v) => v,
            Err(e) => {
                warn!("[{}/{}] {}: collect failed: {}", i + 1, total, padded, e);
                skipped += 1;
                continue;
            }
        };
        let mut opts = SvgOptions {
            width_px: batch.width_px,
            theme: batch.theme.clone(),
            ..SvgOptions::default()
        };
        opts.signal_shared_max = None; // batch always uses per-track scale
        let path = batch_name(&batch.out_dir, lr.label.as_deref(), &padded, batch.format);
        let result = match batch.format {
            SnapshotFormat::Svg => std::fs::write(&path, render_svg(&inputs, &opts)).map_err(anyhow::Error::from),
            SnapshotFormat::Png => match render_png(&inputs, &opts) {
                Ok(b) => std::fs::write(&path, b).map_err(anyhow::Error::from),
                Err(e) => Err(anyhow::anyhow!("png render: {}", e)),
            },
        };
        match result {
            Ok(()) => {
                info!("[{}/{}] {} → {}", i + 1, total, padded, path.display());
                eprintln!("[{}/{}] {} → {}", i + 1, total, padded, path.display());
                rendered += 1;
            }
            Err(e) => {
                warn!("[{}/{}] {}: write failed: {}", i + 1, total, padded, e);
                eprintln!("[{}/{}] {}: write failed: {}", i + 1, total, padded, e);
                skipped += 1;
            }
        }
    }

    eprintln!("snapshot: rendered {}, skipped {} (total {})", rendered, skipped, total);
    if rendered == 0 && total > 0 {
        anyhow::bail!("no snapshots rendered");
    }
    Ok(())
}

/// Resolve a parsed format string from CLI to the SnapshotFormat enum.
pub fn parse_format(s: &str) -> Result<SnapshotFormat> {
    match s.to_ascii_lowercase().as_str() {
        "svg" => Ok(SnapshotFormat::Svg),
        "png" => Ok(SnapshotFormat::Png),
        _ => Err(anyhow::anyhow!("unknown snapshot format '{}' (svg|png)", s)),
    }
}

/// Resolve a theme string from CLI to a GraphicalTheme. The `tui`
/// option falls back to `igv_light` for now (full crossterm→RGB
/// translation is a follow-up; see spec §Theme).
pub fn parse_theme(s: &str) -> Result<GraphicalTheme> {
    match s.to_ascii_lowercase().as_str() {
        "igv" | "tui" => Ok(GraphicalTheme::igv_light()),
        _ => Err(anyhow::anyhow!("unknown snapshot theme '{}' (igv|tui)", s)),
    }
}

/// Region-only entry point used by `--snapshot-bed`.
pub fn label_from_bed_path(p: &std::path::Path) -> Option<String> {
    p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
}

/// Region helper exposed only for tests.
pub fn _padded_region(region: &Region, flank: f64, chrom_len: Option<u64>) -> Region {
    clamp_to_chrom(&apply_flank(region, flank), chrom_len)
}
```

- [ ] **Step 3: Update `crates/igv-tui/src/snapshot/mod.rs`**

```rust
pub mod batch;
pub mod naming;
pub mod regions;
pub mod writer;
```

- [ ] **Step 4: Wire batch path into `main.rs`**

In `crates/igv-tui/src/main.rs`, replace the placeholder headless guard from Task 17 with a real branch. Insert right after the existing `if args.snapshot_bed.is_some() && args.snapshot_genes.is_some()` and `if (...).is_none()` validation, but **before** `enable_raw_mode`:

```rust
if let Some(bed_path) = args.snapshot_bed.as_deref() {
    let regions = igv_tui::snapshot::regions::parse_bed(bed_path)?;
    let format = igv_tui::snapshot::batch::parse_format(&args.snapshot_format)?;
    let theme = igv_tui::snapshot::batch::parse_theme(&args.snapshot_theme)?;
    let batch = igv_tui::snapshot::batch::BatchOpts {
        out_dir: args.snapshot_out.clone().unwrap(),
        format,
        width_px: args.snapshot_width,
        flank: args.snapshot_flank,
        theme,
    };
    let bams_owned = bams
        .iter()
        .map(|t| (t.display.clone(), Arc::clone(&t.source)))
        .collect();
    let annotations_owned = annotations
        .iter()
        .map(|t| (t.display.clone(), Arc::clone(&t.source)))
        .collect();
    let signals_owned = signals
        .iter()
        .map(|t| (t.display.clone(), Arc::clone(&t.source)))
        .collect();
    return igv_tui::snapshot::batch::run(
        fasta,
        vcf,
        bams_owned,
        annotations_owned,
        signals_owned,
        references.clone(),
        regions,
        batch,
    )
    .await;
}
```

**Placement:** insert this branch *after* the local `references = fasta.references().await?` binding and after `bams`/`annotations`/`signals` Vecs are built (around line 113 in current `main.rs`), but *before* the `AppState { ... }` construction. The headless path returns from `main` and never reaches `enable_raw_mode`.

- [ ] **Step 5: Build**

Run: `cargo build -p igv-tui`
Expected: PASS.

- [ ] **Step 6: Integration test**

Create `crates/igv-tui/tests/snapshot_batch_bed.rs`. Use the existing test fixtures in `crates/igv-core/tests/data/` (a small FASTA exists from earlier features — confirm with `ls crates/igv-core/tests/data/` and adapt names below).

```rust
//! End-to-end batch BED snapshot test. Calls the batch entry directly
//! (not the binary) so we can run with no FASTA index handling.

use std::path::PathBuf;
use std::sync::Arc;

use igv_core::region::Region;
use igv_core::source::{FastaSource, RefMeta};
use igv_render::GraphicalTheme;
use igv_tui::app::action::SnapshotFormat;
use igv_tui::snapshot::batch::{run, BatchOpts};
use igv_tui::snapshot::regions::LabeledRegion;

use async_trait::async_trait;

struct StubFasta;

#[async_trait]
impl FastaSource for StubFasta {
    async fn references(&self) -> igv_core::error::Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 1000 }])
    }
    async fn fetch(&self, _r: &Region) -> igv_core::error::Result<Vec<u8>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn batch_bed_emits_one_svg_per_region() {
    let dir = tempfile::tempdir().unwrap();
    let opts = BatchOpts {
        out_dir: dir.path().to_path_buf(),
        format: SnapshotFormat::Svg,
        width_px: 800,
        flank: 0.0,
        theme: GraphicalTheme::igv_light(),
    };
    let regions = vec![
        LabeledRegion {
            region: Region::new("chr1", 100, 200).unwrap(),
            label: Some("A".into()),
        },
        LabeledRegion {
            region: Region::new("chr1", 500, 600).unwrap(),
            label: None,
        },
    ];
    run(
        Arc::new(StubFasta) as Arc<dyn FastaSource>,
        None,
        vec![],
        vec![],
        vec![],
        vec![RefMeta { name: "chr1".into(), length: 1000 }],
        regions,
        opts,
    )
    .await
    .unwrap();

    let out_a = dir.path().join("A_chr1_100_200.svg");
    let out_b = dir.path().join("chr1_500_600.svg");
    assert!(out_a.exists(), "missing {}", out_a.display());
    assert!(out_b.exists(), "missing {}", out_b.display());
    let body = std::fs::read_to_string(&out_a).unwrap();
    assert!(body.starts_with("<?xml"), "not SVG-shaped");

    // Suppress unused warning for the dir path bind.
    let _ = PathBuf::from(dir.path());
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p igv-tui --test snapshot_batch_bed`
Expected: PASS.

Run: `cargo test -p igv-tui`
Expected: PASS.

- [ ] **Step 8: Manual smoke (optional)**

```bash
echo -e "chr1\t99\t1000\tdemo" > /tmp/x.bed
cargo run -p igv-tui -- ref.fa --snapshot-bed /tmp/x.bed --snapshot-out /tmp/snaps/
ls /tmp/snaps/  # expect demo_chr1_100_1000.svg
```

- [ ] **Step 9: Commit**

```bash
git add crates/igv-tui
git commit -m "$(cat <<'EOF'
feat(tui): --snapshot-bed batch path

Headless renderer for BED region lists. Skips ratatui entirely,
fans out one render per region, applies --snapshot-flank and clamps
to chromosome length, never aborts on per-region errors. Reports
rendered/skipped counts to stderr.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 19: `igv-tui` — `--snapshot-genes` batch path

**Files:**
- Create: `crates/igv-tui/src/snapshot/genes.rs`
- Modify: `crates/igv-tui/src/snapshot/mod.rs`
- Modify: `crates/igv-tui/src/main.rs`
- Create: `crates/igv-tui/tests/snapshot_batch_genes.rs`

- [ ] **Step 1: Write `crates/igv-tui/src/snapshot/genes.rs`**

```rust
//! Resolve a list of gene names into LabeledRegions, using loaded
//! AnnotationSource backends. Mirrors AppState::find_gene_region's
//! "union of matches on the same chromosome" rule.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use igv_core::region::Region;
use igv_core::source::AnnotationSource;
use tracing::warn;

use crate::snapshot::regions::LabeledRegion;

/// Read a one-name-per-line file. Lines starting with `#` and blank
/// lines are skipped. Returns the (case-preserved) names.
pub fn read_names(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// Resolve each name into a `LabeledRegion` by querying every
/// annotation source. Names not found are dropped with a warning.
pub fn resolve(
    names: &[String],
    sources: &[Arc<dyn AnnotationSource>],
) -> Vec<LabeledRegion> {
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        match resolve_one(name, sources) {
            Some(lr) => out.push(lr),
            None => {
                warn!("snapshot-genes: unknown gene '{}'", name);
                eprintln!("snapshot-genes: unknown gene '{}'", name);
            }
        }
    }
    out
}

fn resolve_one(query: &str, sources: &[Arc<dyn AnnotationSource>]) -> Option<LabeledRegion> {
    let mut chrom: Option<String> = None;
    let mut span: Option<(u64, u64)> = None;
    for src in sources {
        for (c, tx) in src.find_by_name(query) {
            let Some((s, e)) = tx.span() else { continue };
            match &chrom {
                None => {
                    chrom = Some(c);
                    span = Some((s, e));
                }
                Some(existing) if existing == &c => {
                    let (cs, ce) = span.unwrap();
                    span = Some((cs.min(s), ce.max(e)));
                }
                Some(_) => {} // ignore matches on different chroms
            }
        }
    }
    let chrom = chrom?;
    let (s, e) = span?;
    Region::new(chrom, s, e)
        .ok()
        .map(|region| LabeledRegion { region, label: Some(query.to_string()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_names_strips_blank_and_comments() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("g.txt");
        std::fs::write(&p, "BRCA1\n# comment\n\nTP53\n").unwrap();
        let v = read_names(&p).unwrap();
        assert_eq!(v, vec!["BRCA1".to_string(), "TP53".to_string()]);
    }
}
```

- [ ] **Step 2: Add to `crates/igv-tui/src/snapshot/mod.rs`**

```rust
pub mod batch;
pub mod genes;
pub mod naming;
pub mod regions;
pub mod writer;
```

- [ ] **Step 3: Wire `--snapshot-genes` into `main.rs`**

In the headless block in `main.rs`, add (after the `--snapshot-bed` short-circuit `return`):

```rust
if let Some(genes_path) = args.snapshot_genes.as_deref() {
    if annotation_sources.is_empty() {
        return Err(anyhow!("--snapshot-genes requires at least one -g/--annotation"));
    }
    let names = igv_tui::snapshot::genes::read_names(genes_path)?;
    let regions = igv_tui::snapshot::genes::resolve(&names, &annotation_sources);
    if regions.is_empty() {
        return Err(anyhow!("--snapshot-genes: no genes resolved"));
    }
    let format = igv_tui::snapshot::batch::parse_format(&args.snapshot_format)?;
    let theme = igv_tui::snapshot::batch::parse_theme(&args.snapshot_theme)?;
    let batch = igv_tui::snapshot::batch::BatchOpts {
        out_dir: args.snapshot_out.clone().unwrap(),
        format,
        width_px: args.snapshot_width,
        flank: args.snapshot_flank,
        theme,
    };
    let bams_owned = bams
        .iter()
        .map(|t| (t.display.clone(), Arc::clone(&t.source)))
        .collect();
    let annotations_owned = annotations
        .iter()
        .map(|t| (t.display.clone(), Arc::clone(&t.source)))
        .collect();
    let signals_owned = signals
        .iter()
        .map(|t| (t.display.clone(), Arc::clone(&t.source)))
        .collect();
    return igv_tui::snapshot::batch::run(
        fasta,
        vcf,
        bams_owned,
        annotations_owned,
        signals_owned,
        references.clone(),
        regions,
        batch,
    )
    .await;
}
```

- [ ] **Step 4: Build**

Run: `cargo build -p igv-tui`
Expected: PASS.

- [ ] **Step 5: Test the gene resolver directly**

Create `crates/igv-tui/tests/snapshot_batch_genes.rs`:

```rust
//! Integration test for the gene resolver. Uses a stub
//! AnnotationSource so we don't need a real GFF on disk.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::source::{
    AnnotationBlock, AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};
use igv_tui::snapshot::genes::resolve;

struct StubAnno;

#[async_trait]
impl AnnotationSource for StubAnno {
    async fn fetch(&self, _r: &Region) -> Result<Vec<AnnotationTranscript>> { Ok(vec![]) }
    fn display_name(&self) -> &str { "stub" }
    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        if query.eq_ignore_ascii_case("gene1") {
            vec![("chr1".into(), AnnotationTranscript {
                name: "GENE1".into(),
                id: "tx1".into(),
                gene_id: Some("g1".into()),
                strand: Strand::Forward,
                blocks: vec![AnnotationBlock { start: 100, end: 500, kind: BlockKind::Exon }],
                kind: TranscriptKind::Mrna,
            })]
        } else {
            vec![]
        }
    }
}

#[test]
fn resolve_known_gene_returns_region() {
    let sources: Vec<Arc<dyn AnnotationSource>> = vec![Arc::new(StubAnno)];
    let names = vec!["gene1".to_string()];
    let v = resolve(&names, &sources);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].region.chrom, "chr1");
    assert_eq!(v[0].region.start, 100);
    assert_eq!(v[0].region.end, 500);
    assert_eq!(v[0].label.as_deref(), Some("gene1"));
}

#[test]
fn resolve_unknown_gene_skipped() {
    let sources: Vec<Arc<dyn AnnotationSource>> = vec![Arc::new(StubAnno)];
    let names = vec!["nope".to_string()];
    let v = resolve(&names, &sources);
    assert!(v.is_empty());
}
```

Run: `cargo test -p igv-tui --test snapshot_batch_genes`
Expected: PASS.

- [ ] **Step 6: Full test sweep**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/igv-tui
git commit -m "$(cat <<'EOF'
feat(tui): --snapshot-genes batch path

Resolves names against loaded AnnotationSources using the same
"union of matches per chrom" rule as AppState::find_gene_region;
skips unknowns with a warning. Reuses the BED batch render loop.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 20: README + help overlay update

**Files:**
- Modify: `README.md`
- Modify: `crates/igv-tui/src/ui/widgets/help.rs`

- [ ] **Step 1: Add Snapshot section to README**

Read `README.md`. After the "Wide-zoom behavior" section (line ~70) and before "Keybindings", insert a new section:

```markdown
### Snapshot export (SVG / PNG)

Save publication-style figures of the current view or batches of
regions / genes. Snapshots are graphical, not character art —
matching IGV's PNG / SVG output style.

Interactive (inside the TUI):

- `S` — save the current view to `./snapshot_<chrom>_<start>_<end>.svg`
- `:snapshot path/to/file.svg` (or `:snap`) — save to a chosen path;
  `.png` extension switches to PNG output.

Headless batch (no TUI is opened):

```bash
# One snapshot per BED region (column 4 = output filename stem)
igv-rs ref.fa -b s.bam --snapshot-bed regions.bed --snapshot-out out/

# One snapshot per gene name (requires -g annotation)
igv-rs ref.fa -b s.bam -g genes.gtf \
    --snapshot-genes list.txt --snapshot-out out/
```

Flags shared by both modes:

- `--snapshot-format svg|png` (default `svg`)
- `--snapshot-width <px>` (default `1200`)
- `--snapshot-flank <fraction>` (default `0.1`; pads each side)
- `--snapshot-theme igv|tui` (default `igv` light theme)

Output filenames in batch mode are
`<label>_<chrom>_<start>_<end>.<ext>` (with `<label>` from the BED
4th column or the gene name when set).
```

- [ ] **Step 2: Add `S` to the keybindings table in README**

Find the "Keybindings" section. Insert a new line near `t`:

```
- `S` — save SVG snapshot of current view to `./snapshot_<chrom>_<s>_<e>.svg`
```

- [ ] **Step 3: Add `S` to the help overlay**

Read `crates/igv-tui/src/ui/widgets/help.rs`. Find the array of key descriptions. Add an entry consistent with the existing format (after the entry for `t`):

```rust
("S", "save SVG snapshot of current view"),
```

(If the existing entries use a different tuple shape, mirror the prevailing shape exactly. Read the file to confirm.)

- [ ] **Step 4: Run the help-snapshot tests if any exist**

Run: `cargo test -p igv-tui --test help`
Expected: tests pass (or "no tests run" if no help-targeted tests). Either is fine.

- [ ] **Step 5: Build whole workspace**

Run: `cargo build --workspace`
Run: `cargo test --workspace`
Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add README.md crates/igv-tui/src/ui/widgets/help.rs
git commit -m "$(cat <<'EOF'
docs(snapshot): document SVG/PNG export + add S to help overlay

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Done

After Task 20:

- `igv-render` produces SVG and PNG figures from a `RenderInputs`.
- TUI key `S` and palette `:snapshot <path>` write the current view.
- `--snapshot-bed` and `--snapshot-genes` produce per-region output files headlessly.
- README and help overlay reference the new feature.

Open follow-ups (out of scope, see spec §Non-goals):

- Sequence per-base letter rendering (`--render-sequence`).
- Overview ideogram in snapshots.
- `[snapshot.theme]` TOML customisation.
- Genuine "TUI screenshot" mode (cell-grid character export).
- True crossterm-`Style` → RGB mapping for `--snapshot-theme tui` (currently aliased to `igv`).
