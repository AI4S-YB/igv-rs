//! Genomic region: 1-based inclusive coordinates with parsing and screen
//! coordinate transforms.

use std::fmt;

use crate::error::{IgvError, Result};

pub const DEFAULT_REGION_WIDTH: u64 = 250;
pub const MAX_REGION_WIDTH: u64 = 100_000;

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

impl Region {
    /// Parse a region string. Accepted forms (case-sensitive on chromosome):
    /// - `chr1:1000-2000`
    /// - `chr1:1,000-2,000`
    /// - `chr1:1000`            → centered default window
    /// - `chr1`                 → 1..=DEFAULT_REGION_WIDTH
    pub fn parse(s: &str) -> Result<Self> {
        let trimmed = s.replace(',', "");
        let trimmed = trimmed.trim();
        if trimmed.is_empty() {
            return Err(IgvError::InvalidRegion(s.to_string()));
        }

        match trimmed.split_once(':') {
            Some((chrom, rest)) => match rest.split_once('-') {
                Some((start, end)) => {
                    let start: u64 = start
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(s.to_string()))?;
                    let end: u64 = end
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(s.to_string()))?;
                    Region::new(chrom, start, end)
                }
                None => {
                    let pos: u64 = rest
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(s.to_string()))?;
                    let half = DEFAULT_REGION_WIDTH / 2;
                    let start = pos.saturating_sub(half).max(1);
                    let end = start + DEFAULT_REGION_WIDTH - 1;
                    Region::new(chrom, start, end)
                }
            },
            None => {
                if trimmed.is_empty() || !trimmed.chars().all(is_chrom_char) {
                    return Err(IgvError::InvalidRegion(s.to_string()));
                }
                Region::new(trimmed, 1, DEFAULT_REGION_WIDTH)
            }
        }
    }
}

fn is_chrom_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
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

    #[test]
    fn parse_full_form() {
        let r = Region::parse("chr1:100-200").unwrap();
        assert_eq!(r, Region::new("chr1", 100, 200).unwrap());
    }

    #[test]
    fn parse_strips_commas() {
        let r = Region::parse("chr1:1,000-2,000").unwrap();
        assert_eq!(r, Region::new("chr1", 1000, 2000).unwrap());
    }

    #[test]
    fn parse_position_only_centers_default_window() {
        let r = Region::parse("chr1:1000").unwrap();
        // Default window 250bp; position centers it.
        assert_eq!(r.chrom, "chr1");
        assert_eq!(r.width(), 250);
        assert!(r.start <= 1000 && 1000 <= r.end);
    }

    #[test]
    fn parse_chromosome_only_uses_default_window() {
        let r = Region::parse("chr1").unwrap();
        assert_eq!(r.chrom, "chr1");
        assert_eq!(r.start, 1);
        assert_eq!(r.width(), 250);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(Region::parse("not a region").is_err());
        assert!(Region::parse("chr1:abc-def").is_err());
        assert!(Region::parse("").is_err());
    }
}
