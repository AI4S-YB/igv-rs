//! SAM `MM`/`ML` base-modification tag parser (`5mC`, `5hmC`, `6mA`, etc.).
//!
//! Follows the SAMtags specification:
//! - `MM:Z:` (or legacy `Mm:Z:`): `<base><strand><mods>[.|?],d0,d1,...;`
//! - `ML:B:C` (or legacy `Ml:B:C`): `u8` probability array `(ml + 0.5) / 256.0`
//! - When `FLAG & 0x10 != 0` (`is_reverse == true`), `MM` deltas count
//!   occurrences along the **original sequenced orientation** (the reverse
//!   complement of BAM's `SEQ`), so we walk `seq` backwards and complement bases.

use crate::sequence::complement_base;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseModCall {
    /// 0-based index into BAM's forward-reference-oriented `query_sequence`.
    pub q_idx: usize,
    /// Modification code (`m` = 5mC, `h` = 5hmC, `a` = 6mA, etc.).
    pub code: char,
    /// Opposite-strand modification (e.g., `C-m` on CpG).
    pub opposite_strand: bool,
    /// Modification probability in `[0.0, 1.0]`.
    pub prob: f32,
}

/// Parse `MM`/`ML` tag strings and bytes into per-query-base modification calls.
pub fn parse_mm_ml(
    mm_tag: &str,
    ml_bytes: Option<&[u8]>,
    query_seq: &[u8],
    is_reverse: bool,
) -> Vec<BaseModCall> {
    if mm_tag.is_empty() || query_seq.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut ml_cursor = 0usize;

    for series in mm_tag.split(';') {
        let series = series.trim();
        if series.is_empty() {
            continue;
        }
        let mut parts = series.split(',');
        let Some(header) = parts.next() else {
            continue;
        };
        let mut hchars = header.chars();
        let Some(canon_base) = hchars.next() else {
            continue;
        };
        let Some(strand_ch) = hchars.next() else {
            continue;
        };
        if strand_ch != '+' && strand_ch != '-' {
            continue;
        }
        let opposite_strand = strand_ch == '-';
        let rest: String = hchars.collect();
        let rest_trimmed = rest.trim_end_matches(['.', '?']);
        if rest_trimmed.is_empty() {
            continue;
        }

        // Collect modification codes (either single-letter codes or ChEBI numeric in parens/digits)
        let mod_codes = parse_mod_codes(rest_trimmed);
        if mod_codes.is_empty() {
            continue;
        }

        // Base expected in 5'→3' sequenced orientation:
        // for '+' strand, look for `canon_base`; for '-' strand, look for complement of `canon_base`.
        let target_sequenced_base = if opposite_strand {
            complement_base(canon_base as u8).to_ascii_uppercase()
        } else {
            (canon_base as u8).to_ascii_uppercase()
        };

        // Build iterator over `(q_idx, sequenced_base)` in 5'→3' sequenced orientation.
        let deltas: Vec<usize> = parts.filter_map(|p| p.trim().parse::<usize>().ok()).collect();
        if deltas.is_empty() {
            continue;
        }

        let seq_len = query_seq.len();
        let mut s_pos = 0usize;

        for delta in deltas {
            let mut skipped = 0usize;
            let mut matched_q_idx = None;
            while s_pos < seq_len {
                let q_idx = if is_reverse { seq_len - 1 - s_pos } else { s_pos };
                let seq_base = if is_reverse {
                    complement_base(query_seq[q_idx]).to_ascii_uppercase()
                } else {
                    query_seq[q_idx].to_ascii_uppercase()
                };
                s_pos += 1;
                if target_sequenced_base == b'N' || seq_base == target_sequenced_base {
                    if skipped == delta {
                        matched_q_idx = Some(q_idx);
                        break;
                    }
                    skipped += 1;
                }
            }

            if let Some(q_idx) = matched_q_idx {
                for &code in &mod_codes {
                    let prob = match ml_bytes.and_then(|arr| arr.get(ml_cursor).copied()) {
                        Some(v) => (v as f32 + 0.5) / 256.0,
                        None => 1.0,
                    };
                    ml_cursor += 1;
                    out.push(BaseModCall {
                        q_idx,
                        code,
                        opposite_strand,
                        prob,
                    });
                }
            } else {
                ml_cursor += mod_codes.len();
            }
        }
    }

    out.sort_by_key(|c| c.q_idx);
    out
}

fn parse_mod_codes(s: &str) -> Vec<char> {
    if s.chars().all(|c| c.is_ascii_digit()) {
        // ChEBI code (e.g. 76792 = 5hmC, 27551 = 5mC)
        return match s {
            "27551" => vec!['m'],
            "76792" => vec!['h'],
            "21839" => vec!['a'],
            _ => vec!['m'],
        };
    }
    s.chars().filter(|c| c.is_ascii_alphabetic()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_forward_5mc_mm_ml() {
        // Seq: A C G T C G (C at indices 1 and 4)
        let calls = parse_mm_ml("C+m?,0,0;", Some(&[250, 10]), b"ACGTCG", false);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].q_idx, 1);
        assert_eq!(calls[0].code, 'm');
        assert!(calls[0].prob > 0.95);
        assert_eq!(calls[1].q_idx, 4);
        assert!(calls[1].prob < 0.1);
    }

    #[test]
    fn parses_reverse_strand_mm_ml_using_revcomp_order() {
        // BAM SEQ (ref orientation): C G A T G C
        // Sequenced 5'->3' = revcomp("CGATGC") = G C A T C G (C at seq_pos 1 -> q_idx 4, seq_pos 4 -> q_idx 1)
        // MM: C+m?,0,0; -> first C in sequenced order is q_idx=4 (prob 240), second is q_idx=1 (prob 20).
        let calls = parse_mm_ml("C+m?,0,0;", Some(&[240, 20]), b"CGATGC", true);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].q_idx, 1);
        assert!(calls[0].prob < 0.15);
        assert_eq!(calls[1].q_idx, 4);
        assert!(calls[1].prob > 0.90);
    }
}
