# bigWig signal-track Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add read-only bigWig (`.bw` / `.bigwig`) support as a new "signal track" axis in `igv-rs`, exposed via `-s` / `--signal`, rendered as bar-chart tracks between coverage and alignments with per-track + shared auto-scale modes.

**Architecture:** Mirror the existing `AnnotationSource` split — `igv-core::source::signal` defines the trait, types, and bigtools-backed implementation; `igv-tui::ui::widgets::signal` renders. `Loader` gets a parallel `signals` fetch lane with the existing generation-guard pattern. Per-source `Mutex<BigWigRead>` so BBI is parsed once. Adaptive: zoom-summary when `bp_per_col >= 16`, raw values below.

**Tech Stack:** Rust workspace · `bigtools` (current 0.x) · ratatui 0.29 · tokio async · async-trait.

**Reference spec:** `docs/superpowers/specs/2026-04-27-bigwig-signal-design.md`

---

## File structure

**Created:**
- `crates/igv-core/src/source/signal.rs` — trait + types + format dispatch + `open_signal`
- `crates/igv-core/src/source/signal/bigwig.rs` — `BigWigSignalSource`
- `crates/igv-core/examples/gen_small_bw.rs` — one-off fixture writer
- `crates/igv-core/tests/data/small.bw` — committed binary fixture (~50 KB)
- `crates/igv-core/tests/signal_format.rs` — format dispatch tests
- `crates/igv-core/tests/signal_bigwig.rs` — fetch path tests
- `crates/igv-tui/src/ui/widgets/signal.rs` — `SignalWidget`
- `crates/igv-tui/tests/signal_dispatch.rs` — loader integration test

**Modified:**
- `crates/igv-core/Cargo.toml` — add `bigtools`
- `crates/igv-core/src/source/mod.rs` — re-export signal types
- `crates/igv-tui/src/cli.rs` — `-s`, `--signal-format`
- `crates/igv-tui/src/main.rs` — open signals + thread into Loader/AppState
- `crates/igv-tui/src/app/loader.rs` — signals lane + `LoadResult::Signal`
- `crates/igv-tui/src/app/state.rs` — new fields + apply for new actions + clear-on-region-change
- `crates/igv-tui/src/app/action.rs` — `ToggleSignalSharedScale`, `ResizeSignal`
- `crates/igv-tui/src/input.rs` — `\`, `}`, `{` bindings
- `crates/igv-tui/src/ui/layout.rs` — signal slot between coverage and alignments
- `crates/igv-tui/src/ui/theme.rs` — `SIGNAL` key in dark/light
- `crates/igv-tui/src/ui/widgets/mod.rs` — `pub mod signal;`
- `crates/igv-tui/tests/render_smoke.rs` — signal smoke case
- `README.md` — usage, keys, config, layout, limitations

**Hotkey deviation from spec §6.2:** `\` (was `=`) for `ToggleSignalSharedScale` — `=` is already aliased to `+` for `ResizeAlignments` (`input.rs:62`). `}` / `{` for `ResizeSignal` are unchanged.

---

## Phase 1 — Core trait + types + format dispatch

### Task 1.1: Scaffold `signal.rs` and re-export

**Files:**
- Create: `crates/igv-core/src/source/signal.rs`
- Modify: `crates/igv-core/src/source/mod.rs:1-32`

- [ ] **Step 1: Create the signal module with trait + types + factory stub**

Write `crates/igv-core/src/source/signal.rs`:

```rust
//! Signal-track source — numeric quantitative tracks (bigWig today;
//! bedGraph / wig in the future) rendered as bar-chart widgets.
//!
//! The concrete bigtools-backed implementation lives in `signal::bigwig`.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::error::{IgvError, Result};
use crate::region::Region;

pub mod bigwig;

#[derive(Debug, Clone, PartialEq)]
pub struct SignalBin {
    pub start: u64,   // 1-based inclusive
    pub end: u64,     // 1-based inclusive
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalSummary {
    Max,
    Mean,
    Sum,
    Min,
}

#[derive(Debug, Clone, Copy)]
pub struct FetchSignalOpts {
    pub max_bins: u32,
    pub summary: SignalSummary,
}

impl Default for FetchSignalOpts {
    fn default() -> Self {
        Self { max_bins: 200, summary: SignalSummary::Max }
    }
}

#[async_trait]
pub trait SignalSource: Send + Sync {
    async fn fetch(
        &self,
        region: &Region,
        opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>>;
    fn display_name(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalFormat {
    BigWig,
}

impl SignalFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bw" | "bigwig" => Some(Self::BigWig),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        if lower.ends_with(".bw") || lower.ends_with(".bigwig") {
            return Some(Self::BigWig);
        }
        None
    }
}

/// Open a signal file, dispatching to the right backend by extension
/// (or by `format_override` if given).
pub async fn open_signal(
    path: &Path,
    format_override: Option<SignalFormat>,
) -> Result<Arc<dyn SignalSource>> {
    let format = format_override
        .or_else(|| SignalFormat::from_path(path))
        .ok_or_else(|| {
            IgvError::Other(format!(
                "cannot determine signal format for '{}'; pass --signal-format",
                path.display()
            ))
        })?;
    match format {
        SignalFormat::BigWig => {
            let src = bigwig::BigWigSignalSource::open(path).await?;
            Ok(Arc::new(src))
        }
    }
}
```

- [ ] **Step 2: Create empty bigwig submodule placeholder so the module compiles**

Write `crates/igv-core/src/source/signal/bigwig.rs`:

```rust
//! BigWig signal source backed by the `bigtools` crate.
//! Populated in Phase 2; this stub keeps `signal.rs` compiling until then.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{FetchSignalOpts, SignalBin, SignalSource};

#[derive(Debug)]
pub struct BigWigSignalSource {
    display: String,
}

impl BigWigSignalSource {
    pub async fn open(_path: &Path) -> Result<Self> {
        Err(IgvError::Other("bigwig backend not yet implemented".into()))
    }
}

#[async_trait]
impl SignalSource for BigWigSignalSource {
    async fn fetch(
        &self,
        _region: &Region,
        _opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>> {
        Ok(Vec::new())
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
```

- [ ] **Step 3: Add `pub mod signal;` and re-exports in `source/mod.rs`**

Edit `crates/igv-core/src/source/mod.rs`. After `pub mod annotation;` (line 3) add `pub mod signal;`. Append to the bottom `pub use` block:

```rust
pub use signal::{
    open_signal, FetchSignalOpts, SignalBin, SignalFormat, SignalSource, SignalSummary,
};
```

- [ ] **Step 4: Verify it builds**

Run: `cargo build -p igv-core`
Expected: clean build, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/source/signal.rs \
        crates/igv-core/src/source/signal/bigwig.rs \
        crates/igv-core/src/source/mod.rs
git commit -m "feat(signal): scaffold SignalSource trait + format dispatch"
```

### Task 1.2: Format dispatch tests

**Files:**
- Create: `crates/igv-core/tests/signal_format.rs`

- [ ] **Step 1: Write the failing tests**

Write `crates/igv-core/tests/signal_format.rs`:

```rust
use std::path::PathBuf;

use igv_core::source::SignalFormat;

#[test]
fn format_dispatch_by_extension() {
    let cases = [
        ("a.bw", Some(SignalFormat::BigWig)),
        ("a.bigwig", Some(SignalFormat::BigWig)),
        ("a.bigWig", Some(SignalFormat::BigWig)),
        ("a.BW", Some(SignalFormat::BigWig)),
        ("a.bw.gz", None),
        ("a.bam", None),
        ("plain", None),
    ];
    for (name, expected) in cases {
        let got = SignalFormat::from_path(&PathBuf::from(name));
        assert_eq!(got, expected, "case {name}");
    }
}

#[test]
fn format_parse_string() {
    assert_eq!(SignalFormat::parse("bw"), Some(SignalFormat::BigWig));
    assert_eq!(SignalFormat::parse("BIGWIG"), Some(SignalFormat::BigWig));
    assert_eq!(SignalFormat::parse("BigWig"), Some(SignalFormat::BigWig));
    assert_eq!(SignalFormat::parse("bigbed"), None);
    assert_eq!(SignalFormat::parse(""), None);
}

#[tokio::test]
async fn open_signal_unknown_extension_errors_with_hint() {
    let err = igv_core::source::open_signal(
        std::path::Path::new("/nope.unknown"),
        None,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("--signal-format"), "msg: {msg}");
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p igv-core --test signal_format`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/igv-core/tests/signal_format.rs
git commit -m "test(signal): cover SignalFormat dispatch + open_signal hint"
```

---

## Phase 2 — bigtools backend + fixture + adaptive zoom

### Task 2.1: Add bigtools dependency

**Files:**
- Modify: `crates/igv-core/Cargo.toml:9-25`

- [ ] **Step 1: Add bigtools to `[dependencies]`**

Append after `noodles = { ... }` block (around line 16):

```toml
bigtools = { version = "0.5", default-features = false }
anyhow = "1"   # used by example fixture writer; if already in workspace deps, omit
```

> **Implementer note:** `bigtools` API has shifted across 0.4 / 0.5 / 0.6. If `0.5` doesn't resolve cleanly, pin to whatever current 0.x exposes a `BigWigRead::open(path)` (or `open_file`) returning a struct with `get_interval` / `values` and a way to query precomputed zoom-level summaries. Adjust feature flags so we get the read-side without writers (and without async runtimes that conflict with tokio). Document the chosen version in the commit body.

- [ ] **Step 2: Build and verify**

Run: `cargo build -p igv-core`
Expected: success (first compile of bigtools may take ~60s).

- [ ] **Step 3: Commit**

```bash
git add crates/igv-core/Cargo.toml Cargo.lock
git commit -m "chore(deps): add bigtools for bigwig parsing"
```

### Task 2.2: Generate `small.bw` fixture

**Files:**
- Create: `crates/igv-core/examples/gen_small_bw.rs`
- Create (output): `crates/igv-core/tests/data/small.bw`

The fixture **content** is fixed (test invariant); the **writer code** below is best-effort against bigtools' write API and may need minor adjustment.

Fixture spec (test invariant):
- `chr1` length 1000 — value at base position `i` (0-based) equals `i` (linear ramp 0..1000)
- `chr2` length 500 — value 10.0 across `[100, 200)` and `[300, 400)`, undefined elsewhere

- [ ] **Step 1: Write the example writer**

Create `crates/igv-core/examples/gen_small_bw.rs`:

```rust
//! One-off helper: writes `tests/data/small.bw` consumed by signal tests.
//! Run manually:  cargo run -p igv-core --example gen_small_bw
//! The output is committed; do not run from CI.

use std::collections::HashMap;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/small.bw");
    std::fs::create_dir_all(out.parent().unwrap())?;

    // Implementer: adapt to the bigtools version you pinned in Task 2.1.
    // Pseudocode (real call shape varies by version):
    //
    //   let mut chrom_sizes = HashMap::new();
    //   chrom_sizes.insert("chr1".to_string(), 1000u32);
    //   chrom_sizes.insert("chr2".to_string(), 500u32);
    //
    //   let writer = bigtools::BigWigWrite::create_file(out.to_str().unwrap(), chrom_sizes)?;
    //
    //   let chr1_iter = (0..1000u32).map(|i| ("chr1".to_string(),
    //       bigtools::Value { start: i, end: i + 1, value: i as f32 }));
    //   let chr2_iter = vec![
    //       ("chr2".to_string(), bigtools::Value { start: 100, end: 200, value: 10.0 }),
    //       ("chr2".to_string(), bigtools::Value { start: 300, end: 400, value: 10.0 }),
    //   ].into_iter();
    //
    //   writer.write(chr1_iter.chain(chr2_iter), &runtime)?;

    let _ = out;
    let _: HashMap<String, u32> = HashMap::new();
    todo!("fill in based on bigtools version chosen in Task 2.1");
}
```

- [ ] **Step 2: Adapt to actual bigtools API and run it**

Replace the `todo!` with real bigtools calls; iterate until it compiles. Then run:

```bash
cargo run -p igv-core --example gen_small_bw
```

Expected output: `wrote .../tests/data/small.bw` and a file ~30-100 KB.

- [ ] **Step 3: Sanity-check the fixture with the bigtools binary or a quick reader**

Run a one-off verification (in a Rust test or a `cargo run` ad-hoc):

```rust
let r = bigtools::BigWigRead::open(out.to_str().unwrap())?;
// expected chroms: chr1 (1000), chr2 (500)
```

If the fixture spec is wrong (wrong values, wrong chrom sizes), regenerate.

- [ ] **Step 4: Commit fixture + example**

```bash
git add crates/igv-core/examples/gen_small_bw.rs crates/igv-core/tests/data/small.bw
git commit -m "test(signal): commit small.bw fixture (chr1 ramp, chr2 square wave)"
```

### Task 2.3: Implement `BigWigSignalSource::open` with BBI parse

**Files:**
- Modify: `crates/igv-core/src/source/signal/bigwig.rs` (replace stub)

- [ ] **Step 1: Write the failing open test**

Append to `crates/igv-core/tests/signal_bigwig.rs` (create the file):

```rust
use std::path::PathBuf;

use igv_core::source::{open_signal, SignalSource};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/small.bw")
}

#[tokio::test]
async fn open_succeeds_and_reports_display_name() {
    let src = open_signal(&fixture(), None).await.unwrap();
    assert_eq!(src.display_name(), "small.bw");
}

#[tokio::test]
async fn open_nonexistent_path_returns_error() {
    let err = open_signal(std::path::Path::new("/no/such/file.bw"), None)
        .await
        .unwrap_err();
    let msg = err.to_string().to_lowercase();
    // Either io error or bigtools error; the message must mention the file
    // or be a bigwig parse error — anything useful is fine.
    assert!(!msg.is_empty());
}
```

- [ ] **Step 2: Verify it fails**

Run: `cargo test -p igv-core --test signal_bigwig`
Expected: FAIL — `open_succeeds_and_reports_display_name` errors with "bigwig backend not yet implemented".

- [ ] **Step 3: Replace the stub with a real `open()` that parses BBI**

Rewrite `crates/igv-core/src/source/signal/bigwig.rs`:

```rust
//! BigWig signal source backed by the `bigtools` crate.
//!
//! BBI header is parsed once at `open()` and the reader is held in a
//! `tokio::sync::Mutex` for the lifetime of the source — concurrent
//! `fetch()` calls against the same file serialize, distinct files run
//! fully in parallel.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::warn;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{FetchSignalOpts, SignalBin, SignalSource, SignalSummary};

// Concrete bigtools type alias — kept local so bigtools API churn doesn't
// leak into the trait.
type BwReader = bigtools::BigWigRead<bigtools::utils::reopen::ReopenableFile>;

pub struct BigWigSignalSource {
    display: String,
    #[allow(dead_code)]
    path: PathBuf,
    reader: Arc<Mutex<BwReader>>,
}

impl std::fmt::Debug for BigWigSignalSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigWigSignalSource")
            .field("display", &self.display)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl BigWigSignalSource {
    pub async fn open(path: &Path) -> Result<Self> {
        let p = path.to_path_buf();
        let reader = tokio::task::spawn_blocking(move || -> Result<BwReader> {
            // Implementer: adapt to bigtools API. Conceptually:
            //   BigWigRead::open(path-as-&str)
            // returns Result<BigWigRead<ReopenableFile>, _>.
            bigtools::BigWigRead::open(p.to_str().ok_or_else(|| {
                IgvError::Other(format!("non-utf8 path: {}", p.display()))
            })?)
            .map_err(|e| IgvError::Other(format!("bigwig open: {e}")))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        Ok(Self {
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("signal")
                .to_string(),
            path: path.to_path_buf(),
            reader: Arc::new(Mutex::new(reader)),
        })
    }
}

#[async_trait]
impl SignalSource for BigWigSignalSource {
    async fn fetch(
        &self,
        _region: &Region,
        _opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>> {
        // populated in Task 2.4
        Ok(Vec::new())
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

#[allow(dead_code)]
fn map_summary(s: SignalSummary) -> &'static str {
    // Kept here so Task 2.4 can use it. Bigtools' summary types are named
    // differently per version; map at the call site.
    match s {
        SignalSummary::Max => "max",
        SignalSummary::Mean => "mean",
        SignalSummary::Sum => "sum",
        SignalSummary::Min => "min",
    }
}

#[allow(dead_code)]
fn warn_unused() {
    warn!("placeholder to keep `tracing` import live until Task 2.4");
}
```

- [ ] **Step 4: Run open tests**

Run: `cargo test -p igv-core --test signal_bigwig`
Expected: both tests pass (display name + error case).

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/source/signal/bigwig.rs \
        crates/igv-core/tests/signal_bigwig.rs
git commit -m "feat(signal): BigWigSignalSource::open parses BBI once"
```

### Task 2.4: Implement `fetch()` with adaptive zoom-level selection

**Files:**
- Modify: `crates/igv-core/src/source/signal/bigwig.rs`
- Modify: `crates/igv-core/tests/signal_bigwig.rs`

- [ ] **Step 1: Write the failing fetch tests**

Append to `crates/igv-core/tests/signal_bigwig.rs`:

```rust
use igv_core::region::Region;
use igv_core::source::{FetchSignalOpts, SignalSummary};

#[tokio::test]
async fn fetch_chr1_raw_returns_per_base_ramp() {
    let src = open_signal(&fixture(), None).await.unwrap();
    let region = Region::new("chr1".into(), 1, 100).unwrap();
    let opts = FetchSignalOpts {
        max_bins: 100,           // 100 bp / 100 bins = 1 bp/col → raw path
        summary: SignalSummary::Max,
    };
    let bins = src.fetch(&region, &opts).await.unwrap();
    assert!(!bins.is_empty(), "raw path returned empty");
    // Bin at index 0 should have value ~0, last bin value should be near 99.
    assert!(bins.first().unwrap().value < 1.0);
    let last = bins.last().unwrap().value;
    assert!(last > 90.0, "last bin value = {last}");
}

#[tokio::test]
async fn fetch_chr1_full_uses_zoom_summary() {
    let src = open_signal(&fixture(), None).await.unwrap();
    let region = Region::new("chr1".into(), 1, 1000).unwrap();
    let opts = FetchSignalOpts {
        max_bins: 10,            // 1000 bp / 10 bins = 100 bp/col → zoom path
        summary: SignalSummary::Max,
    };
    let bins = src.fetch(&region, &opts).await.unwrap();
    assert!(bins.len() <= 10);
    assert!(bins.last().unwrap().value > 800.0);
}

#[tokio::test]
async fn fetch_unknown_chrom_returns_empty_no_error() {
    let src = open_signal(&fixture(), None).await.unwrap();
    let region = Region::new("chrZ".into(), 1, 100).unwrap();
    let bins = src
        .fetch(&region, &FetchSignalOpts::default())
        .await
        .unwrap();
    assert!(bins.is_empty());
}
```

- [ ] **Step 2: Verify they fail (current `fetch` always returns `Vec::new()`)**

Run: `cargo test -p igv-core --test signal_bigwig fetch_`
Expected: 2 failures (raw + zoom assertions); chrom-unknown passes.

- [ ] **Step 3: Implement `fetch()` with the adaptive branch**

Replace the `fetch` body in `crates/igv-core/src/source/signal/bigwig.rs`:

```rust
async fn fetch(
    &self,
    region: &Region,
    opts: &FetchSignalOpts,
) -> Result<Vec<SignalBin>> {
    let chrom = region.chrom.clone();
    let start = (region.start.saturating_sub(1)) as u32; // bigtools uses 0-based half-open
    let end = region.end.min(u32::MAX as u64) as u32;
    let max_bins = opts.max_bins.max(1);
    let summary = opts.summary;
    let bp_per_col = (region.width().max(1) as u32).saturating_div(max_bins);

    let reader = Arc::clone(&self.reader);
    let bins = tokio::task::spawn_blocking(move || -> Result<Vec<SignalBin>> {
        let mut guard = futures::executor::block_on(reader.lock());
        if !guard
            .info()
            .chrom_info
            .iter()
            .any(|c| c.name == chrom)
        {
            warn!("bigwig: chrom not found: {chrom}");
            return Ok(Vec::new());
        }

        if bp_per_col >= 16 {
            // Zoom-summary path: bigtools picks closest precomputed zoom
            // level. Map our SignalSummary to bigtools' summary type at the
            // call site. (Implementer: real call shape may differ.)
            let summaries = guard
                .get_zoom_summary(&chrom, start, end, max_bins)
                .map_err(|e| IgvError::Other(format!("bigwig zoom: {e}")))?;
            let _ = summary; // currently unused — bigtools picks max by default;
                             // hook up when API exposes per-bin summary kind.
            let bins = summaries
                .into_iter()
                .map(|s| SignalBin {
                    start: u64::from(s.start) + 1,
                    end: u64::from(s.end),
                    value: s.summary_value as f32,
                })
                .collect();
            Ok(bins)
        } else {
            // Raw values: one SignalBin per genomic position.
            let values = guard
                .get_interval(&chrom, start, end)
                .map_err(|e| IgvError::Other(format!("bigwig values: {e}")))?;
            let bins = values
                .into_iter()
                .filter_map(|v| v.ok())
                .map(|v| SignalBin {
                    start: u64::from(v.start) + 1,
                    end: u64::from(v.end),
                    value: v.value,
                })
                .collect();
            Ok(bins)
        }
    })
    .await
    .map_err(|e| IgvError::Other(e.to_string()))??;

    Ok(bins)
}
```

> **Implementer note:** The exact bigtools method names (`info().chrom_info`, `get_zoom_summary`, `get_interval`) shift between versions. If a method name doesn't exist, find the equivalent in the version you pinned and adjust. Two invariants must hold:
>   1. Unknown chrom → `Ok(Vec::new())` (logged), never `Err`.
>   2. The 16-bp/col threshold determines which path runs.
>
> Also delete the `map_summary` / `warn_unused` placeholders from Task 2.3 once the real impl uses them, and clean up the `#[allow(dead_code)]` attributes accordingly.

- [ ] **Step 4: Run fetch tests until they pass**

Run: `cargo test -p igv-core --test signal_bigwig`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/source/signal/bigwig.rs \
        crates/igv-core/tests/signal_bigwig.rs
git commit -m "feat(signal): adaptive fetch — zoom summary >=16bp/col, raw below"
```

---

## Phase 3 — Loader wiring

### Task 3.1: Add `signals` field to `Loader` + `LoadResult::Signal`

**Files:**
- Modify: `crates/igv-tui/src/app/loader.rs`

- [ ] **Step 1: Add the signals field, the LoadResult variant, and the dispatch lane**

Apply these edits to `crates/igv-tui/src/app/loader.rs`:

1. Add to imports (top of file):

```rust
use igv_core::source::{FetchSignalOpts, SignalBin, SignalSource};
```

2. Add to `LoadResult` enum (after `Annotation`):

```rust
Signal {
    generation: u64,
    track_index: usize,
    bins: Vec<SignalBin>,
},
```

3. Add to the `Loader` struct (after `annotations`):

```rust
pub signals: Vec<Arc<dyn SignalSource>>,
```

4. Update `Loader::new` signature to accept signals:

```rust
pub fn new(
    fasta: Arc<dyn igv_core::source::FastaSource>,
    vcf: Option<Arc<dyn igv_core::source::VcfSource>>,
    bams: Vec<Arc<dyn igv_core::source::BamSource>>,
    annotations: Vec<Arc<dyn igv_core::source::AnnotationSource>>,
    signals: Vec<Arc<dyn SignalSource>>,
    tx: tokio::sync::mpsc::Sender<LoadResult>,
) -> Self {
    Self {
        fasta,
        vcf,
        bams,
        annotations,
        signals,
        tx,
        current: Vec::new(),
    }
}
```

5. Append a signals fetch lane to `dispatch`, mirroring the annotations lane (after the annotations `for` loop, before the closing brace):

```rust
for (idx, sig) in self.signals.iter().enumerate() {
    let sig = Arc::clone(sig);
    let tx = self.tx.clone();
    let r = req.clone();
    self.current.push(tokio::spawn(async move {
        let opts = FetchSignalOpts::default();
        match sig.fetch(&r.region, &opts).await {
            Ok(bins) => {
                let _ = tx
                    .send(LoadResult::Signal {
                        generation: r.generation,
                        track_index: idx,
                        bins,
                    })
                    .await;
            }
            Err(e) => {
                tracing::warn!("signal fetch failed: {e}");
                let _ = tx
                    .send(LoadResult::Signal {
                        generation: r.generation,
                        track_index: idx,
                        bins: Vec::new(),
                    })
                    .await;
            }
        }
    }));
}
```

- [ ] **Step 2: Verify `igv-tui` no longer compiles (`Loader::new` arity mismatch in main.rs)**

Run: `cargo build -p igv-tui`
Expected: FAIL — `Loader::new` is called with 5 args at `main.rs:139` but now requires 6. **This is intentional** — the build will pass again after Phase 4.

- [ ] **Step 3: Verify `igv-core` still compiles**

Run: `cargo build -p igv-core`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/igv-tui/src/app/loader.rs
git commit -m "feat(loader): add signals fetch lane and LoadResult::Signal

Loader::new gains a signals parameter; main.rs updated in next commit."
```

---

## Phase 4 — AppState + main.rs glue

### Task 4.1: Extend `AppState` with signal fields

**Files:**
- Modify: `crates/igv-tui/src/app/state.rs`

- [ ] **Step 1: Add constants + struct fields + clear logic**

Apply these edits to `crates/igv-tui/src/app/state.rs`:

1. Add constants near other height constants (after `COVERAGE_DEFAULT_HEIGHT`, around line 21):

```rust
pub const SIGNAL_MIN_HEIGHT: u16 = 2;
pub const SIGNAL_MAX_HEIGHT: u16 = 12;
pub const SIGNAL_DEFAULT_HEIGHT: u16 = 4;
```

2. Add a track wrapper after `AnnotationTrack` (around line 85):

```rust
#[derive(Clone)]
#[allow(dead_code)]
pub struct SignalTrack {
    pub path: std::path::PathBuf,
    pub display: String,
    pub source: std::sync::Arc<dyn igv_core::source::SignalSource>,
}
```

3. Add fields to `AppState` (after `annotations` / `annotation_rows`, around line 47):

```rust
pub signals: Vec<SignalTrack>,
pub signal_bins: Vec<Vec<igv_core::source::SignalBin>>,
pub signal_shared_scale: bool,
pub signal_track_height: u16,
```

4. In `set_region_pending` (around line 270), add to the clear-stale-data block, after `for rows in &mut self.annotation_rows { rows.clear(); }`:

```rust
for bins in &mut self.signal_bins {
    bins.clear();
}
```

- [ ] **Step 2: Verify `igv-tui` is still in the broken-build state from Task 3.1**

Run: `cargo build -p igv-tui`
Expected: FAIL on `Loader::new` arity (still 5/6) and `AppState { ... }` (now missing four required fields).

- [ ] **Step 3: Commit (state shape only; main.rs not updated yet)**

```bash
git add crates/igv-tui/src/app/state.rs
git commit -m "feat(state): add signals + signal_bins + scale/height fields

Build is intentionally red until main.rs is wired up in the next commit."
```

### Task 4.2: Wire signals through `main.rs`

**Files:**
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Add the imports and the open-signal loop**

Apply these edits:

1. Add to use list (after the existing `use igv_core::source::vcf::NoodlesVcfSource;`):

```rust
use igv_core::source::{open_signal, SignalFormat};
```

2. Add to the imports from `crate::app::state` (around line 33):

```rust
use crate::app::state::{
    AppState, BamTrack, SignalTrack, StatusKind,
    ALIGNMENT_DEFAULT_HEIGHT, COVERAGE_DEFAULT_HEIGHT, SIGNAL_DEFAULT_HEIGHT,
};
```

3. After the annotations loading loop (after line 94), add the signals loading loop:

```rust
let mut signals: Vec<SignalTrack> = Vec::new();
let mut signal_sources: Vec<std::sync::Arc<dyn igv_core::source::SignalSource>> = Vec::new();
let signal_format_override = args
    .signal_format
    .as_deref()
    .and_then(SignalFormat::parse);
for path in &args.signals {
    let src = open_signal(path, signal_format_override).await?;
    signals.push(SignalTrack {
        path: path.clone(),
        display: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("signal")
            .to_string(),
        source: std::sync::Arc::clone(&src),
    });
    signal_sources.push(src);
}
```

4. In the `AppState { ... }` literal (around line 110), add fields:

```rust
signals,
signal_bins: vec![Vec::new(); signal_sources.len()],
signal_shared_scale: false,
signal_track_height: SIGNAL_DEFAULT_HEIGHT,
```

5. Update the `Loader::new` call (line 139):

```rust
let mut loader = Loader::new(fasta, vcf, bam_sources, annotation_sources, signal_sources, tx);
```

6. In `apply_load_result`, add a match arm for `LoadResult::Signal` (after the `Annotation` arm):

```rust
LoadResult::Signal { generation, track_index, bins } => {
    if generation == state.generation {
        if let Some(slot) = state.signal_bins.get_mut(track_index) {
            *slot = bins;
        }
    }
}
```

- [ ] **Step 2: Verify the binary now builds (CLI flag still missing — that comes next)**

Run: `cargo build -p igv-tui`
Expected: FAIL — `args.signals` and `args.signal_format` are unknown fields on `Cli` (added in Task 5.1).

- [ ] **Step 3: Hold the commit until CLI is wired**

This task does not commit by itself — Task 5.1 will commit `main.rs` changes together with `cli.rs` to keep each commit buildable. Stage the changes:

```bash
git add crates/igv-tui/src/main.rs
# do NOT commit yet
```

---

## Phase 5 — CLI flags

### Task 5.1: Add `-s` / `--signal-format` to `Cli`

**Files:**
- Modify: `crates/igv-tui/src/cli.rs:1-55`

- [ ] **Step 1: Add the new fields to `Cli`**

Append to `crates/igv-tui/src/cli.rs` after the `annotation_format` field (around line 41):

```rust
    /// Path to a bigWig signal file (.bw / .bigwig). May be repeated.
    #[arg(short = 's', long = "signal")]
    pub signals: Vec<PathBuf>,

    /// Override signal format auto-detection (currently only `bigwig`).
    /// Applies to all `-s` files.
    #[arg(long = "signal-format")]
    pub signal_format: Option<String>,
```

- [ ] **Step 2: Build the full workspace**

Run: `cargo build`
Expected: clean build of both crates.

- [ ] **Step 3: Smoke-run with no signal**

Run: `cargo run --quiet -- --help 2>&1 | grep -i signal`
Expected: shows `-s, --signal <SIGNALS>` and `--signal-format`.

- [ ] **Step 4: Commit (combines Phase 3 + 4 + 5 into a buildable state)**

```bash
git add crates/igv-tui/src/cli.rs crates/igv-tui/src/main.rs
git commit -m "feat(cli): add -s/--signal and wire into Loader/AppState

Now: igv-rs ref.fa -s a.bw works end-to-end; signals load and dispatch,
but no widget yet (next phase)."
```

---

## Phase 6 — Layout slot + theme key + widget

### Task 6.1: Add `SIGNAL` theme key

**Files:**
- Modify: `crates/igv-tui/src/ui/theme.rs:23-65, 67-109`

- [ ] **Step 1: Add `SIGNAL` to `Theme::dark`**

In `Theme::dark()`, after the `COVERAGE` insert (line 55), add:

```rust
m.insert("SIGNAL".into(), Style::default().fg(Color::Cyan));
```

- [ ] **Step 2: Add `SIGNAL` to `Theme::light`**

In `Theme::light()`, after the `COVERAGE` insert (line 99), add:

```rust
m.insert("SIGNAL".into(), Style::default().fg(Color::Blue));
```

- [ ] **Step 3: Verify**

Run: `cargo test -p igv-tui --lib ui::theme`
Expected: existing theme tests still pass; nothing new failing.

- [ ] **Step 4: Commit**

```bash
git add crates/igv-tui/src/ui/theme.rs
git commit -m "feat(theme): add SIGNAL theme key (cyan dark / blue light)"
```

### Task 6.2: Implement `SignalWidget`

**Files:**
- Create: `crates/igv-tui/src/ui/widgets/signal.rs`
- Modify: `crates/igv-tui/src/ui/widgets/mod.rs`

- [ ] **Step 1: Look at existing CoverageWidget for style/conventions**

Read `crates/igv-tui/src/ui/widgets/coverage.rs` end-to-end so the new widget matches its patterns (Block::default with TOP/BOTTOM borders, title format, `█` character).

- [ ] **Step 2: Write the widget**

Create `crates/igv-tui/src/ui/widgets/signal.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::source::SignalBin;

use crate::ui::theme::Theme;

pub struct SignalWidget<'a> {
    pub display_name: &'a str,
    pub bins: &'a [SignalBin],
    pub region: &'a igv_core::region::Region,
    pub theme: &'a Theme,
    /// `Some(g)` when shared-scale is on; widget uses `g` instead of its
    /// own max so different tracks become visually comparable.
    pub shared_max: Option<f32>,
}

impl Widget for SignalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let self_max = self
            .bins
            .iter()
            .map(|b| b.value)
            .fold(0.0_f32, f32::max);
        let scale_max = self.shared_max.unwrap_or(self_max);
        let suffix = if self.shared_max.is_some() { "*" } else { "" };
        let title = format!(
            "signal[{}] [0-{:.1}{}]",
            self.display_name, scale_max, suffix
        );
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || self.bins.is_empty() || scale_max <= 0.0 {
            return;
        }

        let style = self.theme.get("SIGNAL");
        let height = inner.height as usize;
        let region = self.region;

        // For each terminal column, take the max value among bins whose
        // [start, end] overlap the genomic range that maps to this column.
        let cols = inner.width as u32;
        for col in 0..cols {
            // Inverse map column → genomic range
            let col_start = region.start
                + (col as u64 * region.width()) / cols.max(1) as u64;
            let col_end = region.start
                + ((col + 1) as u64 * region.width()) / cols.max(1) as u64;
            let mut col_max = 0.0_f32;
            for b in self.bins {
                if b.end >= col_start && b.start < col_end {
                    if b.value > col_max {
                        col_max = b.value;
                    }
                }
            }
            if col_max <= 0.0 {
                continue;
            }
            let bar_h =
                ((col_max / scale_max) * height as f32).ceil() as u16;
            for row in 0..bar_h.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                let x = inner.x + col as u16;
                if x < inner.x + inner.width {
                    buf[(x, y)].set_char('█').set_style(style);
                }
            }
        }
    }
}
```

- [ ] **Step 3: Add `pub mod signal;` to widgets module**

Edit `crates/igv-tui/src/ui/widgets/mod.rs` and add `pub mod signal;` alongside the other widget modules.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p igv-tui`
Expected: builds (widget exists but nothing renders it yet — that's Task 6.4).

- [ ] **Step 5: Commit**

```bash
git add crates/igv-tui/src/ui/widgets/signal.rs \
        crates/igv-tui/src/ui/widgets/mod.rs
git commit -m "feat(widget): SignalWidget bar chart for bigwig tracks"
```

### Task 6.3: Add a signal slot to layout

**Files:**
- Modify: `crates/igv-tui/src/ui/layout.rs`

- [ ] **Step 1: Add fields to `LayoutSpec` + `LayoutAreas`**

Apply these edits to `crates/igv-tui/src/ui/layout.rs`:

1. Add to `LayoutAreas` (after `coverage`):

```rust
pub signals: Vec<Rect>,
```

2. Add to `LayoutSpec` (after `annotation_height_per_track`):

```rust
pub signal_count: usize,
pub signal_height_per_track: u16,
```

3. Update `Default for LayoutSpec`:

```rust
signal_count: 0,
signal_height_per_track: 4,
```

4. After the coverage constraint push (around line 71), before the alignments loop, add:

```rust
for _ in 0..spec.signal_count {
    constraints.push(Constraint::Length(spec.signal_height_per_track));
}
```

5. After the coverage area is read (around line 102), before the alignments loop, add:

```rust
let mut signals = Vec::new();
for _ in 0..spec.signal_count {
    signals.push(chunks[idx]);
    idx += 1;
}
```

6. Add `signals` to the returned `LayoutAreas { ... }` literal.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p igv-tui`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/igv-tui/src/ui/layout.rs
git commit -m "feat(layout): reserve N signal slots between coverage and alignments"
```

### Task 6.4: Render signals in `draw()`

**Files:**
- Modify: `crates/igv-tui/src/main.rs:284-321`

- [ ] **Step 1: Pass the signal spec into `compute()` and render the widgets**

In `draw()` (around line 285):

1. Update the `LayoutSpec` literal:

```rust
let spec = LayoutSpec {
    has_vcf: state.vcf.is_some(),
    bam_count: state.bams.len(),
    annotation_tracks: state.annotations.len(),
    coverage_height: state.coverage_height,
    alignments_min_per_track: state.alignment_height,
    signal_count: state.signals.len(),
    signal_height_per_track: state.signal_track_height,
    ..Default::default()
};
```

2. After the `coverage` render block and before the alignments loop, add:

```rust
let global_signal_max = if state.signal_shared_scale {
    state
        .signal_bins
        .iter()
        .flatten()
        .map(|b| b.value)
        .fold(0.0_f32, f32::max)
} else {
    0.0
};
for (i, area) in areas.signals.iter().enumerate() {
    let track = &state.signals[i];
    let bins: &[igv_core::source::SignalBin] =
        state.signal_bins.get(i).map(|v| v.as_slice()).unwrap_or(&[]);
    f.render_widget(
        widgets::signal::SignalWidget {
            display_name: &track.display,
            bins,
            region: &state.region,
            theme: &state.theme,
            shared_max: if state.signal_shared_scale {
                Some(global_signal_max)
            } else {
                None
            },
        },
        *area,
    );
}
```

- [ ] **Step 2: Manual smoke run**

Run: `cargo run --quiet -- <some.fa> -s <some.bw>`
Expected: a signal track appears between coverage (if any) and alignments. Quit with `q`.

If you don't have a sample bigwig handy, copy the test fixture:

```bash
cp crates/igv-core/tests/data/small.bw /tmp/test.bw
cargo run --quiet -- <some.fa> -s /tmp/test.bw -r chr1:1-1000
```

- [ ] **Step 3: Commit**

```bash
git add crates/igv-tui/src/main.rs
git commit -m "feat(render): wire SignalWidget into draw() with shared-scale support"
```

---

## Phase 7 — Hotkeys for shared-scale + resize

### Task 7.1: Add `Action` variants and bindings

**Files:**
- Modify: `crates/igv-tui/src/app/action.rs`
- Modify: `crates/igv-tui/src/input.rs`
- Modify: `crates/igv-tui/src/app/state.rs:160-268`

- [ ] **Step 1: Add the variants to `Action`**

Append to `crates/igv-tui/src/app/action.rs` (inside the `Action` enum):

```rust
/// Toggle per-track / shared auto-scale across all signal tracks.
ToggleSignalSharedScale,
/// Resize signal-track height. Positive = grow.
ResizeSignal(i16),
```

- [ ] **Step 2: Bind keys in `input.rs`**

Edit `crates/igv-tui/src/input.rs`. Inside the `match code` block (after `KeyCode::Char('[')`, around line 65), add:

```rust
KeyCode::Char('\\') => Action::ToggleSignalSharedScale,
KeyCode::Char('}') => Action::ResizeSignal(1),
KeyCode::Char('{') => Action::ResizeSignal(-1),
```

- [ ] **Step 3: Implement the action handlers in `AppState::apply`**

Edit `crates/igv-tui/src/app/state.rs`. In `apply` (around line 160), add arms before `Action::None`:

```rust
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
```

Also add `SIGNAL_MIN_HEIGHT` and `SIGNAL_MAX_HEIGHT` to the imports at the top of `state.rs`'s `apply` impl block — they're already defined as constants earlier in the file, just ensure they're in scope (no extra import needed since they're in the same module).

- [ ] **Step 4: Add unit tests for the new key bindings**

Append to `crates/igv-tui/src/input.rs` `mod tests`:

```rust
#[test]
fn backslash_toggles_signal_shared_scale() {
    let mut s = InputState::default();
    assert!(matches!(
        s.map(&key('\\'), false),
        Action::ToggleSignalSharedScale
    ));
}

#[test]
fn close_brace_grows_signal_track() {
    let mut s = InputState::default();
    assert!(matches!(s.map(&key('}'), false), Action::ResizeSignal(1)));
}

#[test]
fn open_brace_shrinks_signal_track() {
    let mut s = InputState::default();
    assert!(matches!(s.map(&key('{'), false), Action::ResizeSignal(-1)));
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p igv-tui --lib input`
Expected: existing input tests still pass + 3 new tests pass.

- [ ] **Step 6: Manual confirmation**

Run: `cargo run --quiet -- <some.fa> -s /tmp/test.bw -s /tmp/test2.bw`
Manual:
1. Press `\` — status bar should flicker `signal scale: shared` and titles get `*`.
2. Press `\` again — `signal scale: per-track`, no `*`.
3. Press `}` — track grows; status `signal height: 5`.
4. Press `{` — shrinks back. Hold `{` to verify clamp at 2.

- [ ] **Step 7: Commit**

```bash
git add crates/igv-tui/src/app/action.rs \
        crates/igv-tui/src/input.rs \
        crates/igv-tui/src/app/state.rs
git commit -m "feat(hotkey): \\ toggles signal shared-scale; }/{ resize"
```

---

## Phase 8 — Smoke + integration tests

### Task 8.1: Append signal smoke case to `render_smoke.rs`

**Files:**
- Modify: `crates/igv-tui/tests/render_smoke.rs`

- [ ] **Step 1: Add a mock SignalSource and a smoke test**

Read the existing smoke test first to match its style and helper functions. Then append a test that:

1. Builds a hand-written `MockSignal` impl returning a fixed `Vec<SignalBin>` of, say, 50 bins with values `0..50`.
2. Constructs `AppState` with one signal track, `signal_track_height = 4`, `signal_shared_scale = false`, and pre-filled `signal_bins[0]`.
3. Renders one frame to a `TestBackend` of size 80×24.
4. Asserts:
   - The frame contains the substring `signal[mock]`
   - The frame contains the substring `[0-49`
   - At least one buffer cell has the `█` character

A minimal sketch (adapt to existing helpers):

```rust
use igv_core::region::Region;
use igv_core::source::{FetchSignalOpts, SignalBin, SignalSource};
use std::sync::Arc;

struct MockSignal {
    bins: Vec<SignalBin>,
}

#[async_trait::async_trait]
impl SignalSource for MockSignal {
    async fn fetch(
        &self,
        _r: &Region,
        _o: &FetchSignalOpts,
    ) -> igv_core::error::Result<Vec<SignalBin>> {
        Ok(self.bins.clone())
    }
    fn display_name(&self) -> &str { "mock" }
}

#[test]
fn signal_track_renders_bars_and_title() {
    // existing helper to build an AppState; extend it (or inline it)
    // to also accept a signals vec.
    let region = Region::new("chr1".into(), 1, 100).unwrap();
    let bins: Vec<SignalBin> = (0..50)
        .map(|i| SignalBin { start: i + 1, end: i + 1, value: i as f32 })
        .collect();
    let mock: Arc<dyn SignalSource> = Arc::new(MockSignal { bins: bins.clone() });
    // ... build state with signals=[track], signal_bins=[bins]
    // render to a TestBackend, then assert.
}
```

> If `render_smoke.rs` doesn't currently expose a builder for `AppState`, this test may need to inline the builder. Keep the test self-contained.

- [ ] **Step 2: Run the smoke test**

Run: `cargo test -p igv-tui --test render_smoke`
Expected: existing tests still pass + 1 new pass.

- [ ] **Step 3: Commit**

```bash
git add crates/igv-tui/tests/render_smoke.rs
git commit -m "test(signal): smoke render check — bars + title formatting"
```

### Task 8.2: Loader-dispatch integration test

**Files:**
- Create: `crates/igv-tui/tests/signal_dispatch.rs`

- [ ] **Step 1: Write the test**

Create `crates/igv-tui/tests/signal_dispatch.rs`:

```rust
//! Integration test: Loader::dispatch routes signal fetches and emits
//! LoadResult::Signal with the correct track_index for each source.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::source::{
    FetchOpts, FetchSignalOpts, SignalBin, SignalSource,
};
use tokio::sync::mpsc;

// We need access to the binary crate's modules via integration tests;
// use the hidden test-only surface. If not present, route through
// re-exporting modules in igv-tui's lib.rs (small refactor).
use igv_tui_test_helpers::loader::{LoadRequest, LoadResult, Loader};

struct MockSignal {
    bins: Vec<SignalBin>,
    name: &'static str,
}

#[async_trait]
impl SignalSource for MockSignal {
    async fn fetch(&self, _r: &Region, _o: &FetchSignalOpts) -> Result<Vec<SignalBin>> {
        Ok(self.bins.clone())
    }
    fn display_name(&self) -> &str { self.name }
}

#[tokio::test]
async fn dispatch_routes_signals_to_correct_indices() {
    let (tx, mut rx) = mpsc::channel(8);
    // Need a no-op fasta; reuse existing fixture or pick a minimal stub.
    // (Implementer: there's an in-tree fasta fixture used by other
    // integration tests — locate and reuse it; otherwise add a tiny one.)
    let fasta: Arc<dyn igv_core::source::FastaSource> =
        // ... locate or build a minimal FastaSource
        unimplemented!("locate test fasta fixture");

    let bins_a = vec![SignalBin { start: 1, end: 100, value: 1.0 }];
    let bins_b = vec![SignalBin { start: 1, end: 100, value: 2.0 }];
    let signals: Vec<Arc<dyn SignalSource>> = vec![
        Arc::new(MockSignal { bins: bins_a.clone(), name: "a" }),
        Arc::new(MockSignal { bins: bins_b.clone(), name: "b" }),
    ];

    let mut loader = Loader::new(fasta, None, vec![], vec![], signals, tx);
    loader.dispatch(LoadRequest {
        generation: 1,
        region: Region::new("chr1".into(), 1, 100).unwrap(),
        fetch_opts: FetchOpts::default(),
    });

    // Drain the channel for ~50 ms and collect the signal results.
    let mut got_a = false;
    let mut got_b = false;
    while let Ok(Some(r)) =
        tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await
    {
        if let LoadResult::Signal { track_index, bins, generation } = r {
            assert_eq!(generation, 1);
            match track_index {
                0 => { assert_eq!(bins, bins_a); got_a = true; }
                1 => { assert_eq!(bins, bins_b); got_b = true; }
                _ => panic!("unexpected track_index {track_index}"),
            }
        }
    }
    assert!(got_a && got_b, "missing one of the signal results");
}
```

> **Implementer note:** Integration tests can't reach `crate::app::loader` directly because `igv-tui` is a `bin`-only crate. Two viable workarounds:
>
> 1. Add a `lib.rs` to `igv-tui` that re-exports `pub mod app;` etc. and have the binary `main.rs` use it. Smallest churn.
> 2. Promote `Loader` and friends into a small internal helper crate `igv-tui-test-helpers`.
>
> Option 1 is recommended — touch `Cargo.toml` `[lib]`/`[[bin]]` sections, expose what's needed, run `cargo build`, then write the test.

- [ ] **Step 2: Run the test**

Run: `cargo test -p igv-tui --test signal_dispatch`
Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add crates/igv-tui/tests/signal_dispatch.rs \
        crates/igv-tui/src/lib.rs        # if you exposed via lib
        crates/igv-tui/Cargo.toml         # if [lib] entry was added
git commit -m "test(signal): integration test for Loader signal dispatch"
```

---

## Phase 9 — Docs

### Task 9.1: Update README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add `-s` to Usage examples**

After the existing `-g` examples, add:

```bash
igv-rs reference.fa -s chip.bw -s input.bw -r chr1:1-10000000
igv-rs reference.fa -b sample.bam -s rna.bw -g genes.gff3 -r chr1:1000-2000
```

- [ ] **Step 2: Add a Signal-tracks paragraph after the annotation paragraph**

```markdown
Signal tracks (bigWig, `.bw` / `.bigwig`) are accepted via the repeatable
`-s` / `--signal` flag and rendered as bar-chart tracks between coverage
and alignments. At wide zoom the bigwig file's precomputed zoom-level
summaries are used (≥16 bp/col); at fine zoom raw per-base values are
fetched. Override extension auto-detection with
`--signal-format bigwig`.
```

- [ ] **Step 3: Add three rows to Keybindings**

After `]` / `[` row:

```markdown
- `\` — toggle signal shared / per-track Y-scale
- `}` / `{` — grow / shrink signal-track height
```

- [ ] **Step 4: Add `SIGNAL` to Configuration example**

In the `[theme.custom]` example block, add a row:

```toml
"SIGNAL" = "cyan"
```

- [ ] **Step 5: Add a Layout bullet**

In the Layout section:

```markdown
- `crates/igv-core/src/source/signal.rs` — `SignalSource` trait + bigtools-backed `BigWigSignalSource`.
```

- [ ] **Step 6: Add bullets to Known limitations**

```markdown
- **No signal-track caching** — every region change re-fetches bigwig.
  In practice bigtools' R-tree lookup is fast enough; revisit if it's
  ever observed to lag.
- **Single signal colormap** — all bigwig tracks share the `SIGNAL`
  theme key. Per-track colormap is not yet supported.
- **Signal summary statistic** is fixed at `Max`. `--signal-summary` is
  not yet a flag.
- **bigBed (`.bb`)** is not supported — separate spec.
```

- [ ] **Step 7: Commit**

```bash
git add README.md
git commit -m "docs(readme): document bigwig signal tracks + new hotkeys"
```

---

## Phase 10 — Manual verification

### Task 10.1: Manual checklist (no commit)

These are not automated. Run them on a real terminal before declaring the feature done.

- [ ] `igv-rs ref.fa -s chip.bw` — single track renders, title shows `[0-N]`.
- [ ] `igv-rs ref.fa -s a.bw -s b.bw` — two tracks stacked, each with own scale.
- [ ] Press `\` — `signal[a] [0-N*]`, both scales unified.
- [ ] Press `}` / `{` — height grows/shrinks; clamps at 2 and 12.
- [ ] `:chr1:1-100000000⏎` (or `g` then enter), then rapid `h`/`l` panning — no flicker, no stale data.
- [ ] Resize terminal — refetch happens, no stale frame leaked.
- [ ] `igv-rs ref.fa -s nonexistent.bw` — startup fails with clear error message.
- [ ] Jump to a chrom not in the bigwig — empty band + `tracing::warn!` line in logs.
- [ ] Set `RUST_LOG=igv_core=debug`, look at fetch timings — no obvious regressions.

When all green, the feature is ready to merge to `main`.

---

## Self-review notes

Before handing off:

1. **Spec coverage:** §1 motivation → addressed in Phase 9 docs. §2 architecture → Phases 1-6. §3 components → Phases 1, 2, 3, 4, 6. §4 data flow → Phases 3, 4, 6. §5 errors → Tasks 2.3, 2.4 + Loader's degrade path. §6 hotkeys/theme/README → Phases 6, 7, 9. §7 testing → Phases 1, 2, 8. §8 phasing — this plan IS the phasing. §9 deferred — explicitly captured in Known limitations.

2. **Placeholder scan:** All "TBD"-shaped lines are flagged as **Implementer note** and bound to a real, testable invariant. The bigtools-API uncertainty is intentional and explicit.

3. **Type consistency:** `SignalBin`, `FetchSignalOpts`, `SignalSummary`, `SignalSource`, `SignalFormat`, `BigWigSignalSource`, `LoadResult::Signal`, `SignalTrack`, `SIGNAL_*_HEIGHT` constants — verified consistent across tasks.

4. **Hotkey deviation:** `\` (not `=`) for shared-scale toggle, called out in File structure and at Task 7.1.
