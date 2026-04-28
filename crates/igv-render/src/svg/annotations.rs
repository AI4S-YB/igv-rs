//! Annotations track: one row per loaded GFF/BED track.
//!
//! Within the row each visible transcript is laid out one per "lane"
//! (simple greedy non-overlap). For v1 we render at most `max_lanes` per
//! track and append a "+N more" label if truncated.

use igv_core::render_inputs::AnnotationTrackSnapshot;
use igv_core::source::{AnnotationTranscript, BlockKind, Strand};

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

const MAX_LANES_PER_TRACK: usize = 4;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    track: &AnnotationTrackSnapshot,
    theme: &GraphicalTheme,
) {
    let label_y = (area.y + area.h / 2) as f64 + (theme.font_px_normal as f64 / 3.0);
    doc.text(
        (plot.margin_left - 6) as f64,
        label_y,
        &track.display,
        theme.muted,
        theme.font_px_small,
        TextAnchor::End,
    );

    if track.transcripts.is_empty() {
        return;
    }

    let lanes = assign_lanes(&track.transcripts);
    let lane_count = (lanes
        .iter()
        .copied()
        .max()
        .map(|m| m as usize + 1)
        .unwrap_or(0))
    .min(MAX_LANES_PER_TRACK);
    let usable = (lane_count.max(1)) as f64;
    let lane_h = (area.h as f64 - 4.0) / usable;
    let exon_h = lane_h * 0.6;
    let intron_y_offset = lane_h / 2.0;

    let mut drawn = 0usize;
    for (lane, tx) in lanes.iter().zip(track.transcripts.iter()) {
        let lane = *lane as usize;
        if lane >= MAX_LANES_PER_TRACK {
            continue;
        }
        let lane_top = area.y as f64 + 2.0 + (lane as f64) * lane_h;
        draw_transcript(doc, plot, tx, lane_top, lane_h, exon_h, intron_y_offset, theme);
        drawn += 1;
    }

    let truncated = track.transcripts.len().saturating_sub(drawn);
    if truncated > 0 {
        doc.text(
            (plot.margin_left + 6) as f64,
            (area.y + area.h - 4) as f64,
            &format!("+{} more", truncated),
            theme.muted,
            theme.font_px_small,
            TextAnchor::Start,
        );
    }
}

fn draw_transcript(
    doc: &mut SvgDoc,
    plot: &PlotMetrics,
    tx: &AnnotationTranscript,
    lane_top: f64,
    lane_h: f64,
    exon_h: f64,
    intron_y_offset: f64,
    theme: &GraphicalTheme,
) {
    let Some((s, e)) = tx.span() else { return };
    let x0 = plot.bp_to_px(s);
    let x1 = plot.bp_to_px(e);

    let intron_y = lane_top + intron_y_offset;
    doc.line(x0, intron_y, x1, intron_y, theme.transcript_intron, 1.0);
    draw_strand_chevrons(doc, x0, x1, intron_y, tx.strand, theme);

    let exon_y = lane_top + (lane_h - exon_h) / 2.0;
    for block in &tx.blocks {
        let bx0 = plot.bp_to_px(block.start);
        let bx1 = plot.bp_to_px(block.end);
        let w = (bx1 - bx0).max(1.0);
        let h = match block.kind {
            BlockKind::Utr5 | BlockKind::Utr3 => exon_h * 0.5,
            _ => exon_h,
        };
        let y = exon_y + (exon_h - h) / 2.0;
        doc.rect(bx0, y, w, h, theme.transcript_exon);
    }

    let label_x = (x0 + x1) / 2.0;
    doc.text(
        label_x,
        lane_top + theme.font_px_small as f64,
        &tx.name,
        theme.transcript_label,
        theme.font_px_small,
        TextAnchor::Middle,
    );
}

fn draw_strand_chevrons(
    doc: &mut SvgDoc,
    x0: f64,
    x1: f64,
    y: f64,
    strand: Strand,
    theme: &GraphicalTheme,
) {
    let direction: i32 = match strand {
        Strand::Forward => 1,
        Strand::Reverse => -1,
        Strand::Unknown => return,
    };
    let span = x1 - x0;
    if span < 12.0 {
        return;
    }
    let step = 30.0;
    let mut x = x0 + step;
    while x < x1 - 4.0 {
        let dx = 4.0 * direction as f64;
        let dy = 3.0;
        doc.line(x - dx, y - dy, x, y, theme.transcript_intron, 1.0);
        doc.line(x - dx, y + dy, x, y, theme.transcript_intron, 1.0);
        x += step;
    }
}

fn assign_lanes(transcripts: &[AnnotationTranscript]) -> Vec<u32> {
    let mut lane_ends: Vec<u64> = Vec::new();
    let mut lanes = Vec::with_capacity(transcripts.len());
    for tx in transcripts {
        let Some((s, e)) = tx.span() else {
            lanes.push(0);
            continue;
        };
        let mut placed = None;
        for (i, end) in lane_ends.iter_mut().enumerate() {
            if s > *end {
                *end = e;
                placed = Some(i as u32);
                break;
            }
        }
        let lane = placed.unwrap_or_else(|| {
            lane_ends.push(e);
            (lane_ends.len() - 1) as u32
        });
        lanes.push(lane);
    }
    lanes
}
