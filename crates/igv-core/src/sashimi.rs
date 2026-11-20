//! Splice-junction extraction for RNA-seq Sashimi plots (`CigarKind::Skip` `N` operations).

use std::collections::BTreeMap;

use crate::source::annotation::Strand;
use crate::source::bam::{AlignmentRow, CigarKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpliceJunction {
    /// 1-based inclusive start of the intron (first skipped base).
    pub start: u64,
    /// 1-based inclusive end of the intron (last skipped base).
    pub end: u64,
    pub count: u32,
    pub strand: Strand,
}

/// Extract unique intron splice junctions from alignment rows (`CigarKind::Skip`),
/// keeping junctions with supporting reads `>= min_count`.
pub fn extract_splice_junctions(rows: &[AlignmentRow], min_count: u32) -> Vec<SpliceJunction> {
    let mut map: BTreeMap<(u64, u64, u8), u32> = BTreeMap::new();
    for row in rows {
        let strand_key = match row.xs_strand {
            Some(Strand::Forward) => 1u8,
            Some(Strand::Reverse) => 2u8,
            _ => {
                if row.is_reverse {
                    2u8
                } else {
                    1u8
                }
            }
        };
        let mut ref_pos = row.ref_start;
        for op in &row.cigar {
            match op.kind {
                CigarKind::Skip => {
                    if op.len > 0 {
                        let start = ref_pos;
                        let end = ref_pos + op.len as u64 - 1;
                        *map.entry((start, end, strand_key)).or_insert(0) += 1;
                    }
                    ref_pos += op.len as u64;
                }
                CigarKind::Match
                | CigarKind::Deletion
                | CigarKind::SeqMatch
                | CigarKind::SeqMismatch => {
                    ref_pos += op.len as u64;
                }
                CigarKind::Insertion
                | CigarKind::SoftClip
                | CigarKind::HardClip
                | CigarKind::Padding => {}
            }
        }
    }

    map.into_iter()
        .filter(|&(_, count)| count >= min_count.max(1))
        .map(|((start, end, sk), count)| SpliceJunction {
            start,
            end,
            count,
            strand: match sk {
                1 => Strand::Forward,
                2 => Strand::Reverse,
                _ => Strand::Unknown,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::bam::CigarOp;

    #[test]
    fn aggregates_intron_skips_into_junctions() {
        let r = AlignmentRow {
            ref_start: 100,
            ref_end: 219,
            cigar: vec![
                CigarOp { kind: CigarKind::Match, len: 10 },
                CigarOp { kind: CigarKind::Skip, len: 100 },
                CigarOp { kind: CigarKind::Match, len: 10 },
            ],
            xs_strand: Some(Strand::Forward),
            ..Default::default()
        };
        let juncs = extract_splice_junctions(&[r.clone(), r], 1);
        assert_eq!(juncs.len(), 1);
        assert_eq!(juncs[0].start, 110);
        assert_eq!(juncs[0].end, 209);
        assert_eq!(juncs[0].count, 2);
        assert_eq!(juncs[0].strand, Strand::Forward);
    }
}
