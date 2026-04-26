# GFF / GTF / BED Annotation Support — Design Spec

**Date:** 2026-04-26
**Status:** Approved (brainstorming phase)
**Project:** igv-rs (terminal genome viewer)

## 1. Background and Goals

`igv-rs` ships with FASTA, VCF, and BAM tracks. The most-requested next
capability for any genome viewer is annotation: showing where genes,
transcripts, and arbitrary named regions sit relative to the reference and
the alignments. This spec adds that capability for the three dominant
formats — GFF3, GTF, and BED — through a single CLI flag.

The work also clarifies the project's positioning. `igv-rs` is **inspired
by** [cligv](https://github.com/jonasfreudig/cligv) (Jonas Freudigmann), not
a rewrite of it. The original Python implementation under `cligv/` is kept
locally as a reference but is git-ignored and not redistributed.

User-confirmed redesign decisions during brainstorming:

- **Rendering style:** transcript-expanded — every mRNA / BED feature gets
  its own row inside the annotation track. (Alternatives considered: flat
  collapsed; intelligent per-zoom switching. Rejected for the reasons
  recorded in §11.)
- **CLI surface:** unified `-g/--annotation` flag, repeatable, format
  detected by file extension. Mirrors the existing `-b` BAM convention.

## 2. Scope

### In scope

- Parse and display **GFF3** features with parent/child hierarchy
  (`gene → mRNA → exon/CDS/UTR`).
- Parse and display **GTF** features (a GFF2 dialect that `noodles-gff`
  reads with the same parser).
- Parse and display **BED** features (BED3 through BED12; BED12 block
  starts/sizes contribute to the rendered transcript blocks).
- Multiple annotation files in a single session (`-g a.gff -g b.bed`),
  rendered as independent stacked tracks above the variants band.
- Adaptive rendering tied to the existing `RenderMode` ladder: hidden in
  `OverviewOnly`, density bar in `HeatBar`, transcript rows otherwise.
- Stale-data clearing on navigation, mirroring the bug fix already applied
  to `bam_rows` and `reference_seq`.
- Project repositioning in the README: "inspired by cligv" framing
  throughout.

### Explicitly out of scope (this iteration)

- GFF `phase` (CDS reading frame) glyphs.
- Attribute-driven coloring (e.g. `gene_biotype=protein_coding` →
  different color).
- BedGraph continuous-value display (that is coverage-style, not
  annotation-style; would belong in a separate track).
- Annotation hover / detail panel — terminal UI does not support hover
  cleanly without a custom popup.
- Interactive transcript collapse / expand toggling.
- Custom feature filters (`--include-type CDS,exon` etc.).

## 3. Data Model

New module `crates/igv-core/src/source/annotation.rs`:

```rust
#[async_trait]
pub trait AnnotationSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>>;
    fn display_name(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strand {
    Forward,
    Reverse,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Exon,        // generic exon (used when no CDS info is available)
    Cds,         // coding region
    Utr5,        // 5' UTR
    Utr3,        // 3' UTR
    BedSegment,  // BED12 sub-block, or whole BED feature
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationBlock {
    pub start: u64, // 1-based inclusive
    pub end: u64,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptKind {
    Mrna,
    BedFeature,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationTranscript {
    pub name: String,                   // gene_name or BED `name`
    pub id: String,                     // transcript_id or synthetic
    pub strand: Strand,
    pub blocks: Vec<AnnotationBlock>,   // sorted by start
    pub kind: TranscriptKind,
}
```

`AnnotationTranscript::span()` returns `(start, end)` of the leftmost block
to the rightmost block. Used for stacking.

## 4. Source Implementations

### 4.1 `NoodlesGffSource`

Wraps `noodles-gff 0.56`. Handles GFF3 and GTF (extension drives the
attribute-key heuristic — GTF uses `gene_id` / `transcript_id`, GFF3 uses
`ID` / `Parent`).

Algorithm:

1. On `open(path)`:
   - If sibling `.tbi` exists → keep path only, rely on tabix range queries
     per fetch.
   - Otherwise → load entire file once into an in-memory map keyed by
     chromosome → `Vec<AnnotationTranscript>` sorted by start. Bounded by a
     warning if the file exceeds 50 MiB uncompressed.
2. On `fetch(region)`:
   - Tabix path: stream records in the region, group by `Parent` /
     `transcript_id`, build transcripts.
   - In-memory path: binary-search the per-chromosome vector for the
     overlapping slice and return clones.
3. Block classification:
   - `CDS` features → `BlockKind::Cds`
   - `five_prime_UTR` / `5UTR` → `Utr5`
   - `three_prime_UTR` / `3UTR` → `Utr3`
   - `exon` features only counted if no CDS info exists for that
     transcript (otherwise the CDS + UTR blocks already cover the exon
     extent and adding `Exon` would double-render).

### 4.2 `NoodlesBedSource`

Wraps `noodles-bed 0.33`. BED3–BED6 produce a single-block transcript;
BED12 produces multi-block transcripts using `blockStarts` and
`blockSizes`. `chrom`, `chromStart`, `chromEnd`, `name`, `strand` are honored;
score and color fields are ignored this iteration. Same in-memory vs.
tabix split as GFF.

### 4.3 Format dispatcher

```rust
pub async fn open_annotation(path: &Path) -> Result<Arc<dyn AnnotationSource>>;
```

Resolution rules (case-insensitive on the lowercased extension chain):

| Suffix                                          | Backend           |
|-------------------------------------------------|-------------------|
| `.gff`, `.gff3`, `.gff.gz`, `.gff3.gz`          | `NoodlesGffSource` |
| `.gtf`, `.gtf.gz`                               | `NoodlesGffSource` |
| `.bed`, `.bed.gz`                               | `NoodlesBedSource` |
| anything else                                   | error              |

Override via `--annotation-format gff|gtf|bed` if the extension is
ambiguous or wrong.

## 5. UI Integration

### 5.1 Layout

`LayoutSpec` gains `annotation_tracks: usize`. Region order from top to
bottom of the body becomes:

```
overview        height 3
ruler           height 1
sequence        height 2
annotations × N each height ≥ 3 (one row of feature labels + 1+ feature rows)
variants        height 3 (when present)
coverage        height 5 (when present)
alignments × M  Min(6 each)
```

Annotations sit between sequence and variants because gene structure +
reference bases + variants is a natural top-to-bottom narrative.

### 5.2 Rendering

New widget `crates/igv-tui/src/ui/widgets/annotations.rs`. Per track:

- Stack transcripts greedily into lanes (no horizontal overlap within a
  lane), reuse the algorithm from `AlignmentsWidget::stack_reads`.
- Per lane, render in this order:
  1. Introns: `─` (theme `ANNOTATION_INTRON`) over the gaps between blocks
     within a single transcript.
  2. UTRs (`Utr5`, `Utr3`): `▯` (theme `ANNOTATION_UTR`).
  3. CDS / Exon / BED segments: `▮` (theme `ANNOTATION_EXON`) — drawn after
     introns/UTRs so they overwrite where they overlap.
  4. Strand glyph at the rightmost screen column of the transcript: `>` or
     `<`. Skip if no room.
  5. Name: drawn in the column **before** the leftmost block, right-aligned.
     Skip if it would collide with the previous transcript on the same lane
     or extend off-screen.

Empty lanes do not consume vertical space; the widget reserves
`min(lanes_needed, available_rows)` rows, dropping any lanes that overflow
(same policy as the alignments widget — a future iteration may add
scroll).

### 5.3 RenderMode behavior

- `PerBase` / `DetailedReads` / `CoverageDense`: render as described above.
- `HeatBar`: collapse all transcripts to a single row; each column shows
  `▮` if any transcript covers that genomic span, blank otherwise. Strand
  and name are dropped at this density.
- `OverviewOnly`: the entire annotations panel is hidden (the layout
  removes its rows when no transcripts are visible).

### 5.4 Theme keys

Added to both presets in `crates/igv-tui/src/ui/theme.rs`:

- `ANNOTATION_EXON` — bold, dark-mode green / light-mode dark-green
- `ANNOTATION_UTR` — dim version of exon
- `ANNOTATION_INTRON` — theme `BORDER`
- `ANNOTATION_NAME` — bold cyan / blue
- `ANNOTATION_STRAND` — same as `BORDER` plus bold

Override via the existing `[theme.custom]` TOML mechanism.

## 6. CLI

```rust
/// Path to a GFF3 / GTF / BED annotation file. Repeat for multiple tracks.
#[arg(short = 'g', long = "annotation")]
pub annotations: Vec<PathBuf>,

/// Override format auto-detection. Only needed when the file extension
/// does not match the contents.
#[arg(long = "annotation-format")]
pub annotation_format: Option<String>,
```

`--annotation-format` is a single value applied to **all** annotations on
the command line for this iteration; mixed-format CLIs that need
overrides should use a config file or rename files. Acceptable values:
`gff`, `gff3`, `gtf`, `bed`.

## 7. AppState and Loader

`AppState` additions:

```rust
pub annotations: Vec<AnnotationTrack>,
pub annotation_rows: Vec<Vec<AnnotationTranscript>>, // parallel to `annotations`

pub struct AnnotationTrack {
    pub path: PathBuf,
    pub display: String,
    pub source: Arc<dyn AnnotationSource>,
}
```

`Loader` extensions:

- New field `annotations: Vec<Arc<dyn AnnotationSource>>`.
- `dispatch()` spawns one extra task per annotation source (`spawn_blocking`
  if not already async-friendly; noodles-gff has sync IO).
- `LoadResult::Annotation { generation, track_index: usize, transcripts: Vec<...> }`.

`set_region_pending` clears `annotation_rows` per the same stale-data
pattern used for `bam_rows` and `reference_seq`.

`apply_load_result` slot for the new variant: same generation guard,
update `state.annotation_rows[track_index]`.

## 8. Error Handling

- Missing file at startup: `anyhow` error from `main`, prints and exits.
- Tabix index missing on a `.gz` file: warn that range queries will be
  slow but continue using the in-memory load path.
- Parse error on a single record: log via `tracing::warn!`, skip the record,
  continue.
- Empty fetch result: not an error.

The annotation track UI shows nothing when no transcripts intersect — no
empty-state placeholder.

## 9. Testing Strategy

### 9.1 Fixtures

- `crates/igv-core/tests/data/sample.gff3` — one `gene` with two `mRNA`
  children, each with three `exon` records, two of which carry `CDS` and
  one of which is a `5UTR` / `3UTR` pair. Hand-written, ~30 lines.
- `crates/igv-core/tests/data/sample.gtf` — a small GTF version of the
  same gene, to verify GTF dialect parsing.
- `crates/igv-core/tests/data/sample.bed` — four BED4 features and one
  BED12 with three blocks, on `chr1`.

### 9.2 Tests

- `crates/igv-core/tests/annotation_source.rs`:
  - GFF3: returns 2 transcripts for a region intersecting the gene; CDS /
    UTR block kinds tagged correctly.
  - GTF: same parent gene → 2 transcripts via `transcript_id` grouping.
  - BED4: simple one-block transcripts; strand inferred from column 6
    when present.
  - BED12: a single transcript with 3 blocks at the right offsets.
  - Format dispatch: `open_annotation` returns the right backend by
    extension.
- `crates/igv-core/src/source/annotation.rs` unit tests:
  - GFF parent-child grouping when `Parent` lists multiple ancestors.
  - Block sort + dedup on overlapping CDS / exon entries.

### 9.3 No widget snapshot tests

Same convention as the rest of the project: TUI widgets are exercised
manually. The annotation widget is small enough that a TestBackend
snapshot would mostly assert on layout coordinates already covered by
`compute(LayoutSpec)`.

## 10. Project Repositioning

`README.md` changes:

- First paragraph: replace "Rust rewrite of [cligv]" with "Interactive
  terminal genome viewer for FASTA / VCF / BAM / GFF / BED, written in
  Rust. Inspired by [cligv](https://github.com/jonasfreudig/cligv) by Jonas
  Freudigmann."
- "Layout" section: rename `cligv/` description from "original Python
  implementation" to "the project that inspired this work; kept locally as
  a reference and not part of this repository".
- "Usage" examples: add an annotation example
  (`igv-rs ref.fa -g genes.gff3 -b sample.bam`).
- "Keybindings" / "Configuration": no changes.
- "Known limitations": remove any line that becomes obsolete; otherwise
  unchanged.

The earlier spec
(`docs/superpowers/specs/2026-04-26-igv-rs-rust-rewrite-design.md`) and the
implementation plan from that round are not edited — they are historical
records of the prior iteration. New work always references this spec.

## 11. Decisions Recorded

- **Transcript-expanded over flat-collapsed (option B over A):** A flat
  rendering loses transcript structure (isoform comparison, UTR/CDS
  boundaries) which is the whole point of looking at annotations. Cost
  difference is small once `Parent` parsing is in place.
- **Transcript-expanded over per-zoom switching (option B over C):**
  switching rendering modes mid-session is disorienting; the existing
  `HeatBar` collapse handles the wide-zoom case adequately.
- **Unified `-g` over split `--gff` / `--bed` (option A over B):** matches
  the `-b` BAM repeating-flag pattern. Avoids forcing users to remember
  which flag belongs to which format.
- **Track placement above variants:** keeps a sequence → annotation →
  variants → coverage → alignments narrative.

## 12. Next Step

Once approved, the work transitions to `superpowers:writing-plans` to
produce a per-step TDD implementation plan.
