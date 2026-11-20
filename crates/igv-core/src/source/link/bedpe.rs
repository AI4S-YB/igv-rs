//! BEDPE link source — tabix-indexed `.bedpe.gz` streaming OR in-memory
//! `IntervalMap` per anchor side (local & remote URLs supported).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use iset::IntervalMap;
use noodles::csi::{self, BinningIndex};
use noodles::tabix;
use tracing::warn;

type LoadResult = (
    Vec<LinkRecord>,
    HashMap<Arc<str>, IntervalMap<u64, usize>>,
    HashMap<Arc<str>, IntervalMap<u64, usize>>,
);

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, maybe_gunzip, Location, SeekableStream};
use crate::region::Region;
use crate::source::annotation::Strand;
use crate::source::link::{FetchLinkOpts, LinkRecord, LinkScope, LinkSource, VisibleLink};

enum BedpeBackend {
    InMemory {
        records: Vec<LinkRecord>,
        tree_a: HashMap<Arc<str>, IntervalMap<u64, usize>>,
        tree_b: HashMap<Arc<str>, IntervalMap<u64, usize>>,
        chroms: Vec<String>,
    },
    TabixTbi {
        location: Location,
        remote_stream: Option<SeekableStream>,
        index: Arc<tabix::Index>,
        chroms: Vec<String>,
    },
    TabixCsi {
        location: Location,
        remote_stream: Option<SeekableStream>,
        index: Arc<csi::Index>,
        chroms: Vec<String>,
    },
}

pub struct BedpeLinkSource {
    display: String,
    backend: BedpeBackend,
}

impl std::fmt::Debug for BedpeLinkSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BedpeLinkSource")
            .field("display", &self.display)
            .field("records", &self.record_count())
            .finish_non_exhaustive()
    }
}

impl BedpeLinkSource {
    pub async fn open(path: &Path) -> Result<Self> {
        let loc = Location::Local(path.to_path_buf());
        Self::open_location(&loc, None).await
    }

    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let tbi_loc = location.sibling("tbi");
        let csi_loc = location.sibling("csi");

        if location_exists(&tbi_loc, auth_token).await {
            let idx_bytes = fetch_bytes(&tbi_loc, auth_token).await?;
            let index = tokio::task::spawn_blocking(move || {
                let mut r = tabix::io::Reader::new(Cursor::new(idx_bytes));
                r.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            let remote_stream = if location.is_remote() {
                Some(SeekableStream::open(location, auth_token).await?)
            } else {
                None
            };
            let chroms = index
                .header()
                .map(|h| {
                    h.reference_sequence_names()
                        .iter()
                        .map(|s| String::from_utf8_lossy(s.as_ref()).into_owned())
                        .collect()
                })
                .unwrap_or_default();
            return Ok(Self {
                display,
                backend: BedpeBackend::TabixTbi {
                    location: location.clone(),
                    remote_stream,
                    index: Arc::new(index),
                    chroms,
                },
            });
        }

        if location_exists(&csi_loc, auth_token).await {
            let idx_bytes = fetch_bytes(&csi_loc, auth_token).await?;
            let index = tokio::task::spawn_blocking(move || {
                let mut r = csi::io::Reader::new(Cursor::new(idx_bytes));
                r.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            let remote_stream = if location.is_remote() {
                Some(SeekableStream::open(location, auth_token).await?)
            } else {
                None
            };
            let chroms = index
                .header()
                .map(|h| {
                    h.reference_sequence_names()
                        .iter()
                        .map(|s| String::from_utf8_lossy(s.as_ref()).into_owned())
                        .collect()
                })
                .unwrap_or_default();
            return Ok(Self {
                display,
                backend: BedpeBackend::TabixCsi {
                    location: location.clone(),
                    remote_stream,
                    index: Arc::new(index),
                    chroms,
                },
            });
        }

        let raw = fetch_bytes(location, auth_token).await?;
        let label = display.clone();
        let (records, tree_a, tree_b) = tokio::task::spawn_blocking(move || -> Result<_> {
            let plain = maybe_gunzip(raw)?;
            load_from_reader(BufReader::new(Cursor::new(plain)), &label)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let mut chrom_set: HashSet<String> = HashSet::new();
        for k in tree_a.keys().chain(tree_b.keys()) {
            chrom_set.insert(k.to_string());
        }
        let chroms = chrom_set.into_iter().collect();

        Ok(Self {
            display,
            backend: BedpeBackend::InMemory {
                records,
                tree_a,
                tree_b,
                chroms,
            },
        })
    }

    #[cfg(test)]
    pub(crate) fn record_at_name(&self, name: &str) -> Option<&LinkRecord> {
        let BedpeBackend::InMemory { records, .. } = &self.backend else {
            return None;
        };
        records.iter().find(|r| r.name.as_deref() == Some(name))
    }

    #[cfg(test)]
    pub(crate) fn unnamed_records(&self) -> Vec<&LinkRecord> {
        let BedpeBackend::InMemory { records, .. } = &self.backend else {
            return Vec::new();
        };
        records.iter().filter(|r| r.name.is_none()).collect()
    }
}

pub(crate) fn scope_link(rec: &LinkRecord, chrom: &str, region: &Region) -> Option<VisibleLink> {
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
        (false, false) => return None,
    };
    Some(VisibleLink {
        record: rec.clone(),
        scope,
    })
}

#[async_trait]
impl LinkSource for BedpeLinkSource {
    async fn query(
        &self,
        region: &Region,
        opts: &FetchLinkOpts,
    ) -> Result<Vec<VisibleLink>> {
        match &self.backend {
            BedpeBackend::InMemory {
                records,
                tree_a,
                tree_b,
                chroms,
            } => {
                let Some(chrom) = resolve_chrom_name(&region.chrom, chroms) else {
                    return Ok(Vec::new());
                };
                let lo = region.start;
                let hi = region.end.saturating_add(1);

                let mut seen: HashSet<usize> = HashSet::new();
                if let Some(t) = tree_a.get(chrom) {
                    for (_, &idx) in t.iter(lo..hi) {
                        seen.insert(idx);
                    }
                }
                if let Some(t) = tree_b.get(chrom) {
                    for (_, &idx) in t.iter(lo..hi) {
                        seen.insert(idx);
                    }
                }

                let mut out = Vec::with_capacity(seen.len());
                for idx in seen {
                    let rec = &records[idx];
                    if let (Some(min), Some(s)) = (opts.min_score, rec.score) {
                        if s < min {
                            continue;
                        }
                    }
                    if let Some(v) = scope_link(rec, chrom, region) {
                        out.push(v);
                    }
                }
                Ok(out)
            }
            BedpeBackend::TabixTbi {
                location,
                remote_stream,
                index,
                chroms,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = open_stream(location, remote_stream.as_ref())?;
                let index = Arc::clone(index);
                let reg = region.clone();
                let min_score = opts.min_score;
                tokio::task::spawn_blocking(move || -> Result<Vec<VisibleLink>> {
                    let region_str = format!("{resolved}:{}-{}", reg.start, reg.end);
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut out = Vec::new();
                    for (idx, res) in reader.query(&r).map_err(IgvError::noodles)?.enumerate() {
                        let line_rec = res.map_err(IgvError::noodles)?;
                        if let Ok(rec) = parse_line(line_rec.as_ref(), idx + 1) {
                            if let (Some(min), Some(s)) = (min_score, rec.score) {
                                if s < min {
                                    continue;
                                }
                            }
                            if let Some(v) = scope_link(&rec, &resolved, &reg) {
                                out.push(v);
                            }
                        }
                    }
                    Ok(out)
                })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))?
            }
            BedpeBackend::TabixCsi {
                location,
                remote_stream,
                index,
                chroms,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = open_stream(location, remote_stream.as_ref())?;
                let index = Arc::clone(index);
                let reg = region.clone();
                let min_score = opts.min_score;
                tokio::task::spawn_blocking(move || -> Result<Vec<VisibleLink>> {
                    let region_str = format!("{resolved}:{}-{}", reg.start, reg.end);
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut out = Vec::new();
                    for (idx, res) in reader.query(&r).map_err(IgvError::noodles)?.enumerate() {
                        let line_rec = res.map_err(IgvError::noodles)?;
                        if let Ok(rec) = parse_line(line_rec.as_ref(), idx + 1) {
                            if let (Some(min), Some(s)) = (min_score, rec.score) {
                                if s < min {
                                    continue;
                                }
                            }
                            if let Some(v) = scope_link(&rec, &resolved, &reg) {
                                out.push(v);
                            }
                        }
                    }
                    Ok(out)
                })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))?
            }
        }
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn record_count(&self) -> usize {
        match &self.backend {
            BedpeBackend::InMemory { records, .. } => records.len(),
            BedpeBackend::TabixTbi { .. } | BedpeBackend::TabixCsi { .. } => 0,
        }
    }

    fn all_records(&self) -> Vec<LinkRecord> {
        match &self.backend {
            BedpeBackend::InMemory { records, .. } => records.clone(),
            _ => Vec::new(),
        }
    }
}

fn open_stream(
    location: &Location,
    remote_stream: Option<&SeekableStream>,
) -> Result<SeekableStream> {
    match (location, remote_stream) {
        (Location::Local(p), _) => SeekableStream::open_local(p),
        (Location::Remote(_), Some(rs)) => rs
            .clone_handle()
            .map_err(|e| IgvError::Other(e.to_string())),
        (Location::Remote(u), None) => Err(IgvError::Other(format!("no stream for {u}"))),
    }
}

fn midpoint(s: u64, e: u64) -> u64 {
    s + (e - s) / 2
}

fn load_from_reader<R: BufRead>(reader: R, label: &str) -> Result<LoadResult> {
    let mut records: Vec<LinkRecord> = Vec::new();
    let mut tree_a: HashMap<Arc<str>, IntervalMap<u64, usize>> = HashMap::new();
    let mut tree_b: HashMap<Arc<str>, IntervalMap<u64, usize>> = HashMap::new();

    for (i, line_res) in reader.lines().enumerate() {
        let lineno = i + 1;
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                warn!("bedpe {label}: read error at line {lineno}: {e}");
                continue;
            }
        };
        let trimmed = line.trim_end();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("track ")
            || trimmed.starts_with("browser ")
        {
            continue;
        }
        match parse_line(trimmed, lineno) {
            Ok(rec) => {
                let (sa, ea) = (rec.start_a, rec.end_a);
                let (sb, eb) = (rec.start_b, rec.end_b);
                if ea == u64::MAX || eb == u64::MAX {
                    warn!("bedpe {label}: line {lineno}: anchor end at u64::MAX; skipping");
                    continue;
                }
                let idx = records.len();
                let chrom_a = Arc::clone(&rec.chrom_a);
                let chrom_b = Arc::clone(&rec.chrom_b);
                records.push(rec);
                tree_a
                    .entry(chrom_a)
                    .or_default()
                    .force_insert(sa..ea.saturating_add(1), idx);
                tree_b
                    .entry(chrom_b)
                    .or_default()
                    .force_insert(sb..eb.saturating_add(1), idx);
            }
            Err(e) => warn!("bedpe {label}: line {lineno}: {e}; skipping"),
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
            "bedpe line {lineno}: anchor end <= start"
        )));
    }
    if start_a_zb == u64::MAX || start_b_zb == u64::MAX {
        return Err(IgvError::Other(format!(
            "bedpe line {lineno}: anchor start at u64::MAX"
        )));
    }
    let name = match cols.get(6).copied() {
        Some(".") | None | Some("") => None,
        Some(n) => Some(n.to_string()),
    };
    let score = match cols.get(7).copied() {
        Some(".") | None | Some("") => None,
        Some(s) => match s.parse::<f64>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => None,
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/sample.bedpe")
    }

    #[tokio::test]
    async fn open_parses_valid_records_and_skips_malformed() {
        let src = BedpeLinkSource::open(&fixture()).await.unwrap();
        assert_eq!(src.record_count(), 7);
        assert_eq!(src.display_name(), "sample.bedpe");
    }

    #[tokio::test]
    async fn missing_optional_columns_become_none_or_unknown() {
        use crate::source::annotation::Strand;
        let src = BedpeLinkSource::open(&fixture()).await.unwrap();
        let l2 = src.record_at_name("loop2").expect("loop2");
        assert_eq!(l2.score, Some(2.0));
        let unnamed: Vec<_> = src.unnamed_records();
        assert_eq!(unnamed.len(), 1);
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
        assert_eq!(l1.start_a, 1_000_001);
        assert_eq!(l1.end_a, 1_001_000);
        assert_eq!(l1.start_b, 1_009_001);
        assert_eq!(l1.end_b, 1_010_000);
    }

    #[tokio::test]
    async fn nan_inf_score_treated_as_missing() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        let mut tf = NamedTempFile::new().unwrap();
        writeln!(tf, "chr1\t100\t200\tchr1\t300\t400\tnan_score\tNaN\t+\t-").unwrap();
        writeln!(tf, "chr1\t500\t600\tchr1\t700\t800\tinf_score\tinf\t+\t-").unwrap();
        writeln!(tf, "chr1\t900\t1000\tchr1\t1100\t1200\tinfinity_score\tInfinity\t+\t-").unwrap();
        let src = BedpeLinkSource::open(tf.path()).await.unwrap();
        assert_eq!(src.record_count(), 3);
        let nan = src.record_at_name("nan_score").unwrap();
        let inf = src.record_at_name("inf_score").unwrap();
        let infinity = src.record_at_name("infinity_score").unwrap();
        assert_eq!(nan.score, None);
        assert_eq!(inf.score, None);
        assert_eq!(infinity.score, None);
    }
}
