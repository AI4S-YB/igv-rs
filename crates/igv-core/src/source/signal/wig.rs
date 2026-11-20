//! WIG (`fixedStep` / `variableStep`) and `bedGraph` signal source,
//! supporting gzip, remote URLs, and tabix-indexed `.bedgraph.gz`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor};
use std::sync::Arc;

use async_trait::async_trait;
use noodles::csi::{self, BinningIndex};
use noodles::tabix;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, maybe_gunzip, Location, SeekableStream};
use crate::region::Region;
use crate::source::signal::{FetchSignalOpts, SignalBin, SignalSource, SignalSummary};

#[derive(Debug)]
enum WigBackend {
    InMemory {
        by_chrom: HashMap<String, Vec<SignalBin>>,
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
pub struct WigSignalSource {
    display: String,
    backend: WigBackend,
}

impl WigSignalSource {
    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let tbi_loc = location.sibling("tbi");
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
                backend: WigBackend::TabixTbi {
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
            Ok(parse_wig_or_bedgraph(BufReader::new(Cursor::new(plain))))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        let chroms = by_chrom.keys().cloned().collect();
        Ok(Self {
            display,
            backend: WigBackend::InMemory { by_chrom, chroms },
        })
    }
}

#[derive(Debug, Clone)]
enum WigMode {
    BedGraph,
    VariableStep { chrom: String, span: u64 },
    FixedStep { chrom: String, next_start: u64, step: u64, span: u64 },
}

fn parse_wig_or_bedgraph<R: BufRead>(reader: R) -> HashMap<String, Vec<SignalBin>> {
    let mut by_chrom: HashMap<String, Vec<SignalBin>> = HashMap::new();
    let mut mode = WigMode::BedGraph;

    for line_res in reader.lines() {
        let Ok(line) = line_res else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("track ")
            || trimmed.starts_with("browser ")
        {
            continue;
        }
        if trimmed.starts_with("variableStep") {
            let mut chrom = String::new();
            let mut span = 1u64;
            for tok in trimmed.split_whitespace().skip(1) {
                if let Some((k, v)) = tok.split_once('=') {
                    match k {
                        "chrom" => chrom = v.to_string(),
                        "span" => span = v.parse().unwrap_or(1).max(1),
                        _ => {}
                    }
                }
            }
            if !chrom.is_empty() {
                mode = WigMode::VariableStep { chrom, span };
            }
            continue;
        }
        if trimmed.starts_with("fixedStep") {
            let mut chrom = String::new();
            let mut start = 1u64;
            let mut step = 1u64;
            let mut span = 1u64;
            for tok in trimmed.split_whitespace().skip(1) {
                if let Some((k, v)) = tok.split_once('=') {
                    match k {
                        "chrom" => chrom = v.to_string(),
                        "start" => start = v.parse().unwrap_or(1).max(1),
                        "step" => step = v.parse().unwrap_or(1).max(1),
                        "span" => span = v.parse().unwrap_or(1).max(1),
                        _ => {}
                    }
                }
            }
            if !chrom.is_empty() {
                mode = WigMode::FixedStep {
                    chrom,
                    next_start: start,
                    step,
                    span,
                };
            }
            continue;
        }

        match &mut mode {
            WigMode::FixedStep {
                chrom,
                next_start,
                step,
                span,
            } => {
                if let Ok(val) = trimmed.parse::<f32>() {
                    if val.is_finite() {
                        by_chrom.entry(chrom.clone()).or_default().push(SignalBin {
                            start: *next_start,
                            end: *next_start + *span - 1,
                            value: val,
                        });
                    }
                    *next_start += *step;
                }
            }
            WigMode::VariableStep { chrom, span } => {
                let mut parts = trimmed.split_whitespace();
                if let (Some(pos_s), Some(val_s)) = (parts.next(), parts.next()) {
                    if let (Ok(pos), Ok(val)) = (pos_s.parse::<u64>(), val_s.parse::<f32>()) {
                        if pos >= 1 && val.is_finite() {
                            by_chrom.entry(chrom.clone()).or_default().push(SignalBin {
                                start: pos,
                                end: pos + *span - 1,
                                value: val,
                            });
                        }
                    }
                }
            }
            WigMode::BedGraph => {
                if let Some((chrom, bin)) = parse_bedgraph_line(trimmed) {
                    by_chrom.entry(chrom).or_default().push(bin);
                }
            }
        }
    }

    for v in by_chrom.values_mut() {
        v.sort_by_key(|b| (b.start, b.end));
    }
    by_chrom
}

fn parse_bedgraph_line(line: &str) -> Option<(String, SignalBin)> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return None;
    }
    let s0 = cols[1].parse::<u64>().ok()?;
    let e0 = cols[2].parse::<u64>().ok()?;
    let val = cols[3].parse::<f32>().ok()?;
    if e0 <= s0 || !val.is_finite() {
        return None;
    }
    Some((
        cols[0].to_string(),
        SignalBin {
            start: s0 + 1,
            end: e0,
            value: val,
        },
    ))
}

fn downsample_bins(bins: Vec<SignalBin>, region: &Region, opts: &FetchSignalOpts) -> Vec<SignalBin> {
    let max_bins = opts.max_bins.max(1) as usize;
    if bins.len() <= max_bins {
        return bins;
    }
    let span = region.width().max(1);
    let mut buckets: Vec<Vec<f32>> = vec![Vec::new(); max_bins];
    for b in bins {
        let mid = b.start.saturating_add(b.end) / 2;
        if mid < region.start || mid > region.end {
            continue;
        }
        let col = (((mid - region.start) * max_bins as u64) / span).min(max_bins as u64 - 1) as usize;
        buckets[col].push(b.value);
    }
    let mut out = Vec::with_capacity(max_bins);
    for (col, vals) in buckets.into_iter().enumerate() {
        if vals.is_empty() {
            continue;
        }
        let value = match opts.summary {
            SignalSummary::Max => vals.into_iter().fold(f32::NEG_INFINITY, f32::max),
            SignalSummary::Min => vals.into_iter().fold(f32::INFINITY, f32::min),
            SignalSummary::Sum => vals.into_iter().sum(),
            SignalSummary::Mean => {
                let n = vals.len() as f32;
                vals.into_iter().sum::<f32>() / n
            }
        };
        let b_start = region.start + (col as u64 * span) / max_bins as u64;
        let b_end = (region.start + ((col as u64 + 1) * span) / max_bins as u64).max(b_start);
        out.push(SignalBin {
            start: b_start,
            end: b_end,
            value,
        });
    }
    out
}

#[async_trait]
impl SignalSource for WigSignalSource {
    async fn fetch(&self, region: &Region, opts: &FetchSignalOpts) -> Result<Vec<SignalBin>> {
        match &self.backend {
            WigBackend::InMemory { by_chrom, chroms } => {
                let Some(chrom) = resolve_chrom_name(&region.chrom, chroms) else {
                    return Ok(Vec::new());
                };
                let Some(all) = by_chrom.get(chrom) else {
                    return Ok(Vec::new());
                };
                let filtered: Vec<SignalBin> = all
                    .iter()
                    .filter(|b| b.end >= region.start && b.start <= region.end)
                    .cloned()
                    .collect();
                Ok(downsample_bins(filtered, region, opts))
            }
            WigBackend::TabixTbi {
                location,
                remote_stream,
                index,
                chroms,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chroms).map(str::to_string)
                else {
                    return Ok(Vec::new());
                };
                let stream = match (location, remote_stream) {
                    (Location::Local(p), _) => SeekableStream::open_local(p)?,
                    (Location::Remote(_), Some(rs)) => rs
                        .clone_handle()
                        .map_err(|e| IgvError::Other(e.to_string()))?,
                    (Location::Remote(u), None) => {
                        return Err(IgvError::Other(format!("no stream for {u}")));
                    }
                };
                let index = Arc::clone(index);
                let reg = region.clone();
                let opts = *opts;
                tokio::task::spawn_blocking(move || -> Result<Vec<SignalBin>> {
                    let region_str = format!("{resolved}:{}-{}", reg.start, reg.end);
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str))?;
                    let mut reader = csi::io::IndexedReader::new(stream, index.as_ref().clone());
                    let mut raw = Vec::new();
                    for res in reader.query(&r).map_err(IgvError::noodles)? {
                        let rec = res.map_err(IgvError::noodles)?;
                        if let Some((_, bin)) = parse_bedgraph_line(rec.as_ref()) {
                            raw.push(bin);
                        }
                    }
                    Ok(downsample_bins(raw, &reg, &opts))
                })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))?
            }
        }
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixed_and_variable_step_and_bedgraph() {
        let text = "\
fixedStep chrom=chr1 start=10 step=5 span=3
1.5
2.5
variableStep chrom=chr2 span=2
100 4.0
";
        let map = parse_wig_or_bedgraph(BufReader::new(Cursor::new(text)));
        assert_eq!(map["chr1"].len(), 2);
        assert_eq!(map["chr1"][0], SignalBin { start: 10, end: 12, value: 1.5 });
        assert_eq!(map["chr1"][1], SignalBin { start: 15, end: 17, value: 2.5 });
        assert_eq!(map["chr2"][0], SignalBin { start: 100, end: 101, value: 4.0 });
    }
}
