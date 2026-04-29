# BEDPE link-track support — design

**Date**: 2026-04-29
**Status**: design accepted; implementation pending
**Scope**: add BEDPE (`.bedpe` / `.bedpe.gz`) read-only support as a new
"link track" axis in `igv-rs`, alongside reference / variants / alignments /
coverage / annotations / signals. Renders pairwise genomic interactions
(chromatin loops, enhancer-promoter links, ChIA-PET, structural-variant
breakpoints) as adaptive arcs / heatmap. Same data model feeds both the
TUI and the SVG / PNG snapshot renderer in `igv-render`.

Out of scope for v1: bigInteract / UCSC interact format; pairix / tabix
indexing for files >1M records; HiC contact matrices (`.cool`, `.hic`);
per-track theme color; per-record score-column override; trans-link arc
rendering across panels.

## 1. Motivation & use cases

Pairwise interaction data is the missing axis in the current `igv-rs`. The
canonical view of a regulated locus needs to show:

- which enhancers loop to a promoter (ABC predictions, ChIA-PET, Hi-C
  loop calls);
- which CTCF anchors form TAD boundaries near a gene of interest;
- where structural-variant breakpoints land relative to coding regions.

Today the only workaround is opening a parallel desktop IGV. Adding native
BEDPE support means a server-side genome viewer covers the third major
genomics modality (after read alignments and quantitative signal).

User-confirmed scope for v1:

- Format is **BEDPE** (chosen over UCSC interact / pairix because BEDPE is
  the most common output across HiCCUPS, cooltools, ChIA-PET pipelines,
  ABC-model, MACS3, bedtools).
- Render mode is **adaptive**: arcs when sparse, heatmap when dense, with
  automatic switch based on visible-record count vs. track height.
- Cross-window scope is **B**: render links with both anchors visible
  *and* links with one anchor visible (off-window anchor shown as a
  half-arrow with distance label). Drop "spanning" links where both
  anchors are outside the window. `trans` links (different chromosome)
  show an edge marker only.
- Loading is **fully in-memory** with per-anchor per-chromosome
  `IntervalTree`. Large files (>1M records) deferred to a future spec.
- Score is **BEDPE column 8**, normalized per visible window (`min..max`).
  4 dim/normal/bold buckets in TUI; continuous viridis gradient in SVG.
  `--link-min-score N` filters; missing score degrades to base color.
- Heatmap aggregation statistic is **`max`** (matches signal-track
  default; preserves "is there a strong loop here?").

## 2. Architecture

A link track is treated as an **interaction track**, semantically distinct
from annotations (which are 1-D intervals of structure) and signals (which
are 1-D quantitative). The split:

- **`igv-core::source::link`** (new module) — `LinkRecord`, `LinkSource`
  trait, `LinkFormat` enum, `BedpeLinkSource` implementation. Async, UI-free.
- **`igv-tui::ui::widgets::link::LinkWidget`** (new) — terminal renderer
  (box-drawing arcs / `░▒▓█` heatmap).
- **`igv-render::link`** (new module) — SVG renderer (Bézier arcs,
  continuous color gradient, real arrowheads, pixel heatmap).
- **`Loader`** (extended) — adds a parallel `links` fetch lane,
  generation-guarded like every other source.
- **`AppState`** (extended) — holds `links`, `link_records`,
  `link_track_height`, `link_min_score`.
- **CLI** (extended) — `-l` / `--link` (repeatable), `--link-format`
  override, `--link-min-score` filter.
- **`igv-core::render_inputs`** (extended) — pass `link_records` to the
  SVG renderer alongside the existing alignment / annotation inputs.

Boundary rationale: `LinkSource` does not depend on ratatui, tokio's
runtime, or the SVG layer. The same trait will accept a future
`InteractLinkSource` (UCSC interact / bigInteract) and a future
`PairixLinkSource` (tabix-style queryable backend) without API churn.
The UI and SVG layers consume `Vec<LinkRecord>` exactly as
`AnnotationsWidget` consumes `Vec<AnnotationTranscript>` today.

## 3. Components

### 3.1 `igv-core::source::link`

```rust
use std::sync::Arc;
use crate::source::annotation::Strand;

#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub chrom_a: Arc<str>,
    pub start_a: u32,           // 0-based half-open, BEDPE convention
    pub end_a: u32,
    pub chrom_b: Arc<str>,
    pub start_b: u32,
    pub end_b: u32,
    pub name: Option<String>,   // BEDPE col 7; "." → None
    pub score: Option<f64>,     // BEDPE col 8; "." or missing → None
    pub strand_a: Strand,       // BEDPE col 9; "." → Unknown
    pub strand_b: Strand,       // BEDPE col 10; "." → Unknown
}

impl LinkRecord {
    pub fn is_trans(&self) -> bool { self.chrom_a != self.chrom_b }

    /// Span of the link on a single chromosome (cis only); None for trans.
    pub fn cis_span(&self) -> Option<(u32, u32)> {
        if self.is_trans() { return None; }
        let lo = self.start_a.min(self.start_b);
        let hi = self.end_a.max(self.end_b);
        Some((lo, hi))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LinkScope {
    /// Both anchors inside the visible region.
    BothIn,
    /// Exactly one anchor inside; off-window anchor on same chromosome.
    PartialCis { off_anchor_mid: u32, off_anchor_to_left: bool },
    /// One anchor inside; the other on a different chromosome.
    Trans { off_chrom: Arc<str>, off_anchor_mid: u32 },
}

#[derive(Debug, Clone)]
pub struct VisibleLink<'a> {
    pub record: &'a LinkRecord,
    pub scope: LinkScope,
}

#[derive(Debug, Clone, Copy)]
pub struct FetchLinkOpts {
    pub min_score: Option<f64>,
}

#[async_trait]
pub trait LinkSource: Send + Sync {
    async fn query<'a>(
        &'a self,
        region: &Region,
        opts: &FetchLinkOpts,
    ) -> Result<Vec<VisibleLink<'a>>>;
    fn display_name(&self) -> &str;
    fn record_count(&self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkFormat { Bedpe }

impl LinkFormat {
    pub fn from_path(p: &Path) -> Option<Self>;  // .bedpe, .bedpe.gz
    pub fn parse(s: &str) -> Option<Self>;       // "bedpe"
}

pub async fn open_link(
    path: &Path,
    fmt: Option<LinkFormat>,
) -> Result<Arc<dyn LinkSource>>;
```

### 3.2 `BedpeLinkSource`

```rust
pub struct BedpeLinkSource {
    display: String,
    records: Vec<LinkRecord>,
    /// per-chromosome interval tree on anchor A; values are indices into `records`.
    tree_a: HashMap<Arc<str>, IntervalTree<u32, usize>>,
    /// same on anchor B.
    tree_b: HashMap<Arc<str>, IntervalTree<u32, usize>>,
}
```

**Construction (synchronous, off the runtime via `spawn_blocking`):**

1. Open the file. If extension is `.gz`, wrap in `flate2::read::MultiGzDecoder`.
2. For each non-comment, non-empty line:
   - Split on `\t`, expect ≥ 6 columns. Lines with < 6 columns: log
     `tracing::warn!("bedpe line {n}: too few columns; skipping")` and skip.
   - Parse `chrom_a / start_a / end_a / chrom_b / start_b / end_b` strictly.
     Failures log `tracing::warn!` and skip.
   - Cols 7-10 are best-effort: missing or `.` → `None` / `Strand::Unknown`.
   - Cols 11+ are ignored (BEDPE allows arbitrary trailing columns).
3. Push the parsed `LinkRecord` onto `records`; record its index in
   `tree_a[chrom_a]` and `tree_b[chrom_b]`.
4. Both anchors of the same record may live on different chromosomes
   (trans case); both trees still get the index, so a query against
   either chromosome surfaces the record.

**Query path (`query(region, opts)`)**:

1. `let candidates_a = tree_a.get(&region.chrom)?.query(region.start..region.end);`
2. `let candidates_b = tree_b.get(&region.chrom)?.query(region.start..region.end);`
3. Union the indices (`HashSet<usize>` to dedupe links where both anchors
   fall in the same window).
4. For each unique record:
   - Apply `opts.min_score`: if both `score.is_some()` and the score is
     below the threshold, drop the record. (Records without a score are
     never filtered by `--link-min-score`.)
   - Determine `LinkScope`:
     - both anchors overlap region → `BothIn`
     - exactly one anchor overlaps and `chrom_a == chrom_b` → `PartialCis`
     - exactly one anchor overlaps and `chrom_a != chrom_b` → `Trans`
   - `off_anchor_mid` is the midpoint of the off-window anchor; used by the
     widget to render the half-arrow distance label.
5. Return `Vec<VisibleLink>` (zero-copy borrows into `self.records`).

The "spanning link" case (cis span crosses the window but neither anchor
overlaps) is not surfaced by either tree query and therefore cannot reach
this loop — the cross-window scope rule (B) is enforced by the tree
geometry itself, not by an explicit filter here.

The interval tree crate is `iset` (already used by the annotation source);
no new dependency.

### 3.3 `igv-tui::ui::widgets::link::LinkWidget`

```rust
pub struct LinkWidget<'a> {
    pub display_name: &'a str,
    pub region: &'a Region,
    pub theme: &'a Theme,
    pub visible: &'a [VisibleLink<'a>],
    pub height_rows: u16,
}
```

**Mode selection:**

```rust
let arc_count = visible.iter().filter(|v| matches!(v.scope,
    LinkScope::BothIn | LinkScope::PartialCis { .. })).count();

let mode = if arc_count <= height_rows.saturating_sub(1) as usize {
    Mode::Arc
} else {
    Mode::Heatmap
};
```

`height_rows - 1` reserves one row for the bottom anchor block strip.
`Trans` links never count toward the arc budget — they always render as a
single edge marker regardless of mode.

**Arc mode layout (`height_rows = 6` example):**

```
row 0 │ ╭──────────────────╮          ←—► 500kb         │  ┐
row 1 │ │     ╭────────╮   │                            │  │  arc rows
row 2 │ │     │        │   │                            │  │
row 3 │ │     │        │   │                            │  ┘
row 4 │ █████████  ████████████   ████                  │  ← anchor strip
row 5 │ link[loops.bedpe]                  3 loops      │  ← title row
```

- Arcs are drawn into the top `height_rows - 2` rows using box-drawing
  characters `╭ ╮ ─ │` for the curve body.
- Greedy stacking: sort arcs by left anchor end, place each into the
  lowest row whose latest occupied column is left of the new arc's start.
  When more arcs than available rows, the spillover collapses into mode
  switch (`arc_count > height_rows - 2`).
- Anchor strip row uses `█` for cells covered by any anchor in the window.
  Color of the cell is the **max-score quantile** across overlapping anchors.
- Score buckets (per visible window): quartile thresholds `(q25, q50, q75)`
  over records where `score.is_some()`. Each arc paints with one of four
  style variants of the `LINK` theme key:
  - bucket 0 (< q25): `dim`
  - bucket 1 (q25..q50): default
  - bucket 2 (q50..q75): default + bold for the curve, anchor at default
  - bucket 3 (≥ q75): bold for both
- Records with `score == None` always render at default style.
- **Degenerate case**: when the visible window contains fewer than 4
  scored records, quartile bucketing is skipped entirely and every arc /
  anchor paints at the default style of `LINK`. This avoids meaningless
  contrast (e.g. one arc rendering `dim` just because it is the lowest
  of two).
- `PartialCis` arcs: in-window anchor draws normally; the arc travels from
  the in-window anchor's edge to the window edge, terminating in a
  half-arrow `─►` (or `◄─`) followed by the distance label
  (`500kb`, `1.2Mb`, `42b`). Distance is `|off_anchor_mid - window_edge_bp|`.
- `Trans` links: skip the arc entirely; in-window anchor draws as a normal
  block; immediately above the anchor block, paint `⤴ chr2:5M` (or
  `chr2:5M ⤵` if anchor is on the right half of the screen). When several
  trans links target the same off-chromosome and overlap the same column,
  collapse to `⤴ +N`.

**Heatmap mode layout:**

```
row 0 │  ░░▒▒▓▓██▓▓▒▒░░     ░░▒▒▓▓▓▒▒░     ░▒▓██▓▒░     │
row 1 │                                                   │
row 2 │  same as row 0 (multi-row makes the strip denser) │
row 3 │                                                   │
row 4 │  ─────────────────────────────────────────────    │  ← divider
row 5 │ link[loops.bedpe] · heatmap (834 loops in window) │
```

- For each terminal column `c`, gather all anchors of all visible records
  whose anchor `[start, end]` overlaps the genomic range mapped to `c`.
- Reduce to a per-column score: `max` over all overlapping anchors'
  effective score. The "effective score" of an anchor with `score == None`
  is the per-window `q25` of the scored distribution (so unscored anchors
  contribute as low-confidence rather than as zero or as max). When fewer
  than 4 scored records are visible, fall back to per-anchor count: the
  per-column value is the number of anchors overlapping `c`, rendered with
  the same `(0, ░, ▒, ▓, █)` ramp normalized to `(0, max_count)`.
- Map to one of `(0, ░, ▒, ▓, █)` using quartile thresholds of the per-window
  score distribution (column gets `█` only if its score lands in top 25%).
- Paint identical character on every row of the heatmap band so the
  density is visible at any track height.

**Title row (always present, last row):**

```
link[loops.bedpe]                     N loops               (arc mode)
link[loops.bedpe] · heatmap            N loops in window    (heatmap mode)
```

### 3.4 `Loader` extension

```rust
pub struct Loader {
    // ...existing...
    pub links: Vec<Arc<dyn LinkSource>>,
}

pub enum LoadResult {
    // ...existing...
    Link {
        generation: u64,
        track_index: usize,
        records: Vec<LinkRecord>,    // owned snapshot — see §3.6
    },
}
```

`dispatch()` adds a fetch lane mirroring the `signals` block. On error it
logs `tracing::warn!` and emits an empty `records` payload (per §5).

### 3.5 CLI

```rust
/// Path to a BEDPE link file (.bedpe / .bedpe.gz). May be repeated.
#[arg(short = 'l', long = "link")]
pub links: Vec<PathBuf>,

/// Override link format auto-detection (currently only "bedpe").
#[arg(long = "link-format")]
pub link_format: Option<String>,

/// Drop links whose score column is below this value.
/// Records without a score are unaffected.
#[arg(long = "link-min-score")]
pub link_min_score: Option<f64>,
```

Auto-detection from extension; `--link-format bedpe` overrides.

README usage gains:

```
igv-rs ref.fa -l loops.bedpe
igv-rs ref.fa -l hiccups.bedpe.gz -l abc.bedpe
igv-rs ref.fa -l loops.bedpe --link-min-score 5.0
igv-rs ref.fa -b s.bam -g genes.gff3 -l loops.bedpe -r chr1:1000-2000
```

### 3.6 `AppState` fields

```rust
pub links: Vec<Arc<dyn LinkSource>>,
pub link_records: Vec<Vec<LinkRecord>>,  // index = track_index
pub link_track_height: u16,              // default 6, clamped [3, 16]
pub link_min_score: Option<f64>,         // mirrors CLI flag
```

`Vec<LinkRecord>` (owned) is held in `AppState` rather than the borrowed
`VisibleLink<'a>` because the loader runs on its own task and ownership
must transfer across the channel. The widget re-derives `LinkScope` from
the in-window slice each frame; the cost is `O(n_visible)` and dwarfed by
the IntervalTree query upstream.

### 3.7 `igv-render::link`

```rust
pub fn paint_link_track(
    canvas: &mut Svg,
    layout: &TrackLayout,
    inputs: &LinkRenderInputs,
    theme: &GraphicalTheme,
    opts: &SvgOptions,
);

pub struct LinkRenderInputs<'a> {
    pub records: &'a [LinkRecord],
    pub region: &'a Region,
    pub min_score: Option<f64>,
    pub display_name: &'a str,
}
```

The renderer mirrors the TUI mode-selection logic but with graphics-grade
detail:

- Arc mode: each arc is a cubic Bézier curve from anchor A midpoint to
  anchor B midpoint, control points lifted vertically by `0.5 *
  pixel_distance` for a shallow loop, capped at the track top. Anchor
  bars are filled rectangles. Score → continuous color via a viridis-like
  ramp (sampled in linear space across the per-window score range).
  PartialCis arcs render with a real arrowhead at the off-window edge and
  a typeset distance label. Trans markers are typeset text with a small
  diamond glyph.
- Heatmap mode: a single horizontal strip whose per-pixel color is
  `score → viridis`. No quantile bucketing — full continuous range.
- Mode selection reuses the same `arc_count vs row_budget` rule, with
  `row_budget` derived from the SVG track height in scaled pixels (so a
  user requesting `--snapshot-link-height 200px` gets ~25 arc rows
  before mode-switching).
- `igv-core::render_inputs::RenderInputs` gains a `links: Vec<&LinkRecord>`
  field plus per-track display metadata; populated by `AppState` for
  interactive snapshots and by the batch loop for non-TUI runs.

## 4. Data flow

### 4.1 Startup

```
CLI -l a.bedpe -l b.bedpe.gz [--link-min-score N]
  → for each path: open_link(path, fmt).await?
       └─ spawn_blocking(parse_bedpe_file)
       └─ build per-chromosome IntervalTree on each anchor
  → Vec<Arc<dyn LinkSource>>
  → AppState { links, link_records: vec![Vec::new(); n], link_min_score: N, … }
  → first dispatch(LoadRequest { generation: 0, region, fetch_opts })
```

If any link source's `record_count() > 100_000`, write one footer toast
on the first frame: `loaded N links from <basename>` (so users feel where
the startup latency went). Threshold is `100_000`, chosen as 1-2 s of
parse on a typical workstation.

### 4.2 Per-dispatch (pan / zoom / goto / resize)

```
dispatch(req):
    cancel all in-flight handles
    spawn fetch(reference)
    spawn fetch(vcf) if any
    for each bam: spawn fetch(bam, idx)
    for each annotation: spawn fetch(ann, idx)
    for each signal: spawn fetch(signal, idx)
    for each link: spawn fetch(link, idx)
                   │
                   └─→ FetchLinkOpts { min_score: state.link_min_score }
```

`link.query()` is fully in-memory IntervalTree; expected latency < 1 ms
even at 1M records. Returned `Vec<VisibleLink>` is materialized into
owned `Vec<LinkRecord>` for channel transport, then becomes
`state.link_records[track_index]`.

Generation guard identical to existing tracks; stale results dropped.

### 4.3 Render path (per frame)

Layout, top to bottom (additions in **bold**):

```
Header
Ruler
Sequence
Variants            (if VCF)
Coverage            (if BAM)
SignalWidget × N    (if -s)
Alignments          (if BAM)
Annotations × M     (if -g)
**LinkWidget × K    (if -l)**
Footer
```

**Position rationale:** the brainstorming default proposed "annotations
above, signals below" for the link track. The existing layout, however,
puts signals **above** annotations (signals between coverage and
alignments; annotations at the bottom). Reshuffling that order would
churn every existing widget test and snapshot for no real gain. We
therefore place links **immediately below annotations**, at the bottom
of the data band. The intended grouping ("links live alongside the
regulatory-context tracks") is honored by being adjacent to annotations;
the "above signals" half of the original phrasing is dropped because
signals are physically higher in the existing layout. No existing track
moves.

### 4.4 No caching (v1)

Every region change re-runs `query()`. Because the IntervalTree lives in
memory, the cost is bounded by the number of *visible* records, not the
file size. Empirically:

- 100k records, 100 visible per query → < 100 µs.
- 1M records, 10k visible per query → ~5 ms.

A future LRU cache keyed on `(track_index, region, min_score)` can be
added without trait changes if profiling justifies it.

## 5. Error handling

| Stage | Failure | Behavior |
|---|---|---|
| Startup: `open_link()` IO error | bad/missing file | **fatal** — `IgvError`, exit non-zero (matches BAM / signal) |
| Startup: extension unrecognised, no `--link-format` | e.g. `data.bedpe.bak` | **fatal** — `IgvError::Other("cannot determine link format for '<path>'; pass --link-format")` |
| Startup: malformed BEDPE line | non-numeric coords, < 6 cols | **degrade** — `tracing::warn!("bedpe {path}:{n}: <reason>; skipping")`, parser continues |
| Startup: zero valid records after parse | empty file, all malformed | **degrade** — track loads with `record_count == 0`; widget shows "no links". Not fatal because the file might be intentionally filtered. |
| Runtime: `query()` chrom not in tree | jump to absent chromosome | **degrade** — return `Vec::new()`; widget renders empty band |
| Runtime: stale result | rapid pan | dropped via `generation` guard (existing) |

Principle: **strict at startup for IO and format, lenient for record-level
parse errors and runtime queries** — same as BAM / annotation code paths.

## 6. Hotkeys, theme, README

### 6.1 New `Action` variants

```rust
ResizeLink(i16),
```

(No "toggle mode" action — mode switch is automatic. Users can force arc
mode by growing the track with `>` until `arc_count <= height - 2`.)

### 6.2 Key bindings

| Key | Action |
|---|---|
| `>` | `ResizeLink(+1)` |
| `<` | `ResizeLink(-1)` |

Existing bindings: `+`/`-` (alignments), `]`/`[` (coverage), `}`/`{`
(signals). `<`/`>` is the next visually adjacent pair, suggests
shrink/grow on the qwerty layout, and does not collide with command
palette characters (`>` is not a palette command).

`ResizeLink` clamps `link_track_height` to `[3, 16]`. Minimum 3 ensures
at least one arc row + anchor strip + title.

### 6.3 Theme key

```toml
[theme.custom]
LINK = "magenta"
```

Both `Theme::default_dark()` and `default_light()` add a `LINK` entry.
`paper`, `solarized-*`, `dracula`, `gruvbox-dark` all gain a `LINK`
entry tuned to their palette. v1 uses one base color across all link
tracks; the four score buckets use `dim`, default, `bold`, and `bold` of
this color (curve and anchor styled separately within bucket 2 only). A
future per-track color scheme can use indexed keys (`LINK.0`, `LINK.1`,
…) without a schema change.

`igv-render::theme::GraphicalTheme` gains a corresponding `link_color`
field plus a `link_gradient` (a small viridis-like ramp baked into the
theme, so SVG output is theme-consistent).

### 6.4 README updates

- `Usage`: add `-l` examples.
- `Wide-zoom behavior` table: add a `links` column. Always `yes` (in-memory
  query is cheap at every zoom level).
- `Keybindings`: add `<` / `>` rows.
- `Configuration`: mention `LINK` theme key.
- `Layout`: mention `crates/igv-core/src/source/link.rs` and
  `crates/igv-render/src/link.rs`.
- `Known limitations`: bullets per §8.

## 7. Testing strategy

### 7.1 `igv-core` unit tests

`crates/igv-core/tests/link_format.rs`:

- `format_dispatch_by_extension` — `.bedpe`, `.bedpe.gz`, `.BEDPE` →
  `Some(Bedpe)`; `.bedpe.bak` → `None`; `.bw` → `None`
- `format_parse_string` — `"bedpe"`, `"BEDPE"` → `Some(Bedpe)`; `"interact"`
  → `None`

`crates/igv-core/tests/link_bedpe.rs`:

Fixture `crates/igv-core/tests/data/sample.bedpe` (text, ~30 lines, hand-
crafted), covering:

- Two cis loops on chr1, both anchors in `[1_000_000, 2_000_000]`.
- One cis loop on chr1 spanning `[500_000, 5_000_000]` (used to test
  partial-window and "spanning" cases).
- One trans pair `chr1 ↔ chr2`.
- Lines with `.` for name / score / strand.
- One malformed line (only 4 columns) — expected to be skipped with warn.
- One line with an unknown chromosome (`chrX_random`) — kept; query for
  that chromosome should return it.

Cases:

1. `open_link(sample.bedpe, None)` succeeds; `record_count() == 5`
   (malformed line skipped).
2. `query(chr1:1_500_000-1_600_000, opts default)` returns the two cis
   loops as `BothIn` and the spanning loop as `PartialCis` for the
   off-window anchor; the trans pair if its anchor lies in the window.
3. `query(chr1:1_000-2_000, opts default)` returns empty (no anchors).
4. `query(chr2:4_999_500-5_000_500, opts default)` returns the trans
   pair as `Trans { off_chrom: chr1, .. }`.
5. `query(chr1:1_500_000-1_600_000, opts { min_score: Some(10.0) })`
   filters out a low-score loop (depending on fixture scores).
6. Records with `score == None` survive any `--link-min-score` filter.
7. `.bedpe.gz` round trip: gzip the same fixture; results identical.

### 7.2 `igv-tui` widget snapshot tests

`crates/igv-tui/tests/link_widget_snapshot.rs`:

5 fixed-input cases rendered to a `TestBackend`, asserted via inline
strings (matches existing widget snapshot style):

1. **arc-sparse**: 3 in-window cis links, height 6 → 3 stacked arcs +
   anchor strip + title.
2. **arc-partial**: 1 in-window cis link + 1 partial-cis with off-window
   anchor 500 kb to the right → arc + half-arrow `─►500kb`.
3. **arc-trans**: 1 in-window cis link + 1 trans link to chr2 → arc +
   `⤴ chr2:5M` marker.
4. **heatmap**: 500 in-window links, height 6 → 4-row heatmap strip + title.
5. **empty**: 0 visible links → blank band + "0 loops" title.

### 7.3 `igv-tui` integration test

`crates/igv-tui/tests/link_dispatch.rs`:

- Hand-written mock `LinkSource` (no IntervalTree, returns canned `Vec`).
- Construct `Loader` with two mock link tracks; call `dispatch(req)`.
- Drain the channel; assert both `LoadResult::Link` arrive with correct
  `track_index` and expected record counts.

### 7.4 `igv-render` SVG snapshot tests

`crates/igv-render/tests/link_svg_snapshot.rs`:

- arc-sparse case → SVG; assert presence of `<path d="M ... C ...">`
  (Bézier) and `<rect>` (anchor) elements; assert color hex matches the
  expected viridis bucket.
- heatmap case → SVG; assert presence of one `<rect>` strip per column.

(Snapshot text is committed; comparison is exact-string.)

### 7.5 Manual test checklist (pre-merge)

1. `igv-rs ref.fa -l loops.bedpe` — single track renders.
2. `igv-rs ref.fa -l a.bedpe -l b.bedpe.gz` — two tracks stacked.
3. Pan to a region with 3 links → arc mode; pan to one with 500 → heatmap.
4. Press `>` repeatedly with 50 visible links → eventually flips to arc mode.
5. Pan so a link becomes partial → half-arrow appears with correct distance.
6. Trans link visible → `⤴` marker shows correct off-chromosome.
7. `--link-min-score 10` drops low-score loops; un-scored loops remain.
8. `:snapshot out.svg` while a link track is loaded → SVG opens in browser
   with Bézier arcs and viridis colors.
9. Snapshot batch mode `--snapshot-bed regions.bed` with `-l` → each
   region's SVG includes the link panel.
10. Startup with malformed BEDPE → warns per line, app starts.
11. Startup with nonexistent BEDPE → fails fast.

### 7.6 Out of scope for tests

- IntervalTree internals (trust `iset`).
- ratatui pixel-level correctness beyond the snapshot strings.
- Production BEDPE file shapes (HiCCUPS, ABC, ChIA-PET) — exercised
  manually, not in fixtures.
- resvg PNG byte-level snapshot (PNG path is delegated to existing
  `igv-render::png` infrastructure; covered by snapshot-export spec).

## 8. Phasing

The implementation plan (separate doc, `docs/superpowers/plans/`) breaks
this into phases:

1. **Core trait + types** — `link.rs` skeleton, `LinkFormat`, `LinkRecord`,
   `LinkScope`, `FetchLinkOpts`, format-dispatch tests.
2. **BEDPE parser + IntervalTree backend** — `BedpeLinkSource`, fixture
   generation, parse + query tests.
3. **Loader wiring** — `Loader::new` signature, `LoadResult::Link`,
   dispatch lane, integration test.
4. **CLI + AppState** — clap flags, state fields, `main.rs` glue,
   record-count toast.
5. **Widget** — `LinkWidget` arc mode, heatmap mode, partial / trans
   markers, theme key, snapshot tests.
6. **Hotkeys** — `<` / `>`; `Action::ResizeLink`; input dispatch.
7. **`igv-render` integration** — `LinkRenderInputs`, `paint_link_track`,
   Bézier + viridis, SVG snapshot tests.
8. **Docs** — README updates, this spec → done state.

## 9. Open / deferred items

- **bigInteract / UCSC interact format** — separate spec; will reuse
  `LinkRecord` and add a `InteractLinkSource` impl. Source-typed fields
  (`source` vs `target`, RGB color, value semantics) become extensions on
  `LinkRecord` rather than a new type.
- **Pairix / tabix backend for >1M records** — separate spec. The
  `LinkSource` trait is already async to support a streaming impl; the
  existing in-memory impl will remain the default.
- **Per-track color scheme** — indexed theme keys `LINK.0`, `LINK.1`, …
  with a fallback chain to `LINK`.
- **Per-record score-column override** (`--link-score-col N`) — for users
  whose pipeline writes the score in column 11+. Keep deferred: no
  user-confirmed demand and CLI surface grows.
- **Window-stable score normalization** — v1 normalizes per visible
  window; adjacent panning frames may shift colors. A future
  per-chromosome (or whole-file) normalization mode behind a toggle would
  fix this for users who care about cross-window comparability.
- **Trans link arc rendering across panels** — current design shows trans
  as edge markers only. A future split-view layout could render two
  region panels side-by-side with a connecting line.
- **HiC contact matrices (`.cool`, `.hic`)** — fundamentally different
  data shape (dense matrix); separate spec, separate widget.
- **bigBed (`.bb`)** — already deferred by the bigwig spec; unrelated to
  this one.
