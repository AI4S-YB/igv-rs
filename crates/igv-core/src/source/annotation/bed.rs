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
    #[allow(dead_code)]
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
