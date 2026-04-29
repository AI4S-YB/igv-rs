//! BEDPE link source — in-memory IntervalMap per anchor side.
//!
//! BEDPE is a 10-column tab-separated format (extra columns ignored):
//!   chromA startA endA  chromB startB endB  name  score  strandA strandB
//! Coordinates are 0-based half-open on disk; we store them as 1-based
//! inclusive (`u64`) to match the rest of igv-core.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use iset::IntervalMap;
use tracing::warn;

use crate::error::{IgvError, Result};
use crate::region::Region;
use crate::source::annotation::Strand;
use crate::source::link::{FetchLinkOpts, LinkRecord, LinkScope, LinkSource, VisibleLink};

pub struct BedpeLinkSource {
    display: String,
    #[allow(dead_code)]
    path: PathBuf,
    records: Vec<LinkRecord>,
    /// Per-chromosome interval map keyed by anchor A's [start, end+1).
    /// Values are indices into `records`.
    tree_a: HashMap<Arc<str>, IntervalMap<u64, usize>>,
    /// Same for anchor B.
    tree_b: HashMap<Arc<str>, IntervalMap<u64, usize>>,
}

impl std::fmt::Debug for BedpeLinkSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BedpeLinkSource")
            .field("display", &self.display)
            .field("records", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl BedpeLinkSource {
    pub async fn open(path: &Path) -> Result<Self> {
        let display = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("link")
            .to_string();
        let p = path.to_path_buf();
        let (records, tree_a, tree_b) =
            tokio::task::spawn_blocking(move || -> Result<_> { load(&p) })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))??;
        Ok(Self {
            display,
            path: path.to_path_buf(),
            records,
            tree_a,
            tree_b,
        })
    }

    /// Test-only lookup by `name`. Returns the first match.
    #[cfg(test)]
    pub(crate) fn record_at_name(&self, name: &str) -> Option<&LinkRecord> {
        self.records
            .iter()
            .find(|r| r.name.as_deref() == Some(name))
    }

    /// Test-only: return all records whose name is missing (was `.`).
    #[cfg(test)]
    pub(crate) fn unnamed_records(&self) -> Vec<&LinkRecord> {
        self.records.iter().filter(|r| r.name.is_none()).collect()
    }
}

#[async_trait]
impl LinkSource for BedpeLinkSource {
    async fn query(
        &self,
        region: &Region,
        opts: &FetchLinkOpts,
    ) -> Result<Vec<VisibleLink>> {
        // IntervalMap is half-open; convert region [start, end] → [start, end+1).
        let lo = region.start;
        let hi = region.end.saturating_add(1);
        let chrom = region.chrom.as_str();

        // Collect candidate indices from both per-chromosome trees.
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        if let Some(t) = self.tree_a.get(chrom) {
            for (_, &idx) in t.iter(lo..hi) {
                seen.insert(idx);
            }
        }
        if let Some(t) = self.tree_b.get(chrom) {
            for (_, &idx) in t.iter(lo..hi) {
                seen.insert(idx);
            }
        }

        let mut out = Vec::with_capacity(seen.len());
        for idx in seen {
            let rec = &self.records[idx];

            if let (Some(min), Some(s)) = (opts.min_score, rec.score) {
                if s < min {
                    continue;
                }
            }

            let a_in = rec.chrom_a.as_ref() == chrom
                && rec.end_a >= region.start
                && rec.start_a <= region.end;
            let b_in = rec.chrom_b.as_ref() == chrom
                && rec.end_b >= region.start
                && rec.start_b <= region.end;

            let scope = match (a_in, b_in) {
                (true, true) => LinkScope::BothIn,
                (true, false) => {
                    if rec.is_trans() {
                        LinkScope::Trans {
                            off_chrom: Arc::clone(&rec.chrom_b),
                            off_anchor_mid: midpoint(rec.start_b, rec.end_b),
                        }
                    } else {
                        let mid = midpoint(rec.start_b, rec.end_b);
                        LinkScope::PartialCis {
                            off_anchor_mid: mid,
                            off_to_left: mid < region.start,
                        }
                    }
                }
                (false, true) => {
                    if rec.is_trans() {
                        LinkScope::Trans {
                            off_chrom: Arc::clone(&rec.chrom_a),
                            off_anchor_mid: midpoint(rec.start_a, rec.end_a),
                        }
                    } else {
                        let mid = midpoint(rec.start_a, rec.end_a);
                        LinkScope::PartialCis {
                            off_anchor_mid: mid,
                            off_to_left: mid < region.start,
                        }
                    }
                }
                (false, false) => continue, // safety net: tree surfaced it but neither anchor overlaps
            };
            out.push(VisibleLink {
                record: rec.clone(),
                scope,
            });
        }
        Ok(out)
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn record_count(&self) -> usize {
        self.records.len()
    }
}

fn midpoint(s: u64, e: u64) -> u64 {
    s + (e - s) / 2
}

fn load(
    path: &Path,
) -> Result<(
    Vec<LinkRecord>,
    HashMap<Arc<str>, IntervalMap<u64, usize>>,
    HashMap<Arc<str>, IntervalMap<u64, usize>>,
)> {
    let file = std::fs::File::open(path).map_err(|e| IgvError::io(path, e))?;
    let lower = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_ascii_lowercase());
    let reader: Box<dyn BufRead> = if lower
        .as_deref()
        .is_some_and(|s| s.ends_with(".gz"))
    {
        Box::new(std::io::BufReader::new(flate2::read::MultiGzDecoder::new(file)))
    } else {
        Box::new(std::io::BufReader::new(file))
    };

    let mut records: Vec<LinkRecord> = Vec::new();
    let mut tree_a: HashMap<Arc<str>, IntervalMap<u64, usize>> = HashMap::new();
    let mut tree_b: HashMap<Arc<str>, IntervalMap<u64, usize>> = HashMap::new();

    for (i, line_res) in reader.lines().enumerate() {
        let lineno = i + 1;
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                warn!("bedpe {}: read error at line {lineno}: {e}", path.display());
                continue;
            }
        };
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with('#')
            || trimmed.starts_with("track ")
            || trimmed.starts_with("browser ")
        {
            continue;
        }
        match parse_line(trimmed, lineno) {
            Ok(rec) => {
                let (sa, ea) = (rec.start_a, rec.end_a);
                let (sb, eb) = (rec.start_b, rec.end_b);
                // Guard against u64::MAX which would make an empty interval
                // after saturating_add(1).
                if ea == u64::MAX || eb == u64::MAX {
                    warn!(
                        "bedpe {}: line {lineno}: anchor end at u64::MAX is not representable; skipping",
                        path.display()
                    );
                    continue;
                }
                let idx = records.len();
                let chrom_a = Arc::clone(&rec.chrom_a);
                let chrom_b = Arc::clone(&rec.chrom_b);
                records.push(rec);
                // IntervalMap uses half-open [start, end); we store
                // inclusive [s, e] as half-open [s, e+1).
                tree_a
                    .entry(chrom_a)
                    .or_insert_with(IntervalMap::new)
                    .force_insert(sa..ea.saturating_add(1), idx);
                tree_b
                    .entry(chrom_b)
                    .or_insert_with(IntervalMap::new)
                    .force_insert(sb..eb.saturating_add(1), idx);
            }
            Err(e) => warn!("bedpe {}: line {lineno}: {e}; skipping", path.display()),
        }
    }
    Ok((records, tree_a, tree_b))
}

fn parse_line(line: &str, lineno: usize) -> Result<LinkRecord> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 6 {
        return Err(IgvError::Other(format!(
            "bedpe line {lineno}: too few columns ({})",
            cols.len()
        )));
    }
    let parse_u64 = |s: &str, what: &str| -> Result<u64> {
        s.parse::<u64>().map_err(|_| {
            IgvError::Other(format!("bedpe line {lineno}: invalid {what}: {s:?}"))
        })
    };
    let chrom_a = Arc::<str>::from(cols[0]);
    let start_a_zb = parse_u64(cols[1], "startA")?;
    let end_a_zb = parse_u64(cols[2], "endA")?;
    let chrom_b = Arc::<str>::from(cols[3]);
    let start_b_zb = parse_u64(cols[4], "startB")?;
    let end_b_zb = parse_u64(cols[5], "endB")?;
    if end_a_zb <= start_a_zb || end_b_zb <= start_b_zb {
        return Err(IgvError::Other(format!(
            "bedpe line {lineno}: anchor end ≤ start"
        )));
    }
    let name = match cols.get(6).copied() {
        Some(".") | None | Some("") => None,
        Some(n) => Some(n.to_string()),
    };
    let score = match cols.get(7).copied() {
        Some(".") | None | Some("") => None,
        Some(s) => match s.parse::<f64>() {
            Ok(v) => Some(v),
            Err(_) => {
                warn!(
                    "bedpe line {lineno}: non-numeric score {s:?}; treating as missing"
                );
                None
            }
        },
    };
    let parse_strand = |s: Option<&str>| match s {
        Some("+") => Strand::Forward,
        Some("-") => Strand::Reverse,
        _ => Strand::Unknown,
    };
    let strand_a = parse_strand(cols.get(8).copied());
    let strand_b = parse_strand(cols.get(9).copied());

    Ok(LinkRecord {
        chrom_a,
        // 0-based half-open [s, e) → 1-based inclusive [s+1, e]
        start_a: start_a_zb + 1,
        end_a: end_a_zb,
        chrom_b,
        start_b: start_b_zb + 1,
        end_b: end_b_zb,
        name,
        score,
        strand_a,
        strand_b,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/sample.bedpe")
    }

    #[tokio::test]
    async fn open_parses_valid_records_and_skips_malformed() {
        let src = BedpeLinkSource::open(&fixture()).await.unwrap();
        // 7 valid records, 1 malformed line skipped.
        assert_eq!(src.record_count(), 7);
        assert_eq!(src.display_name(), "sample.bedpe");
    }

    #[tokio::test]
    async fn missing_optional_columns_become_none_or_unknown() {
        use crate::source::annotation::Strand;
        let src = BedpeLinkSource::open(&fixture()).await.unwrap();
        let l2 = src.record_at_name("loop2").expect("loop2");
        assert_eq!(l2.score, Some(2.0));
        // The record at chr1:2000001-2001000 has all dot-fields (name/score/strands).
        let unnamed: Vec<_> = src.unnamed_records();
        assert_eq!(unnamed.len(), 1, "fixture has exactly one all-dot record");
        let dot = unnamed[0];
        assert_eq!(dot.score, None);
        assert_eq!(dot.name, None);
        assert!(matches!(dot.strand_a, Strand::Unknown));
        assert!(matches!(dot.strand_b, Strand::Unknown));
    }

    #[tokio::test]
    async fn coordinates_are_converted_to_one_based_inclusive() {
        let src = BedpeLinkSource::open(&fixture()).await.unwrap();
        let l1 = src.record_at_name("loop1").expect("loop1");
        // BEDPE 0-based half-open [1000000, 1001000) →
        // 1-based inclusive [1000001, 1001000].
        assert_eq!(l1.start_a, 1_000_001);
        assert_eq!(l1.end_a, 1_001_000);
        assert_eq!(l1.start_b, 1_009_001);
        assert_eq!(l1.end_b, 1_010_000);
    }
}
