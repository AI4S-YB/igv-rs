# igv-rs Design Spec — Rust Rewrite of cligv

**Date:** 2026-04-26
**Status:** Approved (brainstorming phase)
**Author:** Brainstorming session with user

## 1. Background and Goals

`cligv` (command line Interactive Genome Viewer) is an existing Python CLI tool
(~1,900 lines of unique source) that renders FASTA reference, VCF variants, and
BAM alignments interactively in the terminal. It uses `pysam` for genome file IO
and `rich` for terminal rendering.

This project, `igv-rs`, rewrites it in Rust as a redesign rather than a 1:1
port. User-confirmed redesign priorities, in order:

1. **Architecture & performance** — async IO, non-blocking UI, prefetch and
   cancel support
2. **Visual readability** — adaptive rendering by zoom level, configurable
   themes, clearer ruler
3. **Feature extensions** — multiple BAM tracks, command palette, bookmarks
4. **Interaction model** — kept close to the original (single-key navigation);
   no mouse / draggable panels / mode-switching this round

The Python source under `cligv/` is preserved as a reference implementation and
is not modified.

## 2. Scope

### In scope

- All of cligv's core capabilities: FASTA / VCF / BAM tracks; header / ruler /
  sequence / variants / coverage / alignments layout; dark and light themes;
  BAM tag-based read coloring; keyboard navigation (`a/d/w/s/g/t/q`).
- New capabilities driven by priorities 4 and 2:
  - Ruler with auto-scaling tick marks and unit suffix (bp / kb / Mb).
  - Theme externalized to `~/.config/igv-rs/config.toml` (preset + overrides).
  - **Adaptive rendering strategy** keyed on view width (see §6).
  - **Multiple BAM tracks** (`-b a.bam -b b.bam`), each in its own panel.
  - **Command palette** triggered by `:` (vim-style); supersedes the original
    `g` prompt for region entry but `g` remains as a shortcut.
  - **Bookmarks** with vim-style `m<char>` to set and `'<char>` to jump;
    persistent named bookmarks loadable from config.

### Explicitly out of scope (this iteration)

- Mouse interactions (drag, scroll, click-to-navigate).
- Resizable / re-orderable panels.
- GFF / BED annotation tracks.
- Sequence motif / k-mer search.
- Plugin or scripting system.
- Distribution beyond `cargo install` (no conda / homebrew / AUR yet).
- Windows-specific terminal compatibility testing (crossterm is cross-platform,
  but Linux and macOS are the primary supported targets).
- Two-way migration tooling between Python `cligv` and `igv-rs`.

## 3. Project Layout

```
/home/xzg/project/igv_rs/
├── cligv/                       # Reference Python implementation, unchanged
├── crates/
│   ├── igv-core/                # Library crate: genome IO, region, state
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── region.rs        # Region type, parsing, coordinate transforms
│   │   │   ├── source/          # Async data-source traits and impls
│   │   │   │   ├── mod.rs
│   │   │   │   ├── fasta.rs
│   │   │   │   ├── bam.rs
│   │   │   │   └── vcf.rs
│   │   │   ├── alignment.rs     # CIGAR expansion, character-level rows
│   │   │   ├── coverage.rs      # Coverage track computation
│   │   │   ├── render.rs        # Rendering thresholds, mode selection
│   │   │   └── error.rs
│   │   └── tests/               # Unit + integration tests with golden samples
│   └── igv-tui/                 # Binary crate: ratatui UI + main loop
│       ├── src/
│       │   ├── main.rs
│       │   ├── cli.rs           # clap argument parsing
│       │   ├── app/
│       │   │   ├── mod.rs
│       │   │   ├── state.rs     # AppState, bookmarks, navigation history
│       │   │   ├── action.rs    # Action enum (Move/Zoom/Goto/...)
│       │   │   └── loader.rs    # tokio fetch tasks (cancellable)
│       │   ├── ui/
│       │   │   ├── mod.rs
│       │   │   ├── layout.rs
│       │   │   ├── theme.rs     # TOML loader, default presets, overrides
│       │   │   └── widgets/
│       │   │       ├── header.rs
│       │   │       ├── footer.rs
│       │   │       ├── overview.rs    # Chromosome-level mini-map
│       │   │       ├── ruler.rs
│       │   │       ├── sequence.rs
│       │   │       ├── variants.rs
│       │   │       ├── coverage.rs
│       │   │       └── alignments.rs  # One panel per BAM
│       │   ├── input.rs         # crossterm Event -> Action mapping
│       │   └── command.rs       # Command palette state
│       └── tests/
├── Cargo.toml                   # workspace
├── docs/
│   └── superpowers/specs/       # Spec documents
└── README.md
```

Rationale for splitting into a workspace:

- `igv-core` has no UI dependency, which keeps unit tests fast and makes the
  data layer reusable.
- `igv-tui` only concerns itself with rendering, input, and async glue, so
  changes to the UI never force recompilation of the IO layer.

## 4. Architecture and Data Flow

The runtime is a single-owner async application: one Tokio task owns
`AppState`, and side effects (file IO) happen in spawned tasks that send
results back through channels.

```
┌──────────────┐      Action      ┌──────────────┐
│ Input source │ ───────────────→ │              │
│ (crossterm   │                  │   AppState   │
│  EventStream)│ ←─── redraw ──── │  (single     │
└──────────────┘                  │   owner)     │
                                  │              │
┌──────────────┐  LoadResult      │              │
│ Loader tasks │ ───────────────→ │              │
│ (tokio       │                  │              │
│  spawn,      │ ←── LoadRequest ─│              │
│  cancellable)│                  └──────┬───────┘
└──────────────┘                         │
                                  draw   │
                                         ↓
                                  ┌──────────────┐
                                  │   ratatui    │
                                  │   render     │
                                  └──────────────┘
```

Key behaviors:

- **Main loop:** `tokio::select!` listens to the `crossterm` event stream and a
  `mpsc::Receiver<LoadResult>` simultaneously. The render pass runs after any
  state-changing message.
- **Event to Action translation:** keypresses become typed `Action` variants
  (`MoveLeft`, `Zoom(Direction)`, `Goto(Region)`, `SetBookmark(char)`,
  `JumpBookmark(char)`, `ToggleTheme`, `OpenCommand`, `Quit`, ...).
- **Single owner:** all mutable state lives on the main task. Loader tasks
  receive immutable `LoadRequest` values and return `LoadResult` values.
- **Cancellation:** when a new fetch supersedes an older one (e.g., the user
  zooms while a previous fetch is in flight), the older `JoinHandle` is
  aborted. Each in-flight request also carries a generation counter so any
  late-arriving result that does not match the current generation is dropped.
- **Debounce:** rapid repeated keys (held `d`, `s`, etc.) are coalesced via a
  small idle window (about 30 ms) before issuing a fetch.
- **Prefetch:** the loader / cache layer exposes hooks for prefetching
  adjacent regions, but actual prefetching (and the LRU cache backing it) is
  deferred to a follow-up iteration. The first cut renders on demand and
  relies on cancellation + debounce to keep the UI responsive.

## 5. Data-Source Trait Design

`igv-core::source` defines async traits so the UI layer is decoupled from the
underlying file format. This also enables fakes for testing.

```rust
#[async_trait]
pub trait FastaSource: Send + Sync {
    async fn references(&self) -> Result<Vec<RefMeta>>;
    async fn fetch(&self, region: &Region) -> Result<Vec<u8>>;
}

#[async_trait]
pub trait BamSource: Send + Sync {
    async fn fetch(
        &self,
        region: &Region,
        opts: &FetchOpts,
    ) -> Result<Vec<AlignmentRow>>;
}

#[async_trait]
pub trait VcfSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>>;
}
```

- Concrete impls wrap `noodles-fasta`, `noodles-bam` (with `noodles-bai` /
  `noodles-csi` for indexing), and `noodles-vcf` (with `noodles-tabix`).
- CIGAR expansion, mismatch annotation against the reference, and tag-based
  styling all live in `igv-core::alignment` as pure functions consuming bytes
  and references. The UI receives ready-to-render `AlignmentRow` values.

## 6. Adaptive Rendering Strategy

To keep large-region views readable and fast, the renderer chooses a strategy
based on `view_width = end - start + 1`. Defaults shown below; thresholds
configurable in `config.toml`.

| view_width        | sequence track     | variants track       | alignments track                       |
|-------------------|--------------------|----------------------|----------------------------------------|
| ≤ 200 bp          | Per-base letters   | ALT letter shown     | Character-level reads, full CIGAR      |
| 201 – 1,000       | Per-base letters   | REF→ALT marker       | Character-level reads; matches as `.`  |
| 1,001 – 10,000    | Hidden             | `▼` density markers  | Coverage only (alignments hidden)      |
| 10,001 – 100,000  | Hidden             | Density heat bar     | Coverage with reduced-resolution bins  |
| > 100,000         | Hidden             | Hidden               | Overview thumbnail + warning toast     |

Threshold constants live in `igv-core::render::thresholds` and are overridable
through the configuration file (see §8).

## 7. Error Handling and Logging

- Top-level uses `anyhow::Result` for error chaining. `igv-core` defines
  `IgvError` variants with `thiserror` so library callers can match on cause.
- User-facing errors (missing file, missing index, unknown chromosome) surface
  as a footer toast; the application does not exit.
- A `panic::set_hook` resets the terminal (disables raw mode, leaves alternate
  screen) before printing the panic so the user is never left with a broken
  terminal.
- Logging uses the `tracing` ecosystem with a non-blocking file appender.
  Logs go to `$XDG_STATE_HOME/igv-rs/debug.log` (default
  `~/.local/state/igv-rs/debug.log`). `--log-level` controls verbosity.

## 8. Configuration File

Path: `~/.config/igv-rs/config.toml` (resolved with the `directories` crate so
the right XDG path is used on each platform).

```toml
[theme]
preset = "dark"  # "dark" | "light" | "custom"

[theme.custom]
# Any keys here override the chosen preset
"A" = "bold green"
"MISMATCH_STYLE" = "bold white on red"

[render]
zoom_factor = 1.5
nav_overlap = 0.5
sequence_threshold = 200
detailed_threshold = 1000
coverage_only_threshold = 10000
heat_threshold = 100000

[bookmarks]
# Persistent named bookmarks
goi = "chr7:140000000-140100000"
```

Resolution order (lowest to highest precedence): built-in defaults → user
config file → CLI flags. The merge is a deep merge for nested tables, scalar
overwrite for leaves.

## 9. Testing Strategy

- **Unit tests** (in `igv-core`):
  - Region parsing (covers the same input grammar as Python `parse_region`,
    plus comma stripping and edge cases).
  - Coordinate transforms (`genomic_to_screen` / `screen_to_genomic`) for the
    full range of view widths and screen widths.
  - CIGAR expansion against known inputs, including soft clips, insertions,
    deletions, mismatch detection against reference.
  - Coverage computation determinism on fixed inputs.
- **Integration tests** (in `igv-tui/tests`):
  - `ratatui::backend::TestBackend` snapshot tests for representative views:
    a small per-base region with one BAM, a coverage-only large region, an
    empty-BAM region, multiple-BAM layout.
  - These assert on the rendered character buffer, not on style escape codes,
    to keep tests robust against theme changes.
- **Golden samples**: small FASTA / BAM / VCF / index files committed under
  `crates/igv-core/tests/data/`, sized to keep the repository light. Sourced
  from public test datasets used by `noodles` and `samtools`.
- **TDD discipline**: implementation work follows the
  `superpowers:test-driven-development` skill. The writing-plans phase
  expands this into per-step red/green/refactor loops.

## 10. Dependencies

```toml
# igv-core
noodles = { version = "0.x", features = [
    "fasta", "bam", "vcf", "csi", "tabix", "async",
] }
tokio = { version = "1", features = [
    "fs", "io-util", "rt-multi-thread", "macros", "sync", "time",
] }
async-trait = "0.1"
thiserror = "1"
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# igv-tui
ratatui = "0.x"
crossterm = { version = "0.x", features = ["event-stream"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tracing = "0.1"
tracing-appender = "0.2"
directories = "5"
tui-input = "0.x"
```

Exact `noodles`, `ratatui`, `crossterm`, and `tui-input` versions are pinned
during the writing-plans phase by inspecting `crates.io` at implementation
start.

## 11. Open Items Resolved

- Workspace with two crates (`igv-core`, `igv-tui`) — confirmed.
- Feature additions (multiple BAM tracks, command palette, bookmarks) — all
  in scope.
- Adaptive-rendering thresholds — accepted as proposed.
- Configuration path and format — accepted as proposed.

## 12. Next Step

Once this spec is reviewed and approved, the work moves to the
`superpowers:writing-plans` skill to produce a per-step implementation plan
with TDD checkpoints.
