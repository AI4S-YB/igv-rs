//! Pairwise link source — interactions between two genomic anchors
//! (BEDPE + tabix-indexed `.bedpe.gz` + UCSC `bigInteract` `.bb`).

use async_trait::async_trait;
use std::path::Path;
use std::sync::{Arc, Mutex};

use bigtools::BigBedRead;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{Location, SeekableStream};
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

    pub fn cis_span(&self) -> Option<(u64, u64)> {
        if self.is_trans() {
            return None;
        }
        let lo = self.start_a.min(self.start_b);
        let hi = self.end_a.max(self.end_b);
        Some((lo, hi))
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LinkScope {
    BothIn,
    PartialCis {
        off_anchor_mid: u64,
        off_to_left: bool,
    },
    Trans {
        off_chrom: Arc<str>,
        off_anchor_mid: u64,
    },
}

#[derive(Debug, Clone)]
pub struct VisibleLink {
    pub record: LinkRecord,
    pub scope: LinkScope,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FetchLinkOpts {
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
    fn all_records(&self) -> Vec<LinkRecord> {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkFormat {
    Bedpe,
    BigInteract,
}

impl LinkFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bedpe" => Some(Self::Bedpe),
            "biginteract" | "bb" => Some(Self::BigInteract),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        Self::from_name(&lower)
    }

    pub fn from_name(lower: &str) -> Option<Self> {
        if lower.ends_with(".bedpe") || lower.ends_with(".bedpe.gz") {
            return Some(Self::Bedpe);
        }
        if lower.ends_with(".biginteract")
            || lower.ends_with(".inter.bb")
            || lower.ends_with(".interact.bb")
            || lower.ends_with(".bb")
        {
            return Some(Self::BigInteract);
        }
        None
    }
}

/// UCSC `bigInteract` (`.bb`) pairwise link reader backed by `bigtools`.
pub struct BigInteractLinkSource {
    display: String,
    reader: Arc<Mutex<BigBedRead<SeekableStream>>>,
    chroms: Vec<String>,
}

impl std::fmt::Debug for BigInteractLinkSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigInteractLinkSource")
            .field("display", &self.display)
            .field("chroms", &self.chroms.len())
            .finish_non_exhaustive()
    }
}

impl BigInteractLinkSource {
    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let stream = SeekableStream::open(location, auth_token).await?;
        let reader = tokio::task::spawn_blocking(move || -> Result<BigBedRead<SeekableStream>> {
            BigBedRead::open(stream)
                .map_err(|e| IgvError::Other(format!("bigInteract open: {e}")))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        let chroms = reader.chroms().iter().map(|c| c.name.clone()).collect();
        Ok(Self {
            display,
            reader: Arc::new(Mutex::new(reader)),
            chroms,
        })
    }
}

#[async_trait]
impl LinkSource for BigInteractLinkSource {
    async fn query(
        &self,
        region: &Region,
        opts: &FetchLinkOpts,
    ) -> Result<Vec<VisibleLink>> {
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chroms).map(str::to_string)
        else {
            return Ok(Vec::new());
        };
        let start = region.start.saturating_sub(1) as u32;
        let end = region.end.min(u32::MAX as u64) as u32;
        let min_score = opts.min_score;
        let reg = region.clone();
        let reader = Arc::clone(&self.reader);

        tokio::task::spawn_blocking(move || -> Result<Vec<VisibleLink>> {
            let mut guard = reader
                .lock()
                .map_err(|_| IgvError::Other("bigInteract mutex poisoned".into()))?;
            let entries: Vec<_> = guard
                .get_interval(&chrom, start, end)
                .map_err(|e| IgvError::Other(format!("bigInteract query: {e}")))?
                .filter_map(|r| r.ok())
                .collect();
            drop(guard);

            let mut out = Vec::new();
            for entry in entries {
                let cols: Vec<&str> = entry.rest.split('\t').collect();
                if cols.len() < 15 {
                    continue;
                }
                let name = match cols[0].trim() {
                    "" | "." => None,
                    s => Some(s.to_string()),
                };
                let value = cols[2].parse::<f64>().ok().filter(|v| v.is_finite());
                let score_int = cols[1].parse::<f64>().ok().filter(|v| v.is_finite());
                let score = value.or(score_int);
                if let (Some(min), Some(s)) = (min_score, score) {
                    if s < min {
                        continue;
                    }
                }
                let chrom_a = Arc::<str>::from(cols[5]);
                let start_a_0 = cols[6].parse::<u64>().unwrap_or(0);
                let end_a_0 = cols[7].parse::<u64>().unwrap_or(start_a_0 + 1);
                let strand_a = match cols[9] {
                    "+" => Strand::Forward,
                    "-" => Strand::Reverse,
                    _ => Strand::Unknown,
                };
                let chrom_b = Arc::<str>::from(cols[10]);
                let start_b_0 = cols[11].parse::<u64>().unwrap_or(0);
                let end_b_0 = cols[12].parse::<u64>().unwrap_or(start_b_0 + 1);
                let strand_b = match cols[14].trim() {
                    "+" => Strand::Forward,
                    "-" => Strand::Reverse,
                    _ => Strand::Unknown,
                };
                let rec = LinkRecord {
                    chrom_a,
                    start_a: start_a_0 + 1,
                    end_a: end_a_0.max(start_a_0 + 1),
                    chrom_b,
                    start_b: start_b_0 + 1,
                    end_b: end_b_0.max(start_b_0 + 1),
                    name,
                    score,
                    strand_a,
                    strand_b,
                };
                if let Some(vis) = bedpe::scope_link(&rec, &chrom, &reg) {
                    out.push(vis);
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn record_count(&self) -> usize {
        0
    }
}

pub async fn open_link(
    path: &Path,
    format_override: Option<LinkFormat>,
) -> Result<Arc<dyn LinkSource>> {
    let loc = Location::Local(path.to_path_buf());
    open_link_location(&loc, format_override, None).await
}

pub async fn open_link_location(
    location: &Location,
    format_override: Option<LinkFormat>,
    auth_token: Option<&str>,
) -> Result<Arc<dyn LinkSource>> {
    let format = format_override
        .or_else(|| LinkFormat::from_name(&location.lowercase_name()))
        .ok_or_else(|| {
            IgvError::Other(format!(
                "cannot determine link format for '{}'; pass --link-format",
                location.as_str()
            ))
        })?;
    match format {
        LinkFormat::Bedpe => {
            let src = bedpe::BedpeLinkSource::open_location(location, auth_token).await?;
            Ok(Arc::new(src))
        }
        LinkFormat::BigInteract => {
            let src = BigInteractLinkSource::open_location(location, auth_token).await?;
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
        assert!(!rec("chr1", 10, 20, "chr1", 30, 40).is_trans());
        assert!(rec("chr1", 10, 20, "chr2", 30, 40).is_trans());
    }

    #[test]
    fn cis_span_returns_min_max_envelope() {
        assert_eq!(rec("chr1", 50, 60, "chr1", 10, 20).cis_span(), Some((10, 60)));
    }

    #[test]
    fn cis_span_returns_none_for_trans() {
        assert_eq!(rec("chr1", 10, 20, "chr2", 30, 40).cis_span(), None);
    }
}
