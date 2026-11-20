//! Alignment expansion, lane packing, sorting, grouping, pairing,
//! downsampling, bisulfite & `MM`/`ML` base-modification classification.

pub mod mods;

use std::collections::{BTreeMap, HashMap};

use crate::alignment::mods::BaseModCall;
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
    /// 1-based inclusive coordinate of the first aligned cell.
    pub ref_start: u64,
    /// One entry per reference position consumed; insertions are tracked
    /// separately in `insertions`.
    pub cells: Vec<BaseGlyph>,
    /// Query sequence index for each cell (`None` for deletions/skips).
    pub cell_q_idx: Vec<Option<usize>>,
    /// Insertions to the reference, keyed by 1-based reference position
    /// **before** which the insertion sits. Value is the inserted bases.
    pub insertions: Vec<(u64, Vec<u8>)>,
    /// Optional left soft-clip overhang bases `(start_1_based, bases)` when enabled.
    pub soft_clip_left: Option<(u64, Vec<u8>)>,
    /// Optional right soft-clip overhang bases `(start_1_based, bases)` when enabled.
    pub soft_clip_right: Option<(u64, Vec<u8>)>,
}

/// Expand a single alignment row into reference-space display cells (hiding soft clips).
pub fn expand(row: &AlignmentRow, reference: &[u8], view_start: u64) -> ReadCells {
    expand_with_opts(row, reference, view_start, false)
}

/// Expand a single alignment row into reference-space display cells, optionally
/// including left/right soft-clipped overhangs.
pub fn expand_with_opts(
    row: &AlignmentRow,
    reference: &[u8],
    view_start: u64,
    show_soft_clips: bool,
) -> ReadCells {
    let mut cells = Vec::new();
    let mut cell_q_idx = Vec::new();
    let mut insertions = Vec::new();
    let mut ref_pos: u64 = row.ref_start;
    let mut q_idx: usize = 0;
    let mut soft_clip_left = None;
    let mut soft_clip_right = None;
    let mut seen_ref_consuming = false;

    for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                seen_ref_consuming = true;
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
                    cell_q_idx.push(Some(q_idx));
                    ref_pos += 1;
                    q_idx += 1;
                }
            }
            CigarKind::Deletion | CigarKind::Skip => {
                seen_ref_consuming = true;
                for _ in 0..op.len {
                    cells.push(BaseGlyph::Deletion);
                    cell_q_idx.push(None);
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
                if show_soft_clips {
                    let bases = row
                        .query_sequence
                        .get(q_idx..q_idx + op.len as usize)
                        .map(|s| s.to_vec())
                        .unwrap_or_default();
                    if !bases.is_empty() {
                        if !seen_ref_consuming {
                            let clip_start = row.ref_start.saturating_sub(bases.len() as u64).max(1);
                            soft_clip_left = Some((clip_start, bases));
                        } else {
                            soft_clip_right = Some((ref_pos, bases));
                        }
                    }
                }
                q_idx += op.len as usize;
            }
            CigarKind::HardClip | CigarKind::Padding => {}
        }
    }

    ReadCells {
        ref_start: row.ref_start,
        cells,
        cell_q_idx,
        insertions,
        soft_clip_left,
        soft_clip_right,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignmentColorMode {
    #[default]
    Strand,
    InsertSize,
    PairOrientation,
    MateChrom,
    Tag,
    MappingQuality,
    BaseModification,
    Bisulfite,
}

impl AlignmentColorMode {
    pub const ALL: [Self; 8] = [
        Self::Strand,
        Self::InsertSize,
        Self::PairOrientation,
        Self::MateChrom,
        Self::Tag,
        Self::MappingQuality,
        Self::BaseModification,
        Self::Bisulfite,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "strand" | "read_strand" | "none" => Some(Self::Strand),
            "insert" | "insert_size" | "isize" => Some(Self::InsertSize),
            "pair" | "pair_orientation" | "orientation" => Some(Self::PairOrientation),
            "mate" | "mate_chr" | "mate_chrom" => Some(Self::MateChrom),
            "tag" => Some(Self::Tag),
            "mapq" | "quality" => Some(Self::MappingQuality),
            "mod" | "mods" | "methylation" | "base_modification" => Some(Self::BaseModification),
            "bisulfite" | "cg" | "cpg" => Some(Self::Bisulfite),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Strand => "strand",
            Self::InsertSize => "insert-size",
            Self::PairOrientation => "pair-orientation",
            Self::MateChrom => "mate-chrom",
            Self::Tag => "tag",
            Self::MappingQuality => "mapq",
            Self::BaseModification => "base-mod",
            Self::Bisulfite => "bisulfite",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignmentSortMode {
    #[default]
    None,
    Start,
    Base,
    Strand,
    MappingQuality,
    InsertSize,
    MateChrom,
    Tag,
    Haplotype,
}

impl AlignmentSortMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "off" => Some(Self::None),
            "start" | "pos" => Some(Self::Start),
            "base" => Some(Self::Base),
            "strand" => Some(Self::Strand),
            "mapq" | "quality" => Some(Self::MappingQuality),
            "insert" | "insert_size" | "isize" => Some(Self::InsertSize),
            "mate" | "mate_chr" | "mate_chrom" => Some(Self::MateChrom),
            "tag" => Some(Self::Tag),
            "hp" | "haplotype" | "phase" => Some(Self::Haplotype),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Start => "start",
            Self::Base => "base",
            Self::Strand => "strand",
            Self::MappingQuality => "mapq",
            Self::InsertSize => "insert",
            Self::MateChrom => "mate",
            Self::Tag => "tag",
            Self::Haplotype => "haplotype",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignmentGroupMode {
    #[default]
    None,
    Strand,
    ReadGroup,
    Haplotype,
    PairOrientation,
    MateChrom,
    Tag,
    Chimeric,
}

impl AlignmentGroupMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "off" => Some(Self::None),
            "strand" => Some(Self::Strand),
            "rg" | "read_group" | "readgroup" => Some(Self::ReadGroup),
            "hp" | "haplotype" | "phase" => Some(Self::Haplotype),
            "pair" | "pair_orientation" | "orientation" => Some(Self::PairOrientation),
            "mate" | "mate_chr" | "mate_chrom" => Some(Self::MateChrom),
            "tag" => Some(Self::Tag),
            "chimeric" | "split" => Some(Self::Chimeric),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Strand => "strand",
            Self::ReadGroup => "read-group",
            Self::Haplotype => "haplotype",
            Self::PairOrientation => "pair-orientation",
            Self::MateChrom => "mate-chrom",
            Self::Tag => "tag",
            Self::Chimeric => "chimeric",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlignmentRenderOpts {
    pub color_mode: AlignmentColorMode,
    pub sort_mode: AlignmentSortMode,
    pub group_mode: AlignmentGroupMode,
    pub view_as_pairs: bool,
    pub show_soft_clips: bool,
    /// Optional reservoir cap per window (`None` = unlimited).
    pub max_depth: Option<usize>,
    pub insert_min: i32,
    pub insert_max: i32,
}

impl Default for AlignmentRenderOpts {
    fn default() -> Self {
        Self {
            color_mode: AlignmentColorMode::Strand,
            sort_mode: AlignmentSortMode::None,
            group_mode: AlignmentGroupMode::None,
            view_as_pairs: false,
            show_soft_clips: false,
            max_depth: Some(200),
            insert_min: 100,
            insert_max: 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairOrientation {
    Lr,
    Rl,
    Ll,
    Rr,
    Other,
}

/// Classify paired-end read orientation when mate position is known on the same chrom.
pub fn pair_orientation(row: &AlignmentRow) -> PairOrientation {
    let is_paired = row.flag & 0x1 != 0;
    let mate_unmapped = row.flag & 0x8 != 0;
    if !is_paired || mate_unmapped || row.mate_chrom.as_deref() == Some("*") {
        return PairOrientation::Other;
    }
    let Some(mpos) = row.mate_pos else {
        return PairOrientation::Other;
    };
    let mate_rev = row.flag & 0x20 != 0;
    let is_left = row.ref_start <= mpos;
    match (row.is_reverse, mate_rev, is_left) {
        (false, true, true) | (true, false, false) => PairOrientation::Lr,
        (true, false, true) | (false, true, false) => PairOrientation::Rl,
        (false, false, _) => PairOrientation::Ll,
        (true, true, _) => PairOrientation::Rr,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BisulfiteCall {
    Methylated,
    Unmethylated,
}

/// Evaluate bisulfite CpG status at query base `q_base` against `ref_base` and
/// next/prev neighbor context (`CG` context).
pub fn bisulfite_call(
    row_is_reverse: bool,
    q_base: u8,
    ref_base: u8,
    next_ref_base: Option<u8>,
    prev_ref_base: Option<u8>,
) -> Option<BisulfiteCall> {
    let qb = q_base.to_ascii_uppercase();
    let rb = ref_base.to_ascii_uppercase();
    if !row_is_reverse {
        if rb == b'C' && next_ref_base.map(|b| b.to_ascii_uppercase()) == Some(b'G') {
            if qb == b'C' {
                return Some(BisulfiteCall::Methylated);
            }
            if qb == b'T' {
                return Some(BisulfiteCall::Unmethylated);
            }
        }
    } else if rb == b'G' && prev_ref_base.map(|b| b.to_ascii_uppercase()) == Some(b'C') {
        if qb == b'G' {
            return Some(BisulfiteCall::Methylated);
        }
        if qb == b'A' {
            return Some(BisulfiteCall::Unmethylated);
        }
    }
    None
}

/// Lookup the highest-confidence base modification at query index `q_idx`.
pub fn mod_at_query_index(mods: &[BaseModCall], q_idx: usize) -> Option<BaseModCall> {
    mods.iter()
        .filter(|m| m.q_idx == q_idx)
        .max_by(|a, b| a.prob.total_cmp(&b.prob))
        .copied()
}

/// Deterministic reservoir downsampling across fixed genomic bins (`window_bp`).
/// Returns `(kept_rows, dropped_count)`.
pub fn downsample_reads(
    rows: Vec<AlignmentRow>,
    max_depth: usize,
    window_bp: u64,
    seed: u64,
) -> (Vec<AlignmentRow>, usize) {
    if max_depth == 0 || rows.len() <= max_depth {
        return (rows, 0);
    }
    let win = window_bp.max(1);
    let mut bins: BTreeMap<u64, Vec<AlignmentRow>> = BTreeMap::new();
    for row in rows {
        let bin_id = row.ref_start / win;
        bins.entry(bin_id).or_default().push(row);
    }

    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut next_rand = || -> u64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut kept = Vec::new();
    let mut dropped = 0usize;
    for (_bin, bucket) in bins {
        if bucket.len() <= max_depth {
            kept.extend(bucket);
            continue;
        }
        let mut reservoir: Vec<AlignmentRow> = Vec::with_capacity(max_depth);
        for (i, row) in bucket.into_iter().enumerate() {
            if i < max_depth {
                reservoir.push(row);
            } else {
                let j = (next_rand() as usize) % (i + 1);
                if j < max_depth {
                    reservoir[j] = row;
                }
                dropped += 1;
            }
        }
        kept.extend(reservoir);
    }
    kept.sort_by_key(|r| (r.ref_start, r.ref_end));
    (kept, dropped)
}

/// Sort alignment rows in place according to `mode` evaluated around `center_bp`.
pub fn sort_rows(
    rows: &mut [AlignmentRow],
    mode: AlignmentSortMode,
    center_bp: u64,
    reference: &[u8],
    view_start: u64,
) {
    match mode {
        AlignmentSortMode::None => {}
        AlignmentSortMode::Start => rows.sort_by_key(|r| (r.ref_start, r.ref_end)),
        AlignmentSortMode::Strand => rows.sort_by_key(|r| (r.is_reverse, r.ref_start)),
        AlignmentSortMode::MappingQuality => {
            rows.sort_by_key(|r| (std::cmp::Reverse(r.mapq), r.ref_start));
        }
        AlignmentSortMode::InsertSize => {
            rows.sort_by_key(|r| (std::cmp::Reverse(r.template_len.unsigned_abs()), r.ref_start));
        }
        AlignmentSortMode::MateChrom => {
            rows.sort_by(|a, b| {
                a.mate_chrom
                    .as_deref()
                    .unwrap_or("~")
                    .cmp(b.mate_chrom.as_deref().unwrap_or("~"))
                    .then_with(|| a.ref_start.cmp(&b.ref_start))
            });
        }
        AlignmentSortMode::Tag => {
            rows.sort_by(|a, b| {
                a.tag
                    .as_ref()
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("~")
                    .cmp(b.tag.as_ref().map(|(_, v)| v.as_str()).unwrap_or("~"))
                    .then_with(|| a.ref_start.cmp(&b.ref_start))
            });
        }
        AlignmentSortMode::Haplotype => {
            rows.sort_by_key(|r| (r.haplotype.unwrap_or(u32::MAX), r.ref_start));
        }
        AlignmentSortMode::Base => {
            let ref_idx = center_bp.saturating_sub(view_start) as usize;
            let ref_base = reference
                .get(ref_idx)
                .map(|b| b.to_ascii_uppercase())
                .unwrap_or(b'N');
            rows.sort_by_key(|r| {
                let rank = base_sort_rank_at(r, center_bp, ref_base);
                (rank, r.ref_start)
            });
        }
    }
}

fn base_sort_rank_at(row: &AlignmentRow, pos: u64, ref_base: u8) -> (u8, u8) {
    if pos < row.ref_start || pos > row.ref_end {
        return (3, b'~');
    }
    let mut ref_pos = row.ref_start;
    let mut q_idx = 0usize;
    for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                let end = ref_pos + op.len as u64;
                if pos >= ref_pos && pos < end {
                    let offset = (pos - ref_pos) as usize;
                    let qb = row
                        .query_sequence
                        .get(q_idx + offset)
                        .copied()
                        .unwrap_or(b'N')
                        .to_ascii_uppercase();
                    if qb == ref_base {
                        return (2, qb);
                    }
                    return (0, qb);
                }
                ref_pos = end;
                q_idx += op.len as usize;
            }
            CigarKind::Deletion | CigarKind::Skip => {
                let end = ref_pos + op.len as u64;
                if pos >= ref_pos && pos < end {
                    return (1, b'*');
                }
                ref_pos = end;
            }
            CigarKind::Insertion | CigarKind::SoftClip => {
                q_idx += op.len as usize;
            }
            CigarKind::HardClip | CigarKind::Padding => {}
        }
    }
    (3, b'~')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentBand {
    pub label: Option<String>,
    pub lane_offset: u32,
    pub lane_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MateLink {
    pub lane: u32,
    pub left_end: u64,   // 1-based inclusive end of left read
    pub right_start: u64, // 1-based inclusive start of right read
}

#[derive(Debug, Clone, Default)]
pub struct PackedAlignments {
    pub lanes: Vec<u32>,
    pub total_lanes: u16,
    pub bands: Vec<AlignmentBand>,
    pub mate_links: Vec<MateLink>,
}

/// Group reads (optional) and pack lanes (optionally pairing mates onto the same lane).
pub fn group_and_pack(
    rows: &[AlignmentRow],
    group_mode: AlignmentGroupMode,
    view_as_pairs: bool,
) -> PackedAlignments {
    if rows.is_empty() {
        return PackedAlignments::default();
    }

    if group_mode == AlignmentGroupMode::None {
        let (lanes, mate_links, count) = pack_subset(rows, &(0..rows.len()).collect::<Vec<_>>(), view_as_pairs);
        let mut out_lanes = vec![0u32; rows.len()];
        for (idx, lane) in lanes {
            out_lanes[idx] = lane;
        }
        return PackedAlignments {
            lanes: out_lanes,
            total_lanes: count.min(u16::MAX as u32) as u16,
            bands: vec![AlignmentBand {
                label: None,
                lane_offset: 0,
                lane_count: count,
            }],
            mate_links,
        };
    }

    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, row) in rows.iter().enumerate() {
        let key = group_key(row, group_mode);
        groups.entry(key).or_default().push(idx);
    }

    let mut out_lanes = vec![0u32; rows.len()];
    let mut bands = Vec::new();
    let mut all_links = Vec::new();
    let mut cursor_lane = 0u32;

    for (label, indices) in groups {
        // Reserve 1 header row per named group
        let header_lane = cursor_lane;
        let data_start = header_lane + 1;
        let (sub_lanes, mut sub_links, sub_count) = pack_subset(rows, &indices, view_as_pairs);
        for (orig_idx, lane) in sub_lanes {
            out_lanes[orig_idx] = data_start + lane;
        }
        for link in &mut sub_links {
            link.lane += data_start;
        }
        all_links.extend(sub_links);
        let band_lanes = 1 + sub_count.max(1);
        bands.push(AlignmentBand {
            label: Some(label),
            lane_offset: header_lane,
            lane_count: band_lanes,
        });
        cursor_lane += band_lanes;
    }

    PackedAlignments {
        lanes: out_lanes,
        total_lanes: cursor_lane.min(u16::MAX as u32) as u16,
        bands,
        mate_links: all_links,
    }
}

fn group_key(row: &AlignmentRow, mode: AlignmentGroupMode) -> String {
    match mode {
        AlignmentGroupMode::None => String::new(),
        AlignmentGroupMode::Strand => {
            if row.is_reverse {
                "Reverse (-)".into()
            } else {
                "Forward (+)".into()
            }
        }
        AlignmentGroupMode::ReadGroup => row
            .read_group
            .clone()
            .unwrap_or_else(|| "No RG".into()),
        AlignmentGroupMode::Haplotype => row
            .haplotype
            .map(|h| format!("HP:{h}"))
            .unwrap_or_else(|| "Unphased".into()),
        AlignmentGroupMode::PairOrientation => match pair_orientation(row) {
            PairOrientation::Lr => "LR".into(),
            PairOrientation::Rl => "RL".into(),
            PairOrientation::Ll => "LL".into(),
            PairOrientation::Rr => "RR".into(),
            PairOrientation::Other => "Other".into(),
        },
        AlignmentGroupMode::MateChrom => row
            .mate_chrom
            .clone()
            .unwrap_or_else(|| "None".into()),
        AlignmentGroupMode::Tag => row
            .tag
            .as_ref()
            .map(|(k, v)| format!("{k}:{v}"))
            .unwrap_or_else(|| "Untagged".into()),
        AlignmentGroupMode::Chimeric => {
            if row.sa_tag.is_some() || (row.flag & 0x800 != 0) {
                "Chimeric/SA".into()
            } else {
                "Primary".into()
            }
        }
    }
}

fn pack_subset(
    rows: &[AlignmentRow],
    indices: &[usize],
    view_as_pairs: bool,
) -> (Vec<(usize, u32)>, Vec<MateLink>, u32) {
    if !view_as_pairs {
        let mut last_ends: Vec<u64> = Vec::new();
        let mut out = Vec::with_capacity(indices.len());
        for &idx in indices {
            let row = &rows[idx];
            let mut placed = None;
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
            out.push((idx, lane as u32));
        }
        return (out, Vec::new(), last_ends.len() as u32);
    }

    // Group pairs sharing query_name (primary/paired reads) into composite intervals
    let mut by_qname: HashMap<&str, Vec<usize>> = HashMap::new();
    for &idx in indices {
        by_qname.entry(rows[idx].query_name.as_str()).or_default().push(idx);
    }

    #[derive(Clone)]
    struct Item {
        members: Vec<usize>,
        start: u64,
        end: u64,
        link: Option<(u64, u64)>,
    }

    let mut items = Vec::new();
    for (_qname, mut members) in by_qname {
        members.sort_by_key(|&i| rows[i].ref_start);
        if members.len() == 2 {
            let a = &rows[members[0]];
            let b = &rows[members[1]];
            let start = a.ref_start.min(b.ref_start);
            let end = a.ref_end.max(b.ref_end);
            let link = if a.ref_end + 1 < b.ref_start {
                Some((a.ref_end, b.ref_start))
            } else {
                None
            };
            items.push(Item {
                members,
                start,
                end,
                link,
            });
        } else {
            for idx in members {
                let r = &rows[idx];
                items.push(Item {
                    members: vec![idx],
                    start: r.ref_start,
                    end: r.ref_end,
                    link: None,
                });
            }
        }
    }
    items.sort_by_key(|it| (it.start, it.end));

    let mut last_ends: Vec<u64> = Vec::new();
    let mut assigned = Vec::with_capacity(indices.len());
    let mut links = Vec::new();
    for it in items {
        let mut placed = None;
        for (i, end) in last_ends.iter_mut().enumerate() {
            if *end + 1 < it.start {
                *end = it.end;
                placed = Some(i);
                break;
            }
        }
        let lane = match placed {
            Some(i) => i,
            None => {
                last_ends.push(it.end);
                last_ends.len() - 1
            }
        } as u32;
        for m in it.members {
            assigned.push((m, lane));
        }
        if let Some((le, rs)) = it.link {
            links.push(MateLink {
                lane,
                left_end: le,
                right_start: rs,
            });
        }
    }

    (assigned, links, last_ends.len() as u32)
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
            ..Default::default()
        }
    }

    #[test]
    fn match_only_no_mismatches() {
        let r = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 1);
        let cells = expand(&r, b"ACGT", 1);
        assert_eq!(
            cells.cells,
            vec![
                BaseGlyph::Match,
                BaseGlyph::Match,
                BaseGlyph::Match,
                BaseGlyph::Match,
            ]
        );
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

    #[test]
    fn soft_clips_expand_when_enabled() {
        let r = row(
            vec![
                CigarOp { kind: CigarKind::SoftClip, len: 2 },
                CigarOp { kind: CigarKind::Match, len: 4 },
                CigarOp { kind: CigarKind::SoftClip, len: 1 },
            ],
            b"GGACGTC",
            10,
        );
        let cells = expand_with_opts(&r, b"ACGT", 10, true);
        assert_eq!(cells.soft_clip_left, Some((8, b"GG".to_vec())));
        assert_eq!(cells.soft_clip_right, Some((14, b"C".to_vec())));
    }

    #[test]
    fn view_as_pairs_places_mates_on_same_lane() {
        let mut r1 = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 10);
        r1.query_name = "pair1".into();
        let mut r2 = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 30);
        r2.query_name = "pair1".into();
        let mut r3 = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 20);
        r3.query_name = "interloper".into();

        let packed = group_and_pack(&[r1, r2, r3], AlignmentGroupMode::None, true);
        assert_eq!(packed.lanes[0], packed.lanes[1]);
        assert_ne!(packed.lanes[0], packed.lanes[2]);
        assert_eq!(packed.mate_links.len(), 1);
        assert_eq!(packed.mate_links[0].left_end, 13);
        assert_eq!(packed.mate_links[0].right_start, 30);
    }

    #[test]
    fn sort_by_base_places_mismatches_before_matches() {
        let mut r_match = row(vec![CigarOp { kind: CigarKind::Match, len: 3 }], b"ACG", 1);
        r_match.query_name = "match".into();
        let mut r_snp = row(vec![CigarOp { kind: CigarKind::Match, len: 3 }], b"ATG", 1);
        r_snp.query_name = "snp".into();
        let mut rows = vec![r_match, r_snp];
        sort_rows(&mut rows, AlignmentSortMode::Base, 2, b"ACG", 1);
        assert_eq!(rows[0].query_name, "snp");
    }

    #[test]
    fn reservoir_downsampling_caps_per_bin() {
        let rows: Vec<AlignmentRow> = (0..50)
            .map(|i| {
                let mut r = row(vec![CigarOp { kind: CigarKind::Match, len: 4 }], b"ACGT", 10);
                r.query_name = format!("r{i}");
                r
            })
            .collect();
        let (kept, dropped) = downsample_reads(rows, 10, 100, 42);
        assert_eq!(kept.len(), 10);
        assert_eq!(dropped, 40);
    }
}
