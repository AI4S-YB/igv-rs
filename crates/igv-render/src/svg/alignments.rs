//! Alignments track: lane-packed read polygons with mismatches, deletions,
//! insertions, optional soft-clip overhangs, mate pair links, group bands,
//! bisulfite CpG calls, and `MM`/`ML` 5mC/6mA modification ticks.

use igv_core::alignment::{
    bisulfite_call, mod_at_query_index, pair_orientation, AlignmentColorMode, AlignmentRenderOpts,
    BisulfiteCall, PairOrientation,
};
use igv_core::render_inputs::BamTrackSnapshot;
use igv_core::source::bam::CigarKind;
use igv_core::source::AlignmentRow;

use crate::layout::{PlotMetrics, Rect};
use crate::options::TrackHeights;
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::{GraphicalTheme, Rgb};

#[allow(clippy::too_many_arguments)]
pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &BamTrackSnapshot,
    h: &TrackHeights,
    reference_seq: &[u8],
    region_start: u64,
    theme: &GraphicalTheme,
) {
    draw_with_opts(
        doc,
        area,
        plot,
        track,
        h,
        reference_seq,
        region_start,
        &AlignmentRenderOpts::default(),
        theme,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_with_opts(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &BamTrackSnapshot,
    h: &TrackHeights,
    reference_seq: &[u8],
    region_start: u64,
    opts: &AlignmentRenderOpts,
    theme: &GraphicalTheme,
) {
    let label_y = (area.y + h.lane_height / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    let max_lanes_visible = (area.h / h.lane_height).max(1);
    let has_named_bands = track.bands.iter().any(|b| b.label.is_some());

    if has_named_bands {
        for band in &track.bands {
            if band.lane_offset >= max_lanes_visible {
                continue;
            }
            if let Some(label) = &band.label {
                let by = area.y as f64 + (band.lane_offset as f64) * h.lane_height as f64;
                doc.line(
                    plot.plot_x0 as f64,
                    by,
                    plot.plot_x1 as f64,
                    by,
                    Rgb(0xe2, 0xe8, 0xf0),
                    0.8,
                );
                doc.text(
                    (plot.margin_left + 4) as f64,
                    by + h.lane_height as f64 - 2.0,
                    label,
                    theme.muted,
                    8,
                    TextAnchor::Start,
                );
            }
        }
    }

    if opts.view_as_pairs {
        for link in &track.mate_links {
            if link.lane >= max_lanes_visible {
                continue;
            }
            let cy = area.y as f64
                + (link.lane as f64) * h.lane_height as f64
                + (h.lane_height as f64) / 2.0;
            let lx = plot.bp_to_px(link.left_end + 1);
            let rx = plot.bp_to_px(link.right_start);
            if rx > lx {
                doc.line(lx, cy, rx, cy, Rgb(0x94, 0xa3, 0xb8), 1.0);
            }
        }
    }

    let use_enhanced = opts.color_mode != AlignmentColorMode::Strand
        || opts.view_as_pairs
        || opts.show_soft_clips
        || track.rows.iter().any(|r| !r.base_mods.is_empty());

    let mut shown = 0u32;
    for (row, lane) in track.rows.iter().zip(track.lanes.iter()) {
        if *lane >= max_lanes_visible {
            continue;
        }
        let y = area.y as f64 + (*lane as f64) * h.lane_height as f64 + 1.0;
        let body_h = h.lane_height as f64 - 2.0;

        if opts.show_soft_clips {
            draw_soft_clips(doc, plot, row, y, body_h);
        }

        let fill = read_color(row, opts, theme);
        draw_read(doc, plot, row, y, body_h, fill);

        if use_enhanced {
            draw_indels_and_mods(doc, plot, row, y, body_h, reference_seq, region_start, opts, theme);
        } else if !reference_seq.is_empty() {
            draw_mismatches(doc, plot, row, y, body_h, reference_seq, region_start, theme);
        }
        shown += 1;
    }
    let truncated = track.rows.len().saturating_sub(shown as usize);
    if truncated > 0 {
        doc.text(
            (plot.margin_left + 6) as f64,
            (area.y + area.h - 4) as f64,
            &format!("+{} reads not shown", truncated),
            theme.muted,
            theme.font_px_small,
            TextAnchor::Start,
        );
    }
}

fn read_color(row: &AlignmentRow, opts: &AlignmentRenderOpts, theme: &GraphicalTheme) -> Rgb {
    match opts.color_mode {
        AlignmentColorMode::Strand => {
            if row.is_reverse {
                theme.read_reverse
            } else {
                theme.read_forward
            }
        }
        AlignmentColorMode::MappingQuality => {
            let t = (row.mapq.min(60) as f32) / 60.0;
            let shade = (230.0 - t * 130.0).round() as u8;
            Rgb(shade, shade + 10, 235)
        }
        AlignmentColorMode::InsertSize => {
            let isize = row.template_len.abs();
            if isize == 0 {
                Rgb(0xcb, 0xd5, 0xe1)
            } else if isize < opts.insert_min {
                Rgb(0x93, 0xc5, 0xfd)
            } else if isize > opts.insert_max {
                Rgb(0xfc, 0xa5, 0xa5)
            } else {
                Rgb(0xcb, 0xd5, 0xe1)
            }
        }
        AlignmentColorMode::PairOrientation => match pair_orientation(row) {
            PairOrientation::Lr => Rgb(0xcb, 0xd5, 0xe1),
            PairOrientation::Rl => Rgb(0x86, 0xef, 0xac),
            PairOrientation::Ll => Rgb(0x5e, 0xea, 0xd4),
            PairOrientation::Rr => Rgb(0xfd, 0xba, 0x74),
            PairOrientation::Other => Rgb(0xd8, 0xb4, 0xfe),
        },
        AlignmentColorMode::MateChrom => match &row.mate_chrom {
            Some(mc) if mc != "*" && mc != "=" => hash_palette(mc),
            _ => Rgb(0xcb, 0xd5, 0xe1),
        },
        AlignmentColorMode::Tag => match &row.tag {
            Some((_, v)) => hash_palette(v),
            None => Rgb(0xd4, 0xd4, 0xd8),
        },
        AlignmentColorMode::BaseModification | AlignmentColorMode::Bisulfite => {
            Rgb(0xd4, 0xd4, 0xd8)
        }
    }
}

fn hash_palette(key: &str) -> Rgb {
    const PALETTE: [Rgb; 6] = [
        Rgb(0x93, 0xc5, 0xfd),
        Rgb(0x86, 0xef, 0xac),
        Rgb(0xfd, 0xba, 0x74),
        Rgb(0xd8, 0xb4, 0xfe),
        Rgb(0xf9, 0xa8, 0xd4),
        Rgb(0xfd, 0xe0, 0x47),
    ];
    let h: usize = key
        .bytes()
        .fold(5381usize, |acc, b| acc.wrapping_mul(33) ^ b as usize);
    PALETTE[h % PALETTE.len()]
}

fn draw_soft_clips(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
) {
    let left_sc = match row.cigar.first() {
        Some(op) if op.kind == CigarKind::SoftClip => op.len as u64,
        _ => 0,
    };
    let right_sc = match row.cigar.last() {
        Some(op) if op.kind == CigarKind::SoftClip && row.cigar.len() > 1 => op.len as u64,
        _ => 0,
    };
    if left_sc > 0 && row.ref_start > 1 {
        let s = row.ref_start.saturating_sub(left_sc);
        let x0 = plot.bp_to_px(s);
        let x1 = plot.bp_to_px(row.ref_start);
        if x1 > x0 {
            doc.rect(x0, y + 2.0, (x1 - x0).max(2.0), body_h - 4.0, Rgb(0xfd, 0xe0, 0x47));
        }
    }
    if right_sc > 0 {
        let x0 = plot.bp_to_px(row.ref_end + 1);
        let x1 = plot.bp_to_px(row.ref_end + 1 + right_sc);
        if x1 > x0 {
            doc.rect(x0, y + 2.0, (x1 - x0).max(2.0), body_h - 4.0, Rgb(0xfd, 0xe0, 0x47));
        }
    }
}

fn draw_read(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
    fill: Rgb,
) {
    let x0 = plot.bp_to_px(row.ref_start);
    let x1 = plot.bp_to_px(row.ref_end + 1);
    let tip = (body_h / 2.0).min(4.0);
    if x1 - x0 < tip * 2.0 {
        doc.rect(x0, y, (x1 - x0).max(1.0), body_h, fill);
        return;
    }
    if row.is_reverse {
        doc.polygon(
            &[
                (x0 + tip, y),
                (x1, y),
                (x1, y + body_h),
                (x0 + tip, y + body_h),
                (x0, y + body_h / 2.0),
            ],
            fill,
        );
    } else {
        doc.polygon(
            &[
                (x0, y),
                (x1 - tip, y),
                (x1, y + body_h / 2.0),
                (x1 - tip, y + body_h),
                (x0, y + body_h),
            ],
            fill,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_mismatches(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
    reference_seq: &[u8],
    region_start: u64,
    theme: &GraphicalTheme,
) {
    let region_end = region_start + reference_seq.len() as u64;
    let mut ref_pos: u64 = row.ref_start;
    let mut q_pos: usize = 0;
    'cigar: for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                for _ in 0..op.len {
                    if ref_pos >= region_end {
                        break 'cigar;
                    }
                    let qbase = row.query_sequence.get(q_pos).copied().unwrap_or(b'N');
                    let ref_idx = (ref_pos as i64 - region_start as i64) as isize;
                    if ref_idx >= 0 && (ref_idx as usize) < reference_seq.len() {
                        let rbase = reference_seq[ref_idx as usize];
                        if !bases_match(qbase, rbase) {
                            let x = plot.bp_to_px(ref_pos);
                            doc.rect(x, y, 1.0, body_h, theme.mismatch_color(qbase));
                        }
                    }
                    ref_pos += 1;
                    q_pos += 1;
                }
            }
            CigarKind::Insertion | CigarKind::SoftClip => {
                q_pos += op.len as usize;
            }
            CigarKind::Deletion | CigarKind::Skip => {
                ref_pos += op.len as u64;
                if ref_pos >= region_end {
                    break 'cigar;
                }
            }
            CigarKind::HardClip | CigarKind::Padding => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_indels_and_mods(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
    reference_seq: &[u8],
    region_start: u64,
    opts: &AlignmentRenderOpts,
    theme: &GraphicalTheme,
) {
    let mut ref_pos = row.ref_start;
    let mut q_pos = 0usize;
    for op in &row.cigar {
        match op.kind {
            CigarKind::Match | CigarKind::SeqMatch | CigarKind::SeqMismatch => {
                for _ in 0..op.len {
                    let qbase = row.query_sequence.get(q_pos).copied().unwrap_or(b'N');
                    let x = plot.bp_to_px(ref_pos);

                    if !row.base_mods.is_empty() {
                        if let Some(m) = mod_at_query_index(&row.base_mods, q_pos) {
                            let color = if m.prob >= 0.5 {
                                if m.code == 'a' {
                                    Rgb(0x93, 0x33, 0xea)
                                } else if m.code == 'h' {
                                    Rgb(0xf9, 0x73, 0x16)
                                } else {
                                    Rgb(0xdc, 0x26, 0x26)
                                }
                            } else {
                                Rgb(0x25, 0x63, 0xeb)
                            };
                            doc.rect(x, y, 1.5, body_h, color);
                        }
                    }

                    if !reference_seq.is_empty() {
                        let ref_idx = ref_pos as i64 - region_start as i64;
                        if ref_idx >= 0 && (ref_idx as usize) < reference_seq.len() {
                            let ridx = ref_idx as usize;
                            let rbase = reference_seq[ridx];
                            if opts.color_mode == AlignmentColorMode::Bisulfite {
                                let next_r = reference_seq.get(ridx + 1).copied();
                                let prev_r = ridx.checked_sub(1).and_then(|k| reference_seq.get(k).copied());
                                if let Some(call) = bisulfite_call(row.is_reverse, qbase, rbase, next_r, prev_r) {
                                    let c = match call {
                                        BisulfiteCall::Methylated => Rgb(0xdc, 0x26, 0x26),
                                        BisulfiteCall::Unmethylated => Rgb(0x25, 0x63, 0xeb),
                                    };
                                    doc.rect(x, y, 1.5, body_h, c);
                                }
                            } else if !bases_match(qbase, rbase) {
                                doc.rect(x, y, 1.0, body_h, theme.mismatch_color(qbase));
                            }
                        }
                    }

                    ref_pos += 1;
                    q_pos += 1;
                }
            }
            CigarKind::Insertion => {
                let x = plot.bp_to_px(ref_pos);
                doc.rect(x - 0.5, y, 1.5, body_h, Rgb(0x7e, 0x22, 0xce));
                q_pos += op.len as usize;
            }
            CigarKind::SoftClip => {
                q_pos += op.len as usize;
            }
            CigarKind::Deletion | CigarKind::Skip => {
                let x0 = plot.bp_to_px(ref_pos);
                let x1 = plot.bp_to_px(ref_pos + op.len as u64);
                doc.rect(x0, y + body_h / 2.0 - 0.7, (x1 - x0).max(1.0), 1.4, Rgb(0x11, 0x18, 0x27));
                ref_pos += op.len as u64;
            }
            CigarKind::HardClip | CigarKind::Padding => {}
        }
    }
}

fn bases_match(a: u8, b: u8) -> bool {
    a.eq_ignore_ascii_case(&b) || a == b'N' || b == b'N'
}
