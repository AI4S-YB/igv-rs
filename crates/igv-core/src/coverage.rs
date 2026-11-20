//! Pileup-style coverage + per-base allele counts for SNV allele-fraction highlighting.

use crate::source::bam::{AlignmentRow, CigarKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BaseCounts {
    pub a: u32,
    pub c: u32,
    pub g: u32,
    pub t: u32,
    pub n: u32,
    pub del: u32,
}

impl BaseCounts {
    pub fn total(&self) -> u32 {
        self.a + self.c + self.g + self.t + self.n
    }

    pub fn add_base(&mut self, base: u8) {
        match base.to_ascii_uppercase() {
            b'A' => self.a = self.a.saturating_add(1),
            b'C' => self.c = self.c.saturating_add(1),
            b'G' => self.g = self.g.saturating_add(1),
            b'T' => self.t = self.t.saturating_add(1),
            _ => self.n = self.n.saturating_add(1),
        }
    }
}

/// Per-position depth and base counts across an inclusive 1-based window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageVec {
    pub start: u64,
    pub depths: Vec<u32>,
    pub counts: Vec<BaseCounts>,
}

impl CoverageVec {
    pub fn end(&self) -> u64 {
        self.start + self.depths.len() as u64 - 1
    }

    pub fn max(&self) -> u32 {
        self.depths.iter().copied().max().unwrap_or(0)
    }

    /// Return normalized `(A, C, G, T)` fractions when non-reference allele
    /// fraction meets or exceeds `threshold_frac` (IGV default 0.20, min depth 2).
    pub fn mismatch_fractions(
        &self,
        idx: usize,
        ref_base: u8,
        threshold_frac: f32,
    ) -> Option<[(u8, f32); 4]> {
        let bc = self.counts.get(idx)?;
        let tot = bc.total();
        if tot < 2 {
            return None;
        }
        let ref_count = match ref_base.to_ascii_uppercase() {
            b'A' => bc.a,
            b'C' => bc.c,
            b'G' => bc.g,
            b'T' => bc.t,
            _ => return None,
        };
        let non_ref = tot.saturating_sub(ref_count) as f32 / tot as f32;
        if non_ref < threshold_frac {
            return None;
        }
        let f = tot as f32;
        Some([
            (b'A', bc.a as f32 / f),
            (b'C', bc.c as f32 / f),
            (b'G', bc.g as f32 / f),
            (b'T', bc.t as f32 / f),
        ])
    }
}

/// Compute per-position depth and base counts for `[view_start, view_end]` (1-based).
pub fn compute(rows: &[AlignmentRow], view_start: u64, view_end: u64) -> CoverageVec {
    assert!(view_end >= view_start, "view_end must be >= view_start");
    let len = (view_end - view_start + 1) as usize;
    let mut depths = vec![0u32; len];
    let mut counts = vec![BaseCounts::default(); len];

    for row in rows {
        let mut ref_pos = row.ref_start;
        let mut q_idx = 0usize;
        for op in &row.cigar {
            match op.kind {
                CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                    for _ in 0..op.len {
                        if ref_pos >= view_start && ref_pos <= view_end {
                            let idx = (ref_pos - view_start) as usize;
                            depths[idx] = depths[idx].saturating_add(1);
                            let qb = row.query_sequence.get(q_idx).copied().unwrap_or(b'N');
                            counts[idx].add_base(qb);
                        }
                        ref_pos += 1;
                        q_idx += 1;
                    }
                }
                CigarKind::Deletion => {
                    for _ in 0..op.len {
                        if ref_pos >= view_start && ref_pos <= view_end {
                            let idx = (ref_pos - view_start) as usize;
                            counts[idx].del = counts[idx].del.saturating_add(1);
                        }
                        ref_pos += 1;
                    }
                }
                CigarKind::Skip => {
                    ref_pos += op.len as u64;
                }
                CigarKind::Insertion | CigarKind::SoftClip => {
                    q_idx += op.len as usize;
                }
                CigarKind::HardClip | CigarKind::Padding => {}
            }
        }
    }

    CoverageVec {
        start: view_start,
        depths,
        counts,
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
            ..Default::default()
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

    #[test]
    fn snv_allele_fraction_detects_heterozygous_site() {
        let mut r1 = r(1, vec![CigarOp { kind: CigarKind::Match, len: 3 }]);
        r1.query_sequence = b"ACG".to_vec();
        let mut r2 = r(1, vec![CigarOp { kind: CigarKind::Match, len: 3 }]);
        r2.query_sequence = b"ATG".to_vec();
        let cov = compute(&[r1, r2], 1, 3);
        assert!(cov.mismatch_fractions(0, b'A', 0.2).is_none());
        let fracs = cov.mismatch_fractions(1, b'C', 0.2).unwrap();
        assert!((fracs[1].1 - 0.5).abs() < 1e-5);
        assert!((fracs[3].1 - 0.5).abs() < 1e-5);
    }
}
