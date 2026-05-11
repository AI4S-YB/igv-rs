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
    /// Display label: gene_name when known, otherwise gene_id (or
    /// transcript_id for GFF3 records lacking both).
    pub name: String,
    /// Transcript-level identifier (transcript_id for GTF, ID for GFF3,
    /// column-4 name for BED).
    pub id: String,
    /// Gene-level identifier when the source supplies one (gene_id for GTF,
    /// Parent gene ID for GFF3 mRNAs). `None` for BED.
    pub gene_id: Option<String>,
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

    /// Search loaded transcripts whose `name` (gene_name fallback gene_id),
    /// `gene_id`, or `id` (transcript_id) equals `query` case-insensitively.
    /// Returns `(chrom, transcript)` pairs so callers can construct a
    /// `Region`. The default returns nothing — backends with an in-memory
    /// index override.
    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        let _ = query;
        Vec::new()
    }
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
            "bed" | "narrowpeak" | "broadpeak" => Some(Self::Bed),
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
        if lower.ends_with(".bed")
            || lower.ends_with(".bed.gz")
            || lower.ends_with(".narrowpeak")
            || lower.ends_with(".narrowpeak.gz")
            || lower.ends_with(".broadpeak")
            || lower.ends_with(".broadpeak.gz")
        {
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

/// Multi-track gene-name → region resolver used by both the TUI command
/// palette and the HTTP `/api/jump` endpoint. Returns the union span of
/// all transcripts matching `query` on the first chromosome seen, plus a
/// display label (the first matched transcript's `name`).
pub fn find_by_name_union(
    sources: &[std::sync::Arc<dyn AnnotationSource>],
    query: &str,
) -> Option<(crate::region::Region, String)> {
    if query.is_empty() {
        return None;
    }
    let mut chrom: Option<String> = None;
    let mut span: Option<(u64, u64)> = None;
    let mut label: Option<String> = None;
    for src in sources {
        for (c, tx) in src.find_by_name(query) {
            let Some((s, e)) = tx.span() else { continue };
            match &chrom {
                None => {
                    chrom = Some(c);
                    span = Some((s, e));
                    label = Some(tx.name.clone());
                }
                Some(existing) if existing == &c => {
                    let (cs, ce) = span.unwrap();
                    span = Some((cs.min(s), ce.max(e)));
                }
                Some(_) => {}
            }
        }
    }
    let chrom = chrom?;
    let (s, e) = span?;
    let region = crate::region::Region::new(chrom, s, e).ok()?;
    Some((region, label.unwrap_or_else(|| query.to_string())))
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
            ("peaks.narrowPeak", Some(AnnotationFormat::Bed)),
            ("peaks.narrowPeak.gz", Some(AnnotationFormat::Bed)),
            ("peaks.broadPeak", Some(AnnotationFormat::Bed)),
            ("peaks.broadPeak.gz", Some(AnnotationFormat::Bed)),
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
            gene_id: None,
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
            gene_id: None,
            strand: Strand::Unknown,
            blocks: vec![],
            kind: TranscriptKind::Other,
        };
        assert_eq!(t.span(), None);
    }

    use crate::region::Region;
    use std::sync::Arc;

    struct StubSource {
        name: String,
        rows: Vec<(String, AnnotationTranscript)>,
    }

    #[async_trait]
    impl AnnotationSource for StubSource {
        async fn fetch(&self, _region: &Region) -> crate::Result<Vec<AnnotationTranscript>> {
            Ok(Vec::new())
        }
        fn display_name(&self) -> &str {
            &self.name
        }
        fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
            let q = query.to_ascii_lowercase();
            self.rows
                .iter()
                .filter(|(_, tx)| tx.name.to_ascii_lowercase() == q)
                .cloned()
                .collect()
        }
    }

    fn tx(name: &str, blocks: &[(u64, u64)]) -> AnnotationTranscript {
        AnnotationTranscript {
            id: name.into(),
            name: name.into(),
            gene_id: None,
            kind: TranscriptKind::Other,
            strand: Strand::Forward,
            blocks: blocks
                .iter()
                .map(|(s, e)| AnnotationBlock {
                    start: *s,
                    end: *e,
                    kind: BlockKind::Exon,
                })
                .collect(),
        }
    }

    #[test]
    fn find_by_name_union_unions_isoforms_on_same_chrom() {
        let src: Arc<dyn AnnotationSource> = Arc::new(StubSource {
            name: "stub".into(),
            rows: vec![
                ("chr1".into(), tx("BRCA1", &[(1000, 2000)])),
                ("chr1".into(), tx("BRCA1", &[(1500, 3000)])),
            ],
        });
        let (region, label) = find_by_name_union(&[src], "brca1").unwrap();
        assert_eq!(region.chrom, "chr1");
        assert_eq!(region.start, 1000);
        assert_eq!(region.end, 3000);
        assert_eq!(label, "BRCA1");
    }

    #[test]
    fn find_by_name_union_misses_return_none() {
        let src: Arc<dyn AnnotationSource> = Arc::new(StubSource {
            name: "stub".into(),
            rows: vec![],
        });
        assert!(find_by_name_union(&[src], "xyz").is_none());
    }
}
