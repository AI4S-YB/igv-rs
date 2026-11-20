# igv-rs

Interactive terminal & headless genome viewer written in Rust, supporting local files and HTTP/S3/GS remote byte-range streaming across alignments, variants, annotations, quantitative signals, chromatin interactions, CNV segments, and GWAS Manhattan tracks.

Inspired by [cligv](https://github.com/jonasfreudig/cligv) and [IGV](https://igv.org).

## Install & Build

```bash
cargo install --path crates/igv-rs
# or build from source:
cargo build --release
```

The release binary is placed at `target/release/igv-rs`.

## Supported Formats & Remote Streaming

- **References & Genomes (`--genome` or positional)**:
  - Indexed FASTA (`.fa` / `.fasta` + `.fai`, local or remote HTTP/S3/GS)
  - UCSC `.2bit` random-access genomes
  - Built-in genome presets (`hg38`, `hg19`, `mm39`, `mm10`, `danRer11`, `dm6`, `ce11`, `sacCer3`) or custom `.json` genome descriptors with cytoband ideograms and `1` ↔ `chr1` chromosome aliases
  - UCSC Track Hubs (`--hub hub.txt`, supporting both `useOneFile on` and `genomes.txt` / `trackDb.txt`)
- **Alignments (`-b` / `--bam`)**: `BAM` (`.bai`/`.csi`), `CRAM 3.0+` (`.crai`/sequential), `SAM`, and `PAF`
- **Variants (`-v` / `--vcf`, repeatable)**: multi-sample `VCF` (`.vcf.gz` + `.tbi`/`.csi` or unindexed `.vcf`) and `BCF 2.2` (`.bcf`), with optional `PED` (`--ped`) trio ordering and Mendelian violation detection
- **Annotations (`-g` / `--annotation`)**: `GFF3`, `GTF`, `BED3`–`BED12`, `narrowPeak`/`broadPeak` (with `.tbi`/`.csi` tabix streaming or gzipped fallback), and UCSC `bigBed` (`.bb` / `bigGenePred`)
- **Quantitative Signals (`-s` / `--signal`)**: `bigWig` (`.bw`/`.bigwig`), `WIG` (`fixedStep`/`variableStep`), and `bedGraph` (with `--signal-summary max|mean|min|sum`)
- **Chromatin / SV Links (`-l` / `--link`)**: `BEDPE` (`.bedpe`/`.bedpe.gz` with optional `.tbi`/`.csi`) and UCSC `bigInteract` (`.inter.bb`)
- **CNV & Association Tracks**: Copy-number segmentation (`--seg`: `.seg`/`.cbs`/`.cn`) diverging heatmaps and GWAS Manhattan plots (`--gwas`: `.gwas`/`.assoc`)

Remote URLs (`http://`, `https://`, `s3://`, `gs://`) stream via a 64 KiB LRU page-cached HTTP range client with optional bearer tokens (`--auth-token`).

## Usage Examples

```bash
# Built-in hg38 preset + BAM/VCF/GFF tracks across two split panes
igv-rs --genome hg38 -b sample.bam -v cohort.vcf.gz -g genes.gtf \
  -r chr7:55000000-55200000 -r chr17:7668402-7687538

# RNA-seq Sashimi splice junctions + base modification coloring + PED Mendelian check
igv-rs ref.fa -b ont_5mc.bam -b rnaseq.bam -v trio.vcf.gz --ped trio.ped \
  --color-by mod --sashimi --show-soft-clips

# Copy-number CNV heatmap + GWAS Manhattan + Circular SV chord SVG export
igv-rs --genome hg19 --seg cnv.seg --gwas stats.gwas -l sv.bedpe \
  --circular-out whole_genome_circos.svg

# Load saved session (.toml or IGV .json) + enable live TCP command listener
igv-rs --session session.toml --port 60151

# Run an IGV batch script headlessly (renders SVG/PNG/Circular snapshots and exits)
igv-rs --batch run.igv
```

## Keybindings & Mouse Controls

### Navigation & Split Panes
- `a` / `←` / `d` / `→` — page backward / forward (full window)
- `h` / `l` — fine pan backward / forward (1/10 window)
- `w` / `↑` / `s` / `↓` — zoom in / out around center
- `Tab` — cycle active multi-locus split pane (`1..=4`)
- `m<c>` / `'<c>` — set / jump to bookmark `<c>`
- `Mouse` — click/drag on ruler or ideogram to jump/drag-zoom; scroll wheel to scroll lanes; click any read, variant, transcript, or GWAS marker to open the modal inspector (`Esc`/`Enter`/`q` closes)

### Alignments, Sequence & Visualization Toggles
- `j` / `k` — scroll alignment lanes down / up
- `P` — toggle View-as-Pairs (mates grouped on the same lane with connector lines)
- `C` — cycle alignment coloring (`strand` → `insert-size` → `pair-orientation` → `mate-chrom` → `tag` → `mapq` → `base-mod` [`MM`/`ML` 5mC/6mA] → `bisulfite`)
- `J` — toggle RNA-seq Sashimi splice-junction arc tracks
- `T` — toggle 3-frame amino-acid sequence translation (`M` start, `*` stop)
- `G` — render inline pixel snapshot (`Kitty` / `iTerm2` / `Sixel`)
- `+` / `-`, `]` / `[`, `}` / `{`, `<` / `>` — resize alignment, coverage, signal, and link track heights
- `\` — toggle shared vs per-track signal Y-scale
- `t` — cycle color themes (`dark`, `light`, `paper`, `solarized-*`, `dracula`, `gruvbox-*`)
- `S` — export graphical SVG snapshot of the current view
- `B` — open bidirectional `igv.js` browser companion (navigating in either TUI or browser syncs the other)
- `?` — toggle help overlay; `q` / `Ctrl-C` — quit

### Runtime `:` Command Palette
- `:chr7:55000000-55200000` or `:<GENE>` — jump to coordinates, aliases, or gene/transcript IDs
- `:sort <none|start|base|strand|mapq|insert|mate|tag|hp>` — re-sort alignments live
- `:group <none|strand|rg|hp|pair|mate|tag|chimeric>` — group reads with band headers
- `:color <strand|insert|pair|mate|tag|mapq|mod|bisulfite> [TAG]` — set coloring & BAM tag
- `:pairs [on|off]`, `:softclip [on|off]`, `:downsample <N|off>` — toggle pairing, soft-clips, reservoir depth
- `:summary <max|mean|min|sum>` — set signal window aggregation
- `:strand [both|fwd]`, `:translate [fwd|rev|off]`, `:motif <IUPAC|off>` — complement strand, 3-frame AA, IUPAC regex motif search
- `:sashimi [on|off]`, `:circular <path.svg|path.png>`, `:snapshot <path.svg|path.png>` — toggle Sashimi & export graphics
- `:split <locus>`, `:unsplit` — add/close multi-locus split panes (up to 4)
- `:load <path_or_url>`, `:unload <name>`, `:genome <id_or_path>` — dynamically load/unload tracks & genomes
- `:save-session <path.toml>`, `:load-session <path.toml|path.json>` — persist or import IGV sessions
- `:blat <DNA_SEQ>` — query UCSC `hgBlat` and jump to the top hit
- `:graphics <auto|none|kitty|iterm2|sixel|toggle>` — configure inline pixel output

## Architecture

- `crates/igv-core` — pure async data layer (`io`, `genome`, `sequence`, `alignment/{mod,mods}`, `coverage`, `pedigree`, `sashimi`, `hub`, and sources for FASTA/`.2bit`, BAM/CRAM/SAM/PAF, VCF/BCF, GFF/GTF/BED/bigBed, bigWig/WIG/bedGraph, BEDPE/bigInteract, SEG, GWAS).
- `crates/igv-render` (`#![forbid(unsafe_code)]`) — SVG & PNG snapshot engine, Circos-style whole-genome circular chord renderer (`circview`), and pure-Rust Kitty / iTerm2 / Sixel terminal pixel encoders (`term_gfx`).
- `crates/igv-serve` (`#![forbid(unsafe_code)]`) — bundled `igv.js` HTTP server with SSE push (`view`), reverse browser → TUI locus sync (`POST /api/locus`), and range/feature routes.
- `crates/igv-rs` — CLI, multi-pane `ratatui` TUI with mouse capture & modal inspector, runtime `:` command engine, session manager, `--batch` script runner, and `--port` TCP automation listener.

## Citation

If you use this software, please cite:

Xu ZG, Qin G. 2026. LLM-assisted development of Rust for high-performance bioinformatics software: practices, workflows, and boundaries. *Genomics Communications* 3: e018 doi: [10.48130/gcomm-0026-0018](https://doi.org/10.48130/gcomm-0026-0018)
