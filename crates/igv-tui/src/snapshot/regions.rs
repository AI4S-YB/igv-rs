//! Parse a region list (BED) into LabeledRegions and apply flank padding.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use igv_core::region::Region;

#[derive(Debug, Clone)]
pub struct LabeledRegion {
    pub region: Region,
    pub label: Option<String>,
}

pub fn parse_bed(path: &Path) -> Result<Vec<LabeledRegion>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("track")
            || trimmed.starts_with("browser")
        {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 3 {
            return Err(anyhow!(
                "{}: line {}: BED needs ≥3 tab-separated columns",
                path.display(),
                lineno + 1
            ));
        }
        let chrom = cols[0].to_string();
        let start: u64 = cols[1].parse().with_context(|| {
            format!("{}: line {}: bad start", path.display(), lineno + 1)
        })?;
        let end: u64 = cols[2].parse().with_context(|| {
            format!("{}: line {}: bad end", path.display(), lineno + 1)
        })?;
        if end == 0 || end <= start {
            return Err(anyhow!(
                "{}: line {}: end {} <= start {}",
                path.display(),
                lineno + 1,
                end,
                start
            ));
        }
        // BED is 0-based half-open. Convert to igv-core's 1-based inclusive.
        let region = Region::new(chrom, start + 1, end)?;
        let label = cols.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty());
        out.push(LabeledRegion { region, label });
    }
    Ok(out)
}

/// Apply a flank fraction symmetrically.
pub fn apply_flank(region: &Region, flank: f64) -> Region {
    let w = region.width();
    let pad = (w as f64 * flank).floor() as u64;
    let new_start = region.start.saturating_sub(pad).max(1);
    let new_end = region.end.saturating_add(pad);
    Region::new(region.chrom.clone(), new_start, new_end).unwrap_or_else(|_| region.clone())
}

/// Final clamp using a chromosome length lookup.
pub fn clamp_to_chrom(region: &Region, chrom_len: Option<u64>) -> Region {
    let Some(chrom_len) = chrom_len else {
        return region.clone();
    };
    region.clamp_to(chrom_len).unwrap_or_else(|_| region.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flank_zero_is_identity() {
        let r = Region::new("chr1", 100, 200).unwrap();
        let f = apply_flank(&r, 0.0);
        assert_eq!(f.start, 100);
        assert_eq!(f.end, 200);
    }

    #[test]
    fn flank_ten_percent_pads_each_side() {
        let r = Region::new("chr1", 100, 200).unwrap();
        let f = apply_flank(&r, 0.1);
        // width = 101, pad = floor(10.1) = 10
        assert_eq!(f.start, 90);
        assert_eq!(f.end, 210);
    }

    #[test]
    fn flank_clamps_start_to_one() {
        let r = Region::new("chr1", 5, 10).unwrap();
        let f = apply_flank(&r, 1.0); // width=6, pad=6
        assert_eq!(f.start, 1);
        assert_eq!(f.end, 16);
    }

    #[test]
    fn parse_bed_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("r.bed");
        std::fs::write(&p, "chr1\t99\t200\tBRCA1\nchr2\t499\t600\n").unwrap();
        let v = parse_bed(&p).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].region.start, 100);
        assert_eq!(v[0].region.end, 200);
        assert_eq!(v[0].label.as_deref(), Some("BRCA1"));
        assert_eq!(v[1].label, None);
    }
}
