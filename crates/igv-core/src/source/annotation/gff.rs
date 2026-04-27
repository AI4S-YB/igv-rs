//! GFF3 / GTF annotation source. Loads the entire file into memory on
//! `open` and serves range queries from a per-chromosome sorted index.
//! Fine for typical gene-annotation files (≤ a few hundred MB on disk).
//!
//! ## Implementation note
//!
//! The plan was authored against noodles-gff 0.56's `feature::record_buf`
//! API, but the workspace pins `noodles = "0.85"` which resolves to
//! noodles-gff 0.39 (different module layout: `gff::Record`, no
//! `feature::record_buf`, `gff::Line` instead of `gff::LineBuf`).
//! Worse, noodles-gff 0.39 only parses GFF3 (rejects GTF
//! `key "value"` attribute syntax) and rejects CDS records lacking a
//! phase. To support both GFF3 and GTF cleanly, this implementation
//! hand-parses each line into the same 9-column structure rather than
//! using noodles' parser. The strategy preserves the plan's behavior:
//! group by transcript id, classify blocks by feature type, resolve
//! strand and label.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tracing::warn;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{
    AnnotationBlock, AnnotationFormat, AnnotationSource, AnnotationTranscript, BlockKind, Strand,
    TranscriptKind,
};

#[derive(Debug)]
pub struct NoodlesGffSource {
    #[allow(dead_code)]
    path: PathBuf,
    display: String,
    #[allow(dead_code)]
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

    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        let q = query.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (chrom, bucket) in &self.by_chrom {
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

fn load_all(
    path: &Path,
    format: AnnotationFormat,
) -> Result<HashMap<String, Vec<AnnotationTranscript>>> {
    let file = std::fs::File::open(path).map_err(|e| IgvError::io(path, e))?;
    let reader = std::io::BufReader::new(file);

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

    for (lineno, line_res) in reader.lines().enumerate() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                warn!("gff read error at line {}: {e}", lineno + 1);
                continue;
            }
        };
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
                let name = record
                    .attrs
                    .get("Name")
                    .cloned()
                    .or_else(|| id.clone());
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
            AnnotationFormat::Bed => unreachable!(),
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
                    AnnotationFormat::Bed => unreachable!(),
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
            _ => {}
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
        let parent_gene = gene_id_by_tx.get(&p.transcript_id).cloned();
        // Improve label using gene_name lookup (GFF3 Parent → gene name).
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
    Ok(by_chrom)
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
        return Err(format!("expected ≥8 tab-separated columns, got {}", cols.len()));
    }
    let chrom = cols[0].to_string();
    let ty = cols[2].to_string();
    let start: u64 = cols[3]
        .parse()
        .map_err(|e| format!("invalid start: {e}"))?;
    let end: u64 = cols[4]
        .parse()
        .map_err(|e| format!("invalid end: {e}"))?;
    let strand = match cols[6] {
        "+" => Strand::Forward,
        "-" => Strand::Reverse,
        _ => Strand::Unknown,
    };
    let attrs_col = cols.get(8).copied().unwrap_or("");
    let attrs = match format {
        AnnotationFormat::Gff3 => parse_gff3_attrs(attrs_col),
        AnnotationFormat::Gtf => parse_gtf_attrs(attrs_col),
        AnnotationFormat::Bed => unreachable!(),
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
        // GTF format: key "value" or key value
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
