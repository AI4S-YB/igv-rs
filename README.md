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
