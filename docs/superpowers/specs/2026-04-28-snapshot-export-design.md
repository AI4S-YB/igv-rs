# Snapshot export (SVG / PNG) — design

Status: draft (2026-04-28)
Owners: @xuzhougeng

## Motivation

`igv-rs` today renders a character grid in the terminal. For publications,
slide decks, issue reports, and side-by-side comparisons users want
**graphical** images of the same content — not screenshots of the TUI but
proper IGV-style rendering with vector lines, transcript boxes, and bar
charts.

Three usage modes drive this design:

1. **Interactive snapshot** — after navigating in the TUI to an interesting
   region, save the current view to a file with a single keystroke.
2. **BED batch** — given a BED file of regions, render each one to its own
   image. No TUI involved.
3. **Gene-list batch** — given a text file with one gene name per line and a
   loaded GFF/GTF annotation, look up each gene's region (union of all
   isoforms on the same chromosome) and render. No TUI involved.

All three share the same renderer; the only difference is how regions are
fed into it and where the data comes from (live `AppState` for interactive
vs. on-demand fetch for batch).

## Goals

- One graphical renderer that consumes a single data structure and emits
  SVG (primary) or PNG (via SVG → resvg pipeline).
- Headless batch path that does not touch ratatui or raw mode.
- Renderer is independently testable from fixed input fixtures.
- Default output looks like a publishable IGV-style figure (light theme,
  recognisable IGV palette), with a `tui` theme escape hatch for users
  who want a screenshot-of-TUI feel.

## Non-goals (v1)

- Sequence per-base letter rendering (`--render-sequence` is a follow-up).
  At fine zoom we would otherwise emit thousands of `<text>` elements per
  view, hurting both file size and resvg render time.
- Overview ideogram (chromosome-wide bracket strip).
- Multi-locus split views (IGV's "compare regions side by side").
- User-customisable `[snapshot.theme]` TOML section. Two presets ship; a
  config-file extension can come later if there is demand.
- Cell-grid character snapshots (a screenshot of the literal terminal).
  That is a different feature.

## User-facing surface

### Interactive

- Key `S` (uppercase) — save current view to
  `./snapshot_<chrom>_<start>_<end>.svg` in the working directory. Status
  line confirms the path. (`s` is already zoom-out and stays.)
- Command palette: `:snapshot <path>` or `:snap <path>`. Extension
  (`.svg`, `.png`) selects the format. Without extension, defaults to
  `.svg` and appends.
- During an in-flight load (`state.loading == true`) `S` shows
  `snapshot: still loading, try again` and writes nothing.

### Batch CLI (no TUI, no raw mode)

```
igv-rs ref.fa -b s.bam --snapshot-bed regions.bed --snapshot-out out/
igv-rs ref.fa -b s.bam -g genes.gtf --snapshot-genes list.txt --snapshot-out out/
```

`--snapshot-bed` and `--snapshot-genes` are mutually exclusive. When
either is set, `igv-rs` skips `enable_raw_mode` / `EnterAlternateScreen`,
does not construct the ratatui `Terminal`, and runs a serial render loop.

Shared flags (apply to both interactive and batch):

| Flag                 | Default | Notes                                       |
|----------------------|---------|---------------------------------------------|
| `--snapshot-format`  | `svg`   | `svg` or `png`                              |
| `--snapshot-width`   | `1200`  | Image width in px                           |
| `--snapshot-flank`   | `0.1`   | Padding fraction added to each side         |
| `--snapshot-theme`   | `igv`   | `igv` (light) or `tui` (reuse current TUI)  |

### Output naming

Two-segment, always coordinate-suffixed:

- BED batch: `<name>_<chrom>_<start>_<end>.<ext>` where `<name>` is the
  BED column 4. Missing column 4 → just `<chrom>_<start>_<end>.<ext>`.
  Duplicate `<name>` entries get `_2`, `_3` suffixes (pre-coordinate).
- Gene batch: `<gene>_<chrom>_<start>_<end>.<ext>` where `<gene>` is the
  query string (case preserved from the input list).
- Interactive (default path): `snapshot_<chrom>_<start>_<end>.svg`.
- Interactive (`:snapshot <path>`): exact path given by user.

The coordinates reflect the **rendered** region, i.e. the original
region after `--snapshot-flank` expansion.

### Padding behaviour

`--snapshot-flank f` expands each region symmetrically by `floor(width * f)`
on either side, clamped to chromosome bounds. Default `0.1` adds 10 % on
each side (so the rendered window is 1.2× the input). Set to `0` for
exact BED bounds.

## Architecture

### Crate topology

```
crates/
  igv-core/      data, regions, render thresholds, RenderInputs (new struct)
                 + collect_render_inputs() helper (new)
  igv-render/    NEW — depends on igv-core + svg + resvg + usvg + tiny-skia
  igv-tui/       depends on igv-core + igv-render; gains S key, :snapshot, batch flags
```

Rationale:

- `igv-core` is currently a pure data/algorithm crate with no UI deps.
  Putting `resvg` there would force every `igv-core` consumer to pull in
  the PNG pipeline. Keep it clean.
- `igv-tui` already carries ratatui/crossterm/tui-input. Adding resvg on
  top makes the binary bigger and tangles the headless batch path with
  ratatui. Move rendering out.
- A standalone `igv-render` is independently testable: feed a fixed
  `RenderInputs` fixture, snapshot the resulting SVG with `insta`,
  iterate without spinning up a terminal.

### `igv-core` additions

```rust
// crates/igv-core/src/render/inputs.rs (new module)
pub struct RenderInputs {
    pub region: Region,
    pub references: Vec<RefMeta>,
    pub reference_seq: Vec<u8>,         // only used at fine zoom
    pub variants: Vec<VariantRecord>,
    pub bams: Vec<BamTrackSnapshot>,    // display name + rows + lanes
    pub annotations: Vec<AnnotationTrackSnapshot>,
    pub signals: Vec<SignalTrackSnapshot>,
    pub render_mode: RenderMode,
}

// crates/igv-core/src/collect.rs (new module)
pub async fn collect_render_inputs(
    sources: &Sources,                  // bundle of Arc<dyn ...> trait objects
    region: &Region,
    opts: &CollectOpts,                 // signal_max_bins, fetch_opts, render_mode
) -> Result<RenderInputs, CollectError>;
```

`collect_render_inputs` calls each source sequentially with `await`,
returning when everything is in. It does **not** use the existing
mpsc-based `Loader`. Reasoning: the Loader exists to support cancellation
and partial UI updates during interactive use. Batch rendering wants
"all data for one region, then move on" — generation tracking and
channel draining are pure overhead there.

The `Loader` stays unchanged. TUI interactive snapshot does not call
`collect_render_inputs` either; it builds `RenderInputs` from the
already-populated `AppState` fields.

### `igv-render` public API

```rust
pub fn render_svg(inputs: &RenderInputs, opts: &SvgOptions) -> String;
pub fn render_png(inputs: &RenderInputs, opts: &SvgOptions)
    -> Result<Vec<u8>, RenderError>;

pub struct SvgOptions {
    pub width_px: u32,                 // default 1200
    pub track_heights: TrackHeights,   // px per track (defaults below)
    pub theme: GraphicalTheme,
    pub title: Option<String>,         // header line
}

pub struct TrackHeights {
    pub header: u32,                   // default 40
    pub ruler: u32,                    // default 28
    pub annotation_each: u32,          // default 36
    pub variants: u32,                 // default 24
    pub coverage: u32,                 // default 80
    pub signal_each: u32,              // default 80
    pub alignments_each: u32,          // default 160
    pub lane_height: u32,              // default 12 (used inside alignments)
    pub gutter: u32,                   // default 4 (between tracks)
}

pub struct GraphicalTheme { /* RGB + font sizes; see §Theme */ }
impl GraphicalTheme {
    pub fn igv_light() -> Self;        // default
    pub fn from_tui_theme(t: &Theme) -> Self;
}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    #[error("usvg parse: {0}")]
    UsvgParse(String),
    #[error("png encode: {0}")]
    PngEncode(String),
}
```

`render_png` is a thin wrapper:

1. `let svg = render_svg(...)`;
2. `let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())?`;
3. allocate `tiny_skia::Pixmap` of (width × computed_height);
4. `resvg::render(&tree, ..., pixmap.as_mut())`;
5. `pixmap.encode_png()`.

### `igv-tui` integration

- `app::action::Action::SaveSnapshot { path: Option<PathBuf>, format: SnapshotFormat }`.
- Input mapper: `S` → `SaveSnapshot { path: None, format: Svg }`.
- Command palette: parse `:snapshot <path>` / `:snap <path>` into
  `SaveSnapshot { path: Some(...), format: from_ext(...) }`.
- `AppState::apply` stays pure (returns `Option<LoadRequest>`). Handling
  `SaveSnapshot` it sets `state.pending_snapshot: Option<SnapshotJob>`
  and returns `None`. The main loop drains `pending_snapshot` after each
  `apply`, builds `RenderInputs` from current `AppState`, calls
  `igv_render::render_svg|png`, writes the file, and sets a status
  message. This keeps the IO out of the state machine and matches the
  side-effect pattern used elsewhere in the loop.

### Headless batch path

In `main.rs`, before raw-mode setup:

```rust
if let Some(plan) = SnapshotPlan::from_args(&args)? {
    return run_batch_snapshots(plan, fasta, vcf, bams, annotations, signals).await;
}
```

`SnapshotPlan` covers BED-list and gene-list inputs. `run_batch_snapshots`:

1. resolve regions (parse BED or look up genes via the same algorithm as
   `AppState::find_gene_region`),
2. apply `--snapshot-flank` expansion,
3. for each region: `collect_render_inputs(...)` → `render_svg|png` →
   `write(out_dir.join(naming::name_for(region, query_label, format)))`,
4. report progress to stderr, summary at end (`rendered: N, skipped: K`).

A single per-region failure (missing gene, write error, render error)
logs and increments `skipped`; it does not abort the batch.

## Layout (px-based)

`igv-render` ships its own `SnapshotLayout`. It mirrors
`igv-tui::ui::layout::compute` semantically (same track order, same
optional tracks) but works in pixels:

```
[ header (40)             ]
[ gutter (4)              ]
[ ruler (28)              ]
[ gutter (4)              ]
[ annotations[i] (36) … ] (one per loaded annotation track)
[ variants (24) optional  ]
[ coverage (80) optional  ]
[ signal[i] (80) …        ] (one per loaded signal track)
[ alignments[i] (160) …   ] (one per loaded BAM)
[ gutter (4)              ]
```

Total height = sum of present tracks. Width = `--snapshot-width` (default
1200 px), with a left margin (~80 px) reserved for track labels. The
plot area is therefore `width - margin_left - margin_right`.

X-axis mapping: `bp_to_px(b) = margin_left + (b - region.start) / width_bp * plot_width`.

## Track rendering

Each track is an isolated function `draw_<track>(svg, area, inputs, theme)`
where `area` is a `Rect` in px. They share helpers (`bp_to_px`, label
column writer). Per track:

- **header**: title (filename or `--title`) + `chrom:start-end` right-aligned.
- **ruler**: a horizontal line, ticks every "nice" round bp interval
  (10/50/100/500/1k/5k/...), tick labels in `1,234,567 bp` form.
- **annotations**: one row per loaded GFF/BED track. Transcripts laid out
  by the existing `assign_lanes`-style algorithm (or a simplified
  one-row-per-transcript at v1 density). Exons as filled rects, introns
  as thin lines with chevron arrowheads indicating strand. Gene/transcript
  label left of the transcript or above if it fits.
- **variants**: small triangles at variant positions, coloured by
  REF→ALT class (transition/transversion/indel — IGV palette).
- **coverage**: filled bar chart, one bar per pixel column, height = depth
  scaled to the per-track max.
- **signal**: same bar-chart shape as TUI, using `SignalBin.value`.
  Interactive snapshots respect the current TUI `signal_shared_scale`
  toggle (the saved figure matches what's on screen). Batch always uses
  per-track scale; a `--snapshot-signal-shared-scale` flag is **not**
  added in v1 (YAGNI — add when someone asks).
- **alignments**: each lane is one row (`lane_height` px). A read is a
  rect from `bp_to_px(read.start)` to `bp_to_px(read.end)` with a
  triangular tip indicating strand. Mismatches are 1-px-wide vertical
  ticks coloured by base. Soft-clip ends are slightly desaturated.
  When `total_lanes * lane_height` exceeds `alignments_each`, the track
  shows the top N lanes plus a `+K more` text line at the bottom.
  (Matches the truncation policy in the TUI widget.)

At wide zoom (`render_mode != Detailed`) some tracks are empty by
design — coverage/alignments/variants are gated by the loader. The
renderer still draws the empty track bands with their labels and a
small "(zoomed out)" note, matching the TUI's behaviour.

## Theme

```rust
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
    pub mismatch_a: Rgb, pub mismatch_c: Rgb,
    pub mismatch_g: Rgb, pub mismatch_t: Rgb, pub mismatch_n: Rgb,
    pub font_family: &'static str,    // "DejaVu Sans, sans-serif"
    pub font_px_small: u32,           // 10
    pub font_px_normal: u32,          // 12
    pub font_px_label: u32,           // 14
}
```

`igv_light()` defaults (RGB hex):

| Field              | Value     |
|--------------------|-----------|
| bg                 | `#ffffff` |
| fg                 | `#1a1a1a` |
| muted              | `#888888` |
| ruler_text         | `#444444` |
| transcript_exon    | `#1f3b73` |
| transcript_intron  | `#777777` |
| transcript_label   | `#1a1a1a` |
| variant_snv        | `#c0392b` |
| variant_indel      | `#7d3c98` |
| coverage_bar       | `#888888` |
| signal_bar         | `#1f4e79` |
| read_forward       | `#9ec3e0` |
| read_reverse       | `#e8b6b6` |
| mismatch_a         | `#2ca02c` |
| mismatch_c         | `#1f77b4` |
| mismatch_g         | `#ff7f0e` |
| mismatch_t         | `#d62728` |
| mismatch_n         | `#888888` |

`from_tui_theme(t: &Theme)` reads `t`'s `Color` values: `Color::Rgb(r,g,b)`
passes through; named/indexed colors fall back to the same key in
`igv_light()` (so we never produce ambiguous colour mappings).

## Error handling

| Failure                         | Interactive            | Batch                          |
|---------------------------------|------------------------|--------------------------------|
| Write IO error                  | status line (Error)    | stderr, count as skipped       |
| Region parse / gene not found   | status line (Error)    | stderr, count as skipped       |
| usvg/resvg PNG render error     | status (Warn) + retain SVG file | stderr, count as skipped |
| Loading in progress (interactive only) | status `still loading, try again` | n/a |

Batch never aborts on per-region errors; the binary exits 0 if any
region succeeded, exit 1 only if **all** failed.

## Testing strategy

### `igv-render` unit tests

- Per-track fixture tests: a small `RenderInputs` covering only one track
  type (ruler-only, annotations-only, alignments-only, etc.). Use
  `insta::assert_snapshot!` on the SVG string. Avoids one giant unstable
  golden snapshot.
- Determinism: SVG uses `<text>` elements with named font families (no
  text-to-path), so output is byte-stable across machines without
  embedding font glyphs.
- Layout math: pure unit tests on `bp_to_px` rounding edge cases.

### `igv-render` PNG smoke

One end-to-end test: render a small `RenderInputs` to PNG, assert
non-empty output and PNG magic bytes. Do not snapshot pixels — resvg
versions can shift sub-pixels.

### `igv-tui` integration

- Batch path: invoke binary (or a library entry point) with a tiny BED
  and a tiny FASTA fixture, write to `tempfile::tempdir()`, assert files
  exist with the expected names and SVG header.
- Snapshot key dispatch: unit-test that `S` maps to
  `Action::SaveSnapshot { path: None, .. }` and `:snapshot foo.png`
  parses to `SaveSnapshot { path: Some(_), format: Png }`.

### Manual

The smoke tests do not check that the figure looks correct. Manual
verification is required when changing layout or theme: render the
sample BAM/GFF/bigWig used in existing snapshot tests and eyeball the
SVG (or open the PNG).

## Dependencies (new)

| Crate         | Purpose                       | Approx weight |
|---------------|-------------------------------|---------------|
| `svg`         | Element/attribute builder     | small         |
| `usvg`        | Parse SVG into render tree    | medium        |
| `resvg`       | Rasterise the tree            | medium        |
| `tiny-skia`   | Pixmap backend for resvg      | medium        |

All are pure Rust (no system libs). PNG output is `tiny_skia::Pixmap::encode_png`
(uses `png` transitively). `svg` is optional — we may emit SVG by string
templating instead. Decided in implementation; either way the public API
does not change.

## File layout (planned)

```
crates/igv-render/
  Cargo.toml
  src/
    lib.rs              public API, re-exports
    options.rs          SvgOptions, TrackHeights
    theme.rs            GraphicalTheme, igv_light(), from_tui_theme()
    layout.rs           SnapshotLayout, bp_to_px helper
    svg/
      mod.rs            render_svg entry, document scaffolding
      header.rs
      ruler.rs
      annotations.rs
      variants.rs
      coverage.rs
      signal.rs
      alignments.rs
    png.rs              render_png wrapper
    error.rs            RenderError
crates/igv-core/
  src/
    render/
      inputs.rs         RenderInputs + per-track snapshot structs (new)
    collect.rs          collect_render_inputs (new)
crates/igv-tui/
  src/
    snapshot/           interactive + batch glue (new)
      mod.rs
      job.rs            SnapshotJob (drained by main loop)
      batch.rs          run_batch_snapshots
      naming.rs         filename builders
      genes.rs          gene-list resolution (shares logic with AppState::find_gene_region)
    cli.rs              + new flags
    main.rs             + headless branch
    app/action.rs       + SaveSnapshot variant
    input.rs            + S key
    command.rs          + :snapshot / :snap parser
```

## Milestones / decomposition

This spec maps to one implementation plan. Suggested cut order inside
the plan (informs writing-plans, not part of the spec itself):

1. `igv-render` skeleton + `RenderInputs` in `igv-core` + ruler/header
   only. `insta` snapshot test passes.
2. Add annotations + coverage + signal renderers.
3. Add alignments + variants.
4. PNG path (`render_png`).
5. `igv-tui` `S` key + `:snapshot` palette + interactive write.
6. Batch CLI: `--snapshot-bed` first.
7. Batch CLI: `--snapshot-genes` (reuses gene-resolution).
8. Polish: progress reporting, error summary, README updates.

## Open questions left for implementation

- Exact font choice. SVG references font by family name; if the renderer
  PC has neither DejaVu nor a generic sans-serif, glyphs differ. We
  reference `"DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif"`
  and rely on usvg's fontdb to fall back. Acceptable for v1.
- Whether to emit `<defs>` for repeated arrowhead markers vs inline each
  one. Decision in implementation; affects file size, not correctness.
- Whether interactive `S` snapshot should respect the current TUI
  `signal_shared_scale` and other on-screen toggles. v1 says **yes**:
  the snapshot reflects what's on screen. Batch defaults to per-track
  scale (no on-screen toggles to mirror).
