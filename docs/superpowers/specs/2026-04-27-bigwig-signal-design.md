# bigWig signal-track support — design

**Date**: 2026-04-27
**Status**: design accepted; implementation pending
**Scope**: add bigWig (`.bw` / `.bigwig`) read-only support as a new
"signal track" axis in `igv-rs`, alongside the existing reference / variants /
alignments / coverage / annotations axes. Lays groundwork for additional
signal formats (bedGraph, wig) to plug in without a trait change.

Out of scope: bigBed; signal writing; per-track colormap; log scale; window
caching.

## 1. Motivation & use cases

Bigwig is the canonical format for quantitative signal in genomics:

- ChIP-seq / ATAC-seq / CUT&RUN peak intensity
- RNA-seq smoothed depth
- Conservation scores (phyloP / phastCons)

Current `igv-rs` users either pre-convert to BED-like text (loses precision)
or run a parallel desktop IGV. Adding native bigwig means a server-side
genome viewer can replace the local-IGV detour for the dominant ChIP/ATAC/
RNA-seq workflows.

User-confirmed scope for v1:
- Primary data is non-negative (ChIP-seq + RNA-seq); no negative-value /
  zero-baseline / diverging-colormap support.
- Typical workload: 2-4 stacked bigwig tracks (e.g. input vs IP, or several
  histone marks).
- Per-track auto-scale by default, with a hotkey to toggle a shared
  auto-scale across all signal tracks.

## 2. Architecture

A bigwig track is treated as a **signal track**, semantically distinct from
both the per-base BAM coverage widget (which is computed from alignment
records) and from annotations (which carry intervals/structure, not numeric
intensity). The split:

- **`igv-core::source::signal`** (new module) — `SignalSource` trait,
  `SignalBin` struct, `SignalFormat` enum, `BigWigSignalSource`
  implementation. Async, UI-free.
- **`igv-tui::ui::widgets::signal::SignalWidget`** (new) — bar-chart
  rendering matching the visual language of `CoverageWidget`.
- **`Loader`** (extended) — adds a parallel `signals` fetch lane,
  generation-guarded like every other source.
- **`AppState`** (extended) — holds `signals`, `signal_bins`,
  `signal_shared_scale`, `signal_track_height`.
- **CLI** (extended) — `-s` / `--signal` (repeatable) and
  `--signal-format` override.

Boundary rationale: `SignalSource` does not depend on ratatui or tokio's
runtime; the same trait will accept a future `BedGraphSignalSource` /
`WigSignalSource` without API churn. The UI consumes `Vec<SignalBin>`
exactly as `AnnotationsWidget` consumes `Vec<AnnotationTranscript>` today.

## 3. Components

### 3.1 `igv-core::source::signal`

```rust
pub struct SignalBin {
    pub start: u64,   // 1-based inclusive
    pub end: u64,     // 1-based inclusive
    pub value: f32,   // aggregated value
}

#[derive(Debug, Clone, Copy)]
pub enum SignalSummary { Max, Mean, Sum, Min }

#[derive(Debug, Clone, Copy)]
pub struct FetchSignalOpts {
    pub max_bins: u32,
    pub summary: SignalSummary,
}

#[async_trait]
pub trait SignalSource: Send + Sync {
    async fn fetch(&self, region: &Region, opts: &FetchSignalOpts)
        -> Result<Vec<SignalBin>>;
    fn display_name(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalFormat { BigWig }

impl SignalFormat {
    pub fn from_path(p: &Path) -> Option<Self>;  // .bw, .bigwig (case-insensitive)
    pub fn parse(s: &str) -> Option<Self>;       // "bw", "bigwig"
}

pub async fn open_signal(
    path: &Path,
    fmt: Option<SignalFormat>,
) -> Result<Arc<dyn SignalSource>>;
```

`SignalSummary` defaults to `Max` for v1 across all tracks (preserves peaks
in ChIP-seq, "good enough" for RNA-seq at terminal resolution). A future
`--signal-summary` flag can override.

### 3.2 `BigWigSignalSource` (bigtools backend)

```rust
pub struct BigWigSignalSource {
    display: String,
    reader: tokio::sync::Mutex<bigtools::BigWigRead<bigtools::ReopenableFile>>,
}
```

- `open_signal()` opens the file once; bigtools parses the BBI header
  (chrom table, zoom levels) at open time. The `BigWigRead` value is held
  in a `tokio::sync::Mutex` for the lifetime of the source.
- `fetch()` acquires the mutex, runs the read on `tokio::task::spawn_blocking`
  (bigtools is sync), releases. BBI parse happens **once**, not per fetch
  — the reason this differs from BAM (where noodles' `Box<dyn BinningIndex>`
  is `!Send` and forces re-open).
- Concurrency: each `BigWigSignalSource` has its own mutex, so distinct
  bigwig files fetch fully in parallel; concurrent requests against the
  same file serialize (acceptable — the `generation` guard already ensures
  only one in-flight request per track per region change).
- If a future bigtools API exposes a thread-safe / cloneable reader (e.g.
  `CachedBBIFileRead`), the impl can swap to it without changing the
  `SignalSource` trait.

**Adaptive zoom-level selection** (key correctness/performance requirement):

```
let bp_per_col = region.width() / max_bins;
if bp_per_col >= 16 {
    // delegate to bigtools zoom-summary; bigtools picks the closest
    // pre-computed zoom level whose target_bin_size <= bp_per_col
    bins = reader.get_zoom_summary(chrom, start, end, max_bins, summary);
} else {
    // fine zoom — fetch raw values, one bin per genomic position
    let raw = reader.values_overlapping(chrom, start, end);
    bins = raw.map(|(pos, v)| SignalBin { start: pos, end: pos, value: v });
}
```

The `16` threshold is hardcoded for v1; future `[render]` config key
`signal_zoom_threshold_bp` can override. Below 16 bp/col, raw values are
small (a few KB of f32 per terminal-width region) and the column-level
aggregation in the widget is cheap.

### 3.3 `igv-tui::ui::widgets::signal::SignalWidget`

```rust
pub struct SignalWidget<'a> {
    pub bins: &'a [SignalBin],
    pub display_name: &'a str,
    pub region: &'a Region,
    pub theme: &'a Theme,
    pub shared_max: Option<f32>,   // Some(g) when shared_scale is on
}
```

- Renders as a bar chart, column-per-column, using `█` and theme key
  `SIGNAL` (cyan default) — same visual language as `CoverageWidget`.
- Title row: `signal[<name>] [0-<max>]` for per-track scale, or
  `signal[<name>] [0-<max>*]` (asterisk) when shared.
- Y-axis scaling: `track_max = shared_max.unwrap_or(self_max)`. If the
  resolved max is 0, the widget renders an empty band (no bars).
- For each terminal column `c`, find the bins whose `[start, end]` overlap
  the genomic range mapped to column `c`, take the **max** of their
  `value`s, render `bar_h = ceil(v / scale * height)`.

### 3.4 `Loader` extension

Add to `Loader`:

```rust
pub signals: Vec<Arc<dyn SignalSource>>,
```

Add to `LoadResult`:

```rust
Signal {
    generation: u64,
    track_index: usize,
    bins: Vec<SignalBin>,
},
```

`dispatch()` adds a fetch loop mirroring the existing `annotations` block;
on error it logs a `tracing::warn!` and emits an empty `bins` payload (per
§4 below).

### 3.5 CLI

```rust
/// Path to a bigWig signal file (.bw / .bigwig). May be repeated.
#[arg(short = 's', long = "signal")]
pub signals: Vec<PathBuf>,

/// Override signal format auto-detection (currently only "bigwig").
#[arg(long = "signal-format")]
pub signal_format: Option<String>,
```

`-s` chosen to keep namespace open for future `bedGraph` / `wig`. README
usage gains:

```
igv-rs ref.fa -s chip.bw -s input.bw -r chr1:1-10000000
```

### 3.6 `AppState` fields

```rust
pub signals: Vec<Arc<dyn SignalSource>>,
pub signal_bins: Vec<Vec<SignalBin>>,   // index = track_index
pub signal_shared_scale: bool,          // default false
pub signal_track_height: u16,           // default 4, clamped [2, 12]
```

## 4. Data flow

### 4.1 Startup

```
CLI -s a.bw -s b.bw
  → for each path: open_signal(path, fmt).await?
  → Vec<Arc<dyn SignalSource>>
  → Loader::new(fasta, vcf, bams, annotations, signals, tx)
  → AppState { signals, signal_bins: vec![Vec::new(); n], … }
  → first dispatch(LoadRequest { generation: 0, region, fetch_opts })
```

### 4.2 Per-dispatch (pan / zoom / goto / resize)

```
dispatch(req):
    cancel all in-flight handles
    spawn fetch(reference)
    spawn fetch(vcf) if any
    for each bam: spawn fetch(bam, idx)
    for each annotation: spawn fetch(ann, idx)
    for each signal: spawn fetch(signal, idx)
                     │
                     └─→ FetchSignalOpts {
                            max_bins: terminal_inner_width,
                            summary: SignalSummary::Max,
                         }
```

Each `signal.fetch()` runs the §3.2 adaptive logic. Result returns via
channel as `LoadResult::Signal { generation, track_index, bins }`. Main
loop applies it iff `generation == state.generation`; stale results are
dropped (existing mechanism).

### 4.3 Render path (per frame)

Layout, top to bottom:

```
Header
Ruler
Sequence
Variants            (if VCF)
Coverage            (if BAM)
SignalWidget × N    (NEW — between coverage and alignments)
Alignments          (if BAM)
Annotations × M     (if -g)
Footer
```

Shared-scale computation (when `signal_shared_scale == true`):

```rust
let global_max = state.signal_bins.iter().flatten()
    .map(|b| b.value).fold(0.0, f32::max);
```

### 4.4 No caching (v1)

Every region change re-fetches every signal track from scratch. This
matches the existing BAM and annotation behavior (`Loader::dispatch` aborts
in-flight handles and re-spawns). Justification:

- bigtools R-tree lookup is O(log n); a cold fetch over a terminal-width
  region returns in < 5 ms in practice.
- bigwig zoom summaries are pre-computed in the file — wide-zoom panning
  reads only a few KB.
- Caching adds key-management complexity (cache key must include
  `max_bins`; terminal resize invalidates).

A future LRU window cache can be added without trait changes if profiling
ever justifies it.

## 5. Error handling

| Stage | Failure | Behavior |
|---|---|---|
| Startup: `open_signal()` BBI parse fails | bad/missing file | **fatal** — main.rs prints `IgvError` and exits non-zero (matches BAM) |
| Startup: extension unrecognised, no `--signal-format` | e.g. `data.bw.bak` | **fatal** — `IgvError::Other("cannot determine signal format for '<path>'; pass --signal-format")` |
| Runtime: `fetch()` chrom not in bigwig | jump to absent chromosome | **degrade** — return `Vec::new()` + `tracing::warn!`; widget renders empty band |
| Runtime: IO error during fetch | FD lost, file removed | **degrade** — return `Vec::new()` + `tracing::warn!` |
| Runtime: stale result | rapid pan | dropped via `generation` guard (existing) |

Principle: **strict at startup, lenient at runtime** — same as BAM /
annotation code paths.

## 6. Hotkeys, theme, README

### 6.1 New `Action` variants

```rust
ToggleSignalSharedScale,
ResizeSignal(i16),
```

### 6.2 Key bindings

| Key | Action |
|---|---|
| `\` | `ToggleSignalSharedScale` |
| `}` | `ResizeSignal(+1)` |
| `{` | `ResizeSignal(-1)` |

`+`/`-` and `]`/`[` are already bound to alignment / coverage resize, so
signal uses the next visually adjacent pair `}`/`{`. `\` is a single-press
toggle, intentionally not paired.

> **Note on `\` instead of `=`:** the original draft of this design suggested
> `=`, but `=` is already aliased to `+` for `ResizeAlignments` in
> `crates/igv-tui/src/input.rs`. `\` is single-key, ergonomic, and groups
> visually with `}` / `{`.

`ResizeSignal` clamps `signal_track_height` to `[2, 12]`.

### 6.3 Theme key

```toml
[theme.custom]
SIGNAL = "cyan"
```

Both `Theme::default_dark()` and `default_light()` add a `SIGNAL` entry.
v1 uses one color across all signal tracks. Future per-track colors can
use indexed keys (`SIGNAL.0`, `SIGNAL.1`, ...) without a schema change.

### 6.4 README updates

- `Usage`: add `-s` example.
- `Keybindings`: add three rows (`=`, `}`, `{`).
- `Configuration`: mention `SIGNAL` theme key.
- `Layout`: mention `crates/igv-core/src/source/signal.rs`.
- `Known limitations`: add bullets — no in-memory cache, single colormap,
  log/summary deferred, no bigBed.

## 7. Testing strategy

### 7.1 `igv-core` unit tests

`crates/igv-core/tests/signal_format.rs`:

- `format_dispatch_by_extension` — `.bw`, `.bigwig`, `.bigWig` → `Some(BigWig)`;
  `.bw.gz` (gzip not supported by spec) → `None`; `.bam` → `None`
- `format_parse_string` — `"bw"`, `"BIGWIG"` → `Some(BigWig)`; `"bigbed"` → `None`

`crates/igv-core/tests/signal_bigwig.rs`:

Fixture `crates/igv-core/tests/data/small.bw` — committed binary (~50 KB),
pre-generated offline using bigtools' writer. Two chromosomes:

- `chr1` length 1000, signal: ramp 0..1000 (`v[i] = i as f32`)
- `chr2` length 500, signal: square wave (10.0 in `[100, 200)` and
  `[300, 400)`, 0 elsewhere)

Cases:

1. `open_signal(small.bw, None)` succeeds, `display_name() == "small.bw"`
2. `fetch(chr1:1-100, max_bins=100, Max)` → 100 bins, value at bin `i` ≈ `i`
3. `fetch(chr1:1-1000, max_bins=10, Max)` → 10 bins, last bin value ~999
   (zoom-summary path)
4. `fetch(chr2:50-450, max_bins=400, Max)` → bins inside `[100,200)` and
   `[300,400)` show 10.0, others 0.0
5. `fetch(chr3:1-100, …)` → returns empty `Vec`, no panic
6. `fetch(chr1:1-100, max_bins=100, Mean)` → mean-summary path returns
   plausible values

### 7.2 `igv-tui` smoke test

Append to existing `crates/igv-tui/tests/render_smoke.rs`:

- Build `AppState` with one mock `SignalSource` (hand-written impl
  returning fixed `Vec<SignalBin>`).
- Render one frame to a `TestBackend`.
- Assert: signal track row count == `signal_track_height` (default 4);
  title contains `[0-`; at least one cell is `█`.

No full visual snapshot test (matches the existing limitation in §
"Known limitations" for other widgets).

### 7.3 Integration test

`crates/igv-tui/tests/signal_dispatch.rs`:

- Hand-written mock `SignalSource` (no bigtools dependency).
- Construct `Loader` with two mock signal tracks; call `dispatch(req)`.
- Drain the channel; assert both `LoadResult::Signal` arrive with correct
  `track_index`.

### 7.4 Manual test checklist

To be performed before merge:

1. `igv-rs ref.fa -s chip.bw` — single track renders.
2. `igv-rs ref.fa -s a.bw -s b.bw` — two tracks stacked, per-track scale.
3. Press `=` — title shows `[0-N*]`, both tracks share scale.
4. Press `}` / `{` — height changes, clamped.
5. `g chr1:1-100000000⏎` then rapid `h`/`l` — no flicker, generation
   guard works.
6. Resize terminal — refetch happens, no stale frame.
7. `igv-rs ref.fa -s nonexistent.bw` — startup fails with clear error.
8. Jump to chrom not in `.bw` file — empty band + log warn.

### 7.5 Out of scope for tests

- bigtools internals (trust upstream)
- ratatui pixel-level correctness (smoke only)
- Production bigwig file shapes — exercised manually, not in fixtures

## 8. Phasing

The implementation plan (separate doc, `docs/superpowers/plans/`) breaks
this into phases:

1. **Core trait + types** — `signal.rs` skeleton, `SignalFormat`,
   `FetchSignalOpts`, tests for format dispatch.
2. **bigtools backend** — `BigWigSignalSource`, adaptive zoom logic,
   fixture generation, fetch tests.
3. **Loader wiring** — `Loader::new` signature, `LoadResult::Signal`,
   dispatch lane.
4. **CLI + AppState** — clap flags, state fields, main.rs glue.
5. **Widget** — `SignalWidget`, layout slot, theme key.
6. **Hotkeys** — `=`, `}`, `{`; `Action` variants; input dispatch.
7. **Tests + smoke** — integration test, render smoke, manual checklist.
8. **Docs** — README updates, this spec → done state.

## 9. Open / deferred items

- Log scale (`--signal-log` and runtime toggle)
- Per-track colormap (`SIGNAL.0`, `SIGNAL.1`, …)
- `--signal-summary mean|sum|min|max`
- Window LRU cache
- bigBed (`bb`) — separate spec; trait shape will likely accommodate
- bedGraph / wig — same trait, plain-text parsers
- `[render]` config key for `signal_zoom_threshold_bp` (currently 16,
  hardcoded)
