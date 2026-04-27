//! Expand `AlignmentRow` + reference into per-base display cells.

use crate::source::bam::{AlignmentRow, CigarKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseGlyph {
    /// Match against reference. Renderer chooses `.` style by default.
    Match,
    /// Mismatch — actual base is the inner byte (uppercase ASCII).
    Mismatch(u8),
    /// Reference deletion in this read at this position.
    Deletion,
    /// Soft-clipped base — renderer hides by default; carries actual base.
    SoftClip(u8),
}

#[derive(Debug, Clone)]
pub struct ReadCells {
    /// 1-based inclusive coordinate of the first cell.
    pub ref_start: u64,
    /// One entry per reference position consumed; insertions are tracked
    /// separately in `insertions`.
    pub cells: Vec<BaseGlyph>,
    /// Insertions to the reference, keyed by 1-based reference position
    /// **before** which the insertion sits. Value is the inserted bases.
    pub insertions: Vec<(u64, Vec<u8>)>,
}

/// Expand a single alignment row into reference-space display cells.
///
/// `reference` is the bytes of the **viewing** region, indexed by 1-based
/// `view_start`. Mismatch detection only happens for cells that fall inside
/// the view.
pub fn expand(
    row: &AlignmentRow,
    reference: &[u8],
    view_start: u64,
) -> ReadCells {
    let mut cells = Vec::new();
    let mut insertions = Vec::new();
    let mut ref_pos: u64 = row.ref_start;
    let mut q_idx: usize = 0;

    for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                for _ in 0..op.len {
                    let q_base = row.query_sequence.get(q_idx).copied().unwrap_or(b'N');
                    let r_idx_signed = ref_pos as i64 - view_start as i64;
                    let glyph = if r_idx_signed >= 0
                        && (r_idx_signed as usize) < reference.len()
                    {
                        let r_base = reference[r_idx_signed as usize].to_ascii_uppercase();
                        if q_base.to_ascii_uppercase() == r_base {
                            BaseGlyph::Match
                        } else {
                            BaseGlyph::Mismatch(q_base.to_ascii_uppercase())
                        }
                    } else {
                        BaseGlyph::Mismatch(q_base.to_ascii_uppercase())
                    };
                    cells.push(glyph);
                    ref_pos += 1;
                    q_idx += 1;
                }
            }
            CigarKind::Deletion | CigarKind::Skip => {
                for _ in 0..op.len {
                    cells.push(BaseGlyph::Deletion);
                    ref_pos += 1;
                }
            }
            CigarKind::Insertion => {
                let bases = row
                    .query_sequence
                    .get(q_idx..q_idx + op.len as usize)
                    .map(|s| s.to_vec())
                    .unwrap_or_default();
                insertions.push((ref_pos, bases));
                q_idx += op.len as usize;
            }
            CigarKind::SoftClip => {
                // Soft-clipped bases consume query but not reference. We don't
                // place them in cells; they simply advance q_idx.
                q_idx += op.len as usize;
            }
            CigarKind::HardClip | CigarKind::Padding => {
                // Consume neither.
            }
        }
    }

    ReadCells {
        ref_start: row.ref_start,
        cells,
        insertions,
    }
}

/// Greedy first-fit lane assignment. Each entry is the lane index for the
/// corresponding row in `rows`. Reads with `last.ref_end + 1 < row.ref_start`
/// share a lane; otherwise a new lane is created.
pub fn assign_lanes(rows: &[AlignmentRow]) -> Vec<u32> {
    let mut last_ends: Vec<u64> = Vec::new();
    let mut assigned: Vec<u32> = Vec::with_capacity(rows.len());
    for row in rows {
        let mut placed: Option<usize> = None;
        for (i, end) in last_ends.iter_mut().enumerate() {
            if *end + 1 < row.ref_start {
                *end = row.ref_end;
                placed = Some(i);
                break;
            }
        }
        let lane = match placed {
            Some(i) => i,
            None => {
                last_ends.push(row.ref_end);
                last_ends.len() - 1
            }
        };
        assigned.push(lane as u32);
    }
    assigned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::bam::{AlignmentRow, CigarOp};

    fn row(cigar: Vec<CigarOp>, seq: &[u8], start: u64) -> AlignmentRow {
        let consumed: u32 = cigar
            .iter()
            .filter(|op| {
                matches!(
                    op.kind,
                    CigarKind::Match
                        | CigarKind::Deletion
                        | CigarKind::Skip
                        | CigarKind::SeqMatch
                        | CigarKind::SeqMismatch
                )
            })
            .map(|op| op.len)
            .sum();
        AlignmentRow {
            query_name: "r".into(),
            flag: 0,
            ref_start: start,
            ref_end: start + consumed.saturating_sub(1) as u64,
            mapq: 60,
            is_reverse: false,
            query_sequence: seq.to_vec(),
            cigar,
            tag: None,
        }
    }

    #[test]
    fn match_only_no_mismatches() {
        let r = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 1);
        let cells = expand(&r, b"ACGT", 1);
        assert_eq!(cells.cells, vec![
            BaseGlyph::Match,
            BaseGlyph::Match,
            BaseGlyph::Match,
            BaseGlyph::Match,
        ]);
        assert!(cells.insertions.is_empty());
    }

    #[test]
    fn match_with_mismatch() {
        let r = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGA", 1);
        let cells = expand(&r, b"ACGT", 1);
        assert!(matches!(cells.cells[3], BaseGlyph::Mismatch(b'A')));
    }

    #[test]
    fn insertion_recorded_separately() {
        let r = row(
            vec![
                CigarOp { kind: CigarKind::Match, len: 2 },
                CigarOp { kind: CigarKind::Insertion, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 2 },
            ],
            b"ACTTGT",
            1,
        );
        let cells = expand(&r, b"ACGT", 1);
        assert_eq!(cells.cells.len(), 4);
        assert_eq!(cells.insertions.len(), 1);
        assert_eq!(cells.insertions[0].0, 3);
        assert_eq!(cells.insertions[0].1, b"TT".to_vec());
    }

    #[test]
    fn deletion_marked() {
        let r = row(
            vec![
                CigarOp { kind: CigarKind::Match, len: 2 },
                CigarOp { kind: CigarKind::Deletion, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 2 },
            ],
            b"ACGT",
            1,
        );
        let cells = expand(&r, b"ACGTAC", 1);
        assert_eq!(cells.cells[2], BaseGlyph::Deletion);
        assert_eq!(cells.cells[3], BaseGlyph::Deletion);
    }
}
