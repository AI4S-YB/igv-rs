//! GFF3 / GTF annotation source supporting tabix-indexed `.gff3.gz`/`.gtf.gz`
//! streaming as well as gzipped/plain local & remote in-memory indexing.

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
    AnnotationBlock, AnnotationFormat, AnnotationSource, AnnotationTranscript, BlockKind, Strand,
    TranscriptKind,
};

#[derive(Debug)]
enum GffBackend {
    InMemory {
        by_chrom: HashMap<String, Vec<AnnotationTranscript>>,
        chroms: Vec<String>,
    },
    TabixTbi {
        location: Location,
        remote_stream: Option<SeekableStream>,
        index: Arc<tabix::Index>,
        chroms: Vec<String>,
        format: AnnotationFormat,
    },
    TabixCsi {
        location: Location,
        remote_stream: Option<SeekableStream>,
        index: Arc<csi::Index>,
        chroms: Vec<String>,
        format: AnnotationFormat,
    },
}

#[derive(Debug)]
pub struct NoodlesGffSource {
    display: String,
    backend: GffBackend,
}

impl NoodlesGffSource {
    pub async fn open(path: &Path, format: AnnotationFormat) -> Result<Self> {
        let loc = Location::Local(path.to_path_buf());
        Self::open_location(&loc, format, None).await
    }

    pub async fn open_location(
        location: &Location,
        format: AnnotationFormat,
        auth_token: Option<&str>,
    ) -> Result<Self> {
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
                backend: GffBackend::TabixTbi {
                    location: location.clone(),
                    remote_stream,
                    index: Arc::new(index),
                    chroms,
                    format,
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
                backend: GffBackend::TabixCsi {
                    location: location.clone(),
                    remote_stream,
                    index: Arc::new(index),
                    chroms,
                    format,
                },
            });
        }

        let raw = fetch_bytes(location, auth_token).await?;
        let by_chrom = tokio::task::spawn_blocking(move || -> Result<_> {
            let plain = maybe_gunzip(raw)?;
            let reader = BufReader::new(Cursor::new(plain));
            let lines = reader.lines().map_while(|r| r.ok());
            Ok(assemble_gff_lines(lines, format))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let chroms = by_chrom.keys().cloned().collect();
        Ok(Self {
            display,
            backend: GffBackend::InMemory { by_chrom, chroms },
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesGffSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>> {
        match &self.backend {
            GffBackend::InMemory { by_chrom, chroms } => {
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
            GffBackend::TabixTbi {
                location,
                remote_stream,
                index,
                chroms,
                format,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = open_stream(location, remote_stream.as_ref())?;
                let index = Arc::clone(index);
                let fmt = *format;
                let region_str = format!("{resolved}:{}-{}", region.start, region.end);
                tokio::task::spawn_blocking(move || -> Result<Vec<AnnotationTranscript>> {
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut lines = Vec::new();
                    for res in reader.query(&r).map_err(IgvError::noodles)? {
                        let rec = res.map_err(IgvError::noodles)?;
                        lines.push(rec.as_ref().to_string());
                    }
                    let mut map = assemble_gff_lines(lines.into_iter(), fmt);
                    Ok(map.remove(&resolved).unwrap_or_default())
                })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))?
            }
            GffBackend::TabixCsi {
                location,
                remote_stream,
                index,
                chroms,
                format,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = open_stream(location, remote_stream.as_ref())?;
                let index = Arc::clone(index);
                let fmt = *format;
                let region_str = format!("{resolved}:{}-{}", region.start, region.end);
                tokio::task::spawn_blocking(move || -> Result<Vec<AnnotationTranscript>> {
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut lines = Vec::new();
                    for res in reader.query(&r).map_err(IgvError::noodles)? {
                        let rec = res.map_err(IgvError::noodles)?;
                        lines.push(rec.as_ref().to_string());
                    }
                    let mut map = assemble_gff_lines(lines.into_iter(), fmt);
                    Ok(map.remove(&resolved).unwrap_or_default())
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
        let GffBackend::InMemory { by_chrom, .. } = &self.backend else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (chrom, bucket) in by_chrom {
            for tx in bucket {
                let gene_match = tx
                    .gene_id
                    .as_deref()
                    .is_some_and(|g| g.eq_ignore_ascii_case(q));
                if tx.name.eq_ignore_ascii_case(q)
                    || tx.id.eq_ignore_ascii_case(q)
                    || gene_match
                {
                    out.push((chrom.clone(), tx.clone()));
                }
            }
        }
        out
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

pub(crate) fn assemble_gff_lines(
    lines: impl Iterator<Item = String>,
    format: AnnotationFormat,
) -> HashMap<String, Vec<AnnotationTranscript>> {
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
    let mut gene_name_by_id: HashMap<String, String> = HashMap::new();
    let mut gene_id_by_tx: HashMap<String, String> = HashMap::new();

    for (lineno, line) in lines.enumerate() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let record = match parse_record(trimmed, format) {
            Ok(r) => r,
            Err(e) => {
                warn!("gff parse error at line {}: {e}", lineno + 1);
                continue;
            }
        };

        let chrom = record.chrom;
        let kind = record.ty;
        let start = record.start;
        let end = record.end;
        let strand = record.strand;

        let (gene_id_attr, transcript_id_attr, parent_attr, name_attr) = match format {
            AnnotationFormat::Gff3 => {
                let id = record.attrs.get("ID").cloned();
                let parent = record.attrs.get("Parent").cloned();
                let name = record.attrs.get("Name").cloned().or_else(|| id.clone());
                (id, None, parent, name)
            }
            AnnotationFormat::Gtf => {
                let gid = record.attrs.get("gene_id").cloned();
                let tid = record.attrs.get("transcript_id").cloned();
                let name = record
                    .attrs
                    .get("gene_name")
                    .cloned()
                    .or_else(|| gid.clone());
                (gid, tid, None, name)
            }
            AnnotationFormat::Bed | AnnotationFormat::BigBed => unreachable!(),
        };

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
                    _ => unreachable!(),
                };
                let gene_for_tx = match format {
                    AnnotationFormat::Gff3 => parent_attr
                        .clone()
                        .and_then(|p| p.split(',').next().map(|s| s.to_string())),
                    AnnotationFormat::Gtf => record.attrs.get("gene_id").cloned(),
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
                    _ => unreachable!(),
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
            _ => {}
        }
    }

    let mut by_chrom: HashMap<String, Vec<AnnotationTranscript>> = HashMap::new();
    for (_, mut p) in pending {
        if p.has_cds {
            p.blocks.retain(|b| !matches!(b.kind, BlockKind::Exon));
        } else if !p.seen_exon {
            continue;
        }
        let parent_gene = gene_id_by_tx.get(&p.transcript_id).cloned();
        let label = match &parent_gene {
            Some(g) => gene_name_by_id.get(g).cloned().unwrap_or(p.gene_name),
            None => p.gene_name,
        };
        p.blocks.sort_by_key(|b| b.start);
        let tx = AnnotationTranscript {
            name: label,
            id: p.transcript_id,
            gene_id: parent_gene,
            strand: p.strand,
            blocks: p.blocks,
            kind: TranscriptKind::Mrna,
        };
        by_chrom.entry(p.chrom).or_default().push(tx);
    }
    for v in by_chrom.values_mut() {
        v.sort_by_key(|t| t.span().map(|(s, _)| s).unwrap_or(0));
    }
    by_chrom
}

struct GffRecord {
    chrom: String,
    ty: String,
    start: u64,
    end: u64,
    strand: Strand,
    attrs: HashMap<String, String>,
}

fn parse_record(line: &str, format: AnnotationFormat) -> std::result::Result<GffRecord, String> {
    let cols: Vec<&str> = line.split('\t').collect();
    if cols.len() < 8 {
        return Err(format!("expected >=8 tab-separated columns, got {}", cols.len()));
    }
    let chrom = cols[0].to_string();
    let ty = cols[2].to_string();
    let start: u64 = cols[3].parse().map_err(|e| format!("invalid start: {e}"))?;
    let end: u64 = cols[4].parse().map_err(|e| format!("invalid end: {e}"))?;
    let strand = match cols[6] {
        "+" => Strand::Forward,
        "-" => Strand::Reverse,
        _ => Strand::Unknown,
    };
    let attrs_col = cols.get(8).copied().unwrap_or("");
    let attrs = match format {
        AnnotationFormat::Gff3 => parse_gff3_attrs(attrs_col),
        AnnotationFormat::Gtf => parse_gtf_attrs(attrs_col),
        _ => unreachable!(),
    };
    Ok(GffRecord {
        chrom,
        ty,
        start,
        end,
        strand,
        attrs,
    })
}

fn parse_gff3_attrs(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((k, v)) = part.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

fn parse_gtf_attrs(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut iter = part.splitn(2, char::is_whitespace);
        let key = match iter.next() {
            Some(k) => k.trim(),
            None => continue,
        };
        let raw_value = match iter.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let value = raw_value.trim_matches('"');
        out.insert(key.to_string(), value.to_string());
    }
    out
}
