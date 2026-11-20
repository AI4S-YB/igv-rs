//! Minimap2 PAF alignment source (`.paf` / `.paf.gz`) adapting long-read /
//! assembly alignments into `AlignmentRow`s (including `cg:Z:` CIGAR tags).

use std::collections::HashMap;

use async_trait::async_trait;
use iset::IntervalMap;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_text_maybe_gz, Location};
use crate::region::Region;
use crate::source::bam::{AlignmentRow, BamSource, CigarKind, CigarOp, FetchOpts};

pub struct PafAlignmentSource {
    records: Vec<AlignmentRow>,
    trees: HashMap<String, IntervalMap<u64, usize>>,
    chrom_names: Vec<String>,
}

impl std::fmt::Debug for PafAlignmentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PafAlignmentSource")
            .field("records", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl PafAlignmentSource {
    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let text = fetch_text_maybe_gz(location, auth_token).await?;
        let (records, trees) = tokio::task::spawn_blocking(move || parse_paf(&text))
            .await
            .map_err(|e| IgvError::Other(e.to_string()))?;
        let chrom_names = trees.keys().cloned().collect();
        Ok(Self {
            records,
            trees,
            chrom_names,
        })
    }
}

fn parse_paf(text: &str) -> (Vec<AlignmentRow>, HashMap<String, IntervalMap<u64, usize>>) {
    let mut records = Vec::new();
    let mut trees: HashMap<String, IntervalMap<u64, usize>> = HashMap::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let query_name = cols[0].to_string();
        let strand_rev = cols[4] == "-";
        let tname = cols[5].to_string();
        let Ok(tstart_0) = cols[7].parse::<u64>() else {
            continue;
        };
        let Ok(tend_0) = cols[8].parse::<u64>() else {
            continue;
        };
        if tend_0 <= tstart_0 {
            continue;
        }
        let mapq = cols[11].parse::<u8>().unwrap_or(0);

        let mut cigar = Vec::new();
        let mut is_secondary = false;
        for tag in &cols[12..] {
            if let Some(cg) = tag.strip_prefix("cg:Z:") {
                cigar = parse_cigar_str(cg);
            } else if *tag == "tp:A:S" {
                is_secondary = true;
            }
        }
        let ref_start = tstart_0 + 1;
        let ref_end = tend_0;
        if cigar.is_empty() {
            cigar.push(CigarOp {
                kind: CigarKind::Match,
                len: (ref_end - ref_start + 1).min(u32::MAX as u64) as u32,
            });
        }
        let flag = (if strand_rev { 0x10 } else { 0 }) | (if is_secondary { 0x100 } else { 0 });

        let idx = records.len();
        records.push(AlignmentRow {
            query_name,
            flag,
            ref_start,
            ref_end,
            mapq,
            is_reverse: strand_rev,
            cigar,
            ..Default::default()
        });
        trees
            .entry(tname)
            .or_default()
            .insert(ref_start..ref_end.saturating_add(1), idx);
    }

    (records, trees)
}

fn parse_cigar_str(s: &str) -> Vec<CigarOp> {
    let mut out = Vec::new();
    let mut num = 0u32;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            num = num.saturating_mul(10).saturating_add((b - b'0') as u32);
        } else {
            let kind = match b {
                b'M' => CigarKind::Match,
                b'I' => CigarKind::Insertion,
                b'D' => CigarKind::Deletion,
                b'N' => CigarKind::Skip,
                b'S' => CigarKind::SoftClip,
                b'H' => CigarKind::HardClip,
                b'P' => CigarKind::Padding,
                b'=' => CigarKind::SeqMatch,
                b'X' => CigarKind::SeqMismatch,
                _ => {
                    num = 0;
                    continue;
                }
            };
            if num > 0 {
                out.push(CigarOp { kind, len: num });
            }
            num = 0;
        }
    }
    out
}

#[async_trait]
impl BamSource for PafAlignmentSource {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>> {
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chrom_names) else {
            return Ok(Vec::new());
        };
        let Some(tree) = self.trees.get(chrom) else {
            return Ok(Vec::new());
        };
        let lo = region.start;
        let hi = region.end.saturating_add(1);
        let mut out = Vec::new();
        for (_, &idx) in tree.iter(lo..hi) {
            let row = &self.records[idx];
            if !opts.include_secondary && (row.flag & 0x100 != 0) {
                continue;
            }
            out.push(row.clone());
        }
        out.sort_by_key(|r| r.ref_start);
        Ok(out)
    }
}
