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
igv-rs reference.fa -s chip.bw -s input.bw -r chr1:1-10000000
igv-rs reference.fa -b sample.bam -s rna.bw -g genes.gff3 -r chr1:1000-2000
```

The command palette (`:` or `g`) accepts coordinate input
(`chr1:1000-2000`, `chr1`) **and** gene names — type a `gene_name`,
`gene_id`, or `transcript_id` from any loaded GFF/GTF/BED track and the
view jumps to the union span of all matching transcripts (case-insensitive).
For multi-isoform genes the window covers every isoform on the same
chromosome at once.

Annotation tracks are auto-detected by extension:
`.gff` / `.gff3` / `.gtf` (with optional `.gz`), `.bed` / `.bed.gz`, and
MACS2-style `.narrowPeak` / `.broadPeak` (with optional `.gz`) are all
accepted via the repeatable `-g` / `--annotation` flag. Peak files are
treated as BED6 — the extra signal/p-value/q-value/peak columns are
ignored. Override the auto-detection with
`--annotation-format gff|gtf|bed|narrowpeak|broadpeak` when the extension
is ambiguous or missing.

Signal tracks (bigWig, `.bw` / `.bigwig`) are accepted via the repeatable
`-s` / `--signal` flag and rendered as bar-chart tracks between coverage
and alignments. At wide zoom the bigwig file's precomputed zoom-level
summaries are used (≥16 bp/col); at fine zoom raw per-base values are
fetched. Override extension auto-detection with
`--signal-format bigwig`.

### Keybindings

- `a` / `←` — page backward (one full window)
- `d` / `→` — page forward (one full window)
- `h` — move backward by 1/10 of the window (fine pan)
- `l` — move forward by 1/10 of the window (fine pan)
- `w` / `↑` — zoom in
- `s` / `↓` — zoom out
- `j` / `k` — scroll alignment lanes down / up
- `+` / `-` — grow / shrink alignment-track height
- `]` / `[` — grow / shrink coverage-track height
- `\` — toggle signal shared / per-track Y-scale
- `}` / `{` — grow / shrink signal-track height
- `:` or `g` — open command palette (type `chr:start-end`, a chromosome
  name, or a `gene_name` / `gene_id` / `transcript_id` from a loaded
  annotation; `Enter` to jump)
- `m<c>` — set bookmark to letter `c`
- `'<c>` — jump to bookmark `c`
- `t` — toggle dark / light theme
- `?` — toggle keybinding help overlay (any key closes it)
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
"SIGNAL" = "cyan"
```

The full schema (with `[render]` and `[bookmarks]` tables) is described in
`docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`. See "Known
limitations" below for which sections are wired up.

## Layout

- `crates/igv-core` — pure library: regions, async data sources, alignment
  expansion, coverage, render thresholds.
- `crates/igv-tui` — `igv-rs` binary: clap CLI, ratatui custom widgets, tokio
  main loop.
- `crates/igv-core/src/source/signal.rs` — `SignalSource` trait + bigtools-backed `BigWigSignalSource`.
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
- **No signal-track caching** — every region change re-fetches bigwig.
  In practice bigtools' R-tree lookup is fast enough; revisit if it's
  ever observed to lag.
- **Signal-track vertical quantization.** v0.4 paints partial blocks
  (`▁▂▃▄▅▆▇█`) so each terminal row resolves 8 levels — at the default 6
  rows that's 48 vertical levels. For very high-dynamic-range bigwigs
  (or low-amplitude regions next to a tall peak) bars can still look
  stepped. Workaround: grow the track with `}` for more rows × 8 levels
  each. (v0.3 and earlier rendered only `█`, i.e. 1 level per row.)
- **Single signal colormap** — all bigwig tracks share the `SIGNAL`
  theme key. Per-track colormap is not yet supported.
- **Signal summary statistic** is fixed at `Max`. `--signal-summary` is
  not yet a flag.
- **bigBed (`.bb`)** is not supported — separate spec.

## Reference

- Initial design spec: `docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`
- Initial implementation plan: `docs/superpowers/plans/2026-04-26-igv-rs-rust-rewrite.md`
- Annotation track design: `docs/superpowers/specs/2026-04-26-annotations-design.md`
- Annotation track plan: `docs/superpowers/plans/2026-04-26-annotations.md`
- bigWig signal-track design: `docs/superpowers/specs/2026-04-27-bigwig-signal-design.md`
