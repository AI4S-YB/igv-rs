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
        let base_title = self
            .state
            .bams
            .get(self.track_index)
            .map(|t| t.display.clone())
            .unwrap_or_else(|| format!("bam {}", self.track_index));

        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        let renderable = matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads);

        let rows = self.state.bam_rows.get(self.track_index);
        let lanes = self.state.bam_lanes.get(self.track_index);
        let total_lanes = self
            .state
            .bam_total_lanes
            .get(self.track_index)
            .copied()
            .unwrap_or(0);

        let visible_lanes_estimate = area.height.saturating_sub(2).max(1);
        let scroll = self.state.bam_scroll;
        let title = if total_lanes > visible_lanes_estimate {
            format!("{base_title} [{}..{}/{}]", scroll, scroll + visible_lanes_estimate, total_lanes)
        } else {
            base_title
        };

        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        if !renderable || inner.area() == 0 {
            return;
        }
        let (rows, lanes) = match (rows, lanes) {
            (Some(r), Some(l)) if !r.is_empty() && r.len() == l.len() => (r, l),
            _ => return,
        };

        let view_start_0 = region.start - 1;
        let view_width = region.width();
        let visible = inner.height as u32;

        for (row, &lane) in rows.iter().zip(lanes.iter()) {
            if lane < scroll as u32 {
                continue;
            }
            let display = lane - scroll as u32;
            if display >= visible {
                continue;
            }
            let y = inner.y + display as u16;
            let cells = expand(row, &self.state.reference_seq, region.start);
            draw_read(
                buf, inner, y, region.start, view_start_0, view_width, &cells, row,
                self.theme, mode,
            );
        }

        // Scroll affordance: arrows at the right edge when more lanes exist.
        let style = self.theme.get("BORDER");
        if scroll > 0 {
            buf[(inner.x + inner.width.saturating_sub(1), inner.y)]
                .set_char('▲')
                .set_style(style);
        }
        let last_visible_lane = scroll as u32 + visible;
        if last_visible_lane < total_lanes as u32 {
            let y = inner.y + inner.height.saturating_sub(1);
            buf[(inner.x + inner.width.saturating_sub(1), y)]
                .set_char('▼')
                .set_style(style);
        }
    }
}

#[allow(clippy::too_many_arguments)]
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
        buf[(inner.x + col as u16, y)].set_char(ch).set_style(style);
    }

    for (ins_ref_pos_1, _bases) in &cells.insertions {
        let g0 = ins_ref_pos_1.saturating_sub(1);
        if g0 < view_start_0 {
            continue;
        }
        if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
            buf[(inner.x + col as u16, y)]
                .set_char('+')
                .set_style(insertion_style);
        }
    }

    let _ = region_start_1;
}
