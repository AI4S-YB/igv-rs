use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::alignment::{expand, BaseGlyph, ReadCells};
use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;
use igv_core::source::bam::AlignmentRow;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct AlignmentsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub track_index: usize,
}

impl Widget for AlignmentsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .state
            .bams
            .get(self.track_index)
            .map(|t| t.display.clone())
            .unwrap_or_else(|| format!("bam {}", self.track_index));
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if !matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            return;
        }
        if inner.area() == 0 {
            return;
        }

        let rows = match self.state.bam_rows.get(self.track_index) {
            Some(r) => r,
            None => return,
        };

        // Stack reads greedily to avoid horizontal overlap.
        let lanes = stack_reads(rows, inner.height as usize);
        let view_start_0 = region.start - 1;
        let view_width = region.width();

        for (lane_idx, lane) in lanes.iter().enumerate() {
            let y = inner.y + lane_idx as u16;
            for row in lane {
                let cells = expand(row, &self.state.reference_seq, region.start);
                draw_read(
                    buf, inner, y, region.start, view_start_0, view_width, &cells, row,
                    self.theme, mode,
                );
            }
        }
    }
}

fn stack_reads<'a>(rows: &'a [AlignmentRow], lane_count: usize) -> Vec<Vec<&'a AlignmentRow>> {
    let mut lanes: Vec<Vec<&AlignmentRow>> = (0..lane_count).map(|_| Vec::new()).collect();
    'rows: for row in rows {
        for lane in lanes.iter_mut() {
            if lane
                .last()
                .map(|prev| prev.ref_end + 1 < row.ref_start)
                .unwrap_or(true)
            {
                lane.push(row);
                continue 'rows;
            }
        }
        // No room: drop. (Future: scrollable.)
    }
    lanes
}

fn draw_read(
    buf: &mut Buffer,
    inner: Rect,
    y: u16,
    region_start_1: u64,
    view_start_0: u64,
    view_width: u64,
    cells: &ReadCells,
    row: &AlignmentRow,
    theme: &Theme,
    _mode: RenderMode,
) {
    let mismatch_style = theme.get("MISMATCH");
    let deletion_style = theme.get("DELETION");
    let insertion_style = theme.get("INSERTION");
    let match_style = if row.is_reverse {
        theme.get("MATCH_REV")
    } else {
        theme.get("MATCH_FWD")
    };

    for (i, glyph) in cells.cells.iter().enumerate() {
        let ref_pos_1 = cells.ref_start + i as u64;
        let g0 = ref_pos_1 - 1;
        if g0 < view_start_0 {
            continue;
        }
        let col = match genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            Some(c) => c,
            None => continue,
        };
        let (ch, style) = match glyph {
            BaseGlyph::Match => ('.', match_style),
            BaseGlyph::Mismatch(b) => (*b as char, mismatch_style),
            BaseGlyph::Deletion => ('*', deletion_style),
            BaseGlyph::SoftClip(b) => (*b as char, theme.get("BORDER")),
        };
        buf.get_mut(inner.x + col as u16, y).set_char(ch).set_style(style);
    }

    for (ins_ref_pos_1, _bases) in &cells.insertions {
        let g0 = ins_ref_pos_1.saturating_sub(1);
        if g0 < view_start_0 {
            continue;
        }
        if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            buf.get_mut(inner.x + col as u16, y)
                .set_char('+')
                .set_style(insertion_style);
        }
    }

    let _ = region_start_1;
}
