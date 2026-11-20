//! Sequence utilities: complement strand, 3-frame amino-acid translation,
//! and IUPAC / regex motif search across both DNA strands.

use regex::RegexBuilder;

use crate::error::{IgvError, Result};
use crate::source::annotation::Strand;

/// DNA complement of a single ASCII base, preserving case.
pub fn complement_base(b: u8) -> u8 {
    match b {
        b'A' => b'T',
        b'T' | b'U' => b'A',
        b'C' => b'G',
        b'G' => b'C',
        b'a' => b't',
        b't' | b'u' => b'a',
        b'c' => b'g',
        b'g' => b'c',
        b'R' => b'Y',
        b'Y' => b'R',
        b'M' => b'K',
        b'K' => b'M',
        b'B' => b'V',
        b'V' => b'B',
        b'D' => b'H',
        b'H' => b'D',
        b'r' => b'y',
        b'y' => b'r',
        b'm' => b'k',
        b'k' => b'm',
        b'b' => b'v',
        b'v' => b'b',
        b'd' => b'h',
        b'h' => b'd',
        other => other,
    }
}

/// Strand-wise base-by-base complement (same coordinate order as `seq`).
pub fn complement_seq(seq: &[u8]) -> Vec<u8> {
    seq.iter().copied().map(complement_base).collect()
}

/// Reverse-complement of `seq`.
pub fn reverse_complement(seq: &[u8]) -> Vec<u8> {
    seq.iter().rev().copied().map(complement_base).collect()
}

/// Standard genetic code lookup for an uppercase 3-byte DNA codon.
pub fn translate_codon(codon: [u8; 3]) -> char {
    match [
        codon[0].to_ascii_uppercase(),
        codon[1].to_ascii_uppercase(),
        codon[2].to_ascii_uppercase(),
    ] {
        [b'T', b'T', b'T' | b'C'] => 'F',
        [b'T', b'T', b'A' | b'G'] | [b'C', b'T', _] => 'L',
        [b'A', b'T', b'T' | b'C' | b'A'] => 'I',
        [b'A', b'T', b'G'] => 'M',
        [b'G', b'T', _] => 'V',
        [b'T', b'C', _] | [b'A', b'G', b'T' | b'C'] => 'S',
        [b'C', b'C', _] => 'P',
        [b'A', b'C', _] => 'T',
        [b'G', b'C', _] => 'A',
        [b'T', b'A', b'T' | b'C'] => 'Y',
        [b'T', b'A', b'A' | b'G'] | [b'T', b'G', b'A'] => '*',
        [b'C', b'A', b'T' | b'C'] => 'H',
        [b'C', b'A', b'A' | b'G'] => 'Q',
        [b'A', b'A', b'T' | b'C'] => 'N',
        [b'A', b'A', b'A' | b'G'] => 'K',
        [b'G', b'A', b'T' | b'C'] => 'D',
        [b'G', b'A', b'A' | b'G'] => 'E',
        [b'T', b'G', b'T' | b'C'] => 'C',
        [b'T', b'G', b'G'] => 'W',
        [b'C', b'G', _] | [b'A', b'G', b'A' | b'G'] => 'R',
        [b'G', b'G', _] => 'G',
        _ => 'X',
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodonGlyph {
    pub start_bp: u64, // 1-based inclusive
    pub end_bp: u64,   // 1-based inclusive
    pub aa: char,
    pub is_start: bool,
    pub is_stop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AminoAcidFrame {
    pub frame: u8, // 0, 1, or 2
    pub codons: Vec<CodonGlyph>,
}

/// Translate `seq` (starting at 1-based `region_start_1`) across all 3 reading
/// frames aligned globally to genomic coordinate `pos_1 % 3` so codons stay
/// phase-locked during panning.
pub fn translate_three_frames(
    seq: &[u8],
    region_start_1: u64,
    reverse: bool,
) -> [AminoAcidFrame; 3] {
    let mut frames = [
        AminoAcidFrame { frame: 0, codons: Vec::new() },
        AminoAcidFrame { frame: 1, codons: Vec::new() },
        AminoAcidFrame { frame: 2, codons: Vec::new() },
    ];
    if seq.len() < 3 {
        return frames;
    }

    for frame_idx in 0u64..3 {
        // First 0-based index `i` such that `(region_start_1 + i) % 3 == frame_idx` (or offset)
        let rem = (region_start_1.saturating_sub(1)) % 3;
        let offset = ((frame_idx + 3 - rem) % 3) as usize;
        let mut i = offset;
        let codons = &mut frames[frame_idx as usize].codons;
        while i + 3 <= seq.len() {
            let raw = [seq[i], seq[i + 1], seq[i + 2]];
            let codon = if reverse {
                // Reverse strand reads 3'→5' (so codon is complement in reverse order)
                [
                    complement_base(raw[2]),
                    complement_base(raw[1]),
                    complement_base(raw[0]),
                ]
            } else {
                raw
            };
            let aa = translate_codon(codon);
            let start_bp = region_start_1 + i as u64;
            codons.push(CodonGlyph {
                start_bp,
                end_bp: start_bp + 2,
                aa,
                is_start: aa == 'M',
                is_stop: aa == '*',
            });
            i += 3;
        }
    }
    frames
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotifHit {
    pub start: u64, // 1-based inclusive
    pub end: u64,   // 1-based inclusive
    pub strand: Strand,
    pub matched: String,
}

/// Convert an IUPAC nucleotide motif (e.g., `NGG`, `TATAAA`, `CANNTG`) into a regex
/// pattern string. Falls back to raw regex characters if the pattern already uses
/// regex syntax (`[`, `(`, `*`, `+`, `?`, `.`).
pub fn iupac_to_regex(pattern: &str) -> String {
    if pattern.chars().any(|c| matches!(c, '[' | '(' | '.' | '*' | '+' | '?' | '|')) {
        return pattern.to_string();
    }
    let mut out = String::with_capacity(pattern.len() * 4);
    for ch in pattern.chars() {
        match ch.to_ascii_uppercase() {
            'A' => out.push('A'),
            'C' => out.push('C'),
            'G' => out.push('G'),
            'T' | 'U' => out.push('T'),
            'R' => out.push_str("[AG]"),
            'Y' => out.push_str("[CT]"),
            'S' => out.push_str("[GC]"),
            'W' => out.push_str("[AT]"),
            'K' => out.push_str("[GT]"),
            'M' => out.push_str("[AC]"),
            'B' => out.push_str("[CGT]"),
            'D' => out.push_str("[AGT]"),
            'H' => out.push_str("[ACT]"),
            'V' => out.push_str("[ACG]"),
            'N' => out.push_str("[ACGTN]"),
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out
}

/// Search `seq` (starting at 1-based `region_start_1`) on both forward and reverse
/// strands for an IUPAC or regex motif.
pub fn search_iupac_motif(
    pattern: &str,
    seq: &[u8],
    region_start_1: u64,
) -> Result<Vec<MotifHit>> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() || seq.is_empty() {
        return Ok(Vec::new());
    }
    let re_str = iupac_to_regex(trimmed);
    let re = RegexBuilder::new(&re_str)
        .case_insensitive(true)
        .build()
        .map_err(|e| IgvError::Other(format!("invalid motif '{pattern}': {e}")))?;

    let fwd_str = String::from_utf8_lossy(seq);
    let mut hits = Vec::new();
    for m in re.find_iter(&fwd_str) {
        if m.end() > m.start() {
            hits.push(MotifHit {
                start: region_start_1 + m.start() as u64,
                end: region_start_1 + m.end() as u64 - 1,
                strand: Strand::Forward,
                matched: m.as_str().to_string(),
            });
        }
    }

    let rev_bytes = reverse_complement(seq);
    let rev_str = String::from_utf8_lossy(&rev_bytes);
    let len = seq.len();
    for m in re.find_iter(&rev_str) {
        if m.end() > m.start() {
            let fwd_start_0 = len - m.end();
            let fwd_end_0 = len - m.start() - 1;
            hits.push(MotifHit {
                start: region_start_1 + fwd_start_0 as u64,
                end: region_start_1 + fwd_end_0 as u64,
                strand: Strand::Reverse,
                matched: m.as_str().to_string(),
            });
        }
    }

    hits.sort_by_key(|h| (h.start, h.end));
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complement_and_reverse_complement_work() {
        assert_eq!(complement_seq(b"ACGTacgt"), b"TGCAtgca");
        assert_eq!(reverse_complement(b"AAGGTT"), b"AACCTT");
    }

    #[test]
    fn translate_three_frames_finds_start_and_stop() {
        // ATG (M) TAA (*)
        let frames = translate_three_frames(b"ATGTAA", 1, false);
        assert_eq!(frames[0].codons.len(), 2);
        assert_eq!(frames[0].codons[0].aa, 'M');
        assert!(frames[0].codons[0].is_start);
        assert_eq!(frames[0].codons[1].aa, '*');
        assert!(frames[0].codons[1].is_stop);
    }

    #[test]
    fn translate_reverse_strand_decodes_revcomp_codons() {
        // Forward "CAT" on reverse strand = revcomp("CAT") = "ATG" -> M
        let frames = translate_three_frames(b"CAT", 1, true);
        assert_eq!(frames[0].codons[0].aa, 'M');
    }

    #[test]
    fn motif_search_matches_both_strands_with_iupac() {
        // "AGG" at 104..=106 (fwd), "CCA" at 102..=104 -> revcomp "TGG" (rev),
        // "CCT" at 107..=109 -> revcomp "AGG" (rev).
        let hits = search_iupac_motif("NGG", b"ACCAGGCCT", 101).unwrap();
        assert!(hits.iter().any(|h| h.strand == Strand::Forward && h.start == 104 && h.end == 106));
        assert!(hits.iter().any(|h| h.strand == Strand::Reverse && h.start == 107 && h.end == 109));
    }
}
