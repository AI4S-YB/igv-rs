use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;
use igv_core::source::{
    AnnotationBlock, AnnotationTranscript, BlockKind, Strand,
};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

/// Bottom-anchored partial blocks: index = number of eighths filled (0–8).
/// Used by the gene-density rendering at wide zoom; mirrors the signal
/// widget's encoding so the two tracks share a visual vocabulary.
const LOWER_EIGHTHS: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}',
    '\u{2587}', '\u{2588}',
];

/// At or above this region width, gene labels move from the left of the
/// transcript to a row directly below it — at wide zooms each gene occupies
/// only a few screen columns, leaving no room for a left-anchored label.
const WIDE_LABEL_THRESHOLD: u64 = 10_000;

pub struct AnnotationsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub track_index: usize,
}

impl Widget for AnnotationsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .state
            .annotations
            .get(self.track_index)
            .map(|t| t.display.clone())
            .unwrap_or_else(|| format!("annotation {}", self.track_index));
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.area() == 0 {
            return;
        }
        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        let txs = match self.state.annotation_rows.get(self.track_index) {
            Some(r) => r,
            None => return,
        };
        if txs.is_empty() {
            return;
        }
        if matches!(mode, RenderMode::OverviewOnly) {
            draw_density(buf, inner, region, txs, self.theme);
            return;
        }

        // Annotations are sparse compared to reads, so lane-stacking still
        // works at HeatBar / CoverageDense zooms. Only OverviewOnly fully
        // suppresses the panel.
        let label_below = region.width() >= WIDE_LABEL_THRESHOLD && inner.height >= 2;
        let rows_per_lane: u16 = if label_below { 2 } else { 1 };
        let lane_count = (inner.height / rows_per_lane).max(1) as usize;
        let lanes = stack_transcripts(txs, lane_count);
        for (lane_idx, lane) in lanes.iter().enumerate() {
            let y = inner.y + (lane_idx as u16) * rows_per_lane;
            for tx in lane {
                draw_transcript(buf, inner, y, region, tx, self.theme, label_below);
            }
        }
    }
}

/// At very wide zoom (`OverviewOnly`), individual transcripts collapse to
/// sub-pixel slivers. Render a per-column density histogram instead: each
/// column counts the number of transcript spans overlapping the genomic range
/// it represents, normalized to the maximum across visible columns.
fn draw_density(
    buf: &mut Buffer,
    inner: Rect,
    region: &igv_core::region::Region,
    txs: &[AnnotationTranscript],
    theme: &Theme,
) {
    let cols = inner.width as u32;
    if cols == 0 || inner.height == 0 {
        return;
    }
    let span = region.width().max(1);
    let mut counts = vec![0u32; cols as usize];
    for tx in txs {
        let (s, e) = match tx.span() {
            Some(p) => p,
            None => continue,
        };
        if e < region.start || s > region.end {
            continue;
        }
        let lo = s.max(region.start);
        let hi = e.min(region.end);
        let lo_col = ((lo - region.start) * cols as u64 / span) as u32;
        let hi_col = ((hi - region.start) * cols as u64 / span) as u32;
        let hi_col = hi_col.min(cols.saturating_sub(1));
        for c in lo_col..=hi_col {
            counts[c as usize] = counts[c as usize].saturating_add(1);
        }
    }
    let max = counts.iter().copied().max().unwrap_or(0);
    if max == 0 {
        return;
    }
    let style = theme.get("ANNOTATION_EXON");
    let height = inner.height as f32;
    for (col, &n) in counts.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let frac = (n as f32 / max as f32).clamp(0.0, 1.0) * height;
        let eighths = (frac * 8.0).round() as u32;
        if eighths == 0 {
            continue;
        }
        let full_rows = (eighths / 8) as u16;
        let partial = (eighths % 8) as u8;
        let x = inner.x + col as u16;
        for row in 0..full_rows.min(inner.height) {
            let y = inner.y + inner.height.saturating_sub(1) - row;
            buf[(x, y)].set_char('\u{2588}').set_style(style);
        }
        if partial > 0 && full_rows < inner.height {
            let y = inner.y + inner.height.saturating_sub(1) - full_rows;
            buf[(x, y)]
                .set_char(LOWER_EIGHTHS[partial as usize])
                .set_style(style);
        }
    }
}

#[allow(clippy::needless_lifetimes)]
fn stack_transcripts<'a>(
    txs: &'a [AnnotationTranscript],
    lane_count: usize,
) -> Vec<Vec<&'a AnnotationTranscript>> {
    let mut lanes: Vec<Vec<&AnnotationTranscript>> = (0..lane_count).map(|_| Vec::new()).collect();
    'tx: for tx in txs {
        let (s, _e) = match tx.span() {
            Some(p) => p,
            None => continue,
        };
        for lane in lanes.iter_mut() {
            let last_end = lane
                .last()
                .and_then(|t| t.span())
                .map(|(_, e)| e)
                .unwrap_or(0);
            if last_end + 1 < s {
                lane.push(tx);
                continue 'tx;
            }
        }
    }
    lanes
}

fn draw_transcript(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region: &igv_core::region::Region,
    tx: &AnnotationTranscript,
    theme: &Theme,
    label_below: bool,
) {
    let view_start_0 = region.start - 1;
    let view_width = region.width();
    let intron_style = theme.get("ANNOTATION_INTRON");
    let utr_style = theme.get("ANNOTATION_UTR");
    let exon_style = theme.get("ANNOTATION_EXON");
    let strand_style = theme.get("ANNOTATION_STRAND");
    let name_style = theme.get("ANNOTATION_NAME");

    // 1. introns: a continuous line over the leftmost..rightmost block extent.
    if let Some((s, e)) = tx.span() {
        let mut g = s.saturating_sub(1);
        let g_end = e.saturating_sub(1);
        while g <= g_end {
            if let Some(col) = genomic_to_screen(g, view_start_0, view_width, inner.width as u32) {
                if col < inner.width as u32 {
                    let cell = &mut buf[(inner.x + col as u16, y)];
                    if cell.symbol().chars().next().unwrap_or(' ') == ' ' {
                        cell.set_char('─').set_style(intron_style);
                    }
                }
            }
            g += 1;
        }
    }

    // 2/3. UTRs first, then CDS / Exon / BedSegment so they overwrite.
    let mut blocks: Vec<&AnnotationBlock> = tx.blocks.iter().collect();
    blocks.sort_by_key(|b| match b.kind {
        BlockKind::Utr5 | BlockKind::Utr3 => 0,
        _ => 1,
    });
    for blk in blocks {
        let (glyph, style) = match blk.kind {
            BlockKind::Utr5 | BlockKind::Utr3 => ('▯', utr_style),
            BlockKind::Exon | BlockKind::Cds | BlockKind::BedSegment => ('▮', exon_style),
        };
        let g_start = blk.start.saturating_sub(1);
        let g_end = blk.end.saturating_sub(1);
        let mut g = g_start;
        while g <= g_end {
            if let Some(col) = genomic_to_screen(g, view_start_0, view_width, inner.width as u32) {
                if col < inner.width as u32 {
                    buf[(inner.x + col as u16, y)].set_char(glyph).set_style(style);
                }
            }
            g += 1;
        }
    }

    // 4. strand glyph at rightmost column of the transcript.
    if let Some((_, e)) = tx.span() {
        let g0 = e.saturating_sub(1);
        if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            let glyph = match tx.strand {
                Strand::Forward => '>',
                Strand::Reverse => '<',
                Strand::Unknown => return,
            };
            if col < inner.width as u32 {
                buf[(inner.x + col as u16, y)].set_char(glyph).set_style(strand_style);
            }
        }
    }

    // 5. name label.
    if !tx.name.is_empty() {
        if label_below {
            draw_name_below(buf, inner, y, region, tx, name_style);
        } else {
            draw_name_left(buf, inner, y, region, tx, name_style);
        }
    }
}

fn draw_name_left(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region: &igv_core::region::Region,
    tx: &AnnotationTranscript,
    name_style: ratatui::style::Style,
) {
    let view_start_0 = region.start - 1;
    let view_width = region.width();
    let (s, _) = match tx.span() {
        Some(p) => p,
        None => return,
    };
    let g0 = s.saturating_sub(1);
    let col = match genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
        Some(c) => c,
        None => return,
    };
    let label = format!("{} ", tx.name);
    let needed = label.len() as u32;
    if col < needed {
        return;
    }
    let start_col = col - needed;
    for (i, ch) in label.chars().enumerate() {
        if start_col as u16 + i as u16 >= inner.width {
            break;
        }
        buf[(inner.x + start_col as u16 + i as u16, y)]
            .set_char(ch)
            .set_style(name_style);
    }
}

fn draw_name_below(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region: &igv_core::region::Region,
    tx: &AnnotationTranscript,
    name_style: ratatui::style::Style,
) {
    let label_y = y + 1;
    if label_y >= inner.y + inner.height {
        return;
    }
    let view_start_0 = region.start - 1;
    let view_width = region.width();
    let (s, e) = match tx.span() {
        Some(p) => p,
        None => return,
    };
    // Anchor at the gene's leftmost visible column. Genes that start before
    // the view but extend into it anchor at column 0.
    let g0_start = s.saturating_sub(1);
    let g0_end = e.saturating_sub(1);
    if g0_end < view_start_0 || g0_start >= view_start_0 + view_width {
        return;
    }
    let start_col =
        genomic_to_screen(g0_start, view_start_0, view_width, inner.width as u32).unwrap_or(0);
    for (i, ch) in tx.name.chars().enumerate() {
        let col = start_col as u16 + i as u16;
        if col >= inner.width {
            break;
        }
        buf[(inner.x + col, label_y)]
            .set_char(ch)
            .set_style(name_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use igv_core::region::Region;
    use igv_core::source::TranscriptKind;

    fn make_tx(s: u64, e: u64) -> AnnotationTranscript {
        AnnotationTranscript {
            name: "g".into(),
            id: "t".into(),
            gene_id: None,
            strand: Strand::Forward,
            blocks: vec![AnnotationBlock { start: s, end: e, kind: BlockKind::Cds }],
            kind: TranscriptKind::Mrna,
        }
    }

    #[test]
    fn density_renders_block_chars_in_overview() {
        // Chromosome-scale view, two clustered genes near the start.
        let region = Region::new("chr1", 1, 100_000_000).unwrap();
        let txs = vec![make_tx(1_000_000, 1_500_000), make_tx(1_200_000, 1_600_000)];
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 4);
        let mut buf = Buffer::empty(area);
        draw_density(&mut buf, area, &region, &txs, &theme);
        // Cluster sits near column 0–2 (1.6Mb / 100Mb * 80 ≈ 1.3); expect at
        // least one full or partial block char in the leftmost columns.
        let mut found = false;
        for x in 0..3u16 {
            for y in 0..area.height {
                let ch = buf[(x, y)].symbol().chars().next().unwrap_or(' ');
                if matches!(ch, '\u{2581}'..='\u{2588}') {
                    found = true;
                }
            }
        }
        assert!(found, "expected density blocks near chr1 start");
    }

    #[test]
    fn density_skips_when_no_overlap() {
        let region = Region::new("chr1", 1, 100_000_000).unwrap();
        let txs = vec![make_tx(99_000_000, 99_500_000)]; // far right, but should still draw
        let theme = Theme::dark();
        let area = Rect::new(0, 0, 80, 4);
        let mut buf = Buffer::empty(area);
        draw_density(&mut buf, area, &region, &txs, &theme);
        // Right side ≈ column 79
        let mut found = false;
        for x in 76..80u16 {
            for y in 0..area.height {
                let ch = buf[(x, y)].symbol().chars().next().unwrap_or(' ');
                if matches!(ch, '\u{2581}'..='\u{2588}') {
                    found = true;
                }
            }
        }
        assert!(found, "expected density block near chr1 end");
    }
}
