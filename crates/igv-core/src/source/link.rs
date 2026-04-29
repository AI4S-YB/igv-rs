//! Pairwise link source — interactions between two genomic anchors
//! (BEDPE today; UCSC interact / pairix in the future). Renders as
//! adaptive arc / heatmap widgets on the TUI side and Bézier curves
//! on the SVG side.
//!
//! All coordinates here are u64 1-based inclusive to match `Region`,
//! `SignalBin`, and `AnnotationBlock`. The on-disk BEDPE parser
//! converts from 0-based half-open at read time.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::error::{IgvError, Result};
use crate::region::Region;
use crate::source::annotation::Strand;

pub mod bedpe;

#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub chrom_a: Arc<str>,
    pub start_a: u64,
    pub end_a: u64,
    pub chrom_b: Arc<str>,
    pub start_b: u64,
    pub end_b: u64,
    pub name: Option<String>,
    pub score: Option<f64>,
    pub strand_a: Strand,
    pub strand_b: Strand,
}

impl LinkRecord {
    pub fn is_trans(&self) -> bool {
        self.chrom_a != self.chrom_b
    }

    /// Envelope of both anchors on a single chromosome (cis only).
    /// Returns `None` for trans records.
    pub fn cis_span(&self) -> Option<(u64, u64)> {
        if self.is_trans() {
            return None;
        }
        let lo = self.start_a.min(self.start_b);
        let hi = self.end_a.max(self.end_b);
        Some((lo, hi))
    }
}

/// How a record relates to the visible region — drives widget rendering.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LinkScope {
    /// Both anchors overlap the region.
    BothIn,
    /// Exactly one anchor overlaps; the other is on the same chromosome
    /// but outside the visible window.
    PartialCis {
        off_anchor_mid: u64,
        off_to_left: bool,
    },
    /// One anchor overlaps; the other is on a different chromosome.
    Trans {
        off_chrom: Arc<str>,
        off_anchor_mid: u64,
    },
}

/// A visible link plus its rendering scope. Owned (cloned out of the source).
#[derive(Debug, Clone)]
pub struct VisibleLink {
    pub record: LinkRecord,
    pub scope: LinkScope,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FetchLinkOpts {
    /// Drop records whose `score.is_some()` and whose value is below this.
    /// `None` → no filter; records without a score are never filtered.
    pub min_score: Option<f64>,
}

#[async_trait]
pub trait LinkSource: Send + Sync {
    async fn query(
        &self,
        region: &Region,
        opts: &FetchLinkOpts,
    ) -> Result<Vec<VisibleLink>>;
    fn display_name(&self) -> &str;
    fn record_count(&self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkFormat {
    Bedpe,
}

impl LinkFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bedpe" => Some(Self::Bedpe),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        if lower.ends_with(".bedpe") || lower.ends_with(".bedpe.gz") {
            return Some(Self::Bedpe);
        }
        None
    }
}

/// Open a link file, dispatching to the right backend by extension
/// (or by `format_override` if given).
pub async fn open_link(
    path: &Path,
    format_override: Option<LinkFormat>,
) -> Result<Arc<dyn LinkSource>> {
    let format = format_override
        .or_else(|| LinkFormat::from_path(path))
        .ok_or_else(|| {
            IgvError::Other(format!(
                "cannot determine link format for '{}'; pass --link-format",
                path.display()
            ))
        })?;
    match format {
        LinkFormat::Bedpe => {
            let src = bedpe::BedpeLinkSource::open(path).await?;
            Ok(Arc::new(src))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::source::annotation::Strand;

    fn rec(chrom_a: &str, sa: u64, ea: u64, chrom_b: &str, sb: u64, eb: u64) -> LinkRecord {
        LinkRecord {
            chrom_a: Arc::from(chrom_a),
            start_a: sa,
            end_a: ea,
            chrom_b: Arc::from(chrom_b),
            start_b: sb,
            end_b: eb,
            name: None,
            score: None,
            strand_a: Strand::Unknown,
            strand_b: Strand::Unknown,
        }
    }

    #[test]
    fn is_trans_detects_chromosome_mismatch() {
        assert!(!rec("chr1", 1, 10, "chr1", 20, 30).is_trans());
        assert!(rec("chr1", 1, 10, "chr2", 20, 30).is_trans());
    }

    #[test]
    fn cis_span_returns_none_for_trans() {
        assert_eq!(rec("chr1", 1, 10, "chr2", 20, 30).cis_span(), None);
    }

    #[test]
    fn cis_span_returns_min_max_envelope() {
        // anchor A: 100..200, anchor B: 50..80 → envelope 50..200
        assert_eq!(
            rec("chr1", 100, 200, "chr1", 50, 80).cis_span(),
            Some((50, 200))
        );
    }
}
