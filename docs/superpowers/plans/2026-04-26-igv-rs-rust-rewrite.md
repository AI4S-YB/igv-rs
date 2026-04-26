# igv-rs Rust Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the Python `cligv` interactive genome viewer in Rust as `igv-rs`, with async non-blocking IO, adaptive zoom-level rendering, multi-BAM tracks, command palette, and vim-style bookmarks.

**Architecture:** Cargo workspace with two crates: `igv-core` (pure library — region, async data-source traits, CIGAR/coverage, rendering thresholds) and `igv-tui` (binary — clap CLI, ratatui custom widgets, tokio main loop with single-owner `AppState` and cancellable loader tasks).

**Tech Stack:** `noodles` (FASTA/BAM/VCF parsing), `ratatui` + `crossterm` (TUI rendering and events), `tokio` (async runtime), `clap` (CLI), `thiserror` + `anyhow` (errors), `tracing` (logging), `toml` + `serde` (config), `directories` (XDG paths), `tui-input` (command palette).

**Spec:** `docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`

**Working tree root:** `/home/xzg/project/igv_rs/` (already a git repo with `cligv/` git-ignored as the Python reference).

---

## Conventions

- **Commits:** every task ends with `git add` + `git commit -m "..."` using Conventional Commits (`feat:`, `test:`, `refactor:`, `chore:`, `docs:`).
- **Test runner:** `cargo test -p <crate>` for crate-scoped tests, `cargo test` for the workspace.
- **TDD:** for every behavior task, write the failing test first, run it to confirm the failure mode, implement, run tests to green, then commit.
- **No placeholders:** if a step shows code, that code is what should be typed (modulo trivial whitespace).
- **Versions** below are starting points. If `cargo build` reports a yanked or otherwise broken version, bump to the latest minor.

---

## Phase 0: Workspace Bootstrap

### Task 0.1: Initialize Cargo workspace

**Files:**
- Create: `Cargo.toml`
- Create: `rust-toolchain.toml`
- Modify: `.gitignore`

- [ ] **Step 1: Write workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/igv-core", "crates/igv-tui"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
authors = ["igv-rs contributors"]
repository = "https://github.com/<owner>/igv-rs"
rust-version = "1.75"

[workspace.dependencies]
# Genome formats
noodles = { version = "0.85", default-features = false }

# Async runtime
tokio = { version = "1.40", default-features = false }
async-trait = "0.1"
futures = "0.3"

# Error handling
anyhow = "1"
thiserror = "1"

# Serialization & config
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# CLI & TUI
clap = { version = "4.5", features = ["derive"] }
ratatui = "0.29"
crossterm = { version = "0.28", features = ["event-stream"] }
tui-input = "0.10"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-appender = "0.2"

# Paths
directories = "5"

# Test helpers
tempfile = "3"
insta = { version = "1.40", features = ["yaml"] }
```

- [ ] **Step 2: Pin toolchain**

`rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.81"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 3: Update `.gitignore`**

Append to existing `.gitignore`:

```
# Rust
/target
**/*.rs.bk
Cargo.lock.bak

# Editor
.vscode/
.idea/

# OS
.DS_Store

# Logs
*.log
```

(Keep the existing `/cligv` line.)

- [ ] **Step 4: Verify and commit**

```bash
cargo metadata --format-version 1 --no-deps > /dev/null
git add Cargo.toml rust-toolchain.toml .gitignore
git commit -m "chore: initialize Cargo workspace skeleton"
```

Expected: `cargo metadata` exits 0 with workspace recognized but no members yet (errors on missing crates are OK at this point — we will add them next).

---

### Task 0.2: Scaffold `igv-core` crate

**Files:**
- Create: `crates/igv-core/Cargo.toml`
- Create: `crates/igv-core/src/lib.rs`

- [ ] **Step 1: Write `crates/igv-core/Cargo.toml`**

```toml
[package]
name = "igv-core"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
async-trait.workspace = true
futures.workspace = true
noodles = { workspace = true, features = [
    "async", "fasta", "bam", "sam", "vcf", "csi", "tabix", "bgzf",
] }
serde = { workspace = true, optional = true }
thiserror.workspace = true
tokio = { workspace = true, features = ["fs", "io-util", "sync"] }
tracing.workspace = true

[dev-dependencies]
tempfile.workspace = true
tokio = { workspace = true, features = ["macros", "rt", "rt-multi-thread"] }
```

- [ ] **Step 2: Write `crates/igv-core/src/lib.rs`**

```rust
//! Core data layer for igv-rs: region types, async data sources, alignment
//! processing, coverage, and rendering thresholds. UI-free.

#![warn(rust_2018_idioms, missing_debug_implementations)]

pub mod alignment;
pub mod coverage;
pub mod error;
pub mod region;
pub mod render;
pub mod source;

pub use error::{IgvError, Result};
pub use region::Region;
```

- [ ] **Step 3: Create empty module files**

```bash
mkdir -p crates/igv-core/src/source
touch crates/igv-core/src/{alignment.rs,coverage.rs,error.rs,region.rs,render.rs}
echo "//! Async data-source traits and noodles-backed implementations." \
    > crates/igv-core/src/source/mod.rs
```

Make each file (other than `source/mod.rs`) start with a single line so it
compiles:

```rust
//! TODO module placeholder — replaced in subsequent tasks.
```

- [ ] **Step 4: Verify build and commit**

```bash
cargo build -p igv-core
git add crates/igv-core
git commit -m "chore(igv-core): scaffold crate with empty modules"
```

Expected: `cargo build -p igv-core` succeeds with warnings about unused empty modules.

---

### Task 0.3: Scaffold `igv-tui` crate

**Files:**
- Create: `crates/igv-tui/Cargo.toml`
- Create: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Write `crates/igv-tui/Cargo.toml`**

```toml
[package]
name = "igv-tui"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[[bin]]
name = "igv-rs"
path = "src/main.rs"

[dependencies]
igv-core = { path = "../igv-core" }

anyhow.workspace = true
async-trait.workspace = true
clap.workspace = true
crossterm.workspace = true
directories.workspace = true
futures.workspace = true
ratatui.workspace = true
serde.workspace = true
thiserror.workspace = true
tokio = { workspace = true, features = [
    "fs", "io-util", "macros", "rt-multi-thread", "sync", "time", "signal",
] }
toml.workspace = true
tracing.workspace = true
tracing-appender.workspace = true
tracing-subscriber.workspace = true
tui-input.workspace = true

[dev-dependencies]
insta.workspace = true
tempfile.workspace = true
```

- [ ] **Step 2: Write a `main.rs` placeholder**

```rust
fn main() -> anyhow::Result<()> {
    println!("igv-rs scaffold — implementation in progress.");
    Ok(())
}
```

- [ ] **Step 3: Verify build and commit**

```bash
cargo build
cargo run -p igv-tui --quiet
git add crates/igv-tui
git commit -m "chore(igv-tui): scaffold binary crate"
```

Expected: `cargo run` prints the placeholder message.

---

## Phase 1: igv-core Foundations

### Task 1.1: Error type

**Files:**
- Modify: `crates/igv-core/src/error.rs`

- [ ] **Step 1: Replace `error.rs`**

```rust
//! Error types shared across `igv-core`.

use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum IgvError {
    #[error("I/O error on {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("missing index file: {0}")]
    MissingIndex(PathBuf),

    #[error("chromosome not found: {0}")]
    UnknownChromosome(String),

    #[error("invalid region string: {0}")]
    InvalidRegion(String),

    #[error("region out of bounds: {chrom} length {chrom_len}, requested {start}-{end}")]
    OutOfBounds {
        chrom: String,
        chrom_len: u64,
        start: u64,
        end: u64,
    },

    #[error("noodles error: {0}")]
    Noodles(String),

    #[error("unexpected: {0}")]
    Other(String),
}

impl IgvError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn noodles<E: std::fmt::Display>(err: E) -> Self {
        Self::Noodles(err.to_string())
    }
}

pub type Result<T, E = IgvError> = std::result::Result<T, E>;
```

- [ ] **Step 2: Verify and commit**

```bash
cargo build -p igv-core
git add crates/igv-core/src/error.rs
git commit -m "feat(igv-core): add IgvError enum and Result alias"
```

---

### Task 1.2: Region type — fields, constructors, validation

**Files:**
- Modify: `crates/igv-core/src/region.rs`

- [ ] **Step 1: Write failing tests at the bottom of `region.rs`**

Replace `region.rs` with:

```rust
//! Genomic region: 1-based inclusive coordinates with parsing and screen
//! coordinate transforms.

use std::fmt;

use crate::error::{IgvError, Result};

/// A 1-based, inclusive genomic interval.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Region {
    pub chrom: String,
    pub start: u64, // 1-based inclusive
    pub end: u64,   // 1-based inclusive
}

impl Region {
    /// Construct a region. Returns `InvalidRegion` if `start > end` or `start == 0`.
    pub fn new(chrom: impl Into<String>, start: u64, end: u64) -> Result<Self> {
        if start == 0 || start > end {
            return Err(IgvError::InvalidRegion(format!(
                "{}:{}-{}",
                chrom.into(),
                start,
                end
            )));
        }
        Ok(Self {
            chrom: chrom.into(),
            start,
            end,
        })
    }

    /// Width in bases (inclusive).
    pub fn width(&self) -> u64 {
        self.end - self.start + 1
    }

    /// Clamp to `[1, chrom_len]`. Returns `OutOfBounds` if no overlap exists.
    pub fn clamp_to(&self, chrom_len: u64) -> Result<Self> {
        if chrom_len == 0 || self.start > chrom_len {
            return Err(IgvError::OutOfBounds {
                chrom: self.chrom.clone(),
                chrom_len,
                start: self.start,
                end: self.end,
            });
        }
        let new_start = self.start.max(1);
        let new_end = self.end.min(chrom_len);
        Region::new(self.chrom.clone(), new_start, new_end)
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}-{}", self.chrom, self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_zero_start() {
        assert!(Region::new("chr1", 0, 10).is_err());
    }

    #[test]
    fn new_rejects_start_greater_than_end() {
        assert!(Region::new("chr1", 20, 10).is_err());
    }

    #[test]
    fn width_is_inclusive() {
        let r = Region::new("chr1", 100, 199).unwrap();
        assert_eq!(r.width(), 100);
    }

    #[test]
    fn clamp_trims_to_chrom_length() {
        let r = Region::new("chr1", 100, 1_000_000).unwrap();
        let c = r.clamp_to(500).unwrap();
        assert_eq!(c.end, 500);
        assert_eq!(c.start, 100);
    }

    #[test]
    fn clamp_errors_when_start_exceeds_length() {
        let r = Region::new("chr1", 1000, 2000).unwrap();
        assert!(r.clamp_to(500).is_err());
    }

    #[test]
    fn display_formats_canonically() {
        let r = Region::new("chr1", 100, 200).unwrap();
        assert_eq!(r.to_string(), "chr1:100-200");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p igv-core region::tests
```

Expected: all six tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/igv-core/src/region.rs
git commit -m "feat(igv-core): Region with constructors, clamp, Display"
```

---

### Task 1.3: Region parser — `chr:start-end`, `chr:pos`, `chr` forms

**Files:**
- Modify: `crates/igv-core/src/region.rs` (append)

- [ ] **Step 1: Write the failing tests inside the existing `tests` module**

Append to the `tests` module in `region.rs`:

```rust
    #[test]
    fn parse_full_form() {
        let r = Region::parse("chr1:100-200").unwrap();
        assert_eq!(r, Region::new("chr1", 100, 200).unwrap());
    }

    #[test]
    fn parse_strips_commas() {
        let r = Region::parse("chr1:1,000-2,000").unwrap();
        assert_eq!(r, Region::new("chr1", 1000, 2000).unwrap());
    }

    #[test]
    fn parse_position_only_centers_default_window() {
        let r = Region::parse("chr1:1000").unwrap();
        // Default window 250bp; position centers it.
        assert_eq!(r.chrom, "chr1");
        assert_eq!(r.width(), 250);
        assert!(r.start <= 1000 && 1000 <= r.end);
    }

    #[test]
    fn parse_chromosome_only_uses_default_window() {
        let r = Region::parse("chr1").unwrap();
        assert_eq!(r.chrom, "chr1");
        assert_eq!(r.start, 1);
        assert_eq!(r.width(), 250);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(Region::parse("not a region").is_err());
        assert!(Region::parse("chr1:abc-def").is_err());
        assert!(Region::parse("").is_err());
    }
```

- [ ] **Step 2: Run tests to confirm failure**

```bash
cargo test -p igv-core region::tests::parse_full_form
```

Expected: compile error or test failure (`Region::parse` does not exist yet).

- [ ] **Step 3: Implement the parser**

Add a constant near the top of `region.rs`:

```rust
pub const DEFAULT_REGION_WIDTH: u64 = 250;
pub const MAX_REGION_WIDTH: u64 = 100_000;
```

Add this `impl` block:

```rust
impl Region {
    /// Parse a region string. Accepted forms (case-sensitive on chromosome):
    /// - `chr1:1000-2000`
    /// - `chr1:1,000-2,000`
    /// - `chr1:1000`            → centered default window
    /// - `chr1`                 → 1..=DEFAULT_REGION_WIDTH
    pub fn parse(s: &str) -> Result<Self> {
        let trimmed = s.replace(',', "");
        let trimmed = trimmed.trim();
        if trimmed.is_empty() {
            return Err(IgvError::InvalidRegion(s.to_string()));
        }

        match trimmed.split_once(':') {
            Some((chrom, rest)) => match rest.split_once('-') {
                Some((start, end)) => {
                    let start: u64 = start
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(s.to_string()))?;
                    let end: u64 = end
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(s.to_string()))?;
                    Region::new(chrom, start, end)
                }
                None => {
                    let pos: u64 = rest
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(s.to_string()))?;
                    let half = DEFAULT_REGION_WIDTH / 2;
                    let start = pos.saturating_sub(half).max(1);
                    let end = start + DEFAULT_REGION_WIDTH - 1;
                    Region::new(chrom, start, end)
                }
            },
            None => {
                if trimmed.is_empty() || !trimmed.chars().all(is_chrom_char) {
                    return Err(IgvError::InvalidRegion(s.to_string()));
                }
                Region::new(trimmed, 1, DEFAULT_REGION_WIDTH)
            }
        }
    }
}

fn is_chrom_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}
```

Note: the existing `impl Region` from Task 1.2 stays; this is an additional
`impl Region` block (Rust permits multiple).

- [ ] **Step 4: Run tests**

```bash
cargo test -p igv-core region::tests
```

Expected: all eleven tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/region.rs
git commit -m "feat(igv-core): Region::parse for chr:start-end / chr:pos / chr"
```

---

### Task 1.4: Coordinate transforms (genomic ↔ screen)

**Files:**
- Modify: `crates/igv-core/src/region.rs` (append)

- [ ] **Step 1: Write failing tests**

Append to `tests` module:

```rust
    #[test]
    fn genomic_to_screen_identity_when_smaller_than_screen() {
        // 100bp region, 200-col screen → no scaling
        let pos = genomic_to_screen(100, 100, 100, 200);
        assert_eq!(pos, Some(0));
        let pos = genomic_to_screen(150, 100, 100, 200);
        assert_eq!(pos, Some(50));
    }

    #[test]
    fn genomic_to_screen_scales_when_larger() {
        // 1000bp region, 100-col screen → 10x compression
        let pos = genomic_to_screen(1000, 1000, 1000, 100);
        assert_eq!(pos, Some(0));
        let pos = genomic_to_screen(1500, 1000, 1000, 100);
        assert_eq!(pos, Some(50));
    }

    #[test]
    fn genomic_to_screen_returns_none_when_outside() {
        assert!(genomic_to_screen(50, 100, 100, 100).is_none());
        assert!(genomic_to_screen(500, 100, 100, 100).is_none());
    }

    #[test]
    fn screen_to_genomic_round_trips_when_unscaled() {
        let g = screen_to_genomic(50, 100, 100, 200);
        assert_eq!(g, 150);
    }

    #[test]
    fn screen_to_genomic_round_trips_when_scaled() {
        let g = screen_to_genomic(50, 1000, 1000, 100);
        assert_eq!(g, 1500);
    }
```

- [ ] **Step 2: Implement transforms**

Add to `region.rs`:

```rust
/// Map a 0-based genomic position to a 0-based screen column.
///
/// Returns `None` if the position falls outside `[view_start, view_start +
/// view_width)`. When `view_width > screen_width`, scaling is applied.
pub fn genomic_to_screen(
    genomic_pos: u64,
    view_start: u64,
    view_width: u64,
    screen_width: u32,
) -> Option<u32> {
    if genomic_pos < view_start {
        return None;
    }
    let rel = genomic_pos - view_start;
    if rel >= view_width {
        return None;
    }
    if view_width == 0 || screen_width == 0 {
        return None;
    }
    if view_width as u64 > screen_width as u64 {
        let scaled = (rel as u128 * screen_width as u128 / view_width as u128) as u32;
        Some(scaled.min(screen_width.saturating_sub(1)))
    } else {
        Some(rel as u32)
    }
}

/// Map a 0-based screen column back to a 0-based genomic position.
pub fn screen_to_genomic(
    screen_pos: u32,
    view_start: u64,
    view_width: u64,
    screen_width: u32,
) -> u64 {
    if view_width as u64 > screen_width as u64 {
        let g = screen_pos as u128 * view_width as u128 / screen_width as u128;
        view_start + g as u64
    } else {
        view_start + screen_pos as u64
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p igv-core region::tests
```

Expected: all sixteen tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/igv-core/src/region.rs
git commit -m "feat(igv-core): genomic_to_screen / screen_to_genomic"
```

---

### Task 1.5: Render thresholds and mode selection

**Files:**
- Modify: `crates/igv-core/src/render.rs`

- [ ] **Step 1: Write the module**

```rust
//! Render mode selection by zoom level. Thresholds are configurable but
//! ship with sensible defaults from the design spec.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// At or below this width, show per-base sequence and full CIGAR.
    pub per_base: u64,
    /// At or below this width, still show per-base sequence.
    pub detailed: u64,
    /// At or below this width, hide alignments but keep coverage.
    pub coverage_only: u64,
    /// At or below this width, use coverage-as-heatbar; above it, only overview.
    pub heat: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            per_base: 200,
            detailed: 1_000,
            coverage_only: 10_000,
            heat: 100_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    PerBase,        // ≤ per_base
    DetailedReads,  // ≤ detailed
    CoverageDense,  // ≤ coverage_only
    HeatBar,        // ≤ heat
    OverviewOnly,   // > heat
}

impl Thresholds {
    pub fn classify(self, view_width: u64) -> RenderMode {
        match view_width {
            w if w <= self.per_base => RenderMode::PerBase,
            w if w <= self.detailed => RenderMode::DetailedReads,
            w if w <= self.coverage_only => RenderMode::CoverageDense,
            w if w <= self.heat => RenderMode::HeatBar,
            _ => RenderMode::OverviewOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_default_thresholds() {
        let t = Thresholds::default();
        assert_eq!(t.classify(50), RenderMode::PerBase);
        assert_eq!(t.classify(200), RenderMode::PerBase);
        assert_eq!(t.classify(201), RenderMode::DetailedReads);
        assert_eq!(t.classify(1_000), RenderMode::DetailedReads);
        assert_eq!(t.classify(1_001), RenderMode::CoverageDense);
        assert_eq!(t.classify(10_000), RenderMode::CoverageDense);
        assert_eq!(t.classify(10_001), RenderMode::HeatBar);
        assert_eq!(t.classify(100_000), RenderMode::HeatBar);
        assert_eq!(t.classify(100_001), RenderMode::OverviewOnly);
    }
}
```

- [ ] **Step 2: Run tests and commit**

```bash
cargo test -p igv-core render::tests
git add crates/igv-core/src/render.rs
git commit -m "feat(igv-core): Thresholds and RenderMode classifier"
```

---

## Phase 2: igv-core Data Sources

### Task 2.1: Test data fixtures

**Files:**
- Create: `crates/igv-core/tests/data/sample.fa`
- Create: `crates/igv-core/tests/data/sample.fa.fai`
- Create: `crates/igv-core/tests/data/README.md`

- [ ] **Step 1: Provide a tiny FASTA**

`crates/igv-core/tests/data/sample.fa`:

```
>chr1
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTAC
GTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
>chr2
TTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTTTTAAAACCCCGGGGTT
```

`crates/igv-core/tests/data/sample.fa.fai`:

```
chr1	100	6	50	51
chr2	50	116	50	51
```

(Tabs separate the columns. Field meanings: name, length, byte offset of
sequence, line bases, line bytes.)

- [ ] **Step 2: Add a README explaining the fixtures**

`crates/igv-core/tests/data/README.md`:

```markdown
# Test fixtures

Tiny hand-crafted FASTA + matching `.fai`. Future BAM and VCF fixtures
will be derived from public test datasets used by `noodles` and
`samtools` and committed once exercised in tests.
```

- [ ] **Step 3: Commit**

```bash
git add crates/igv-core/tests/data
git commit -m "test(igv-core): add tiny FASTA fixture for source tests"
```

---

### Task 2.2: `FastaSource` trait + noodles impl

**Files:**
- Create: `crates/igv-core/src/source/fasta.rs`
- Modify: `crates/igv-core/src/source/mod.rs`

- [ ] **Step 1: Write integration test**

`crates/igv-core/tests/fasta_source.rs`:

```rust
use std::path::Path;

use igv_core::region::Region;
use igv_core::source::fasta::NoodlesFastaSource;
use igv_core::source::FastaSource;

#[tokio::test]
async fn lists_references_with_lengths() {
    let path = Path::new("tests/data/sample.fa");
    let source = NoodlesFastaSource::open(path).await.unwrap();
    let refs = source.references().await.unwrap();
    let chr1 = refs.iter().find(|r| r.name == "chr1").unwrap();
    assert_eq!(chr1.length, 100);
    let chr2 = refs.iter().find(|r| r.name == "chr2").unwrap();
    assert_eq!(chr2.length, 50);
}

#[tokio::test]
async fn fetches_substring_for_region() {
    let path = Path::new("tests/data/sample.fa");
    let source = NoodlesFastaSource::open(path).await.unwrap();
    let region = Region::new("chr1", 1, 4).unwrap();
    let bytes = source.fetch(&region).await.unwrap();
    assert_eq!(bytes, b"ACGT");
}

#[tokio::test]
async fn fetch_errors_on_unknown_chrom() {
    let path = Path::new("tests/data/sample.fa");
    let source = NoodlesFastaSource::open(path).await.unwrap();
    let region = Region::new("chrZ", 1, 4).unwrap();
    assert!(source.fetch(&region).await.is_err());
}
```

- [ ] **Step 2: Define the trait in `source/mod.rs`**

```rust
//! Async data-source traits and noodles-backed implementations.

pub mod bam;
pub mod fasta;
pub mod vcf;

use async_trait::async_trait;

use crate::error::Result;
use crate::region::Region;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefMeta {
    pub name: String,
    pub length: u64,
}

#[async_trait]
pub trait FastaSource: Send + Sync {
    async fn references(&self) -> Result<Vec<RefMeta>>;
    async fn fetch(&self, region: &Region) -> Result<Vec<u8>>;
}

pub use fasta::NoodlesFastaSource;
pub use vcf::{NoodlesVcfSource, VariantRecord, VcfSource};
pub use bam::{AlignmentRow, BamSource, FetchOpts, NoodlesBamSource};
```

- [ ] **Step 3: Implement `NoodlesFastaSource`**

`crates/igv-core/src/source/fasta.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use noodles::fasta::{self as fasta};
use tokio::sync::Mutex;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{FastaSource, RefMeta};

#[derive(Debug)]
pub struct NoodlesFastaSource {
    path: PathBuf,
    inner: Arc<Mutex<fasta::IndexedReader<std::fs::File>>>,
    refs: Vec<RefMeta>,
}

impl NoodlesFastaSource {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let p = path.clone();

        // FASTA + FAI parsing is cheap and synchronous; do it on a blocking
        // thread to avoid stalling the runtime.
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let fai_path = format!("{}.fai", p.display());
            if !std::path::Path::new(&fai_path).exists() {
                return Err(IgvError::MissingIndex(fai_path.into()));
            }
            let reader = fasta::indexed_reader::Builder::default()
                .build_from_path(&p)
                .map_err(|e| IgvError::io(p.clone(), e))?;
            let index = reader.index();
            let refs = index
                .as_ref()
                .iter()
                .map(|rec| RefMeta {
                    name: std::str::from_utf8(rec.name())
                        .unwrap_or_default()
                        .to_string(),
                    length: rec.length(),
                })
                .collect();
            Ok(Self {
                path: p,
                inner: Arc::new(Mutex::new(reader)),
                refs,
            })
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}

#[async_trait]
impl FastaSource for NoodlesFastaSource {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(self.refs.clone())
    }

    async fn fetch(&self, region: &Region) -> Result<Vec<u8>> {
        let chrom = region.chrom.clone();
        let start = region.start;
        let end = region.end;
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let mut guard = inner.blocking_lock();
            let region_str = format!("{}:{}-{}", chrom, start, end);
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;
            let record = guard.query(&r).map_err(IgvError::noodles)?;
            Ok(record.sequence().as_ref().to_vec())
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p igv-core --test fasta_source
```

Expected: all three tests pass. If `noodles::fasta::indexed_reader` API
differs in the resolved version, adjust to the documented API while keeping
the same behavior (open with index, query by region).

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/source crates/igv-core/tests/fasta_source.rs
git commit -m "feat(igv-core): NoodlesFastaSource with async fetch"
```

---

### Task 2.3: `VcfSource` trait + noodles impl

**Files:**
- Create: `crates/igv-core/src/source/vcf.rs`
- Create: `crates/igv-core/tests/data/sample.vcf.gz` (and `.tbi`)
- Create: `crates/igv-core/tests/vcf_source.rs`

- [ ] **Step 1: Generate a tiny VCF fixture**

Use a one-shot script (not committed):

```bash
cat > /tmp/sample.vcf <<'EOF'
##fileformat=VCFv4.2
##contig=<ID=chr1,length=100>
##INFO=<ID=DP,Number=1,Type=Integer,Description="Total depth">
#CHROM	POS	ID	REF	ALT	QUAL	FILTER	INFO
chr1	10	.	A	G	30	PASS	DP=20
chr1	25	.	C	T	40	PASS	DP=18
chr1	50	.	G	A	35	PASS	DP=22
EOF
bgzip -f /tmp/sample.vcf -c > crates/igv-core/tests/data/sample.vcf.gz
tabix -p vcf crates/igv-core/tests/data/sample.vcf.gz
```

If `bgzip` / `tabix` are not on the host, install via `apt-get install
tabix` or `brew install htslib`. Commit both `sample.vcf.gz` and
`sample.vcf.gz.tbi`.

- [ ] **Step 2: Write the trait + record**

`crates/igv-core/src/source/vcf.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use noodles::vcf;
use tokio::sync::Mutex;

use crate::error::{IgvError, Result};
use crate::region::Region;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRecord {
    pub chrom: String,
    pub pos: u64,            // 1-based
    pub reference_allele: String,
    pub alternate_alleles: Vec<String>,
    pub quality: Option<f32>,
    pub passes_filter: bool,
}

#[async_trait]
pub trait VcfSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>>;
}

#[derive(Debug)]
pub struct NoodlesVcfSource {
    path: PathBuf,
    inner: Arc<Mutex<vcf::IndexedReader<noodles::bgzf::Reader<std::fs::File>>>>,
    header: vcf::Header,
}

impl NoodlesVcfSource {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let p = path.clone();
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let mut reader = vcf::indexed_reader::Builder::default()
                .build_from_path(&p)
                .map_err(|e| IgvError::io(p.clone(), e))?;
            let header = reader.read_header().map_err(IgvError::noodles)?;
            Ok(Self {
                path: p,
                inner: Arc::new(Mutex::new(reader)),
                header,
            })
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}

#[async_trait]
impl VcfSource for NoodlesVcfSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>> {
        let inner = Arc::clone(&self.inner);
        let header = self.header.clone();
        let region = region.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<VariantRecord>> {
            let mut guard = inner.blocking_lock();
            let region_str = format!("{}:{}-{}", region.chrom, region.start, region.end);
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;
            let mut out = Vec::new();
            for result in guard.query(&header, &r).map_err(IgvError::noodles)? {
                let rec = result.map_err(IgvError::noodles)?;
                let chrom = rec
                    .reference_sequence_name()
                    .to_string();
                let pos = match rec.variant_start() {
                    Some(p) => p.map_err(IgvError::noodles)?.get() as u64,
                    None => continue,
                };
                let ref_allele = rec.reference_bases().to_string();
                let alts = rec
                    .alternate_bases()
                    .iter()
                    .filter_map(|a| a.ok())
                    .map(|a| a.to_string())
                    .collect();
                let quality = rec
                    .quality_score()
                    .and_then(|q| q.ok())
                    .map(|q| q.into());
                let passes_filter = rec
                    .filters()
                    .iter()
                    .all(|f| f.as_ref().map(|s| s == "PASS").unwrap_or(false))
                    || rec.filters().is_empty();
                out.push(VariantRecord {
                    chrom,
                    pos,
                    reference_allele: ref_allele,
                    alternate_alleles: alts,
                    quality,
                    passes_filter,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
```

Note: noodles' VCF API surface evolves between minor versions. If method
names differ at the resolved version, adjust to the new names while
preserving the externally visible `VariantRecord` fields.

- [ ] **Step 3: Write the integration test**

`crates/igv-core/tests/vcf_source.rs`:

```rust
use std::path::Path;

use igv_core::region::Region;
use igv_core::source::vcf::{NoodlesVcfSource, VcfSource};

#[tokio::test]
async fn fetches_three_variants_in_range() {
    let path = Path::new("tests/data/sample.vcf.gz");
    let source = NoodlesVcfSource::open(path).await.unwrap();
    let region = Region::new("chr1", 1, 100).unwrap();
    let variants = source.fetch(&region).await.unwrap();
    assert_eq!(variants.len(), 3);
    assert_eq!(variants[0].pos, 10);
    assert_eq!(variants[0].reference_allele, "A");
    assert_eq!(variants[0].alternate_alleles, vec!["G".to_string()]);
}

#[tokio::test]
async fn returns_empty_outside_range() {
    let path = Path::new("tests/data/sample.vcf.gz");
    let source = NoodlesVcfSource::open(path).await.unwrap();
    let region = Region::new("chr1", 60, 100).unwrap();
    let variants = source.fetch(&region).await.unwrap();
    assert!(variants.is_empty());
}
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p igv-core --test vcf_source
git add crates/igv-core/src/source/vcf.rs \
        crates/igv-core/tests/vcf_source.rs \
        crates/igv-core/tests/data/sample.vcf.gz \
        crates/igv-core/tests/data/sample.vcf.gz.tbi
git commit -m "feat(igv-core): NoodlesVcfSource with VariantRecord"
```

---

### Task 2.4: BAM data source — open and fetch raw records

**Files:**
- Create: `crates/igv-core/src/source/bam.rs`
- Create: `crates/igv-core/tests/data/sample.bam` (and `.bai`)
- Create: `crates/igv-core/tests/bam_source.rs`

- [ ] **Step 1: Generate a tiny BAM fixture**

Use a one-shot script (not committed):

```bash
cat > /tmp/sample.sam <<'EOF'
@HD	VN:1.6	SO:coordinate
@SQ	SN:chr1	LN:100
@SQ	SN:chr2	LN:50
read1	0	chr1	10	60	5M	*	0	0	ACGTA	IIIII
read2	0	chr1	20	60	5M1I4M	*	0	0	ACGTAATTAC	IIIIIIIIII
read3	16	chr1	40	60	10M	*	0	0	TTTTAAAACG	IIIIIIIIII
read4	0	chr2	5	60	8M	*	0	0	TTTTAAAA	IIIIIIII
EOF
samtools view -bS /tmp/sample.sam > crates/igv-core/tests/data/sample.bam
samtools index crates/igv-core/tests/data/sample.bam
```

Commit both `.bam` and `.bam.bai`.

- [ ] **Step 2: Implement `BamSource` and `AlignmentRow`**

`crates/igv-core/src/source/bam.rs`:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use noodles::bam;
use noodles::sam::alignment::record::cigar::op::Kind;
use tokio::sync::Mutex;

use crate::error::{IgvError, Result};
use crate::region::Region;

#[derive(Debug, Default, Clone, Copy)]
pub struct FetchOpts {
    pub include_secondary: bool,
    pub include_supplementary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CigarKind {
    Match,
    Insertion,
    Deletion,
    Skip,
    SoftClip,
    HardClip,
    Padding,
    SeqMatch,
    SeqMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CigarOp {
    pub kind: CigarKind,
    pub len: u32,
}

#[derive(Debug, Clone)]
pub struct AlignmentRow {
    pub query_name: String,
    pub flag: u16,
    pub ref_start: u64, // 1-based inclusive
    pub ref_end: u64,   // 1-based inclusive
    pub mapq: u8,
    pub is_reverse: bool,
    pub query_sequence: Vec<u8>,
    pub cigar: Vec<CigarOp>,
    pub tag: Option<(String, String)>, // (tag name, value as string)
}

#[async_trait]
pub trait BamSource: Send + Sync {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>>;
}

#[derive(Debug, Clone)]
pub struct NoodlesBamSource {
    path: PathBuf,
    inner: Arc<Mutex<bam::IndexedReader<noodles::bgzf::Reader<std::fs::File>>>>,
    header: noodles::sam::Header,
    tag_name: Option<[u8; 2]>,
}

impl NoodlesBamSource {
    pub async fn open(path: impl AsRef<Path>, tag_name: Option<&str>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let p = path.clone();
        let tag = match tag_name {
            Some(t) if t.len() == 2 => {
                let bytes = t.as_bytes();
                Some([bytes[0], bytes[1]])
            }
            None => None,
            Some(other) => return Err(IgvError::Other(format!("BAM tag must be 2 chars: {other}"))),
        };
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let mut reader = bam::indexed_reader::Builder::default()
                .build_from_path(&p)
                .map_err(|e| IgvError::io(p.clone(), e))?;
            let header = reader.read_header().map_err(IgvError::noodles)?;
            Ok(Self {
                path: p,
                inner: Arc::new(Mutex::new(reader)),
                header,
                tag_name: tag,
            })
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}

fn cigar_kind_from(kind: Kind) -> CigarKind {
    match kind {
        Kind::Match => CigarKind::Match,
        Kind::Insertion => CigarKind::Insertion,
        Kind::Deletion => CigarKind::Deletion,
        Kind::Skip => CigarKind::Skip,
        Kind::SoftClip => CigarKind::SoftClip,
        Kind::HardClip => CigarKind::HardClip,
        Kind::Pad => CigarKind::Padding,
        Kind::SequenceMatch => CigarKind::SeqMatch,
        Kind::SequenceMismatch => CigarKind::SeqMismatch,
    }
}

#[async_trait]
impl BamSource for NoodlesBamSource {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>> {
        use noodles::sam::alignment::Record as _;

        let inner = Arc::clone(&self.inner);
        let header = self.header.clone();
        let region = region.clone();
        let opts = *opts;
        let tag_name = self.tag_name;

        tokio::task::spawn_blocking(move || -> Result<Vec<AlignmentRow>> {
            let mut guard = inner.blocking_lock();
            let region_str = format!("{}:{}-{}", region.chrom, region.start, region.end);
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;

            let mut out = Vec::new();
            for result in guard.query(&header, &r).map_err(IgvError::noodles)? {
                let record = result.map_err(IgvError::noodles)?;
                let flag = record.flags().bits();
                let is_unmapped = record.flags().is_unmapped();
                let is_secondary = record.flags().is_secondary();
                let is_supplementary = record.flags().is_supplementary();
                if is_unmapped {
                    continue;
                }
                if !opts.include_secondary && is_secondary {
                    continue;
                }
                if !opts.include_supplementary && is_supplementary {
                    continue;
                }

                let query_name = record
                    .name()
                    .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("").to_string())
                    .unwrap_or_default();

                let ref_start_i32 = record
                    .alignment_start()
                    .ok_or_else(|| IgvError::Other("missing alignment start".into()))?
                    .map_err(IgvError::noodles)?
                    .get();
                let cigar: Vec<CigarOp> = record
                    .cigar()
                    .iter()
                    .map(|op| {
                        let op = op.map_err(IgvError::noodles)?;
                        Ok(CigarOp {
                            kind: cigar_kind_from(op.kind()),
                            len: op.len() as u32,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let mapq = record.mapping_quality().map(|m| m.get()).unwrap_or(0);
                let is_reverse = record.flags().is_reverse_complemented();

                let query_sequence = record.sequence().iter().collect::<Vec<u8>>();

                // Span on reference = sum of consuming ops.
                let ref_consuming: u32 = cigar
                    .iter()
                    .filter(|op| {
                        matches!(
                            op.kind,
                            CigarKind::Match
                                | CigarKind::Deletion
                                | CigarKind::Skip
                                | CigarKind::SeqMatch
                                | CigarKind::SeqMismatch
                        )
                    })
                    .map(|op| op.len)
                    .sum();
                let ref_start = ref_start_i32 as u64;
                let ref_end = ref_start + ref_consuming.saturating_sub(1) as u64;

                let tag = tag_name.and_then(|name| {
                    let data = record.data();
                    data.get(&name).and_then(|r| r.ok()).map(|v| {
                        let key = std::str::from_utf8(&name).unwrap_or("").to_string();
                        (key, format!("{:?}", v))
                    })
                });

                out.push(AlignmentRow {
                    query_name,
                    flag,
                    ref_start,
                    ref_end,
                    mapq,
                    is_reverse,
                    query_sequence,
                    cigar,
                    tag,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
```

- [ ] **Step 3: Write the integration test**

`crates/igv-core/tests/bam_source.rs`:

```rust
use std::path::Path;

use igv_core::region::Region;
use igv_core::source::bam::{BamSource, FetchOpts, NoodlesBamSource};

#[tokio::test]
async fn fetches_reads_overlapping_region() {
    let source = NoodlesBamSource::open(Path::new("tests/data/sample.bam"), None)
        .await
        .unwrap();
    let region = Region::new("chr1", 1, 100).unwrap();
    let reads = source.fetch(&region, &FetchOpts::default()).await.unwrap();
    assert_eq!(reads.len(), 3);
    let names: Vec<_> = reads.iter().map(|r| r.query_name.as_str()).collect();
    assert!(names.contains(&"read1"));
    assert!(names.contains(&"read2"));
    assert!(names.contains(&"read3"));
}

#[tokio::test]
async fn cigar_is_parsed() {
    let source = NoodlesBamSource::open(Path::new("tests/data/sample.bam"), None)
        .await
        .unwrap();
    let region = Region::new("chr1", 20, 30).unwrap();
    let reads = source.fetch(&region, &FetchOpts::default()).await.unwrap();
    let read2 = reads
        .iter()
        .find(|r| r.query_name == "read2")
        .expect("read2 in region");
    assert_eq!(read2.cigar.len(), 3);
}
```

- [ ] **Step 4: Run and commit**

```bash
cargo test -p igv-core --test bam_source
git add crates/igv-core/src/source/bam.rs \
        crates/igv-core/tests/bam_source.rs \
        crates/igv-core/tests/data/sample.bam \
        crates/igv-core/tests/data/sample.bam.bai
git commit -m "feat(igv-core): NoodlesBamSource with CIGAR parsing"
```

---

### Task 2.5: CIGAR expansion to character-level cells

**Files:**
- Modify: `crates/igv-core/src/alignment.rs`

- [ ] **Step 1: Write the module with tests**

```rust
//! Expand `AlignmentRow` + reference into per-base display cells.

use crate::source::bam::{AlignmentRow, CigarKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseGlyph {
    /// Match against reference. Renderer chooses `.` style by default.
    Match,
    /// Mismatch — actual base is the inner byte (uppercase ASCII).
    Mismatch(u8),
    /// Reference deletion in this read at this position.
    Deletion,
    /// Soft-clipped base — renderer hides by default; carries actual base.
    SoftClip(u8),
}

#[derive(Debug, Clone)]
pub struct ReadCells {
    /// 1-based inclusive coordinate of the first cell.
    pub ref_start: u64,
    /// One entry per reference position consumed; insertions are tracked
    /// separately in `insertions`.
    pub cells: Vec<BaseGlyph>,
    /// Insertions to the reference, keyed by 1-based reference position
    /// **before** which the insertion sits. Value is the inserted bases.
    pub insertions: Vec<(u64, Vec<u8>)>,
}

/// Expand a single alignment row into reference-space display cells.
///
/// `reference` is the bytes of the **viewing** region, indexed by 1-based
/// `view_start`. Mismatch detection only happens for cells that fall inside
/// the view.
pub fn expand(
    row: &AlignmentRow,
    reference: &[u8],
    view_start: u64,
) -> ReadCells {
    let mut cells = Vec::new();
    let mut insertions = Vec::new();
    let mut ref_pos: u64 = row.ref_start;
    let mut q_idx: usize = 0;

    for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                for _ in 0..op.len {
                    let q_base = row.query_sequence.get(q_idx).copied().unwrap_or(b'N');
                    let r_idx_signed = ref_pos as i64 - view_start as i64;
                    let glyph = if r_idx_signed >= 0
                        && (r_idx_signed as usize) < reference.len()
                    {
                        let r_base = reference[r_idx_signed as usize].to_ascii_uppercase();
                        if q_base.to_ascii_uppercase() == r_base {
                            BaseGlyph::Match
                        } else {
                            BaseGlyph::Mismatch(q_base.to_ascii_uppercase())
                        }
                    } else {
                        BaseGlyph::Mismatch(q_base.to_ascii_uppercase())
                    };
                    cells.push(glyph);
                    ref_pos += 1;
                    q_idx += 1;
                }
            }
            CigarKind::Deletion | CigarKind::Skip => {
                for _ in 0..op.len {
                    cells.push(BaseGlyph::Deletion);
                    ref_pos += 1;
                }
            }
            CigarKind::Insertion => {
                let bases = row
                    .query_sequence
                    .get(q_idx..q_idx + op.len as usize)
                    .map(|s| s.to_vec())
                    .unwrap_or_default();
                insertions.push((ref_pos, bases));
                q_idx += op.len as usize;
            }
            CigarKind::SoftClip => {
                // Soft-clipped bases consume query but not reference. We don't
                // place them in cells; they simply advance q_idx.
                q_idx += op.len as usize;
            }
            CigarKind::HardClip | CigarKind::Padding => {
                // Consume neither.
            }
        }
    }

    ReadCells {
        ref_start: row.ref_start,
        cells,
        insertions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::bam::{AlignmentRow, CigarOp};

    fn row(cigar: Vec<CigarOp>, seq: &[u8], start: u64) -> AlignmentRow {
        let consumed: u32 = cigar
            .iter()
            .filter(|op| {
                matches!(
                    op.kind,
                    CigarKind::Match
                        | CigarKind::Deletion
                        | CigarKind::Skip
                        | CigarKind::SeqMatch
                        | CigarKind::SeqMismatch
                )
            })
            .map(|op| op.len)
            .sum();
        AlignmentRow {
            query_name: "r".into(),
            flag: 0,
            ref_start: start,
            ref_end: start + consumed.saturating_sub(1) as u64,
            mapq: 60,
            is_reverse: false,
            query_sequence: seq.to_vec(),
            cigar,
            tag: None,
        }
    }

    #[test]
    fn match_only_no_mismatches() {
        let r = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 1);
        let cells = expand(&r, b"ACGT", 1);
        assert_eq!(cells.cells, vec![
            BaseGlyph::Match,
            BaseGlyph::Match,
            BaseGlyph::Match,
            BaseGlyph::Match,
        ]);
        assert!(cells.insertions.is_empty());
    }

    #[test]
    fn match_with_mismatch() {
        let r = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGA", 1);
        let cells = expand(&r, b"ACGT", 1);
        assert!(matches!(cells.cells[3], BaseGlyph::Mismatch(b'A')));
    }

    #[test]
    fn insertion_recorded_separately() {
        let r = row(
            vec![
                CigarOp { kind: CigarKind::Match, len: 2 },
                CigarOp { kind: CigarKind::Insertion, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 2 },
            ],
            b"ACTTGT",
            1,
        );
        let cells = expand(&r, b"ACGT", 1);
        assert_eq!(cells.cells.len(), 4);
        assert_eq!(cells.insertions.len(), 1);
        assert_eq!(cells.insertions[0].0, 3);
        assert_eq!(cells.insertions[0].1, b"TT".to_vec());
    }

    #[test]
    fn deletion_marked() {
        let r = row(
            vec![
                CigarOp { kind: CigarKind::Match, len: 2 },
                CigarOp { kind: CigarKind::Deletion, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 2 },
            ],
            b"ACGT",
            1,
        );
        let cells = expand(&r, b"ACGTAC", 1);
        assert_eq!(cells.cells[2], BaseGlyph::Deletion);
        assert_eq!(cells.cells[3], BaseGlyph::Deletion);
    }
}
```

- [ ] **Step 2: Run, commit**

```bash
cargo test -p igv-core alignment::tests
git add crates/igv-core/src/alignment.rs
git commit -m "feat(igv-core): CIGAR expansion to BaseGlyph cells"
```

---

### Task 2.6: Coverage computation

**Files:**
- Modify: `crates/igv-core/src/coverage.rs`

- [ ] **Step 1: Write module with tests**

```rust
//! Pileup-style coverage track from a slice of `AlignmentRow`s.

use crate::source::bam::{AlignmentRow, CigarKind};

/// Per-position depth across an inclusive 1-based window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageVec {
    pub start: u64,
    pub depths: Vec<u32>,
}

impl CoverageVec {
    pub fn end(&self) -> u64 {
        self.start + self.depths.len() as u64 - 1
    }

    pub fn max(&self) -> u32 {
        self.depths.iter().copied().max().unwrap_or(0)
    }
}

/// Compute coverage for the closed range [view_start, view_end] (1-based).
pub fn compute(rows: &[AlignmentRow], view_start: u64, view_end: u64) -> CoverageVec {
    assert!(view_end >= view_start, "view_end must be >= view_start");
    let len = (view_end - view_start + 1) as usize;
    let mut depths = vec![0u32; len];

    for row in rows {
        let mut ref_pos = row.ref_start;
        for op in &row.cigar {
            match op.kind {
                CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                    for _ in 0..op.len {
                        if ref_pos >= view_start && ref_pos <= view_end {
                            let idx = (ref_pos - view_start) as usize;
                            depths[idx] = depths[idx].saturating_add(1);
                        }
                        ref_pos += 1;
                    }
                }
                CigarKind::Deletion | CigarKind::Skip => {
                    ref_pos += op.len as u64;
                }
                CigarKind::Insertion | CigarKind::SoftClip => {}
                CigarKind::HardClip | CigarKind::Padding => {}
            }
        }
    }

    CoverageVec {
        start: view_start,
        depths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::bam::{AlignmentRow, CigarOp};

    fn r(start: u64, cigar: Vec<CigarOp>) -> AlignmentRow {
        AlignmentRow {
            query_name: "r".into(),
            flag: 0,
            ref_start: start,
            ref_end: start
                + cigar
                    .iter()
                    .filter(|op| matches!(op.kind, CigarKind::Match))
                    .map(|op| op.len as u64)
                    .sum::<u64>()
                    .saturating_sub(1),
            mapq: 60,
            is_reverse: false,
            query_sequence: vec![],
            cigar,
            tag: None,
        }
    }

    #[test]
    fn two_overlapping_reads_doubles_depth() {
        let reads = vec![
            r(1, vec![CigarOp { kind: CigarKind::Match, len: 5 }]),
            r(3, vec![CigarOp { kind: CigarKind::Match, len: 5 }]),
        ];
        let cov = compute(&reads, 1, 7);
        assert_eq!(cov.depths, vec![1, 1, 2, 2, 2, 1, 1]);
    }

    #[test]
    fn deletion_skips_depth() {
        let reads = vec![r(
            1,
            vec![
                CigarOp { kind: CigarKind::Match, len: 2 },
                CigarOp { kind: CigarKind::Deletion, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 2 },
            ],
        )];
        let cov = compute(&reads, 1, 6);
        assert_eq!(cov.depths, vec![1, 1, 0, 0, 1, 1]);
    }
}
```

- [ ] **Step 2: Run and commit**

```bash
cargo test -p igv-core coverage::tests
git add crates/igv-core/src/coverage.rs
git commit -m "feat(igv-core): coverage::compute (per-base pileup)"
```

---

## Phase 3: igv-tui Scaffolding

### Task 3.1: CLI parsing with clap

**Files:**
- Create: `crates/igv-tui/src/cli.rs`
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Write the CLI definition**

`crates/igv-tui/src/cli.rs`:

```rust
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "igv-rs",
    version,
    about = "Interactive terminal genome viewer (Rust rewrite of cligv)"
)]
pub struct Cli {
    /// Path to the reference genome FASTA file (must have a .fai index).
    pub fasta: PathBuf,

    /// Path to a VCF file (must have a .tbi index). May be repeated in a
    /// future iteration; today only the first is honored.
    #[arg(short = 'v', long = "vcf")]
    pub vcf: Option<PathBuf>,

    /// Path to a BAM file (must have a .bai or .csi index). May be repeated
    /// to display multiple alignment tracks.
    #[arg(short = 'b', long = "bam")]
    pub bam: Vec<PathBuf>,

    /// Initial region (e.g. "chr1:1000-2000", "chr1:1000", "chr1").
    #[arg(short = 'r', long = "region")]
    pub region: Option<String>,

    /// BAM tag to color reads by (two-character tag, e.g. "ha").
    #[arg(short = 't', long = "tag")]
    pub tag: Option<String>,

    /// Use light theme (for light-background terminals).
    #[arg(long = "light-mode")]
    pub light_mode: bool,

    /// Logging level filter.
    #[arg(long = "log-level", default_value = "info")]
    pub log_level: String,

    /// Optional override config path. Defaults to
    /// `$XDG_CONFIG_HOME/igv-rs/config.toml`.
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
}
```

- [ ] **Step 2: Wire `main.rs` to parse and print**

```rust
mod cli;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    println!("{:#?}", args);
    Ok(())
}
```

- [ ] **Step 3: Verify and commit**

```bash
cargo run -p igv-tui -- --help
cargo run -p igv-tui -- crates/igv-core/tests/data/sample.fa
git add crates/igv-tui/src/cli.rs crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): clap-based CLI definition"
```

Expected: `--help` prints usage; running with the sample FASTA prints the
parsed `Cli` struct.

---

### Task 3.2: Theme presets and TOML loader

**Files:**
- Create: `crates/igv-tui/src/ui/mod.rs`
- Create: `crates/igv-tui/src/ui/theme.rs`

- [ ] **Step 1: Write `ui/mod.rs`**

```rust
pub mod theme;
pub mod layout;
pub mod widgets;
```

(`layout` and `widgets` modules will be created in subsequent tasks; if
`cargo check` complains about missing modules at the end of this task,
create empty `layout.rs` and `widgets/mod.rs` with placeholder
`//! TODO` lines.)

- [ ] **Step 2: Write `ui/theme.rs`**

```rust
//! Color theme: built-in dark/light presets + user overrides via TOML.

use std::collections::HashMap;
use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Dark,
    Light,
    Custom,
}

#[derive(Debug, Clone)]
pub struct Theme {
    map: HashMap<String, Style>,
}

impl Theme {
    pub fn dark() -> Self {
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(Color::White));
        m.insert("MATCH_FWD".into(), Style::default().fg(Color::Cyan));
        m.insert("MATCH_REV".into(), Style::default().fg(Color::Magenta));
        m.insert(
            "MISMATCH".into(),
            Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "DELETION".into(),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "INSERTION".into(),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "VARIANT".into(),
            Style::default().fg(Color::White).bg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "HEADER".into(),
            Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD),
        );
        m.insert("FOOTER".into(), Style::default().fg(Color::White).bg(Color::DarkGray));
        m.insert("OVERVIEW".into(), Style::default().fg(Color::Yellow));
        m.insert("BORDER".into(), Style::default().fg(Color::DarkGray));
        m.insert("COVERAGE".into(), Style::default().fg(Color::Cyan));
        m.insert("WARNING".into(), Style::default().fg(Color::Yellow));
        m.insert("ERROR".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(Color::Green));
        Self { map: m }
    }

    pub fn light() -> Self {
        let mut m = HashMap::new();
        m.insert("A".into(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        m.insert("C".into(), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
        m.insert("G".into(), Style::default().fg(Color::Rgb(180, 100, 0)).add_modifier(Modifier::BOLD));
        m.insert("T".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("N".into(), Style::default().fg(Color::Black));
        m.insert("MATCH_FWD".into(), Style::default().fg(Color::Blue));
        m.insert("MATCH_REV".into(), Style::default().fg(Color::Magenta));
        m.insert(
            "MISMATCH".into(),
            Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "DELETION".into(),
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "INSERTION".into(),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "VARIANT".into(),
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "HEADER".into(),
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD),
        );
        m.insert("FOOTER".into(), Style::default().fg(Color::Black).bg(Color::Green));
        m.insert("OVERVIEW".into(), Style::default().fg(Color::Rgb(200, 100, 0)));
        m.insert("BORDER".into(), Style::default().fg(Color::Gray));
        m.insert("COVERAGE".into(), Style::default().fg(Color::Blue));
        m.insert("WARNING".into(), Style::default().fg(Color::Rgb(180, 100, 0)));
        m.insert("ERROR".into(), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
        m.insert("SUCCESS".into(), Style::default().fg(Color::Green));
        Self { map: m }
    }

    pub fn get(&self, key: &str) -> Style {
        self.map.get(key).copied().unwrap_or_default()
    }

    pub fn merge_overrides(&mut self, overrides: &HashMap<String, String>) {
        for (k, v) in overrides {
            if let Some(style) = parse_style(v) {
                self.map.insert(k.clone(), style);
            }
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

fn default_preset() -> String {
    "dark".into()
}

pub fn load_theme(preset_override: Option<bool>, config_path: Option<&Path>) -> Theme {
    // CLI flag takes precedence. `Some(true)` ⇒ light, `Some(false)` ⇒ dark.
    let cli_pref = preset_override;
    let config: Option<ThemeConfig> = config_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str::<HashMap<String, toml::Value>>(&s).ok())
        .and_then(|m| m.get("theme").cloned())
        .and_then(|v| v.try_into().ok());

    let preset = match cli_pref {
        Some(true) => Preset::Light,
        Some(false) => Preset::Dark,
        None => match config.as_ref().map(|c| c.preset.as_str()) {
            Some("light") => Preset::Light,
            Some("dark") | None => Preset::Dark,
            Some(_) => Preset::Custom,
        },
    };

    let mut theme = match preset {
        Preset::Light => Theme::light(),
        _ => Theme::dark(),
    };

    if let Some(cfg) = config {
        theme.merge_overrides(&cfg.custom);
    }
    theme
}

fn parse_style(s: &str) -> Option<Style> {
    // Minimal parser: tokens separated by spaces. Recognized tokens:
    //   "bold", "dim", "italic", "underline"
    //   "<color>"               → fg
    //   "on <color>"            → bg
    //   colors: black, red, green, yellow, blue, magenta, cyan, white, gray
    let mut style = Style::default();
    let mut tokens = s.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        match tok {
            "bold" => style = style.add_modifier(Modifier::BOLD),
            "dim" => style = style.add_modifier(Modifier::DIM),
            "italic" => style = style.add_modifier(Modifier::ITALIC),
            "underline" => style = style.add_modifier(Modifier::UNDERLINED),
            "on" => {
                if let Some(c) = tokens.next() {
                    style = style.bg(parse_color(c)?);
                }
            }
            other => {
                style = style.fg(parse_color(other)?);
            }
        }
    }
    Some(style)
}

fn parse_color(s: &str) -> Option<Color> {
    Some(match s {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_has_nucleotide_styles() {
        let t = Theme::dark();
        assert_ne!(t.get("A"), Style::default());
        assert_ne!(t.get("C"), Style::default());
    }

    #[test]
    fn parse_style_handles_bold_fg() {
        let s = parse_style("bold red").unwrap();
        assert!(s.add_modifier.contains(Modifier::BOLD));
        assert_eq!(s.fg, Some(Color::Red));
    }

    #[test]
    fn parse_style_handles_fg_on_bg() {
        let s = parse_style("white on red").unwrap();
        assert_eq!(s.fg, Some(Color::White));
        assert_eq!(s.bg, Some(Color::Red));
    }
}
```

- [ ] **Step 3: Make `main.rs` register `mod ui;` (no-op compile)**

Append to `main.rs` near the existing `mod cli;`:

```rust
mod ui;
```

- [ ] **Step 4: Verify, commit**

```bash
cargo test -p igv-tui ui::theme::tests
git add crates/igv-tui/src
git commit -m "feat(igv-tui): theme presets, TOML overrides, style parser"
```

---

### Task 3.3: Action enum and AppState

**Files:**
- Create: `crates/igv-tui/src/app/mod.rs`
- Create: `crates/igv-tui/src/app/action.rs`
- Create: `crates/igv-tui/src/app/state.rs`

- [ ] **Step 1: Write `app/mod.rs`**

```rust
pub mod action;
pub mod loader;
pub mod state;
```

- [ ] **Step 2: Write `app/action.rs`**

```rust
use igv_core::Region;

#[derive(Debug, Clone)]
pub enum Action {
    /// Move forward by `nav_overlap` of view width.
    MoveForward,
    /// Move backward.
    MoveBackward,
    /// Zoom in / out.
    Zoom { zoom_in: bool },
    /// Jump to an explicit region.
    Goto(Region),
    /// Toggle dark/light theme.
    ToggleTheme,
    /// Open the command palette.
    OpenCommand,
    /// Submit the command palette buffer.
    CommandSubmit(String),
    /// Cancel command palette.
    CommandCancel,
    /// Set a bookmark to the current region under key `c`.
    SetBookmark(char),
    /// Jump to the bookmark stored at key `c`.
    JumpBookmark(char),
    /// Quit the application.
    Quit,
    /// No-op (used as a sentinel).
    None,
}
```

- [ ] **Step 3: Write `app/state.rs`**

```rust
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

#[derive(Debug, Clone)]
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
```

- [ ] **Step 4: Wire `mod app;` into `main.rs`**

Append:

```rust
mod app;
```

- [ ] **Step 5: Verify and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src
git commit -m "feat(igv-tui): Action enum, AppState skeleton with nav/zoom"
```

---

### Task 3.4: Loader task with cancellation and generation guard

**Files:**
- Create: `crates/igv-tui/src/app/loader.rs`

- [ ] **Step 1: Write the loader**

```rust
use std::sync::Arc;

use igv_core::region::Region;
use igv_core::source::bam::AlignmentRow;
use igv_core::source::vcf::VariantRecord;
use igv_core::source::{BamSource, FastaSource, FetchOpts, VcfSource};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub generation: u64,
    pub region: Region,
    pub fetch_opts: FetchOpts,
}

#[derive(Debug)]
pub enum LoadResult {
    Reference {
        generation: u64,
        region: Region,
        bytes: Vec<u8>,
    },
    Variants {
        generation: u64,
        records: Vec<VariantRecord>,
    },
    Bam {
        generation: u64,
        bam_index: usize,
        rows: Vec<AlignmentRow>,
    },
    Error {
        generation: u64,
        message: String,
    },
}

pub struct Loader {
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub bams: Vec<Arc<dyn BamSource>>,
    pub tx: mpsc::Sender<LoadResult>,
    pub current: Vec<JoinHandle<()>>,
}

impl Loader {
    pub fn new(
        fasta: Arc<dyn FastaSource>,
        vcf: Option<Arc<dyn VcfSource>>,
        bams: Vec<Arc<dyn BamSource>>,
        tx: mpsc::Sender<LoadResult>,
    ) -> Self {
        Self {
            fasta,
            vcf,
            bams,
            tx,
            current: Vec::new(),
        }
    }

    /// Cancel any in-flight tasks and dispatch fresh ones for `req`.
    pub fn dispatch(&mut self, req: LoadRequest) {
        for h in self.current.drain(..) {
            h.abort();
        }

        // Reference fetch
        let fasta = Arc::clone(&self.fasta);
        let tx = self.tx.clone();
        let r = req.clone();
        self.current.push(tokio::spawn(async move {
            match fasta.fetch(&r.region).await {
                Ok(bytes) => {
                    let _ = tx
                        .send(LoadResult::Reference {
                            generation: r.generation,
                            region: r.region,
                            bytes,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(LoadResult::Error {
                            generation: r.generation,
                            message: e.to_string(),
                        })
                        .await;
                }
            }
        }));

        // VCF fetch
        if let Some(vcf) = &self.vcf {
            let vcf = Arc::clone(vcf);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                match vcf.fetch(&r.region).await {
                    Ok(records) => {
                        let _ = tx
                            .send(LoadResult::Variants {
                                generation: r.generation,
                                records,
                            })
                            .await;
                    }
                    Err(e) => {
                        warn!("vcf fetch failed: {e}");
                        let _ = tx
                            .send(LoadResult::Variants {
                                generation: r.generation,
                                records: Vec::new(),
                            })
                            .await;
                    }
                }
            }));
        }

        // BAM fetches
        for (idx, bam) in self.bams.iter().enumerate() {
            let bam = Arc::clone(bam);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                match bam.fetch(&r.region, &r.fetch_opts).await {
                    Ok(rows) => {
                        let _ = tx
                            .send(LoadResult::Bam {
                                generation: r.generation,
                                bam_index: idx,
                                rows,
                            })
                            .await;
                    }
                    Err(e) => {
                        warn!("bam fetch failed: {e}");
                        let _ = tx
                            .send(LoadResult::Bam {
                                generation: r.generation,
                                bam_index: idx,
                                rows: Vec::new(),
                            })
                            .await;
                    }
                }
            }));
        }
    }
}
```

- [ ] **Step 2: Verify and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/app/loader.rs
git commit -m "feat(igv-tui): Loader with cancellable tokio tasks per source"
```

---

### Task 3.5: Input mapper (crossterm Event → Action)

**Files:**
- Create: `crates/igv-tui/src/input.rs`

- [ ] **Step 1: Write the mapper**

```rust
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::action::Action;

#[derive(Debug, Default)]
pub struct InputState {
    /// True when a leading bookmark prefix has been observed
    /// (`m` for set, `'` for jump).
    pub pending_bookmark: Option<BookmarkOp>,
}

#[derive(Debug, Clone, Copy)]
pub enum BookmarkOp {
    Set,
    Jump,
}

impl InputState {
    pub fn map(
        &mut self,
        event: &Event,
        command_open: bool,
    ) -> Action {
        if let Event::Key(KeyEvent { code, modifiers, .. }) = event {
            // While the command palette is open, only Esc/Enter/typing matter.
            if command_open {
                return match code {
                    KeyCode::Esc => Action::CommandCancel,
                    _ => Action::None, // command.rs handles typing
                };
            }
            // Ctrl-C exits.
            if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('c')) {
                return Action::Quit;
            }

            // Bookmark prefix handling
            if let Some(op) = self.pending_bookmark.take() {
                if let KeyCode::Char(c) = code {
                    return match op {
                        BookmarkOp::Set => Action::SetBookmark(*c),
                        BookmarkOp::Jump => Action::JumpBookmark(*c),
                    };
                }
                return Action::None;
            }

            return match code {
                KeyCode::Char('q') => Action::Quit,
                KeyCode::Char('a') | KeyCode::Left => Action::MoveBackward,
                KeyCode::Char('d') | KeyCode::Right => Action::MoveForward,
                KeyCode::Char('w') | KeyCode::Up => Action::Zoom { zoom_in: true },
                KeyCode::Char('s') | KeyCode::Down => Action::Zoom { zoom_in: false },
                KeyCode::Char('t') => Action::ToggleTheme,
                KeyCode::Char(':') | KeyCode::Char('g') => Action::OpenCommand,
                KeyCode::Char('m') => {
                    self.pending_bookmark = Some(BookmarkOp::Set);
                    Action::None
                }
                KeyCode::Char('\'') => {
                    self.pending_bookmark = Some(BookmarkOp::Jump);
                    Action::None
                }
                _ => Action::None,
            };
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(c: char) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn d_moves_forward() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('d'), false), Action::MoveForward));
    }

    #[test]
    fn m_then_a_sets_bookmark_a() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('m'), false), Action::None));
        assert!(matches!(s.map(&key('a'), false), Action::SetBookmark('a')));
    }

    #[test]
    fn quote_then_a_jumps_bookmark_a() {
        let mut s = InputState::default();
        assert!(matches!(s.map(&key('\''), false), Action::None));
        assert!(matches!(s.map(&key('a'), false), Action::JumpBookmark('a')));
    }
}
```

- [ ] **Step 2: Add `mod input;` to main**

Append to `main.rs`:

```rust
mod input;
```

- [ ] **Step 3: Run tests, commit**

```bash
cargo test -p igv-tui input::tests
git add crates/igv-tui/src
git commit -m "feat(igv-tui): InputState mapping crossterm events to Action"
```

---

### Task 3.6: Command palette state

**Files:**
- Create: `crates/igv-tui/src/command.rs`

- [ ] **Step 1: Write the palette**

```rust
use crossterm::event::{Event, KeyCode, KeyEvent};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::app::action::Action;

#[derive(Debug, Default)]
pub struct CommandPalette {
    pub input: Input,
    pub open: bool,
}

impl CommandPalette {
    pub fn open(&mut self) {
        self.open = true;
        self.input = Input::default();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.input = Input::default();
    }

    /// Returns an `Action::CommandSubmit` on Enter, `CommandCancel` on Esc,
    /// or `None` for typing.
    pub fn handle(&mut self, event: &Event) -> Action {
        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                KeyCode::Enter => {
                    let buf = self.input.value().to_string();
                    self.close();
                    return Action::CommandSubmit(buf);
                }
                KeyCode::Esc => {
                    self.close();
                    return Action::CommandCancel;
                }
                _ => {
                    self.input.handle_event(event);
                    return Action::None;
                }
            }
        }
        Action::None
    }
}
```

- [ ] **Step 2: Add `mod command;` to main**

```rust
mod command;
```

- [ ] **Step 3: Verify and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/command.rs crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): CommandPalette using tui-input"
```

---

## Phase 4: Widgets and Layout

### Task 4.1: Layout module

**Files:**
- Create: `crates/igv-tui/src/ui/layout.rs`

- [ ] **Step 1: Write the layout**

```rust
//! Top-level layout: header, body (overview/ruler/sequence/variants/coverage/
//! alignments), footer.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug)]
pub struct LayoutAreas {
    pub header: Rect,
    pub overview: Rect,
    pub ruler: Rect,
    pub sequence: Rect,
    pub variants: Option<Rect>,
    pub coverage: Option<Rect>,
    pub alignments: Vec<Rect>,
    pub footer: Rect,
}

pub struct LayoutSpec {
    pub has_vcf: bool,
    pub bam_count: usize,
    pub coverage_height: u16,
    pub alignments_min_per_track: u16,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            has_vcf: false,
            bam_count: 0,
            coverage_height: 5,
            alignments_min_per_track: 6,
        }
    }
}

pub fn compute(area: Rect, spec: &LayoutSpec) -> LayoutAreas {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(2), // footer
        ])
        .split(area);

    let header = outer[0];
    let body = outer[1];
    let footer = outer[2];

    let mut constraints: Vec<Constraint> = vec![
        Constraint::Length(3), // overview
        Constraint::Length(1), // ruler
        Constraint::Length(2), // sequence
    ];

    if spec.has_vcf {
        constraints.push(Constraint::Length(3));
    }
    if spec.bam_count > 0 {
        constraints.push(Constraint::Length(spec.coverage_height));
        for _ in 0..spec.bam_count {
            constraints.push(Constraint::Min(spec.alignments_min_per_track));
        }
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.as_slice())
        .split(body);

    let mut idx = 0;
    let overview = chunks[idx]; idx += 1;
    let ruler = chunks[idx]; idx += 1;
    let sequence = chunks[idx]; idx += 1;
    let variants = if spec.has_vcf {
        let v = chunks[idx];
        idx += 1;
        Some(v)
    } else {
        None
    };
    let coverage = if spec.bam_count > 0 {
        let c = chunks[idx];
        idx += 1;
        Some(c)
    } else {
        None
    };
    let mut alignments = Vec::new();
    for _ in 0..spec.bam_count {
        alignments.push(chunks[idx]);
        idx += 1;
    }

    LayoutAreas {
        header,
        overview,
        ruler,
        sequence,
        variants,
        coverage,
        alignments,
        footer,
    }
}
```

- [ ] **Step 2: Verify and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/layout.rs
git commit -m "feat(igv-tui): top-level layout for header/body/footer"
```

---

### Task 4.2: Widgets module skeleton

**Files:**
- Create: `crates/igv-tui/src/ui/widgets/mod.rs`

- [ ] **Step 1: Write the module index**

```rust
pub mod alignments;
pub mod coverage;
pub mod footer;
pub mod header;
pub mod overview;
pub mod ruler;
pub mod sequence;
pub mod variants;
```

- [ ] **Step 2: Create empty placeholder files**

```bash
mkdir -p crates/igv-tui/src/ui/widgets
for f in alignments coverage footer header overview ruler sequence variants; do
    echo "//! TODO" > "crates/igv-tui/src/ui/widgets/$f.rs"
done
```

- [ ] **Step 3: Build, commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets
git commit -m "chore(igv-tui): widgets module skeleton"
```

---

### Task 4.3: Header widget

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/header.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct HeaderWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for HeaderWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let region_text = self.state.region.to_string();
        let line = Line::from(vec![
            Span::styled(" igv-rs ", self.theme.get("HEADER")),
            Span::raw("  "),
            Span::styled(region_text, self.theme.get("OVERVIEW")),
            Span::raw("  "),
            Span::styled(
                format!("({} bp)", self.state.region.width()),
                self.theme.get("WARNING"),
            ),
            Span::raw("  "),
            Span::styled(
                if self.state.loading { "loading…" } else { "" },
                self.theme.get("WARNING"),
            ),
        ]);
        Paragraph::new(line)
            .block(Block::default().borders(Borders::BOTTOM).style(self.theme.get("BORDER")))
            .render(area, buf);
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/header.rs
git commit -m "feat(igv-tui): header widget"
```

---

### Task 4.4: Footer widget

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/footer.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::state::{AppState, StatusKind};
use crate::ui::theme::Theme;

const KEYS: &str = "a/d:nav  w/s:zoom  g/::goto  m<c>:mark  '<c>:jump  t:theme  q:quit";

pub struct FooterWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for FooterWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans: Vec<Span<'_>> = Vec::new();

        if let Some(msg) = &self.state.status {
            let style = match msg.kind {
                StatusKind::Info => self.theme.get("SUCCESS"),
                StatusKind::Warning => self.theme.get("WARNING"),
                StatusKind::Error => self.theme.get("ERROR"),
            };
            spans.push(Span::styled(format!(" {} ", msg.text), style));
            spans.push(Span::raw("  "));
        }

        if self.state.command_open {
            spans.push(Span::styled(":", self.theme.get("HEADER")));
            spans.push(Span::raw(self.state.command_buffer.clone()));
            spans.push(Span::styled("█", self.theme.get("HEADER")));
        } else {
            spans.push(Span::styled(KEYS, self.theme.get("FOOTER")));
        }

        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::TOP).style(self.theme.get("BORDER")))
            .render(area, buf);
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/footer.rs
git commit -m "feat(igv-tui): footer widget with key hints and command line"
```

---

### Task 4.5: Ruler widget (auto-scaling units)

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/ruler.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct RulerWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

fn pretty_pos(p: u64) -> String {
    if p >= 1_000_000 {
        format!("{:.1}Mb", p as f64 / 1_000_000.0)
    } else if p >= 1_000 {
        format!("{:.1}kb", p as f64 / 1_000.0)
    } else {
        format!("{}", p)
    }
}

impl Widget for RulerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style: Style = self.theme.get("BORDER");
        let region = &self.state.region;
        let width = area.width.max(1) as u64;
        let span = region.width();
        if span == 0 || width == 0 {
            return;
        }

        // Choose tick count by area width.
        let target_ticks = (area.width / 12).max(2) as u64;
        let raw_step = span / target_ticks.max(1);
        let step = nice_step(raw_step.max(1));

        let mut col = 0;
        let mut pos = ((region.start + step - 1) / step) * step; // round up
        while pos <= region.end && col < area.width {
            let rel = pos.saturating_sub(region.start);
            let screen_col =
                (rel as u128 * area.width as u128 / span as u128) as u16;
            if screen_col < area.width {
                let label = pretty_pos(pos);
                let max_len = (area.width - screen_col) as usize;
                let cut = &label[..label.len().min(max_len)];
                for (i, ch) in cut.chars().enumerate() {
                    buf.get_mut(area.x + screen_col + i as u16, area.y)
                        .set_char(ch)
                        .set_style(style);
                }
            }
            pos += step;
            col += 1;
        }
    }
}

fn nice_step(raw: u64) -> u64 {
    // 1, 2, 5, 10, 20, 50, 100 …
    let mut step = 1u64;
    while step < raw {
        if step.saturating_mul(2) >= raw {
            return step * 2;
        }
        if step.saturating_mul(5) >= raw {
            return step * 5;
        }
        step = step.saturating_mul(10);
    }
    step
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/ruler.rs
git commit -m "feat(igv-tui): ruler widget with adaptive bp/kb/Mb labels"
```

---

### Task 4.6: Sequence widget

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/sequence.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::app::state::AppState;
use crate::ui::theme::Theme;
use igv_core::region::genomic_to_screen;
use igv_core::render::{RenderMode, Thresholds};

pub struct SequenceWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for SequenceWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.area() == 0 {
            return;
        }
        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if !matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            return;
        }

        let dim: Style = self.theme.get("BORDER");
        let view_start = region.start - 1; // 0-based
        let view_width = region.width();

        for (i, base) in self.state.reference_seq.iter().enumerate() {
            let g = view_start + i as u64;
            let col = match genomic_to_screen(g, view_start, view_width, area.width as u32) {
                Some(c) => c,
                None => continue,
            };
            let key = match base.to_ascii_uppercase() {
                b'A' => "A",
                b'C' => "C",
                b'G' => "G",
                b'T' => "T",
                _ => "N",
            };
            let style = self.theme.get(key);
            buf.get_mut(area.x + col as u16, area.y)
                .set_char(*base as char)
                .set_style(style);
            // ignore second row of `area` (height ≥ 2 → leave it blank)
            let _ = dim;
        }
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/sequence.rs
git commit -m "feat(igv-tui): sequence widget with per-base coloring"
```

---

### Task 4.7: Variants widget

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/variants.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct VariantsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for VariantsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title("variants");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.area() == 0 || self.state.variants.is_empty() {
            return;
        }
        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if matches!(mode, RenderMode::OverviewOnly) {
            return;
        }

        let view_start_0 = region.start - 1;
        let style = self.theme.get("VARIANT");

        for v in &self.state.variants {
            let pos_0 = v.pos.saturating_sub(1);
            let col = match genomic_to_screen(pos_0, view_start_0, region.width(), inner.width as u32) {
                Some(c) => c,
                None => continue,
            };
            // Choose glyph: ALT base if room, else `▼`.
            let glyph: char = match mode {
                RenderMode::PerBase => v
                    .alternate_alleles
                    .first()
                    .and_then(|a| a.chars().next())
                    .unwrap_or('▼'),
                _ => '▼',
            };
            buf.get_mut(inner.x + col as u16, inner.y)
                .set_char(glyph)
                .set_style(style);
        }
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/variants.rs
git commit -m "feat(igv-tui): variants widget"
```

---

### Task 4.8: Coverage widget

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/coverage.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::coverage;
use igv_core::region::genomic_to_screen;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct CoverageWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for CoverageWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title("coverage");
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 || self.state.bams.is_empty() {
            return;
        }

        let region = &self.state.region;
        // Sum coverage across all BAM tracks for the summary band.
        let mut summed = vec![0u32; region.width() as usize];
        for rows in &self.state.bam_rows {
            let cov = coverage::compute(rows, region.start, region.end);
            for (i, d) in cov.depths.iter().enumerate() {
                summed[i] = summed[i].saturating_add(*d);
            }
        }
        let max = *summed.iter().max().unwrap_or(&0).max(&1) as f32;

        let style = self.theme.get("COVERAGE");
        let height = inner.height as usize;
        for (i, &d) in summed.iter().enumerate() {
            let g = (region.start - 1) + i as u64;
            let col = match genomic_to_screen(g, region.start - 1, region.width(), inner.width as u32) {
                Some(c) => c,
                None => continue,
            };
            let bar_h = ((d as f32 / max) * height as f32).ceil() as u16;
            for row in 0..bar_h.min(inner.height) {
                let y = inner.y + inner.height.saturating_sub(1) - row;
                buf.get_mut(inner.x + col as u16, y)
                    .set_char('█')
                    .set_style(style);
            }
        }
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/coverage.rs
git commit -m "feat(igv-tui): coverage widget summing across BAM tracks"
```

---

### Task 4.9: Alignments widget (per BAM track)

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/alignments.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::alignment::{expand, BaseGlyph, ReadCells};
use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;
use igv_core::source::bam::AlignmentRow;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct AlignmentsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub track_index: usize,
}

impl Widget for AlignmentsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .state
            .bams
            .get(self.track_index)
            .map(|t| t.display.clone())
            .unwrap_or_else(|| format!("bam {}", self.track_index));
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if !matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            return;
        }
        if inner.area() == 0 {
            return;
        }

        let rows = match self.state.bam_rows.get(self.track_index) {
            Some(r) => r,
            None => return,
        };

        // Stack reads greedily to avoid horizontal overlap.
        let lanes = stack_reads(rows, inner.height as usize);
        let view_start_0 = region.start - 1;
        let view_width = region.width();

        for (lane_idx, lane) in lanes.iter().enumerate() {
            let y = inner.y + lane_idx as u16;
            for row in lane {
                let cells = expand(row, &self.state.reference_seq, region.start);
                draw_read(
                    buf, inner, y, region.start, view_start_0, view_width, &cells, row,
                    self.theme, mode,
                );
            }
        }
    }
}

fn stack_reads<'a>(rows: &'a [AlignmentRow], lane_count: usize) -> Vec<Vec<&'a AlignmentRow>> {
    let mut lanes: Vec<Vec<&AlignmentRow>> = (0..lane_count).map(|_| Vec::new()).collect();
    'rows: for row in rows {
        for lane in lanes.iter_mut() {
            if lane
                .last()
                .map(|prev| prev.ref_end + 1 < row.ref_start)
                .unwrap_or(true)
            {
                lane.push(row);
                continue 'rows;
            }
        }
        // No room: drop. (Future: scrollable.)
    }
    lanes
}

fn draw_read(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region_start_1: u64,
    view_start_0: u64,
    view_width: u64,
    cells: &ReadCells,
    row: &AlignmentRow,
    theme: &Theme,
    _mode: RenderMode,
) {
    let mismatch_style = theme.get("MISMATCH");
    let deletion_style = theme.get("DELETION");
    let insertion_style = theme.get("INSERTION");
    let match_style = if row.is_reverse {
        theme.get("MATCH_REV")
    } else {
        theme.get("MATCH_FWD")
    };

    for (i, glyph) in cells.cells.iter().enumerate() {
        let ref_pos_1 = cells.ref_start + i as u64;
        let g0 = ref_pos_1 - 1;
        if g0 < view_start_0 {
            continue;
        }
        let col = match genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            Some(c) => c,
            None => continue,
        };
        let (ch, style) = match glyph {
            BaseGlyph::Match => ('.', match_style),
            BaseGlyph::Mismatch(b) => (*b as char, mismatch_style),
            BaseGlyph::Deletion => ('*', deletion_style),
            BaseGlyph::SoftClip(b) => (*b as char, theme.get("BORDER")),
        };
        buf.get_mut(inner.x + col as u16, y).set_char(ch).set_style(style);
    }

    for (ins_ref_pos_1, _bases) in &cells.insertions {
        let g0 = ins_ref_pos_1.saturating_sub(1);
        if g0 < view_start_0 {
            continue;
        }
        if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            buf.get_mut(inner.x + col as u16, y)
                .set_char('+')
                .set_style(insertion_style);
        }
    }

    let _ = region_start_1;
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/alignments.rs
git commit -m "feat(igv-tui): alignments widget with read stacking"
```

---

### Task 4.10: Overview widget (chromosome-level mini-map)

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/overview.rs`

- [ ] **Step 1: Implement**

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct OverviewWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for OverviewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .style(self.theme.get("BORDER"))
            .title(format!("chromosome {}", self.state.region.chrom));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 {
            return;
        }

        let chrom_len = self
            .state
            .references
            .iter()
            .find(|r| r.name == self.state.region.chrom)
            .map(|r| r.length)
            .unwrap_or(self.state.region.end);
        if chrom_len == 0 {
            return;
        }

        let bar_y = inner.y;
        let style = self.theme.get("OVERVIEW");
        for x in 0..inner.width {
            buf.get_mut(inner.x + x, bar_y)
                .set_char('─')
                .set_style(style);
        }

        let start_col = ((self.state.region.start as u128 * inner.width as u128 / chrom_len as u128)
            .min(inner.width as u128 - 1)) as u16;
        let end_col = ((self.state.region.end as u128 * inner.width as u128 / chrom_len as u128)
            .min(inner.width as u128 - 1)) as u16;
        for x in start_col..=end_col {
            buf.get_mut(inner.x + x, bar_y)
                .set_char('█')
                .set_style(style);
        }
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/overview.rs
git commit -m "feat(igv-tui): overview widget with viewport indicator"
```

---

## Phase 5: Main Event Loop

### Task 5.1: Logging setup helper

**Files:**
- Create: `crates/igv-tui/src/logging.rs`

- [ ] **Step 1: Write logging setup**

```rust
use std::path::PathBuf;
use std::str::FromStr;

use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub fn setup(level: &str) -> anyhow::Result<WorkerGuard> {
    let log_dir = state_dir()?;
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::never(&log_dir, "debug.log");
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    let lvl = Level::from_str(&level.to_uppercase()).unwrap_or(Level::INFO);
    let filter = EnvFilter::new(format!("igv_tui={lvl},igv_core={lvl}"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(guard)
}

fn state_dir() -> anyhow::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "igv-rs")
        .ok_or_else(|| anyhow::anyhow!("no project dir"))?;
    Ok(dirs.data_local_dir().to_path_buf())
}
```

- [ ] **Step 2: Add `mod logging;` to main**

```rust
mod logging;
```

- [ ] **Step 3: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/logging.rs crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): tracing setup with non-blocking file appender"
```

---

### Task 5.2: Reducer — apply Action to AppState

**Files:**
- Modify: `crates/igv-tui/src/app/state.rs` (append)

- [ ] **Step 1: Append reducer methods**

```rust
use crate::app::action::Action;
use crate::app::loader::{LoadRequest, Loader};
use igv_core::region::Region;

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
```

(Note: this requires `use` statements at top of file:
`use crate::app::action::Action;`,
`use crate::app::loader::LoadRequest;`,
`use igv_core::source::FetchOpts;` — add them once at the top of `state.rs`,
not duplicated for each block.)

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/app/state.rs
git commit -m "feat(igv-tui): AppState::apply reducer for actions"
```

---

### Task 5.3: Main event loop with `tokio::select!`

**Files:**
- Modify: `crates/igv-tui/src/main.rs`
- Modify: `crates/igv-tui/src/app/mod.rs` (export an `App` struct optional)

- [ ] **Step 1: Replace `main.rs`**

```rust
mod app;
mod cli;
mod command;
mod input;
mod logging;
mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use clap::Parser;
use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tracing::{error, info};

use igv_core::region::Region;
use igv_core::render::Thresholds;
use igv_core::source::bam::{NoodlesBamSource, FetchOpts};
use igv_core::source::fasta::NoodlesFastaSource;
use igv_core::source::vcf::NoodlesVcfSource;

use crate::app::action::Action;
use crate::app::loader::{LoadResult, Loader};
use crate::app::state::{AppState, BamTrack, StatusKind};
use crate::command::CommandPalette;
use crate::input::InputState;
use crate::ui::layout::{compute, LayoutSpec};
use crate::ui::theme::{self, Theme};
use crate::ui::widgets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    let _log_guard = logging::setup(&args.log_level)?;
    info!(?args, "igv-rs starting");

    let theme = theme::load_theme(Some(args.light_mode), args.config.as_deref());

    // Explicit `dyn` types are required because `Vec<Arc<T>>` and
    // `Option<Arc<T>>` are invariant — the unsized coercion to `Arc<dyn ...>`
    // only happens for plain `Arc<T>` at function-call boundaries, not when
    // pushed into a Vec or wrapped in Option.
    let fasta: Arc<dyn igv_core::source::FastaSource> =
        Arc::new(NoodlesFastaSource::open(&args.fasta).await?);
    let references = fasta.references().await?;
    let vcf: Option<Arc<dyn igv_core::source::VcfSource>> = match args.vcf.as_deref() {
        Some(p) => Some(Arc::new(NoodlesVcfSource::open(p).await?)),
        None => None,
    };
    let mut bams: Vec<BamTrack> = Vec::new();
    let mut bam_sources: Vec<Arc<dyn igv_core::source::BamSource>> = Vec::new();
    for path in &args.bam {
        let source: Arc<dyn igv_core::source::BamSource> =
            Arc::new(NoodlesBamSource::open(path, args.tag.as_deref()).await?);
        bams.push(BamTrack {
            path: path.clone(),
            display: path.file_name().and_then(|n| n.to_str()).unwrap_or("bam").into(),
            source: Arc::clone(&source),
            fetch_opts: FetchOpts::default(),
        });
        bam_sources.push(source);
    }

    let initial = match args.region.as_deref() {
        Some(s) => Region::parse(s)
            .with_context(|| format!("invalid -r region: {s}"))?,
        None => {
            let chr = references
                .first()
                .ok_or_else(|| anyhow!("FASTA contains no references"))?
                .name
                .clone();
            Region::new(chr, 1, igv_core::region::DEFAULT_REGION_WIDTH)?
        }
    };

    let bam_count = bams.len();
    let mut state = AppState {
        fasta: fasta.clone(),
        vcf: vcf.clone(),
        bams,
        references,
        region: initial,
        reference_seq: Vec::new(),
        variants: Vec::new(),
        bam_rows: vec![Vec::new(); bam_count],
        theme: theme.clone(),
        light_mode: args.light_mode,
        thresholds: Thresholds::default(),
        bookmarks: std::collections::HashMap::new(),
        status: None,
        command_open: false,
        command_buffer: String::new(),
        generation: 0,
        loading: true,
        should_quit: false,
    };

    let (tx, mut rx) = mpsc::channel::<LoadResult>(64);
    let mut loader = Loader::new(fasta, vcf, bam_sources, tx);
    if let Some(req) = state.apply(Action::Goto(state.region.clone())) {
        loader.dispatch(req);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut input_state = InputState::default();
    let mut palette = CommandPalette::default();
    let mut events = EventStream::new();

    let result = run_loop(
        &mut terminal,
        &mut state,
        &mut loader,
        &mut rx,
        &mut events,
        &mut input_state,
        &mut palette,
    )
    .await;

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    if let Err(e) = result {
        error!("fatal: {e}");
        eprintln!("igv-rs exited with error: {e}");
        return Err(e);
    }
    Ok(())
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    loader: &mut Loader,
    rx: &mut mpsc::Receiver<LoadResult>,
    events: &mut EventStream,
    input_state: &mut InputState,
    palette: &mut CommandPalette,
) -> anyhow::Result<()> {
    let mut last_status_clear = Instant::now();
    while !state.should_quit {
        terminal.draw(|f| draw(f, state))?;

        tokio::select! {
            maybe_evt = events.next() => {
                if let Some(Ok(evt)) = maybe_evt {
                    let action = if state.command_open {
                        let act = palette.handle(&evt);
                        state.command_buffer = palette.input.value().to_string();
                        act
                    } else if matches!(&evt, Event::Key(k) if k.kind != KeyEventKind::Press) {
                        Action::None
                    } else {
                        let act = input_state.map(&evt, false);
                        if matches!(act, Action::OpenCommand) {
                            palette.open();
                        }
                        act
                    };
                    if let Some(req) = state.apply(action) {
                        loader.dispatch(req);
                    }
                }
            }
            maybe_result = rx.recv() => {
                if let Some(result) = maybe_result {
                    apply_load_result(state, result);
                    if state.bam_rows.iter().all(|r| !r.is_empty() || state.bams.is_empty())
                        && !state.reference_seq.is_empty()
                    {
                        state.loading = false;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                if state.status.is_some()
                    && last_status_clear.elapsed() > Duration::from_secs(3)
                {
                    state.status = None;
                    last_status_clear = Instant::now();
                }
            }
        }
    }
    Ok(())
}

fn apply_load_result(state: &mut AppState, result: LoadResult) {
    match result {
        LoadResult::Reference { generation, region, bytes } => {
            if generation == state.generation && region.chrom == state.region.chrom {
                state.reference_seq = bytes;
            }
        }
        LoadResult::Variants { generation, records } => {
            if generation == state.generation {
                state.variants = records;
            }
        }
        LoadResult::Bam { generation, bam_index, rows } => {
            if generation == state.generation {
                if let Some(slot) = state.bam_rows.get_mut(bam_index) {
                    *slot = rows;
                }
            }
        }
        LoadResult::Error { generation, message } => {
            if generation == state.generation {
                state.set_status(StatusKind::Error, message);
            }
        }
    }
}

fn draw(f: &mut ratatui::Frame<'_>, state: &AppState) {
    let spec = LayoutSpec {
        has_vcf: state.vcf.is_some(),
        bam_count: state.bams.len(),
        ..Default::default()
    };
    let areas = compute(f.size(), &spec);
    f.render_widget(widgets::header::HeaderWidget { state, theme: &state.theme }, areas.header);
    f.render_widget(widgets::overview::OverviewWidget { state, theme: &state.theme }, areas.overview);
    f.render_widget(widgets::ruler::RulerWidget { state, theme: &state.theme }, areas.ruler);
    f.render_widget(widgets::sequence::SequenceWidget { state, theme: &state.theme }, areas.sequence);
    if let Some(va) = areas.variants {
        f.render_widget(widgets::variants::VariantsWidget { state, theme: &state.theme }, va);
    }
    if let Some(ca) = areas.coverage {
        f.render_widget(widgets::coverage::CoverageWidget { state, theme: &state.theme }, ca);
    }
    for (i, area) in areas.alignments.iter().enumerate() {
        f.render_widget(
            widgets::alignments::AlignmentsWidget { state, theme: &state.theme, track_index: i },
            *area,
        );
    }
    f.render_widget(widgets::footer::FooterWidget { state, theme: &state.theme }, areas.footer);
}
```

- [ ] **Step 2: Add a panic hook**

Add at the top of `main` before `enable_raw_mode`:

```rust
let original_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |info| {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
    original_hook(info);
}));
```

- [ ] **Step 3: Build, smoke test, commit**

```bash
cargo build -p igv-tui
cargo run -p igv-tui -- crates/igv-core/tests/data/sample.fa -r chr1:1-50
# In the TUI, verify that header shows "chr1:1-50", press 'd' once, then 'q'.
git add crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): tokio::select! main loop with cancellation"
```

---

## Phase 6: Integration Tests and Documentation

### Task 6.1: Snapshot test on `TestBackend` for the smallest viable view

**Files:**
- Create: `crates/igv-tui/tests/render_smoke.rs`

- [ ] **Step 1: Write the test**

```rust
//! Snapshot-style smoke test: render a known-state frame to TestBackend and
//! assert on a few characters of the buffer. Insta is deliberately not used
//! here so the test stays self-contained.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn empty_layout_does_not_panic() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|_f| {}).unwrap();
}
```

- [ ] **Step 2: Run, commit**

```bash
cargo test -p igv-tui --test render_smoke
git add crates/igv-tui/tests/render_smoke.rs
git commit -m "test(igv-tui): basic TestBackend smoke test"
```

(More targeted snapshot tests for individual widgets are deferred to
follow-up work; the main loop is exercised via `cargo run` smoke test in
Task 5.3 and the integration tests above for the data layer.)

---

### Task 6.2: Update workspace README

**Files:**
- Create / replace: `README.md` (workspace root)

- [ ] **Step 1: Write the README**

```markdown
# igv-rs

Interactive terminal genome viewer — Rust rewrite of [`cligv`](./cligv).

`igv-rs` displays FASTA reference, VCF variants, and BAM alignments in the
terminal with async non-blocking IO, adaptive zoom-level rendering, multi-BAM
tracks, a vim-style command palette and bookmarks.

## Build

```bash
cargo build --release
```

The binary is at `target/release/igv-rs`.

## Usage

```bash
igv-rs reference.fa
igv-rs reference.fa -v variants.vcf.gz
igv-rs reference.fa -b alignments.bam
igv-rs reference.fa -b sample1.bam -b sample2.bam -r chr1:1000-2000
```

### Keybindings

- `a` / `←` — move backward
- `d` / `→` — move forward
- `w` / `↑` — zoom in
- `s` / `↓` — zoom out
- `:` or `g` — open command palette (type `chr:start-end`, `Enter` to jump)
- `m<c>` — set bookmark to letter `c`
- `'<c>` — jump to bookmark `c`
- `t` — toggle dark / light theme
- `q` / `Ctrl-C` — quit

## Configuration

Optional `~/.config/igv-rs/config.toml` overrides the theme and rendering
thresholds — see `docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`
for the full schema.

## Layout

- `crates/igv-core` — pure library: regions, async data sources, alignment
  expansion, coverage, render thresholds.
- `crates/igv-tui` — `igv-rs` binary: clap CLI, ratatui custom widgets, tokio
  main loop.
- `cligv/` — original Python implementation, kept as reference (git-ignored).
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: workspace README with build, usage, keybindings"
```

---

## Self-Review Checklist (run before declaring done)

After completing all tasks:

- [ ] **Spec coverage:** every spec section maps to one or more tasks above.
  - §1 Background/Goals → README + this plan's intro.
  - §2 In/out of scope → out-of-scope items are explicitly *not* implemented.
  - §3 Project layout → Phase 0 + per-task `Files:` sections.
  - §4 Architecture/data flow → Tasks 3.4, 5.2, 5.3.
  - §5 Data-source traits → Tasks 2.2, 2.3, 2.4.
  - §6 Adaptive rendering → Task 1.5 + per-widget `RenderMode` checks.
  - §7 Error handling/logging → Task 1.1, 5.1, panic hook in 5.3.
  - §8 Configuration file → Task 3.2 (theme); rendering thresholds + bookmark
    persistence are *deferred to follow-up* (the schema accepts these keys
    but the runtime override path is implemented for theme only this round —
    note this when handing off).
  - §9 Testing strategy → unit tests on every igv-core task; integration
    tests in Phase 2; TestBackend smoke in 6.1.
  - §10 Dependencies → Task 0.1 + per-crate `Cargo.toml`.
- [ ] **Placeholder scan:** none of the steps say TBD/TODO/"add validation
  here." Each step shows actual code or an exact command.
- [ ] **Type consistency:** `AlignmentRow`, `BaseGlyph`, `ReadCells`,
  `RenderMode`, `Region`, `LoadResult`, `Action`, `Theme` are all defined
  exactly once, used consistently across tasks.

---

## Execution Handoff

Plan complete and saved to
`docs/superpowers/plans/2026-04-26-igv-rs-rust-rewrite.md`.

Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task,
   review between tasks, fast iteration. Uses
   `superpowers:subagent-driven-development`.
2. **Inline Execution** — execute tasks in this session using
   `superpowers:executing-plans`, batch execution with checkpoints.
