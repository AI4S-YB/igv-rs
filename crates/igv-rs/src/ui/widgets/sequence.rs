use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;
use igv_core::sequence::{complement_base, translate_three_frames};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct SequenceWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for SequenceWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.area() == 0 {
            return;
        }
        let region = &self.state.region;
        let mode = self.state.thresholds.classify(region.width());
        if !matches!(mode, RenderMode::PerBase | RenderMode::DetailedReads) {
            return;
        }

        let view_start = region.start - 1;
        let view_width = region.width();
        let hits = &self.state.sequence_opts.motif_hits;

        let is_motif_pos = |pos_1: u64| -> bool {
            hits.iter().any(|h| pos_1 >= h.start && pos_1 <= h.end)
        };

        for (i, &base) in self.state.reference_seq.iter().enumerate() {
            let pos_1 = region.start + i as u64;
            let g = view_start + i as u64;
            let Some(col) = genomic_to_screen(g, view_start, view_width, area.width as u32) else {
                continue;
            };
            let mut style = self.theme.get(base_theme_key(base));
            if is_motif_pos(pos_1) {
                style = style.bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD);
            }
            buf[(area.x + col as u16, area.y)]
                .set_char(base as char)
                .set_style(style);

            if self.state.sequence_opts.show_complement && area.height >= 2 {
                let comp = complement_base(base);
                let mut cstyle = self.theme.get(base_theme_key(comp));
                if is_motif_pos(pos_1) {
                    cstyle = cstyle.bg(Color::Yellow).fg(Color::Black);
                }
                buf[(area.x + col as u16, area.y + 1)]
                    .set_char(comp as char)
                    .set_style(cstyle);
            }
        }

        if self.state.sequence_opts.show_translation {
            let start_row = if self.state.sequence_opts.show_complement {
                2
            } else {
                1
            };
            let frames = translate_three_frames(
                &self.state.reference_seq,
                region.start,
                self.state.sequence_opts.reverse_translation,
            );
            for (fi, frame) in frames.iter().enumerate() {
                let row_y = area.y + start_row + fi as u16;
                if row_y >= area.y + area.height {
                    break;
                }
                for codon in &frame.codons {
                    let g_mid = codon.start_bp; // place AA glyph cleanly across codon start/mid
                    let g0 = g_mid.saturating_sub(1);
                    let Some(col) = genomic_to_screen(g0, view_start, view_width, area.width as u32)
                    else {
                        continue;
                    };
                    let st = if codon.is_start {
                        Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
                    } else if codon.is_stop {
                        Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Cyan)
                    };
                    buf[(area.x + col as u16, row_y)]
                        .set_char(codon.aa)
                        .set_style(st);
                }
            }
        }
    }
}

fn base_theme_key(base: u8) -> &'static str {
    match base.to_ascii_uppercase() {
        b'A' => "A",
        b'C' => "C",
        b'G' => "G",
        b'T' | b'U' => "T",
        _ => "N",
    }
}
