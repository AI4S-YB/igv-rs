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
        if matches!(mode, RenderMode::OverviewOnly) {
            return;
        }

        let txs = match self.state.annotation_rows.get(self.track_index) {
            Some(r) => r,
            None => return,
        };
        if txs.is_empty() {
            return;
        }

        if matches!(mode, RenderMode::HeatBar) {
            draw_heatbar(buf, inner, region, txs, self.theme);
            return;
        }

        let lanes = stack_transcripts(txs, inner.height as usize);
        for (lane_idx, lane) in lanes.iter().enumerate() {
            let y = inner.y + lane_idx as u16;
            for tx in lane {
                draw_transcript(buf, inner, y, region, tx, self.theme);
            }
        }
    }
}

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

fn draw_heatbar(
    buf: &mut Buffer,
    inner: Rect,
    region: &igv_core::region::Region,
    txs: &[AnnotationTranscript],
    theme: &Theme,
) {
    let style = theme.get("ANNOTATION_EXON");
    let view_start_0 = region.start - 1;
    let view_width = region.width();
    for tx in txs {
        for blk in &tx.blocks {
            let g0_start = blk.start.saturating_sub(1);
            let g0_end = blk.end.saturating_sub(1);
            let mut g = g0_start;
            while g <= g0_end {
                if let Some(col) = genomic_to_screen(g, view_start_0, view_width, inner.width as u32) {
                    if col < inner.width as u32 {
                        let cell = buf.get_mut(inner.x + col as u16, inner.y);
                        cell.set_char('▮').set_style(style);
                    }
                }
                g += 1;
            }
        }
    }
}

fn draw_transcript(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region: &igv_core::region::Region,
    tx: &AnnotationTranscript,
    theme: &Theme,
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
                    let cell = buf.get_mut(inner.x + col as u16, y);
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
                    buf.get_mut(inner.x + col as u16, y).set_char(glyph).set_style(style);
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
                buf.get_mut(inner.x + col as u16, y).set_char(glyph).set_style(strand_style);
            }
        }
    }

    // 5. name label, if it fits to the left of the leftmost block.
    if !tx.name.is_empty() {
        if let Some((s, _)) = tx.span() {
            let g0 = s.saturating_sub(1);
            if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
                let label = format!("{} ", tx.name);
                let needed = label.len() as u32;
                if col >= needed {
                    let start_col = col - needed;
                    for (i, ch) in label.chars().enumerate() {
                        if start_col as u16 + i as u16 >= inner.width {
                            break;
                        }
                        buf.get_mut(inner.x + start_col as u16 + i as u16, y)
                            .set_char(ch)
                            .set_style(name_style);
                    }
                }
            }
        }
    }
}
