# GFF / GTF / BED Annotation Tracks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add GFF3 / GTF / BED annotation track support to `igv-rs`, with transcript-expanded rendering, multi-file support via repeatable `-g` flag, and project repositioning as inspired-by-cligv.

**Architecture:** New `igv-core::source::annotation` module exposing an `AnnotationSource` async trait and two backends (`NoodlesGffSource`, `NoodlesBedSource`) plus an `open_annotation` extension-driven dispatcher. `igv-tui` gains theme keys, a CLI flag, an `AnnotationsWidget`, and slot in the layout between sequence and variants. Loader extends to fetch annotations per generation; AppState clears stale annotation rows on navigation.

**Tech Stack:** noodles-gff 0.56, noodles-bed 0.33 (added under the `noodles` workspace dep features); existing ratatui / crossterm / tokio.

**Spec:** `docs/superpowers/specs/2026-04-26-annotations-design.md`

**Working tree root:** `/home/xzg/project/igv_rs/`. Branch: `main` (origin is `git@github.com:AI4S-YB/igv-rs.git`). Create a feature branch before starting.

---

## Conventions

- **Branch:** create `feat/annotations` from `main` at the start; commit there; merge or PR at end.
- **Commits:** Conventional Commits, one per task.
- **TDD:** for behavior tasks, write failing test first → confirm failure → implement → test green → commit.
- **Versions:** noodles-gff and noodles-bed track the noodles facade (`noodles = "0.85"`). Enable `gff` and `bed` features in the workspace dep.
- **noodles API drift:** if a method or path has changed at the resolved version, adapt minimally and document in the commit body. Several adaptations were already needed in the prior round; expect a few here as well.

---

## Phase 0: Branch + Cargo dep features

### Task 0.1: Branch off and enable gff/bed features

**Files:**
- Modify: `crates/igv-core/Cargo.toml`

- [ ] **Step 1: Create branch**

```bash
cd /home/xzg/project/igv_rs
git checkout -b feat/annotations
```

- [ ] **Step 2: Add `gff` and `bed` to noodles features**

Edit `crates/igv-core/Cargo.toml` and extend the `noodles` features list:

```toml
noodles = { workspace = true, features = [
    "core", "async", "fasta", "bam", "sam", "vcf", "csi", "tabix", "bgzf",
    "gff", "bed",
] }
```

- [ ] **Step 3: Verify and commit**

```bash
cargo build -p igv-core
git add crates/igv-core/Cargo.toml
git commit -m "chore(igv-core): enable noodles gff and bed features"
```

Expected: `cargo build -p igv-core` succeeds.

---

## Phase 1: igv-core data model

### Task 1.1: Annotation types and trait

**Files:**
- Create: `crates/igv-core/src/source/annotation.rs`
- Modify: `crates/igv-core/src/source/mod.rs`

- [ ] **Step 1: Create the new module**

`crates/igv-core/src/source/annotation.rs`:

```rust
//! Annotation-source trait and shared types.
//!
//! Concrete backends live in submodules (`gff`, `bed`) and are wrapped by
//! the public `open_annotation` factory which dispatches by file extension.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::error::{IgvError, Result};
use crate::region::Region;

pub mod gff;
pub mod bed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strand {
    Forward,
    Reverse,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Exon,
    Cds,
    Utr5,
    Utr3,
    BedSegment,
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
    pub name: String,
    pub id: String,
    pub strand: Strand,
    pub blocks: Vec<AnnotationBlock>,
    pub kind: TranscriptKind,
}

impl AnnotationTranscript {
    /// Span as (leftmost block start, rightmost block end). Returns `None`
    /// if blocks is empty.
    pub fn span(&self) -> Option<(u64, u64)> {
        let s = self.blocks.iter().map(|b| b.start).min()?;
        let e = self.blocks.iter().map(|b| b.end).max()?;
        Some((s, e))
    }
}

#[async_trait]
pub trait AnnotationSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>>;
    fn display_name(&self) -> &str;
}

/// Format hint that the user can pass via CLI to override extension-based
/// dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationFormat {
    Gff3,
    Gtf,
    Bed,
}

impl AnnotationFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "gff" | "gff3" => Some(Self::Gff3),
            "gtf" => Some(Self::Gtf),
            "bed" => Some(Self::Bed),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        if lower.ends_with(".gff") || lower.ends_with(".gff3") || lower.ends_with(".gff.gz") || lower.ends_with(".gff3.gz") {
            return Some(Self::Gff3);
        }
        if lower.ends_with(".gtf") || lower.ends_with(".gtf.gz") {
            return Some(Self::Gtf);
        }
        if lower.ends_with(".bed") || lower.ends_with(".bed.gz") {
            return Some(Self::Bed);
        }
        None
    }
}

/// Open an annotation file, dispatching to the right backend by extension
/// (or by `format_override` if given).
pub async fn open_annotation(
    path: &Path,
    format_override: Option<AnnotationFormat>,
) -> Result<Arc<dyn AnnotationSource>> {
    let format = format_override
        .or_else(|| AnnotationFormat::from_path(path))
        .ok_or_else(|| {
            IgvError::Other(format!(
                "cannot determine annotation format for '{}'; pass --annotation-format",
                path.display()
            ))
        })?;
    match format {
        AnnotationFormat::Gff3 | AnnotationFormat::Gtf => {
            let src = gff::NoodlesGffSource::open(path, format).await?;
            Ok(Arc::new(src))
        }
        AnnotationFormat::Bed => {
            let src = bed::NoodlesBedSource::open(path).await?;
            Ok(Arc::new(src))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn format_dispatch_by_extension() {
        let cases = [
            ("a.gff", Some(AnnotationFormat::Gff3)),
            ("a.gff3", Some(AnnotationFormat::Gff3)),
            ("a.gff.gz", Some(AnnotationFormat::Gff3)),
            ("a.gff3.gz", Some(AnnotationFormat::Gff3)),
            ("a.gtf", Some(AnnotationFormat::Gtf)),
            ("a.gtf.gz", Some(AnnotationFormat::Gtf)),
            ("a.bed", Some(AnnotationFormat::Bed)),
            ("a.bed.gz", Some(AnnotationFormat::Bed)),
            ("a.txt", None),
        ];
        for (name, expected) in cases {
            let got = AnnotationFormat::from_path(&PathBuf::from(name));
            assert_eq!(got, expected, "case {}", name);
        }
    }

    #[test]
    fn format_parse_string() {
        assert_eq!(AnnotationFormat::parse("gff"), Some(AnnotationFormat::Gff3));
        assert_eq!(AnnotationFormat::parse("GTF"), Some(AnnotationFormat::Gtf));
        assert_eq!(AnnotationFormat::parse("bed"), Some(AnnotationFormat::Bed));
        assert_eq!(AnnotationFormat::parse("vcf"), None);
    }

    #[test]
    fn span_returns_min_max_of_blocks() {
        let t = AnnotationTranscript {
            name: "g".into(),
            id: "t".into(),
            strand: Strand::Forward,
            blocks: vec![
                AnnotationBlock { start: 10, end: 20, kind: BlockKind::Cds },
                AnnotationBlock { start: 50, end: 60, kind: BlockKind::Cds },
                AnnotationBlock { start: 30, end: 40, kind: BlockKind::Cds },
            ],
            kind: TranscriptKind::Mrna,
        };
        assert_eq!(t.span(), Some((10, 60)));
    }

    #[test]
    fn span_returns_none_when_empty() {
        let t = AnnotationTranscript {
            name: "g".into(),
            id: "t".into(),
            strand: Strand::Unknown,
            blocks: vec![],
            kind: TranscriptKind::Other,
        };
        assert_eq!(t.span(), None);
    }
}
```

- [ ] **Step 2: Wire the module into `source/mod.rs`**

Add at the top of `source/mod.rs` (after the existing `pub mod` lines):

```rust
pub mod annotation;
```

And under the `pub use` block at the bottom, append:

```rust
pub use annotation::{
    AnnotationBlock, AnnotationFormat, AnnotationSource, AnnotationTranscript, BlockKind,
    Strand, TranscriptKind, open_annotation,
};
```

- [ ] **Step 3: Stub the backend submodules so the build passes**

Create `crates/igv-core/src/source/annotation/gff.rs`:

```rust
//! GFF3 / GTF source — implemented in Task 1.3.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{AnnotationFormat, AnnotationSource, AnnotationTranscript};

pub struct NoodlesGffSource {
    path: std::path::PathBuf,
    display: String,
    #[allow(dead_code)]
    format: AnnotationFormat,
}

impl NoodlesGffSource {
    pub async fn open(path: &Path, format: AnnotationFormat) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
            format,
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesGffSource {
    async fn fetch(&self, _region: &Region) -> Result<Vec<AnnotationTranscript>> {
        Err(IgvError::Other(format!(
            "NoodlesGffSource::fetch not yet implemented (path={})",
            self.path.display()
        )))
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
```

Create `crates/igv-core/src/source/annotation/bed.rs`:

```rust
//! BED source — implemented in Task 1.4.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{AnnotationSource, AnnotationTranscript};

pub struct NoodlesBedSource {
    path: std::path::PathBuf,
    display: String,
}

impl NoodlesBedSource {
    pub async fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesBedSource {
    async fn fetch(&self, _region: &Region) -> Result<Vec<AnnotationTranscript>> {
        Err(IgvError::Other(format!(
            "NoodlesBedSource::fetch not yet implemented (path={})",
            self.path.display()
        )))
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test -p igv-core annotation::tests
git add crates/igv-core/src/source crates/igv-core/Cargo.toml 2>/dev/null
git add crates/igv-core/src/source/mod.rs crates/igv-core/src/source/annotation.rs crates/igv-core/src/source/annotation/
git commit -m "feat(igv-core): annotation types, AnnotationSource trait, dispatch by extension"
```

Expected: 4 tests pass.

---

### Task 1.2: Test fixtures

**Files:**
- Create: `crates/igv-core/tests/data/sample.gff3`
- Create: `crates/igv-core/tests/data/sample.gtf`
- Create: `crates/igv-core/tests/data/sample.bed`

- [ ] **Step 1: Write `sample.gff3`**

Tab-separated. Use real tab characters; do not type literal spaces.

```
##gff-version 3
##sequence-region chr1 1 1000
chr1	test	gene	100	500	.	+	.	ID=gene1;Name=GENE1
chr1	test	mRNA	100	500	.	+	.	ID=tx1;Parent=gene1;Name=GENE1.1
chr1	test	exon	100	200	.	+	.	ID=ex1;Parent=tx1
chr1	test	exon	300	400	.	+	.	ID=ex2;Parent=tx1
chr1	test	exon	450	500	.	+	.	ID=ex3;Parent=tx1
chr1	test	five_prime_UTR	100	150	.	+	.	ID=utr5;Parent=tx1
chr1	test	CDS	151	200	.	+	0	ID=cds1;Parent=tx1
chr1	test	CDS	300	400	.	+	2	ID=cds2;Parent=tx1
chr1	test	CDS	450	480	.	+	2	ID=cds3;Parent=tx1
chr1	test	three_prime_UTR	481	500	.	+	.	ID=utr3;Parent=tx1
chr1	test	mRNA	120	500	.	+	.	ID=tx2;Parent=gene1;Name=GENE1.2
chr1	test	exon	120	200	.	+	.	ID=ex4;Parent=tx2
chr1	test	exon	300	500	.	+	.	ID=ex5;Parent=tx2
```

- [ ] **Step 2: Write `sample.gtf`**

GTF dialect of the same single-mRNA gene:

```
chr1	test	gene	100	500	.	+	.	gene_id "gene1"; gene_name "GENE1";
chr1	test	transcript	100	500	.	+	.	gene_id "gene1"; transcript_id "tx1";
chr1	test	exon	100	200	.	+	.	gene_id "gene1"; transcript_id "tx1"; exon_number "1";
chr1	test	exon	300	400	.	+	.	gene_id "gene1"; transcript_id "tx1"; exon_number "2";
chr1	test	exon	450	500	.	+	.	gene_id "gene1"; transcript_id "tx1"; exon_number "3";
chr1	test	CDS	151	200	.	+	0	gene_id "gene1"; transcript_id "tx1";
chr1	test	CDS	300	400	.	+	2	gene_id "gene1"; transcript_id "tx1";
chr1	test	CDS	450	480	.	+	2	gene_id "gene1"; transcript_id "tx1";
```

- [ ] **Step 3: Write `sample.bed`**

```
chr1	99	200	feat1	0	+
chr1	299	400	feat2	0	-
chr1	499	600	feat3	0	+
chr1	699	1000	bigblock	0	+	699	1000	0	3	100,80,50	0,150,251
```

(Last line is BED12: blockCount 3, blockSizes 100/80/50, blockStarts 0/150/251 — meaning blocks at 700-799, 850-929, 951-1000 in 1-based-inclusive coordinates.)

- [ ] **Step 4: Commit**

```bash
git add crates/igv-core/tests/data/sample.gff3 \
        crates/igv-core/tests/data/sample.gtf \
        crates/igv-core/tests/data/sample.bed
git commit -m "test(igv-core): add GFF3/GTF/BED fixtures"
```

---

### Task 1.3: Implement `NoodlesGffSource`

**Files:**
- Modify: `crates/igv-core/src/source/annotation/gff.rs`
- Create: `crates/igv-core/tests/annotation_gff.rs`

- [ ] **Step 1: Write the integration tests first**

`crates/igv-core/tests/annotation_gff.rs`:

```rust
use std::path::Path;

use igv_core::region::Region;
use igv_core::source::annotation::AnnotationFormat;
use igv_core::source::annotation::gff::NoodlesGffSource;
use igv_core::source::{AnnotationSource, BlockKind, Strand, TranscriptKind};

#[tokio::test]
async fn gff3_returns_two_mrna_transcripts() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let names: Vec<_> = txs.iter().map(|t| t.id.as_str()).collect();
    assert!(names.contains(&"tx1"));
    assert!(names.contains(&"tx2"));
    assert_eq!(txs.iter().filter(|t| t.kind == TranscriptKind::Mrna).count(), 2);
}

#[tokio::test]
async fn gff3_classifies_cds_and_utrs_for_first_transcript() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let tx1 = txs.iter().find(|t| t.id == "tx1").expect("tx1 missing");
    let cds = tx1.blocks.iter().filter(|b| b.kind == BlockKind::Cds).count();
    let utr5 = tx1.blocks.iter().filter(|b| b.kind == BlockKind::Utr5).count();
    let utr3 = tx1.blocks.iter().filter(|b| b.kind == BlockKind::Utr3).count();
    assert_eq!(cds, 3);
    assert_eq!(utr5, 1);
    assert_eq!(utr3, 1);
    assert_eq!(tx1.strand, Strand::Forward);
}

#[tokio::test]
async fn gff3_uses_exon_when_no_cds_in_transcript() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let tx2 = txs.iter().find(|t| t.id == "tx2").expect("tx2 missing");
    let exons = tx2.blocks.iter().filter(|b| b.kind == BlockKind::Exon).count();
    assert_eq!(exons, 2);
}

#[tokio::test]
async fn gtf_returns_one_transcript_with_three_cds() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gtf"),
        AnnotationFormat::Gtf,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let tx = txs.iter().find(|t| t.id == "tx1").expect("tx1 missing");
    assert_eq!(tx.blocks.iter().filter(|b| b.kind == BlockKind::Cds).count(), 3);
    assert_eq!(tx.kind, TranscriptKind::Mrna);
}

#[tokio::test]
async fn gff3_returns_empty_outside_chrom() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chrZ", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(txs.is_empty());
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p igv-core --test annotation_gff
```

Expected: 5 tests fail (the stub from Task 1.1 returns an error).

- [ ] **Step 3: Implement the GFF source**

Replace `crates/igv-core/src/source/annotation/gff.rs`:

```rust
//! GFF3 / GTF annotation source. Loads the entire file into memory on
//! `open` and serves range queries from a per-chromosome sorted index.
//! Fine for typical gene-annotation files (≤ a few hundred MB on disk).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use noodles::gff;
use tracing::warn;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{
    AnnotationBlock, AnnotationFormat, AnnotationSource, AnnotationTranscript, BlockKind, Strand,
    TranscriptKind,
};

pub struct NoodlesGffSource {
    path: PathBuf,
    display: String,
    format: AnnotationFormat,
    /// chrom → sorted (by span_start) transcripts
    by_chrom: HashMap<String, Vec<AnnotationTranscript>>,
}

impl NoodlesGffSource {
    pub async fn open(path: &Path, format: AnnotationFormat) -> Result<Self> {
        let p = path.to_path_buf();
        let fmt = format;
        let by_chrom = tokio::task::spawn_blocking(move || -> Result<_> {
            load_all(&p, fmt)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        Ok(Self {
            path: path.to_path_buf(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
            format,
            by_chrom,
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesGffSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>> {
        let bucket = match self.by_chrom.get(&region.chrom) {
            Some(v) => v,
            None => return Ok(Vec::new()),
        };
        // Linear scan: small chromosomes (≤ 100k transcripts) — fine.
        // Could binary-search if it ever becomes a hotspot.
        let mut out = Vec::new();
        for tx in bucket {
            if let Some((s, e)) = tx.span() {
                if e >= region.start && s <= region.end {
                    out.push(tx.clone());
                }
            }
        }
        Ok(out)
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

fn load_all(
    path: &Path,
    format: AnnotationFormat,
) -> Result<HashMap<String, Vec<AnnotationTranscript>>> {
    let file = std::fs::File::open(path).map_err(|e| IgvError::io(path, e))?;
    let buf = std::io::BufReader::new(file);
    let mut reader = gff::io::Reader::new(buf);

    // Group records by transcript id and gene id.
    // For GFF3: child records carry `Parent` listing transcript IDs.
    // For GTF: lines carry `transcript_id` directly.
    struct Pending {
        gene_name: String,
        transcript_id: String,
        chrom: String,
        strand: Strand,
        blocks: Vec<AnnotationBlock>,
        has_cds: bool,
        seen_exon: bool,
    }
    let mut pending: HashMap<String, Pending> = HashMap::new();
    // GFF3: track gene_id → gene_name lookup so we can label transcripts.
    let mut gene_name_by_id: HashMap<String, String> = HashMap::new();
    // GFF3: track tx_id → gene_id so we can resolve a parent gene's name.
    let mut gene_id_by_tx: HashMap<String, String> = HashMap::new();

    for result in reader.line_bufs() {
        let line = match result {
            Ok(l) => l,
            Err(e) => {
                warn!("gff parse error: {e}");
                continue;
            }
        };
        let record = match line {
            gff::LineBuf::Record(r) => r,
            _ => continue,
        };

        let chrom = record.reference_sequence_name().to_string();
        let kind = record.ty().to_string();
        let start = u64::from(record.start());
        let end = u64::from(record.end());
        let strand_kind = record.strand();
        let strand = match strand_kind {
            gff::record_buf::Strand::Forward => Strand::Forward,
            gff::record_buf::Strand::Reverse => Strand::Reverse,
            _ => Strand::Unknown,
        };
        let attrs = record.attributes();

        let (gene_id_attr, transcript_id_attr, parent_attr, name_attr) = match format {
            AnnotationFormat::Gff3 => {
                let id = lookup_attr(attrs, "ID");
                let parent = lookup_attr(attrs, "Parent");
                let name = lookup_attr(attrs, "Name").or_else(|| id.clone());
                (id, None, parent, name)
            }
            AnnotationFormat::Gtf => {
                let gid = lookup_attr(attrs, "gene_id");
                let tid = lookup_attr(attrs, "transcript_id");
                let name = lookup_attr(attrs, "gene_name").or_else(|| gid.clone());
                (gid, tid, None, name)
            }
            AnnotationFormat::Bed => unreachable!(),
        };

        // Build per-kind:
        match kind.as_str() {
            "gene" => {
                if let (Some(id), Some(name)) = (gene_id_attr.clone(), name_attr.clone()) {
                    gene_name_by_id.insert(id, name);
                }
            }
            "mRNA" | "transcript" => {
                let tx_id = match format {
                    AnnotationFormat::Gff3 => match gene_id_attr.clone() {
                        Some(id) => id,
                        None => continue,
                    },
                    AnnotationFormat::Gtf => match transcript_id_attr.clone() {
                        Some(id) => id,
                        None => continue,
                    },
                    AnnotationFormat::Bed => unreachable!(),
                };
                let gene_for_tx = match format {
                    AnnotationFormat::Gff3 => parent_attr
                        .clone()
                        .and_then(|p| p.split(',').next().map(|s| s.to_string())),
                    AnnotationFormat::Gtf => lookup_attr(attrs, "gene_id"),
                    _ => None,
                };
                if let Some(g) = gene_for_tx.clone() {
                    gene_id_by_tx.insert(tx_id.clone(), g);
                }
                pending.entry(tx_id.clone()).or_insert_with(|| Pending {
                    gene_name: name_attr.clone().unwrap_or_else(|| tx_id.clone()),
                    transcript_id: tx_id,
                    chrom: chrom.clone(),
                    strand,
                    blocks: Vec::new(),
                    has_cds: false,
                    seen_exon: false,
                });
            }
            "exon" | "CDS" | "five_prime_UTR" | "5UTR" | "three_prime_UTR" | "3UTR" => {
                let tx_ids: Vec<String> = match format {
                    AnnotationFormat::Gff3 => match parent_attr.clone() {
                        Some(p) => p.split(',').map(|s| s.to_string()).collect(),
                        None => continue,
                    },
                    AnnotationFormat::Gtf => match transcript_id_attr.clone() {
                        Some(id) => vec![id],
                        None => continue,
                    },
                    AnnotationFormat::Bed => unreachable!(),
                };
                let block_kind = match kind.as_str() {
                    "exon" => BlockKind::Exon,
                    "CDS" => BlockKind::Cds,
                    "five_prime_UTR" | "5UTR" => BlockKind::Utr5,
                    "three_prime_UTR" | "3UTR" => BlockKind::Utr3,
                    _ => unreachable!(),
                };
                for tx_id in tx_ids {
                    let entry = pending.entry(tx_id.clone()).or_insert_with(|| Pending {
                        gene_name: tx_id.clone(),
                        transcript_id: tx_id.clone(),
                        chrom: chrom.clone(),
                        strand,
                        blocks: Vec::new(),
                        has_cds: false,
                        seen_exon: false,
                    });
                    if matches!(block_kind, BlockKind::Cds) {
                        entry.has_cds = true;
                    }
                    if matches!(block_kind, BlockKind::Exon) {
                        entry.seen_exon = true;
                    }
                    entry.blocks.push(AnnotationBlock {
                        start,
                        end,
                        kind: block_kind,
                    });
                }
            }
            _ => {} // other feature types ignored this iteration
        }
    }

    // Resolve gene names where possible (GFF3: Parent points to a gene id).
    let mut by_chrom: HashMap<String, Vec<AnnotationTranscript>> = HashMap::new();
    for (_, mut p) in pending {
        // If there's a CDS, drop bare exon blocks: CDS + UTR already cover
        // the visible extent, and overlaying generic exon would
        // double-render.
        if p.has_cds {
            p.blocks.retain(|b| !matches!(b.kind, BlockKind::Exon));
        } else if !p.seen_exon {
            // No exon and no CDS — nothing to render. Skip.
            continue;
        }
        // Improve label using gene_name lookup (GFF3 Parent → gene name).
        let label = if let Some(g) = gene_id_by_tx.get(&p.transcript_id) {
            gene_name_by_id.get(g).cloned().unwrap_or(p.gene_name)
        } else {
            p.gene_name
        };
        p.blocks.sort_by_key(|b| b.start);
        let tx = AnnotationTranscript {
            name: label,
            id: p.transcript_id,
            strand: p.strand,
            blocks: p.blocks,
            kind: TranscriptKind::Mrna,
        };
        by_chrom.entry(p.chrom).or_default().push(tx);
    }
    for v in by_chrom.values_mut() {
        v.sort_by_key(|t| t.span().map(|(s, _)| s).unwrap_or(0));
    }
    Ok(by_chrom)
}

fn lookup_attr(attrs: &gff::record_buf::Attributes, key: &str) -> Option<String> {
    attrs
        .as_ref()
        .iter()
        .find(|(k, _)| k.as_ref() == key)
        .and_then(|(_, v)| match v {
            gff::record_buf::attributes::field::Value::String(s) => Some(s.to_string()),
            gff::record_buf::attributes::field::Value::Array(items) => {
                Some(items.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","))
            }
        })
}
```

**API note for the implementer:** noodles-gff 0.56's exact module paths
for `record_buf::Attributes`, `record_buf::Strand`, and the
`attributes::field::Value` enum may differ slightly. If the compiler
reports a path mismatch, navigate to the actual symbol in
`target/doc` or in the source under
`~/.cargo/registry/src/index.crates.io-*/noodles-gff-0.56.0/src/`. The
behavior to preserve: walk records, group by transcript id, classify
blocks by feature type, resolve strand and label.

- [ ] **Step 4: Run tests until green**

```bash
cargo test -p igv-core --test annotation_gff
```

Expected: 5 tests pass. If a noodles API symbol doesn't compile, adapt
minimally and document in the commit.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-core/src/source/annotation/gff.rs \
        crates/igv-core/tests/annotation_gff.rs
git commit -m "feat(igv-core): NoodlesGffSource with GFF3/GTF parsing and CDS/UTR classification"
```

---

### Task 1.4: Implement `NoodlesBedSource`

**Files:**
- Modify: `crates/igv-core/src/source/annotation/bed.rs`
- Create: `crates/igv-core/tests/annotation_bed.rs`

- [ ] **Step 1: Write integration tests**

`crates/igv-core/tests/annotation_bed.rs`:

```rust
use std::path::Path;

use igv_core::region::Region;
use igv_core::source::annotation::bed::NoodlesBedSource;
use igv_core::source::{AnnotationSource, BlockKind, Strand, TranscriptKind};

#[tokio::test]
async fn bed_loads_simple_features_with_strand() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(txs.iter().any(|t| t.name == "feat1" && t.strand == Strand::Forward));
    assert!(txs.iter().any(|t| t.name == "feat2" && t.strand == Strand::Reverse));
    let feat1 = txs.iter().find(|t| t.name == "feat1").unwrap();
    assert_eq!(feat1.blocks.len(), 1);
    assert_eq!(feat1.blocks[0].kind, BlockKind::BedSegment);
    assert_eq!(feat1.kind, TranscriptKind::BedFeature);
}

#[tokio::test]
async fn bed12_decomposes_into_blocks() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let big = txs.iter().find(|t| t.name == "bigblock").expect("bigblock missing");
    assert_eq!(big.blocks.len(), 3);
    // BED is 0-based half-open; we store 1-based inclusive.
    // Source range: chromStart=699, blockStarts=0,150,251 blockSizes=100,80,50
    // Block 1: 700..=799, Block 2: 850..=929, Block 3: 951..=1000
    let starts: Vec<u64> = big.blocks.iter().map(|b| b.start).collect();
    let ends: Vec<u64> = big.blocks.iter().map(|b| b.end).collect();
    assert_eq!(starts, vec![700, 850, 951]);
    assert_eq!(ends, vec![799, 929, 1000]);
}

#[tokio::test]
async fn bed_returns_only_overlapping_features() {
    let src = NoodlesBedSource::open(Path::new("tests/data/sample.bed"))
        .await
        .unwrap();
    let region = Region::new("chr1", 100, 250).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(txs.iter().any(|t| t.name == "feat1"));
    assert!(!txs.iter().any(|t| t.name == "feat2"));
    assert!(!txs.iter().any(|t| t.name == "bigblock"));
}
```

- [ ] **Step 2: Confirm failure**

```bash
cargo test -p igv-core --test annotation_bed
```

Expected: 3 tests fail (stub still in place).

- [ ] **Step 3: Implement BED source**

Replace `crates/igv-core/src/source/annotation/bed.rs`:

```rust
//! BED annotation source. Reads BED3 through BED12 manually (BED is a
//! whitespace-delimited line format; rolling our own parser is more
//! flexible than dealing with noodles-bed's const-generic Reader<N, R>
//! across mixed-column files).

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tracing::warn;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{
    AnnotationBlock, AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};

pub struct NoodlesBedSource {
    path: PathBuf,
    display: String,
    by_chrom: HashMap<String, Vec<AnnotationTranscript>>,
}

impl NoodlesBedSource {
    pub async fn open(path: &Path) -> Result<Self> {
        let p = path.to_path_buf();
        let by_chrom = tokio::task::spawn_blocking(move || -> Result<_> { load(&p) })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
        Ok(Self {
            path: path.to_path_buf(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
            by_chrom,
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesBedSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>> {
        let bucket = match self.by_chrom.get(&region.chrom) {
            Some(b) => b,
            None => return Ok(Vec::new()),
        };
        let mut out = Vec::new();
        for tx in bucket {
            if let Some((s, e)) = tx.span() {
                if e >= region.start && s <= region.end {
                    out.push(tx.clone());
                }
            }
        }
        Ok(out)
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

fn load(path: &Path) -> Result<HashMap<String, Vec<AnnotationTranscript>>> {
    let file = std::fs::File::open(path).map_err(|e| IgvError::io(path, e))?;
    let mut by_chrom: HashMap<String, Vec<AnnotationTranscript>> = HashMap::new();
    let lower = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_ascii_lowercase());
    let reader: Box<dyn BufRead> = if lower.as_deref().is_some_and(|s| s.ends_with(".gz")) {
        // BED.gz — read raw bytes via flate2; bgzf would also work but
        // plain gzip is the common case.
        let r = std::io::BufReader::new(flate2::read::GzDecoder::new(file));
        Box::new(r)
    } else {
        Box::new(std::io::BufReader::new(file))
    };

    for (i, line_res) in reader.lines().enumerate() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                warn!("bed read error at line {}: {e}", i + 1);
                continue;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("track ")
            || trimmed.starts_with("browser ")
        {
            continue;
        }
        match parse_bed_line(trimmed, i + 1) {
            Ok(Some(tx)) => {
                let chrom = tx.0;
                by_chrom.entry(chrom).or_default().push(tx.1);
            }
            Ok(None) => {}
            Err(e) => warn!("bed parse error at line {}: {e}", i + 1),
        }
    }
    for v in by_chrom.values_mut() {
        v.sort_by_key(|t| t.span().map(|(s, _)| s).unwrap_or(0));
    }
    Ok(by_chrom)
}

fn parse_bed_line(line: &str, lineno: usize) -> Result<Option<(String, AnnotationTranscript)>> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 3 {
        return Err(IgvError::Other(format!("bed line {lineno}: <3 columns")));
    }
    let chrom = cols[0].to_string();
    let chrom_start: u64 = cols[1].parse().map_err(|_| {
        IgvError::Other(format!("bed line {lineno}: invalid chromStart"))
    })?;
    let chrom_end: u64 = cols[2].parse().map_err(|_| {
        IgvError::Other(format!("bed line {lineno}: invalid chromEnd"))
    })?;
    let name = cols.get(3).copied().unwrap_or("").to_string();
    let strand = match cols.get(5).copied() {
        Some("+") => Strand::Forward,
        Some("-") => Strand::Reverse,
        _ => Strand::Unknown,
    };

    let blocks = if cols.len() >= 12 {
        let block_count: usize = cols[9].parse().map_err(|_| {
            IgvError::Other(format!("bed line {lineno}: invalid blockCount"))
        })?;
        let sizes: Vec<u64> = cols[10]
            .trim_end_matches(',')
            .split(',')
            .map(|s| s.parse().unwrap_or(0))
            .collect();
        let starts: Vec<u64> = cols[11]
            .trim_end_matches(',')
            .split(',')
            .map(|s| s.parse().unwrap_or(0))
            .collect();
        if sizes.len() != block_count || starts.len() != block_count {
            return Err(IgvError::Other(format!(
                "bed line {lineno}: blockSizes/blockStarts mismatch with blockCount"
            )));
        }
        (0..block_count)
            .map(|i| {
                let s = chrom_start + starts[i] + 1; // 1-based inclusive
                let e = chrom_start + starts[i] + sizes[i]; // already inclusive end
                AnnotationBlock {
                    start: s,
                    end: e,
                    kind: BlockKind::BedSegment,
                }
            })
            .collect()
    } else {
        // Single-block from chromStart..chromEnd (BED is 0-based half-open;
        // 1-based inclusive equivalent is chrom_start+1 .. chrom_end).
        vec![AnnotationBlock {
            start: chrom_start + 1,
            end: chrom_end,
            kind: BlockKind::BedSegment,
        }]
    };

    let id = if name.is_empty() {
        format!("bed:{}:{}", chrom, chrom_start + 1)
    } else {
        name.clone()
    };
    Ok(Some((
        chrom,
        AnnotationTranscript {
            name,
            id,
            strand,
            blocks,
            kind: TranscriptKind::BedFeature,
        },
    )))
}
```

- [ ] **Step 4: Add `flate2` dev-dependency**

`flate2` is needed for BED.gz support. It's already a transitive dep
through noodles, so no Cargo.toml change *should* be required, but if
`use flate2` fails to resolve, add it explicitly:

```toml
# crates/igv-core/Cargo.toml [dependencies]
flate2 = "1"
```

Build to confirm.

- [ ] **Step 5: Run tests until green**

```bash
cargo test -p igv-core --test annotation_bed
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-core/src/source/annotation/bed.rs \
        crates/igv-core/tests/annotation_bed.rs \
        crates/igv-core/Cargo.toml
git commit -m "feat(igv-core): NoodlesBedSource with BED3-12 support"
```

---

### Task 1.5: Verify the dispatcher end-to-end

**Files:**
- Create: `crates/igv-core/tests/annotation_dispatch.rs`

- [ ] **Step 1: Write the dispatcher test**

```rust
use std::path::Path;

use igv_core::region::Region;
use igv_core::source::annotation::open_annotation;

#[tokio::test]
async fn dispatcher_opens_gff3() {
    let src = open_annotation(Path::new("tests/data/sample.gff3"), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(!txs.is_empty());
    assert!(src.display_name().contains("sample.gff3"));
}

#[tokio::test]
async fn dispatcher_opens_gtf() {
    let src = open_annotation(Path::new("tests/data/sample.gtf"), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert_eq!(txs.len(), 1);
}

#[tokio::test]
async fn dispatcher_opens_bed() {
    let src = open_annotation(Path::new("tests/data/sample.bed"), None).await.unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert_eq!(txs.len(), 4);
}

#[tokio::test]
async fn dispatcher_errors_on_unknown_extension() {
    let result = open_annotation(Path::new("tests/data/sample.fa"), None).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run and commit**

```bash
cargo test -p igv-core --test annotation_dispatch
git add crates/igv-core/tests/annotation_dispatch.rs
git commit -m "test(igv-core): annotation dispatcher integration tests"
```

Expected: all 4 tests pass.

---

## Phase 2: igv-tui integration

### Task 2.1: Theme keys for annotations

**Files:**
- Modify: `crates/igv-tui/src/ui/theme.rs`

- [ ] **Step 1: Insert keys into both presets**

In `Theme::dark()`, just before `Self { map: m }`, add:

```rust
m.insert("ANNOTATION_EXON".into(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
m.insert("ANNOTATION_UTR".into(), Style::default().fg(Color::Green));
m.insert("ANNOTATION_INTRON".into(), Style::default().fg(Color::DarkGray));
m.insert("ANNOTATION_NAME".into(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
m.insert("ANNOTATION_STRAND".into(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
```

In `Theme::light()`, similarly:

```rust
m.insert("ANNOTATION_EXON".into(), Style::default().fg(Color::Rgb(0, 100, 0)).add_modifier(Modifier::BOLD));
m.insert("ANNOTATION_UTR".into(), Style::default().fg(Color::Rgb(0, 100, 0)));
m.insert("ANNOTATION_INTRON".into(), Style::default().fg(Color::Gray));
m.insert("ANNOTATION_NAME".into(), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
m.insert("ANNOTATION_STRAND".into(), Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/theme.rs
git commit -m "feat(igv-tui): annotation theme keys"
```

---

### Task 2.2: CLI flag

**Files:**
- Modify: `crates/igv-tui/src/cli.rs`

- [ ] **Step 1: Add the new fields**

Append two fields to the `Cli` struct (after the existing `tag` field):

```rust
    /// Path to a GFF3, GTF, or BED annotation file. Format auto-detected
    /// by extension. May be repeated.
    #[arg(short = 'g', long = "annotation")]
    pub annotations: Vec<std::path::PathBuf>,

    /// Override annotation format auto-detection
    /// (`gff`, `gff3`, `gtf`, or `bed`). Applies to all `-g` files.
    #[arg(long = "annotation-format")]
    pub annotation_format: Option<String>,
```

- [ ] **Step 2: Verify and commit**

```bash
cargo build -p igv-tui
cargo run -p igv-tui -- --help 2>&1 | grep -E "annotation"
git add crates/igv-tui/src/cli.rs
git commit -m "feat(igv-tui): -g/--annotation CLI flag (repeatable)"
```

Expected: `--help` shows `--annotation` and `--annotation-format`.

---

### Task 2.3: AppState additions

**Files:**
- Modify: `crates/igv-tui/src/app/state.rs`

- [ ] **Step 1: Add types and fields**

In `state.rs`, add a new struct near `BamTrack`:

```rust
#[derive(Clone)]
pub struct AnnotationTrack {
    pub path: std::path::PathBuf,
    pub display: String,
    pub source: std::sync::Arc<dyn igv_core::source::AnnotationSource>,
}
```

In the `AppState` struct, add two fields:

```rust
pub annotations: Vec<AnnotationTrack>,
pub annotation_rows: Vec<Vec<igv_core::source::AnnotationTranscript>>,
```

In `set_region_pending`, extend the stale-data clear:

```rust
for rows in &mut self.annotation_rows {
    rows.clear();
}
```

Add this line right after the existing `for rows in &mut self.bam_rows { rows.clear(); }` block.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/app/state.rs
git commit -m "feat(igv-tui): AppState gains annotation tracks and rows"
```

---

### Task 2.4: Loader extension

**Files:**
- Modify: `crates/igv-tui/src/app/loader.rs`

- [ ] **Step 1: Add LoadResult variant + Loader field + dispatch arm**

In `loader.rs`:

Add to `LoadResult` enum:

```rust
Annotation {
    generation: u64,
    track_index: usize,
    transcripts: Vec<igv_core::source::AnnotationTranscript>,
},
```

Add to `Loader` struct (parallel to `bams`):

```rust
pub annotations: Vec<std::sync::Arc<dyn igv_core::source::AnnotationSource>>,
```

Update `Loader::new` to accept `annotations`:

```rust
pub fn new(
    fasta: std::sync::Arc<dyn igv_core::source::FastaSource>,
    vcf: Option<std::sync::Arc<dyn igv_core::source::VcfSource>>,
    bams: Vec<std::sync::Arc<dyn igv_core::source::BamSource>>,
    annotations: Vec<std::sync::Arc<dyn igv_core::source::AnnotationSource>>,
    tx: tokio::sync::mpsc::Sender<LoadResult>,
) -> Self {
    Self {
        fasta,
        vcf,
        bams,
        annotations,
        tx,
        current: Vec::new(),
    }
}
```

In `Loader::dispatch`, after the existing BAM-fetch loop, append a parallel
annotation-fetch loop:

```rust
for (idx, ann) in self.annotations.iter().enumerate() {
    let ann = std::sync::Arc::clone(ann);
    let tx = self.tx.clone();
    let r = req.clone();
    self.current.push(tokio::spawn(async move {
        match ann.fetch(&r.region).await {
            Ok(transcripts) => {
                let _ = tx
                    .send(LoadResult::Annotation {
                        generation: r.generation,
                        track_index: idx,
                        transcripts,
                    })
                    .await;
            }
            Err(e) => {
                tracing::warn!("annotation fetch failed: {e}");
                let _ = tx
                    .send(LoadResult::Annotation {
                        generation: r.generation,
                        track_index: idx,
                        transcripts: Vec::new(),
                    })
                    .await;
            }
        }
    }));
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/app/loader.rs
git commit -m "feat(igv-tui): Loader handles AnnotationSource per track"
```

Expected: build error in `main.rs` because `Loader::new` signature
changed — that's wired up in Task 4.1.

---

### Task 2.5: apply_load_result handles Annotation

**Files:**
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Extend `apply_load_result`**

Add a new arm in the `match result` block of `apply_load_result`:

```rust
LoadResult::Annotation { generation, track_index, transcripts } => {
    if generation == state.generation {
        if let Some(slot) = state.annotation_rows.get_mut(track_index) {
            *slot = transcripts;
        }
    }
}
```

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui  # main.rs `Loader::new` call still wrong; OK
git add crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): apply_load_result honors LoadResult::Annotation"
```

(Build will still fail until Task 4.1 because `Loader::new` call site
hasn't been updated. That's expected; we're committing in slices.)

---

### Task 2.6: Layout reserves annotation rows

**Files:**
- Modify: `crates/igv-tui/src/ui/layout.rs`

- [ ] **Step 1: Extend LayoutSpec and LayoutAreas**

In `layout.rs`:

Add to `LayoutAreas`:

```rust
pub annotations: Vec<ratatui::layout::Rect>,
```

Add to `LayoutSpec`:

```rust
pub annotation_tracks: usize,
pub annotation_height_per_track: u16,
```

In `LayoutSpec::default()`, set:

```rust
annotation_tracks: 0,
annotation_height_per_track: 3,
```

In `compute()`, after the `sequence` constraint and before the `if spec.has_vcf` block:

```rust
for _ in 0..spec.annotation_tracks {
    constraints.push(ratatui::layout::Constraint::Min(spec.annotation_height_per_track));
}
```

After the `idx` for sequence is consumed and before the variants `if spec.has_vcf` block:

```rust
let mut annotations = Vec::new();
for _ in 0..spec.annotation_tracks {
    annotations.push(chunks[idx]);
    idx += 1;
}
```

In the final `LayoutAreas { ... }` literal, add `annotations,`.

- [ ] **Step 2: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/layout.rs
git commit -m "feat(igv-tui): layout reserves N annotation panels above variants"
```

---

## Phase 3: AnnotationsWidget

### Task 3.1: Widget skeleton with lane stacking

**Files:**
- Modify: `crates/igv-tui/src/ui/widgets/mod.rs`
- Create: `crates/igv-tui/src/ui/widgets/annotations.rs`

- [ ] **Step 1: Register the module**

Append to `crates/igv-tui/src/ui/widgets/mod.rs`:

```rust
pub mod annotations;
```

- [ ] **Step 2: Write the widget**

`crates/igv-tui/src/ui/widgets/annotations.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;
use igv_core::source::{
    AnnotationBlock, AnnotationTranscript, BlockKind, Strand,
};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct AnnotationsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub track_index: usize,
}

impl Widget for AnnotationsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .state
            .annotations
            .get(self.track_index)
            .map(|t| t.display.clone())
            .unwrap_or_else(|| format!("annotation {}", self.track_index));
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.area() == 0 {
            return;
        }
        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if matches!(mode, RenderMode::OverviewOnly) {
            return;
        }

        let txs = match self.state.annotation_rows.get(self.track_index) {
            Some(r) => r,
            None => return,
        };
        if txs.is_empty() {
            return;
        }

        if matches!(mode, RenderMode::HeatBar) {
            draw_heatbar(buf, inner, region, txs, self.theme);
            return;
        }

        let lanes = stack_transcripts(txs, inner.height as usize);
        for (lane_idx, lane) in lanes.iter().enumerate() {
            let y = inner.y + lane_idx as u16;
            for tx in lane {
                draw_transcript(buf, inner, y, region, tx, self.theme);
            }
        }
    }
}

fn stack_transcripts<'a>(
    txs: &'a [AnnotationTranscript],
    lane_count: usize,
) -> Vec<Vec<&'a AnnotationTranscript>> {
    let mut lanes: Vec<Vec<&AnnotationTranscript>> = (0..lane_count).map(|_| Vec::new()).collect();
    'tx: for tx in txs {
        let (s, _e) = match tx.span() {
            Some(p) => p,
            None => continue,
        };
        for lane in lanes.iter_mut() {
            let last_end = lane
                .last()
                .and_then(|t| t.span())
                .map(|(_, e)| e)
                .unwrap_or(0);
            if last_end + 1 < s {
                lane.push(tx);
                continue 'tx;
            }
        }
    }
    lanes
}

fn draw_heatbar(
    buf: &mut Buffer,
    inner: Rect,
    region: &igv_core::region::Region,
    txs: &[AnnotationTranscript],
    theme: &Theme,
) {
    let style = theme.get("ANNOTATION_EXON");
    let view_start_0 = region.start - 1;
    let view_width = region.width();
    for tx in txs {
        for blk in &tx.blocks {
            let g0_start = blk.start.saturating_sub(1);
            let g0_end = blk.end.saturating_sub(1);
            let mut g = g0_start;
            while g <= g0_end {
                if let Some(col) = genomic_to_screen(g, view_start_0, view_width, inner.width as u32) {
                    if col < inner.width as u32 {
                        let cell = buf.get_mut(inner.x + col as u16, inner.y);
                        cell.set_char('▮').set_style(style);
                    }
                }
                g += 1;
            }
        }
    }
}

fn draw_transcript(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region: &igv_core::region::Region,
    tx: &AnnotationTranscript,
    theme: &Theme,
) {
    let view_start_0 = region.start - 1;
    let view_width = region.width();
    let intron_style = theme.get("ANNOTATION_INTRON");
    let utr_style = theme.get("ANNOTATION_UTR");
    let exon_style = theme.get("ANNOTATION_EXON");
    let strand_style = theme.get("ANNOTATION_STRAND");
    let name_style = theme.get("ANNOTATION_NAME");

    // 1. introns: a continuous line over the leftmost..rightmost block extent.
    if let Some((s, e)) = tx.span() {
        let mut g = s.saturating_sub(1);
        let g_end = e.saturating_sub(1);
        while g <= g_end {
            if let Some(col) = genomic_to_screen(g, view_start_0, view_width, inner.width as u32) {
                if col < inner.width as u32 {
                    let cell = buf.get_mut(inner.x + col as u16, y);
                    if cell.symbol().chars().next().unwrap_or(' ') == ' ' {
                        cell.set_char('─').set_style(intron_style);
                    }
                }
            }
            g += 1;
        }
    }

    // 2/3. UTRs first, then CDS / Exon / BedSegment so they overwrite.
    let mut blocks: Vec<&AnnotationBlock> = tx.blocks.iter().collect();
    blocks.sort_by_key(|b| match b.kind {
        BlockKind::Utr5 | BlockKind::Utr3 => 0,
        _ => 1,
    });
    for blk in blocks {
        let (glyph, style) = match blk.kind {
            BlockKind::Utr5 | BlockKind::Utr3 => ('▯', utr_style),
            BlockKind::Exon | BlockKind::Cds | BlockKind::BedSegment => ('▮', exon_style),
        };
        let g_start = blk.start.saturating_sub(1);
        let g_end = blk.end.saturating_sub(1);
        let mut g = g_start;
        while g <= g_end {
            if let Some(col) = genomic_to_screen(g, view_start_0, view_width, inner.width as u32) {
                if col < inner.width as u32 {
                    buf.get_mut(inner.x + col as u16, y).set_char(glyph).set_style(style);
                }
            }
            g += 1;
        }
    }

    // 4. strand glyph at rightmost column of the transcript.
    if let Some((_, e)) = tx.span() {
        let g0 = e.saturating_sub(1);
        if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            let glyph = match tx.strand {
                Strand::Forward => '>',
                Strand::Reverse => '<',
                Strand::Unknown => return,
            };
            if col < inner.width as u32 {
                buf.get_mut(inner.x + col as u16, y).set_char(glyph).set_style(strand_style);
            }
        }
    }

    // 5. name label, if it fits to the left of the leftmost block.
    if !tx.name.is_empty() {
        if let Some((s, _)) = tx.span() {
            let g0 = s.saturating_sub(1);
            if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
                let label = format!("{} ", tx.name);
                let needed = label.len() as u32;
                if col >= needed {
                    let start_col = col - needed;
                    for (i, ch) in label.chars().enumerate() {
                        if start_col as u16 + i as u16 >= inner.width {
                            break;
                        }
                        buf.get_mut(inner.x + start_col as u16 + i as u16, y)
                            .set_char(ch)
                            .set_style(name_style);
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: Build and commit**

```bash
cargo build -p igv-tui
git add crates/igv-tui/src/ui/widgets/mod.rs crates/igv-tui/src/ui/widgets/annotations.rs
git commit -m "feat(igv-tui): annotations widget with lane stacking and heatbar"
```

(Build still fails because `main.rs` and the `Loader::new` site mismatch.
Continues to be expected.)

---

## Phase 4: Wire main loop and finalize

### Task 4.1: Open annotations in main and update Loader::new call

**Files:**
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Add the annotation source loop near the BAM open loop**

After the existing `for path in &args.bam { ... }` loop in `main()`, add:

```rust
let mut annotations: Vec<crate::app::state::AnnotationTrack> = Vec::new();
let mut annotation_sources: Vec<std::sync::Arc<dyn igv_core::source::AnnotationSource>> =
    Vec::new();
let format_override = args
    .annotation_format
    .as_deref()
    .and_then(igv_core::source::AnnotationFormat::parse);
for path in &args.annotations {
    let src = igv_core::source::open_annotation(path, format_override).await?;
    annotations.push(crate::app::state::AnnotationTrack {
        path: path.clone(),
        display: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("annotation")
            .to_string(),
        source: std::sync::Arc::clone(&src),
    });
    annotation_sources.push(src);
}
```

In the `let mut state = AppState { ... };` literal, add:

```rust
annotations,
annotation_rows: vec![Vec::new(); annotation_sources.len()],
```

In the `Loader::new(...)` call, add `annotation_sources` between `bam_sources` and `tx`:

```rust
let mut loader = Loader::new(fasta, vcf, bam_sources, annotation_sources, tx);
```

- [ ] **Step 2: Build, smoke-test, commit**

```bash
cargo build
cargo test --workspace 2>&1 | tail -5
timeout 1s cargo run -p igv-tui -- crates/igv-core/tests/data/sample.fa \
    -g crates/igv-core/tests/data/sample.gff3 -r chr1:1-1000 2>&1 | tail -3
git add crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): open annotation sources from CLI and feed Loader"
```

Expected:
- Workspace test pass count ≥ 49 (37 prior + 5 GFF + 3 BED + 4 dispatch).
- Smoke run terminates on missing TTY without panicking.

---

### Task 4.2: draw() renders annotation panels

**Files:**
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Update LayoutSpec construction**

In `draw()`, change the `LayoutSpec` literal:

```rust
let spec = LayoutSpec {
    has_vcf: state.vcf.is_some(),
    bam_count: state.bams.len(),
    annotation_tracks: state.annotations.len(),
    ..Default::default()
};
```

- [ ] **Step 2: Render each annotation widget**

Right after the `f.render_widget(widgets::sequence::SequenceWidget { ... }, areas.sequence);`
line, insert:

```rust
for (i, area) in areas.annotations.iter().enumerate() {
    f.render_widget(
        widgets::annotations::AnnotationsWidget {
            state,
            theme: &state.theme,
            track_index: i,
        },
        *area,
    );
}
```

- [ ] **Step 3: Build and commit**

```bash
cargo build
git add crates/igv-tui/src/main.rs
git commit -m "feat(igv-tui): draw() renders annotation panels"
```

---

### Task 4.3: README repositioning + usage example

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README**

Replace the opening paragraph:

```markdown
# igv-rs

Interactive terminal genome viewer for FASTA / VCF / BAM / GFF / BED, written in Rust.
Inspired by [cligv](https://github.com/jonasfreudig/cligv) by Jonas Freudigmann.
```

Replace the "Layout" line about cligv:

```markdown
- `cligv/` — the project that inspired this work; kept locally as a
  reference and not part of this repository.
```

Append a usage example to the existing list:

```bash
igv-rs reference.fa -g genes.gff3
igv-rs reference.fa -g genes.gff3 -g peaks.bed -b sample.bam -r chr1:1000-2000
```

Add `-g <path>` and `--annotation-format` to the implicit doc by leaving
`--help` as the canonical reference. (No explicit additions here; the
clap `--help` is already up to date.)

In the "Known limitations" section, **remove** any line that becomes
obsolete after this work (none should). Leave the rest unchanged.

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README reframes igv-rs as inspired-by-cligv with annotation usage"
```

---

### Task 4.4: Final verification

- [ ] **Step 1: Run full workspace test suite**

```bash
cargo test --workspace 2>&1 | tail -10
```

Expected: at least 49 passing tests, 0 failed.

- [ ] **Step 2: Build release binary**

```bash
cargo build --release 2>&1 | tail -3
```

Expected: `Finished` with no errors.

- [ ] **Step 3: Smoke run with annotations**

```bash
timeout 1s cargo run --release -- \
    crates/igv-core/tests/data/sample.fa \
    -g crates/igv-core/tests/data/sample.gff3 \
    -g crates/igv-core/tests/data/sample.bed \
    -b crates/igv-core/tests/data/sample.bam \
    -r chr1:1-500 2>&1 | tail -5
```

Expected: process opens all sources, dispatches first load, exits when no
TTY is available — no panic, no `Error opening` lines.

- [ ] **Step 4: Push branch + open PR (optional)**

```bash
git push -u origin feat/annotations
gh pr create --title "Add GFF/GTF/BED annotation tracks" --body "$(cat <<'EOF'
## Summary
- Add GFF3 / GTF / BED annotation tracks via repeatable `-g/--annotation` flag
- Transcript-expanded rendering with exon/UTR/intron/strand glyphs
- noodles-gff and noodles-bed integration; in-memory loading with stale-data
  clearing on navigation
- Reframe project as inspired-by cligv

## Test plan
- [ ] `cargo test --workspace` — 49+ tests pass
- [ ] Manual: open with `-g sample.gff3 -g sample.bed` and verify annotation
      panels render between sequence and variants
- [ ] Manual: zoom out past 100 kb and verify heatbar collapse
- [ ] Manual: zoom out past 1 Mb and verify panels hidden

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

(Skip if you prefer to merge to main directly. Coordinate with the user.)

---

## Self-Review

### 1. Spec coverage

| Spec section | Tasks |
|--------------|-------|
| §3 Data model | 1.1 (types, trait, dispatcher) |
| §4.1 NoodlesGffSource | 1.3 |
| §4.2 NoodlesBedSource | 1.4 |
| §4.3 Format dispatcher | 1.1, 1.5 |
| §5.1 Layout slot | 2.6 |
| §5.2 Rendering (intron/UTR/exon/strand/name) | 3.1 |
| §5.3 RenderMode behavior (heatbar collapse, hidden in OverviewOnly) | 3.1 |
| §5.4 Theme keys | 2.1 |
| §6 CLI flag + format override | 2.2 |
| §7 AppState + Loader extension | 2.3, 2.4, 2.5 |
| §8 Error handling | 1.3, 1.4 (warn-and-continue + IgvError variants) |
| §9.1 Fixtures | 1.2 |
| §9.2 Tests | 1.3, 1.4, 1.5 |
| §9.3 No widget snapshot | (intentionally omitted) |
| §10 Project repositioning | 4.3 |

No gaps.

### 2. Placeholder scan

No "TBD" / "implement later" / "add validation" lurking. Every step that
changes code shows the code.

### 3. Type consistency

- `AnnotationTranscript`, `AnnotationBlock`, `BlockKind`, `Strand`,
  `TranscriptKind`, `AnnotationSource`, `AnnotationFormat` defined once in
  Task 1.1 and used unchanged across 1.3, 1.4, 1.5, 2.3, 2.4, 2.5, 3.1, 4.1.
- `Loader::new` signature change in Task 2.4 is reflected in the new call
  site in 4.1.
- `LoadResult::Annotation` variant fields match in 2.4 (producer) and 2.5
  (consumer).
- `LayoutSpec.annotation_tracks` set by `state.annotations.len()` in 4.2
  and consumed by `compute()` in 2.6.
- `AppState.annotation_rows` written in 2.5 and read in 3.1.

---

## Execution Handoff

Plan complete and saved to
`docs/superpowers/plans/2026-04-26-annotations.md`.

Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per phase, fast
   iteration, uses `superpowers:subagent-driven-development`.
2. **Inline Execution** — execute in-session via
   `superpowers:executing-plans`.
