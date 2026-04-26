//! Pileup-style coverage track from a slice of `AlignmentRow`s.

use crate::source::bam::{AlignmentRow, CigarKind};

/// Per-position depth across an inclusive 1-based window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageVec {
    pub start: u64,
    pub depths: Vec<u32>,
}

impl CoverageVec {
    pub fn end(&self) -> u64 {
        self.start + self.depths.len() as u64 - 1
    }

    pub fn max(&self) -> u32 {
        self.depths.iter().copied().max().unwrap_or(0)
    }
}

/// Compute coverage for the closed range [view_start, view_end] (1-based).
pub fn compute(rows: &[AlignmentRow], view_start: u64, view_end: u64) -> CoverageVec {
    assert!(view_end >= view_start, "view_end must be >= view_start");
    let len = (view_end - view_start + 1) as usize;
    let mut depths = vec![0u32; len];

    for row in rows {
        let mut ref_pos = row.ref_start;
        for op in &row.cigar {
            match op.kind {
                CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                    for _ in 0..op.len {
                        if ref_pos >= view_start && ref_pos <= view_end {
                            let idx = (ref_pos - view_start) as usize;
                            depths[idx] = depths[idx].saturating_add(1);
                        }
                        ref_pos += 1;
                    }
                }
                CigarKind::Deletion | CigarKind::Skip => {
                    ref_pos += op.len as u64;
                }
                CigarKind::Insertion | CigarKind::SoftClip => {}
                CigarKind::HardClip | CigarKind::Padding => {}
            }
        }
    }

    CoverageVec {
        start: view_start,
        depths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::bam::{AlignmentRow, CigarOp};

    fn r(start: u64, cigar: Vec<CigarOp>) -> AlignmentRow {
        AlignmentRow {
            query_name: "r".into(),
            flag: 0,
            ref_start: start,
            ref_end: start
                + cigar
                    .iter()
                    .filter(|op| matches!(op.kind, CigarKind::Match))
                    .map(|op| op.len as u64)
                    .sum::<u64>()
                    .saturating_sub(1),
            mapq: 60,
            is_reverse: false,
            query_sequence: vec![],
            cigar,
            tag: None,
        }
    }

    #[test]
    fn two_overlapping_reads_doubles_depth() {
        let reads = vec![
            r(1, vec![CigarOp { kind: CigarKind::Match, len: 5 }]),
            r(3, vec![CigarOp { kind: CigarKind::Match, len: 5 }]),
        ];
        let cov = compute(&reads, 1, 7);
        assert_eq!(cov.depths, vec![1, 1, 2, 2, 2, 1, 1]);
    }

    #[test]
    fn deletion_skips_depth() {
        let reads = vec![r(
            1,
            vec![
                CigarOp { kind: CigarKind::Match, len: 2 },
                CigarOp { kind: CigarKind::Deletion, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 2 },
            ],
        )];
        let cov = compute(&reads, 1, 6);
        assert_eq!(cov.depths, vec![1, 1, 0, 0, 1, 1]);
    }
}
