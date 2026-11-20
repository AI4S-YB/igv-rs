//! Copy-number segmentation track source (`.seg` / `.cbs` / `.cn` / `.seg.gz`).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Cursor};

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, maybe_gunzip, Location};
use crate::region::Region;

#[derive(Debug, Clone, PartialEq)]
pub struct SegInterval {
    pub sample: String,
    pub chrom: String,
    pub start: u64, // 1-based inclusive
    pub end: u64,   // 1-based inclusive
    pub num_marks: Option<u32>,
    pub value: f32, // log2 copy-ratio / seg mean
}

#[derive(Debug, Clone, Default)]
pub struct SegTrackData {
    pub samples: Vec<String>,
    pub segments: Vec<SegInterval>,
}

#[async_trait]
pub trait SegSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<SegTrackData>;
    fn samples(&self) -> Vec<String>;
    fn display_name(&self) -> &str;
}

#[derive(Debug)]
pub struct FileSegSource {
    display: String,
    samples: Vec<String>,
    by_chrom: HashMap<String, Vec<SegInterval>>,
    chroms: Vec<String>,
}

impl FileSegSource {
    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let raw = fetch_bytes(location, auth_token).await?;
        let (samples, by_chrom) = tokio::task::spawn_blocking(move || -> Result<_> {
            let plain = maybe_gunzip(raw)?;
            Ok(parse_seg(BufReader::new(Cursor::new(plain))))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        let chroms = by_chrom.keys().cloned().collect();
        Ok(Self {
            display,
            samples,
            by_chrom,
            chroms,
        })
    }
}

fn parse_seg<R: BufRead>(reader: R) -> (Vec<String>, HashMap<String, Vec<SegInterval>>) {
    let mut samples = Vec::new();
    let mut seen_samples = HashSet::new();
    let mut by_chrom: HashMap<String, Vec<SegInterval>> = HashMap::new();
    let mut saw_header = false;

    for line_res in reader.lines() {
        let Ok(line) = line_res else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("track ") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 5 {
            continue;
        }
        if !saw_header {
            let c2_num = cols[2].parse::<u64>().is_ok();
            if !c2_num {
                saw_header = true;
                continue;
            }
            saw_header = true;
        }

        let sample = cols[0].trim().to_string();
        let chrom = cols[1].trim().to_string();
        let Ok(start) = cols[2].trim().parse::<u64>() else {
            continue;
        };
        let Ok(end) = cols[3].trim().parse::<u64>() else {
            continue;
        };
        let (num_marks, val_str) = if cols.len() >= 6 {
            (cols[4].trim().parse::<u32>().ok(), cols[5].trim())
        } else {
            (None, cols[4].trim())
        };
        let Ok(value) = val_str.parse::<f32>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        if seen_samples.insert(sample.clone()) {
            samples.push(sample.clone());
        }
        by_chrom.entry(chrom.clone()).or_default().push(SegInterval {
            sample,
            chrom,
            start: start.max(1),
            end: end.max(start.max(1)),
            num_marks,
            value,
        });
    }

    for v in by_chrom.values_mut() {
        v.sort_by_key(|s| s.start);
    }
    (samples, by_chrom)
}

#[async_trait]
impl SegSource for FileSegSource {
    async fn fetch(&self, region: &Region) -> Result<SegTrackData> {
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chroms) else {
            return Ok(SegTrackData {
                samples: self.samples.clone(),
                segments: Vec::new(),
            });
        };
        let segments = self
            .by_chrom
            .get(chrom)
            .map(|v| {
                v.iter()
                    .filter(|s| s.end >= region.start && s.start <= region.end)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(SegTrackData {
            samples: self.samples.clone(),
            segments,
        })
    }

    fn samples(&self) -> Vec<String> {
        self.samples.clone()
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seg_header_and_samples() {
        let data = "\
#type=CNV
SampleID\tChr\tStart\tEnd\tNum\tValue
S1\tchr1\t100\t500\t10\t-1.2
S2\tchr1\t200\t600\t12\t0.9
";
        let (samples, map) = parse_seg(BufReader::new(Cursor::new(data)));
        assert_eq!(samples, vec!["S1", "S2"]);
        assert_eq!(map["chr1"].len(), 2);
        assert!((map["chr1"][0].value - (-1.2)).abs() < 1e-5);
    }
}
