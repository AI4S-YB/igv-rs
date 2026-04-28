//! Alignments track: one rect per read, lane-packed using the per-row
//! lane indices already computed by collect_render_inputs / AppState.
//! Mismatches are 1-px ticks; soft-clip handling is left to a follow-up.

use igv_core::render_inputs::BamTrackSnapshot;
use igv_core::source::AlignmentRow;

use crate::layout::{PlotMetrics, Rect};
use crate::options::TrackHeights;
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

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
    let mut shown = 0u32;
    for (row, lane) in track.rows.iter().zip(track.lanes.iter()) {
        if *lane >= max_lanes_visible {
            continue;
        }
        let y = area.y as f64 + (*lane as f64) * h.lane_height as f64 + 1.0;
        let body_h = h.lane_height as f64 - 2.0;
        let fill = if row.is_reverse { theme.read_reverse } else { theme.read_forward };
        draw_read(doc, plot, row, y, body_h, fill);
        if !reference_seq.is_empty() {
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

fn draw_read(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    row: &AlignmentRow,
    y: f64,
    body_h: f64,
    fill: crate::theme::Rgb,
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
    use igv_core::source::bam::CigarKind;
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

fn bases_match(a: u8, b: u8) -> bool {
    a.eq_ignore_ascii_case(&b) || a == b'N' || b == b'N'
}
