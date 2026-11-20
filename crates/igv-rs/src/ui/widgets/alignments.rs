use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::alignment::{
    bisulfite_call, expand_with_opts, mod_at_query_index, pair_orientation, AlignmentColorMode,
    AlignmentGroupMode, AlignmentSortMode, BaseGlyph, BisulfiteCall, PairOrientation, ReadCells,
};
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
        let bands = self.state.bam_bands.get(self.track_index);
        let mate_links = self.state.bam_mate_links.get(self.track_index);
        let downsampled = self
            .state
            .bam_downsampled
            .get(self.track_index)
            .copied()
            .unwrap_or(0);
        let total_lanes = self
            .state
            .bam_total_lanes
            .get(self.track_index)
            .copied()
            .unwrap_or(0);

        let visible_lanes_estimate = area.height.saturating_sub(2).max(1);
        let scroll = self.state.bam_scroll;

        let mut tags = Vec::new();
        if self.state.alignment_opts.color_mode != AlignmentColorMode::Strand {
            tags.push(format!("c:{}", self.state.alignment_opts.color_mode.name()));
        }
        if self.state.alignment_opts.sort_mode != AlignmentSortMode::None {
            tags.push(format!("s:{}", self.state.alignment_opts.sort_mode.name()));
        }
        if self.state.alignment_opts.group_mode != AlignmentGroupMode::None {
            tags.push(format!("g:{}", self.state.alignment_opts.group_mode.name()));
        }
        if self.state.alignment_opts.view_as_pairs {
            tags.push("pairs".into());
        }
        if downsampled > 0 {
            tags.push(format!("-{downsampled} reads"));
        }

        let suffix = if tags.is_empty() {
            String::new()
        } else {
            format!(" ({})", tags.join(", "))
        };
        let title = if total_lanes > visible_lanes_estimate {
            format!(
                "{base_title}{suffix} [{}..{}/{}]",
                scroll,
                scroll + visible_lanes_estimate,
                total_lanes
            )
        } else {
            format!("{base_title}{suffix}")
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

        // Draw group band headers when grouped
        if let Some(bands) = bands {
            for band in bands {
                let Some(label) = &band.label else {
                    continue;
                };
                if band.lane_offset < scroll as u32 {
                    continue;
                }
                let disp = band.lane_offset - scroll as u32;
                if disp >= visible {
                    continue;
                }
                let y = inner.y + disp as u16;
                let hdr = format!("── {label} ──");
                let st = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                for (i, ch) in hdr.chars().take(inner.width as usize).enumerate() {
                    buf[(inner.x + i as u16, y)].set_char(ch).set_style(st);
                }
            }
        }

        // Draw mate connector dashes in View-as-Pairs mode
        if self.state.alignment_opts.view_as_pairs {
            if let Some(links) = mate_links {
                let link_style = self.theme.get("BORDER");
                for link in links {
                    if link.lane < scroll as u32 {
                        continue;
                    }
                    let disp = link.lane - scroll as u32;
                    if disp >= visible {
                        continue;
                    }
                    let y = inner.y + disp as u16;
                    let c0 = genomic_to_screen(link.left_end, view_start_0, view_width, inner.width as u32)
                        .unwrap_or(0);
                    let c1 = genomic_to_screen(
                        link.right_start.saturating_sub(2),
                        view_start_0,
                        view_width,
                        inner.width as u32,
                    )
                    .unwrap_or(inner.width.saturating_sub(1) as u32);
                    for col in c0 + 1..c1 {
                        if col < inner.width as u32 {
                            buf[(inner.x + col as u16, y)]
                                .set_char('─')
                                .set_style(link_style);
                        }
                    }
                }
            }
        }

        for (row, &lane) in rows.iter().zip(lanes.iter()) {
            if lane < scroll as u32 {
                continue;
            }
            let display = lane - scroll as u32;
            if display >= visible {
                continue;
            }
            let y = inner.y + display as u16;
            let cells = expand_with_opts(
                row,
                &self.state.reference_seq,
                region.start,
                self.state.alignment_opts.show_soft_clips,
            );
            draw_read(
                buf,
                inner,
                y,
                region.start,
                view_start_0,
                view_width,
                &cells,
                row,
                &self.state.reference_seq,
                &self.state.alignment_opts,
                self.theme,
            );
        }

        let style = self.theme.get("BORDER");
        if scroll > 0 {
            buf[(inner.x + inner.width.saturating_sub(1), inner.y)]
                .set_char('▲')
                .set_style(style);
        }
        if scroll as u32 + visible < total_lanes as u32 {
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
    reference_seq: &[u8],
    opts: &igv_core::alignment::AlignmentRenderOpts,
    theme: &Theme,
) {
    let mismatch_style = theme.get("MISMATCH");
    let deletion_style = theme.get("DELETION");
    let insertion_style = theme.get("INSERTION");
    let base_match_style = read_base_style(row, opts, theme);

    for (i, glyph) in cells.cells.iter().enumerate() {
        let ref_pos_1 = cells.ref_start + i as u64;
        let g0 = ref_pos_1 - 1;
        if g0 < view_start_0 {
            continue;
        }
        let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) else {
            continue;
        };

        let q_idx = cells.cell_q_idx.get(i).and_then(|x| *x);

        // 5mC / 6mA overlay when BaseModification active or MM/ML present in Mod mode
        if opts.color_mode == AlignmentColorMode::BaseModification {
            if let Some(qi) = q_idx {
                if let Some(m) = mod_at_query_index(&row.base_mods, qi) {
                    let (ch, color) = if m.prob >= 0.5 {
                        (
                            m.code,
                            match m.code {
                                'a' => Color::Magenta,
                                'h' => Color::Yellow,
                                _ => Color::Red,
                            },
                        )
                    } else {
                        ('c', Color::Blue)
                    };
                    buf[(inner.x + col as u16, y)]
                        .set_char(ch)
                        .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
                    continue;
                }
            }
        }

        // Bisulfite CG mode
        if opts.color_mode == AlignmentColorMode::Bisulfite {
            if let (Some(qi), Some(ridx)) = (q_idx, ref_pos_1.checked_sub(region_start_1)) {
                let ridx = ridx as usize;
                if let (Some(&qb), Some(&rb)) =
                    (row.query_sequence.get(qi), reference_seq.get(ridx))
                {
                    let next_r = reference_seq.get(ridx + 1).copied();
                    let prev_r = ridx.checked_sub(1).and_then(|k| reference_seq.get(k).copied());
                    if let Some(call) = bisulfite_call(row.is_reverse, qb, rb, next_r, prev_r) {
                        let (ch, color) = match call {
                            BisulfiteCall::Methylated => ('C', Color::Red),
                            BisulfiteCall::Unmethylated => ('T', Color::Blue),
                        };
                        buf[(inner.x + col as u16, y)]
                            .set_char(ch)
                            .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
                        continue;
                    }
                }
            }
        }

        let (ch, style) = match glyph {
            BaseGlyph::Match => ('.', base_match_style),
            BaseGlyph::Mismatch(b) => (*b as char, mismatch_style),
            BaseGlyph::Deletion => ('*', deletion_style),
            BaseGlyph::SoftClip(b) => (*b as char, Style::default().fg(Color::Yellow)),
        };
        buf[(inner.x + col as u16, y)].set_char(ch).set_style(style);
    }

    if let Some((sc_start, sc_bases)) = &cells.soft_clip_left {
        for (j, &b) in sc_bases.iter().enumerate() {
            let g0 = (*sc_start + j as u64).saturating_sub(1);
            if g0 < view_start_0 {
                continue;
            }
            if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
                buf[(inner.x + col as u16, y)]
                    .set_char(b.to_ascii_lowercase() as char)
                    .set_style(Style::default().fg(Color::Yellow));
            }
        }
    }
    if let Some((sc_start, sc_bases)) = &cells.soft_clip_right {
        for (j, &b) in sc_bases.iter().enumerate() {
            let g0 = (*sc_start + j as u64).saturating_sub(1);
            if g0 < view_start_0 {
                continue;
            }
            if let Some(col) = genomic_to_screen(g0, view_start_0, view_width, inner.width as u32) {
                buf[(inner.x + col as u16, y)]
                    .set_char(b.to_ascii_lowercase() as char)
                    .set_style(Style::default().fg(Color::Yellow));
            }
        }
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
}

fn read_base_style(
    row: &AlignmentRow,
    opts: &igv_core::alignment::AlignmentRenderOpts,
    theme: &Theme,
) -> Style {
    match opts.color_mode {
        AlignmentColorMode::Strand => {
            if row.is_reverse {
                theme.get("MATCH_REV")
            } else {
                theme.get("MATCH_FWD")
            }
        }
        AlignmentColorMode::MappingQuality => {
            if row.mapq >= 30 {
                Style::default().fg(Color::Green)
            } else if row.mapq >= 10 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            }
        }
        AlignmentColorMode::InsertSize => {
            let isize = row.template_len.abs();
            if isize == 0 {
                Style::default().fg(Color::DarkGray)
            } else if isize < opts.insert_min {
                Style::default().fg(Color::LightBlue)
            } else if isize > opts.insert_max {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Gray)
            }
        }
        AlignmentColorMode::PairOrientation => match pair_orientation(row) {
            PairOrientation::Lr => Style::default().fg(Color::Gray),
            PairOrientation::Rl => Style::default().fg(Color::Green),
            PairOrientation::Ll => Style::default().fg(Color::Cyan),
            PairOrientation::Rr => Style::default().fg(Color::Yellow),
            PairOrientation::Other => Style::default().fg(Color::Magenta),
        },
        AlignmentColorMode::MateChrom => match &row.mate_chrom {
            Some(mc) if mc != "*" && mc != "=" => Style::default().fg(palette_color(mc)),
            _ => Style::default().fg(Color::DarkGray),
        },
        AlignmentColorMode::Tag => match &row.tag {
            Some((_, v)) => Style::default().fg(palette_color(v)),
            None => Style::default().fg(Color::DarkGray),
        },
        AlignmentColorMode::BaseModification | AlignmentColorMode::Bisulfite => {
            Style::default().fg(Color::DarkGray)
        }
    }
}

fn palette_color(key: &str) -> Color {
    const COLS: [Color; 6] = [
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Magenta,
        Color::LightBlue,
        Color::LightRed,
    ];
    let h: usize = key
        .bytes()
        .fold(5381usize, |acc, b| acc.wrapping_mul(33) ^ b as usize);
    COLS[h % COLS.len()]
}
