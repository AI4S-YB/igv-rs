# igv-rs

Interactive terminal genome viewer for FASTA / VCF / BAM / GFF / BED, written in Rust.
Inspired by [cligv](https://github.com/jonasfreudig/cligv) by Jonas Freudigmann.

`igv-rs` renders the reference sequence, variants, read alignments, and gene /
region annotations side-by-side in the terminal, with async non-blocking IO,
adaptive zoom-level rendering, multi-track support, a vim-style command palette
and bookmarks.

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
igv-rs reference.fa -g genes.gff3
igv-rs reference.fa -g genes.gff3 -g peaks.bed -b sample.bam -r chr1:1000-2000
```

Annotation tracks are auto-detected by extension:
`.gff` / `.gff3` / `.gtf` (with optional `.gz`) and `.bed` / `.bed.gz` are all
accepted via the repeatable `-g` / `--annotation` flag. Override the
auto-detection with `--annotation-format gff|gtf|bed` when the extension is
ambiguous or missing.

### Keybindings

- `a` / `←` — move backward by 1/10 of the window (fine step)
- `d` / `→` — move forward by 1/10 of the window
- `A` — page backward (one full window)
- `D` — page forward (one full window)
- `w` / `↑` — zoom in
- `s` / `↓` — zoom out
- `j` / `k` — scroll alignment lanes down / up
- `+` / `-` — grow / shrink alignment-track height
- `]` / `[` — grow / shrink coverage-track height
- `:` or `g` — open command palette (type `chr:start-end`, `Enter` to jump)
- `m<c>` — set bookmark to letter `c`
- `'<c>` — jump to bookmark `c`
- `t` — toggle dark / light theme
- `q` / `Ctrl-C` — quit

## Configuration

Optional `~/.config/igv-rs/config.toml` is read at startup. Today only the
`[theme]` section is honored:

```toml
[theme]
preset = "dark"  # "dark" | "light"

[theme.custom]
# Override individual style keys
"A" = "bold green"
"MISMATCH" = "bold white on red"
```

The full schema (with `[render]` and `[bookmarks]` tables) is described in
`docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`. See "Known
limitations" below for which sections are wired up.

## Layout

- `crates/igv-core` — pure library: regions, async data sources, alignment
  expansion, coverage, render thresholds.
- `crates/igv-tui` — `igv-rs` binary: clap CLI, ratatui custom widgets, tokio
  main loop.
- `cligv/` — the project that inspired this work; kept locally as a reference
  and not part of this repository (git-ignored).

## Known limitations

The 0.1 release ships the architectural backbone described in the spec; some
configuration knobs and refinements are tracked for follow-up:

- **Held-key debounce** is not implemented. Holding `d` or `s` issues one
  fetch per keystroke; cancellation reduces the load but does not eliminate
  it. Workaround: tap rather than hold.
- **`[render]` config keys** (`zoom_factor`, `nav_overlap`, threshold
  overrides) are not read yet. Hardcoded defaults match the spec.
- **`[bookmarks]` config table** is not loaded. In-session bookmarks via
  `m<c>` / `'<c>` work fully.
- **Coverage widget at very wide zoom** still renders full-resolution bars
  rather than the heat-bar mode described in spec §6.
- **BAM tag display** uses Rust's `Debug` formatting (e.g. `Int8(42)` instead
  of `42`) when colored by tag.
- **Snapshot tests** of full TUI frames are limited to a smoke test; widget
  rendering is exercised manually.

## Reference

- Initial design spec: `docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`
- Initial implementation plan: `docs/superpowers/plans/2026-04-26-igv-rs-rust-rewrite.md`
- Annotation track design: `docs/superpowers/specs/2026-04-26-annotations-design.md`
- Annotation track plan: `docs/superpowers/plans/2026-04-26-annotations.md`
