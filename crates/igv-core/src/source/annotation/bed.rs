//! BED annotation source (BED3–BED12, narrowPeak/broadPeak) supporting
//! tabix-indexed `.bed.gz` streaming and in-memory local/remote fallback.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use noodles::csi::{self, BinningIndex};
use noodles::tabix;
use tracing::warn;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, maybe_gunzip, Location, SeekableStream};
use crate::region::Region;

use super::{
    AnnotationBlock, AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};

#[derive(Debug)]
enum BedBackend {
    InMemory {
        by_chrom: HashMap<String, Vec<AnnotationTranscript>>,
        chroms: Vec<String>,
    },
    Tabix {
        location: Location,
        remote_stream: Option<SeekableStream>,
        index: Arc<csi::Index>,
        chroms: Vec<String>,
    },
    TabixTbi {
        location: Location,
        remote_stream: Option<SeekableStream>,
        index: Arc<tabix::Index>,
        chroms: Vec<String>,
    },
}

#[derive(Debug)]
pub struct NoodlesBedSource {
    display: String,
    backend: BedBackend,
}

impl NoodlesBedSource {
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
                backend: BedBackend::TabixTbi {
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
                backend: BedBackend::Tabix {
                    location: location.clone(),
                    remote_stream,
                    index: Arc::new(index),
                    chroms,
                },
            });
        }

        let raw = fetch_bytes(location, auth_token).await?;
        let by_chrom = tokio::task::spawn_blocking(move || -> Result<_> {
            let plain = maybe_gunzip(raw)?;
            load_from_reader(BufReader::new(Cursor::new(plain)))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let chroms = by_chrom.keys().cloned().collect();
        Ok(Self {
            display,
            backend: BedBackend::InMemory { by_chrom, chroms },
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesBedSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>> {
        match &self.backend {
            BedBackend::InMemory { by_chrom, chroms } => {
                let Some(chrom) = resolve_chrom_name(&region.chrom, chroms) else {
                    return Ok(Vec::new());
                };
                let Some(bucket) = by_chrom.get(chrom) else {
                    return Ok(Vec::new());
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
            BedBackend::TabixTbi {
                location,
                remote_stream,
                index,
                chroms,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = open_tabix_stream(location, remote_stream.as_ref())?;
                let index = Arc::clone(index);
                let region_str = format!("{resolved}:{}-{}", region.start, region.end);
                tokio::task::spawn_blocking(move || -> Result<Vec<AnnotationTranscript>> {
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut out = Vec::new();
                    for (idx, res) in reader.query(&r).map_err(IgvError::noodles)?.enumerate() {
                        let rec = res.map_err(IgvError::noodles)?;
                        if let Ok(Some((_, tx))) = parse_bed_line(rec.as_ref(), idx + 1) {
                            out.push(tx);
                        }
                    }
                    Ok(out)
                })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))?
            }
            BedBackend::Tabix {
                location,
                remote_stream,
                index,
                chroms,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = open_tabix_stream(location, remote_stream.as_ref())?;
                let index = Arc::clone(index);
                let region_str = format!("{resolved}:{}-{}", region.start, region.end);
                tokio::task::spawn_blocking(move || -> Result<Vec<AnnotationTranscript>> {
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut out = Vec::new();
                    for (idx, res) in reader.query(&r).map_err(IgvError::noodles)?.enumerate() {
                        let rec = res.map_err(IgvError::noodles)?;
                        if let Ok(Some((_, tx))) = parse_bed_line(rec.as_ref(), idx + 1) {
                            out.push(tx);
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

    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let BedBackend::InMemory { by_chrom, .. } = &self.backend else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (chrom, bucket) in by_chrom {
            for tx in bucket {
                if tx.name.eq_ignore_ascii_case(q) || tx.id.eq_ignore_ascii_case(q) {
                    out.push((chrom.clone(), tx.clone()));
                }
            }
        }
        out
    }
}

fn open_tabix_stream(
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

fn load_from_reader<R: BufRead>(reader: R) -> Result<HashMap<String, Vec<AnnotationTranscript>>> {
    let mut by_chrom: HashMap<String, Vec<AnnotationTranscript>> = HashMap::new();
    for (i, line_res) in reader.lines().enumerate() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                warn!("bed read error at line {}: {e}", i + 1);
                continue;
            }
        };
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("track ")
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

pub(crate) fn parse_bed_line(
    line: &str,
    lineno: usize,
) -> Result<Option<(String, AnnotationTranscript)>> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 3 {
        return Err(IgvError::Other(format!("bed line {lineno}: <3 columns")));
    }
    let chrom = cols[0].to_string();
    let chrom_start: u64 = cols[1]
        .parse()
        .map_err(|_| IgvError::Other(format!("bed line {lineno}: invalid chromStart")))?;
    let chrom_end: u64 = cols[2]
        .parse()
        .map_err(|_| IgvError::Other(format!("bed line {lineno}: invalid chromEnd")))?;
    let name = cols.get(3).copied().unwrap_or("").to_string();
    let strand = match cols.get(5).copied() {
        Some("+") => Strand::Forward,
        Some("-") => Strand::Reverse,
        _ => Strand::Unknown,
    };

    let blocks = if cols.len() >= 12 {
        let block_count: usize = cols[9]
            .parse()
            .map_err(|_| IgvError::Other(format!("bed line {lineno}: invalid blockCount")))?;
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
                let s = chrom_start + starts[i] + 1;
                let e = chrom_start + starts[i] + sizes[i];
                AnnotationBlock {
                    start: s,
                    end: e,
                    kind: BlockKind::BedSegment,
                }
            })
            .collect()
    } else {
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
            gene_id: None,
            strand,
            blocks,
            kind: TranscriptKind::BedFeature,
        },
    )))
}
