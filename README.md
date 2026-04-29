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
igv-rs reference.fa -l loops.bedpe
igv-rs reference.fa -l hiccups.bedpe.gz -l abc.bedpe
igv-rs reference.fa -l loops.bedpe --link-min-score 5.0
igv-rs reference.fa -b sample.bam -g genes.gff3 -l loops.bedpe -r chr1:1000-2000
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

Link tracks (BEDPE pairwise interactions, `.bedpe` / `.bedpe.gz`) are
accepted via the repeatable `-l` / `--link` flag. Each file becomes
its own track showing chromatin loops, enhancer-promoter interactions,
ChIA-PET, or any other paired-region data. Visualization is adaptive:
sparse data renders as box-drawing arcs; dense data switches to a
per-column heatmap. Off-window anchors render as half-arrows with a
distance label; cross-chromosome (trans) links show a `⤴ chr2:5M`
edge marker. Override extension auto-detection with
`--link-format bedpe`. Filter low-confidence loops with
`--link-min-score N`.

### Wide-zoom behavior

You can zoom out all the way to the full chromosome (`s` / `↓` repeatedly).
At wide zoom the loader skips the heaviest fetches so chromosome-scale
views don't OOM:

| view width            | reference | reads | coverage | variants | annotations    | signals | links |
|-----------------------|-----------|-------|----------|----------|----------------|---------|-------|
| ≤ 50 kb (per-base)    | yes       | yes   | yes      | yes      | transcripts    | yes     | yes   |
| 50 kb – 500 kb        | no        | no    | no       | yes      | transcripts    | yes     | yes   |
| 500 kb – 5 Mb         | no        | no    | no       | no       | transcripts    | yes     | yes   |
| > 5 Mb (overview)     | no        | no    | no       | no       | gene density   | yes     | yes   |

The footer shows a yellow "overview" hint when fetches are gated. BigWig
signal tracks remain visible at every zoom level — bigtools' precomputed
zoom pyramid handles chromosome-scale queries cheaply.

### Snapshot export (SVG / PNG)

Save publication-style figures of the current view or batches of regions
/ genes. Snapshots are graphical, not character art — matching IGV's
PNG / SVG output style.

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
`<label>_<chrom>_<start>_<end>.<ext>` (with `<label>` from the BED 4th
column or the gene name when set).

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
- `<` / `>` — shrink / grow link-track height
- `:` or `g` — open command palette (type `chr:start-end`, a chromosome
  name, or a `gene_name` / `gene_id` / `transcript_id` from a loaded
  annotation; `Enter` to jump)
- `m<c>` — set bookmark to letter `c`
- `'<c>` — jump to bookmark `c`
- `t` — cycle theme (`dark` → `light` → `paper` → `solarized-dark` →
  `solarized-light` → `dracula` → `gruvbox-dark` → ...). The new theme name
  briefly shows in the footer. `paper` paints every cell with an explicit
  white background — useful on terminals whose default background isn't
  pure white.
- `S` — save SVG snapshot of current view to
  `./snapshot_<chrom>_<s>_<e>.svg` (see "Snapshot export" above)
- `?` — toggle keybinding help overlay (any key closes it)
- `q` / `Ctrl-C` — quit

## Configuration

Optional `~/.config/igv-rs/config.toml` is read at startup. Today only the
`[theme]` section is honored:

```toml
[theme]
# "dark" | "light" | "paper" | "solarized-dark" | "solarized-light"
# | "dracula" | "gruvbox-dark"
preset = "dark"

[theme.custom]
# Override individual style keys
"A" = "bold green"
"MISMATCH" = "bold white on red"
"SIGNAL" = "cyan"
"LINK" = "magenta"
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
- `crates/igv-core/src/source/link.rs` — `LinkSource` trait + `BedpeLinkSource`
  in-memory IntervalMap backend.
- `crates/igv-tui/src/ui/widgets/link.rs` — adaptive arc / heatmap widget.
- `crates/igv-render/src/svg/link.rs` — SVG painter (Bézier arcs +
  viridis-like color ramp).
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
- **Coverage at wide zoom is hidden, not heat-mapped.** Beyond the
  `detailed` threshold (50 kb by default) BAM is no longer fetched, so the
  coverage track shows just a `coverage (zoomed out)` title. Use a
  precomputed bigWig if you need a chromosome-scale depth view.
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
- **No tabix / pairix support for link tracks.** BEDPE files >1M
  records load slowly (whole-file in-memory parse). Workaround:
  pre-filter the file. Tracked as a follow-up spec.
- **No bigInteract / UCSC interact format.** Separate spec; will reuse
  the same `LinkSource` trait and `LinkRecord` shape.
- **Single `LINK` theme key.** All link tracks share one base color;
  per-track palette is deferred.
- **Score column fixed to BEDPE col 8.** `--link-score-col` not yet
  available for files using a custom scoring column.
- **Per-window score normalization.** Adjacent panning frames may
  show slightly shifted colors for the same link as the visible
  score range moves. Acceptable for a no-config UX.

## Reference

- Initial design spec: `docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`
- Initial implementation plan: `docs/superpowers/plans/2026-04-26-igv-rs-rust-rewrite.md`
- Annotation track design: `docs/superpowers/specs/2026-04-26-annotations-design.md`
- Annotation track plan: `docs/superpowers/plans/2026-04-26-annotations.md`
- bigWig signal-track design: `docs/superpowers/specs/2026-04-27-bigwig-signal-design.md`
- BEDPE link-track design: `docs/superpowers/specs/2026-04-29-bedpe-link-design.md`
- BEDPE link-track plan: `docs/superpowers/plans/2026-04-29-bedpe-link.md`
