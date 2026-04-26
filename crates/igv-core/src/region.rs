//! Genomic region: 1-based inclusive coordinates with parsing and screen
//! coordinate transforms.

use std::fmt;

use crate::error::{IgvError, Result};

/// A 1-based, inclusive genomic interval.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Region {
    pub chrom: String,
    pub start: u64, // 1-based inclusive
    pub end: u64,   // 1-based inclusive
}

impl Region {
    /// Construct a region. Returns `InvalidRegion` if `start > end` or `start == 0`.
    pub fn new(chrom: impl Into<String>, start: u64, end: u64) -> Result<Self> {
        if start == 0 || start > end {
            return Err(IgvError::InvalidRegion(format!(
                "{}:{}-{}",
                chrom.into(),
                start,
                end
            )));
        }
        Ok(Self {
            chrom: chrom.into(),
            start,
            end,
        })
    }

    /// Width in bases (inclusive).
    pub fn width(&self) -> u64 {
        self.end - self.start + 1
    }

    /// Clamp to `[1, chrom_len]`. Returns `OutOfBounds` if no overlap exists.
    pub fn clamp_to(&self, chrom_len: u64) -> Result<Self> {
        if chrom_len == 0 || self.start > chrom_len {
            return Err(IgvError::OutOfBounds {
                chrom: self.chrom.clone(),
                chrom_len,
                start: self.start,
                end: self.end,
            });
        }
        let new_start = self.start.max(1);
        let new_end = self.end.min(chrom_len);
        Region::new(self.chrom.clone(), new_start, new_end)
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}-{}", self.chrom, self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_zero_start() {
        assert!(Region::new("chr1", 0, 10).is_err());
    }

    #[test]
    fn new_rejects_start_greater_than_end() {
        assert!(Region::new("chr1", 20, 10).is_err());
    }

    #[test]
    fn width_is_inclusive() {
        let r = Region::new("chr1", 100, 199).unwrap();
        assert_eq!(r.width(), 100);
    }

    #[test]
    fn clamp_trims_to_chrom_length() {
        let r = Region::new("chr1", 100, 1_000_000).unwrap();
        let c = r.clamp_to(500).unwrap();
        assert_eq!(c.end, 500);
        assert_eq!(c.start, 100);
    }

    #[test]
    fn clamp_errors_when_start_exceeds_length() {
        let r = Region::new("chr1", 1000, 2000).unwrap();
        assert!(r.clamp_to(500).is_err());
    }

    #[test]
    fn display_formats_canonically() {
        let r = Region::new("chr1", 100, 200).unwrap();
        assert_eq!(r.to_string(), "chr1:100-200");
    }
}
