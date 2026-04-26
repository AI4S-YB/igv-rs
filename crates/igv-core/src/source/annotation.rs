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
