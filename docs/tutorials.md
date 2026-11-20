# igv-rs Interactive Feature Tutorials & Video Narratives

Every feature guide below pairs an annotated **1080p narrated screen capture** (`docs/videos/*.mp4`) and **animated GIF preview** (`docs/videos/*.gif`) recorded directly from a live `igv-rs` PTY session with:

- **On-screen keystroke indicators**: every key (`d`, `a`, `Shift + C`, `:sort base`, `Tab ⇥`, `🖱 Click`, etc.) flashes live in both the floating key-caster callout and the rolling Key Trail bar.
- **Spoken & written narratives**: synchronized step-by-step commentary explaining both the terminal mechanics and the underlying genomics interpretation.
- **Reproducible demo fixtures** in [`examples/demo/`](../examples/demo/) so you can run every command verbatim.

---

## Feature Walkthrough Index

| # | Feature Area | Highlighted Keys | Animated Preview | Video & Narrative |
|---|---|---|---|---|
| 01 | **Navigation, Bookmarks & Multi-Locus Split Panes** | `d` `a` `l` `h` `w` `s` `:TP53` `m` `'` `:split` `Tab` `?` `q` | [01 GIF](videos/01-navigation-and-panes.gif) | [MP4](videos/01-navigation-and-panes.mp4) · [Narrative](narratives/01-navigation-and-panes.md) |
| 02 | **Alignment Coloring, 5mC Mods, Pairing & Sorting** | `+` `j` `k` `Shift+P` `Shift+C` `:sort base` `:group hp` `:softclip on` | [02 GIF](videos/02-alignment-analytics.gif) | [MP4](videos/02-alignment-analytics.mp4) · [Narrative](narratives/02-alignment-analytics.md) |
| 03 | **Sequence Strands, 3-Frame Translation & IUPAC Motifs** | `w` `Shift+T` `:strand both` `:translate rev` `:motif TATAWAW` | [03 GIF](videos/03-sequence-and-motifs.gif) | [MP4](videos/03-sequence-and-motifs.mp4) · [Narrative](narratives/03-sequence-and-motifs.md) |
| 04 | **Multi-Sample VCF, PED Trios, Inspector & Themes** | `l` `h` `🖱 Click` `Esc` `t` | [04 GIF](videos/04-variants-and-pedigree.gif) | [MP4](videos/04-variants-and-pedigree.mp4) · [Narrative](narratives/04-variants-and-pedigree.md) |
| 05 | **Sashimi Splice Junctions, Quantitative Signals & 3D Links** | `Shift+J` `}` `>` `\` `:summary mean` `d` `a` | [05 GIF](videos/05-sashimi-signals-and-links.gif) | [MP4](videos/05-sashimi-signals-and-links.mp4) · [Narrative](narratives/05-sashimi-signals-and-links.md) |
| 06 | **CNV Heatmaps, GWAS Manhattan & Vector/Circular Export** | `w` `d` `a` `Shift+S` `:snapshot` `:circular` | [06 GIF](videos/06-cnv-gwas-and-snapshots.gif) | [MP4](videos/06-cnv-gwas-and-snapshots.mp4) · [Narrative](narratives/06-cnv-gwas-and-snapshots.md) |
| 07 | **Live Hot-Loading, Sessions, Browser Sync & TCP Socket** | `:load` `:save-session` `Shift+B` `TCP ⇄` `:unload` | [07 GIF](videos/07-sessions-batch-and-automation.gif) | [MP4](videos/07-sessions-batch-and-automation.mp4) · [Narrative](narratives/07-sessions-batch-and-automation.md) |

---

## Reproducing Locally

After building (`cargo build --release`), run any tutorial command directly from the repository root:

```bash
# Example: Tutorial 02 (Alignments & Base Modifications)
./target/release/igv-rs examples/demo/demo.fa \
  -b examples/demo/reads.sam \
  -g examples/demo/genes.gtf \
  -r chr1:100-270

# Or regenerate all fixtures and videos with:
python3 scripts/generate_demos.py
```

Headless batch reproduction example:

```bash
./target/release/igv-rs --batch examples/demo/batch_example.igv
```
