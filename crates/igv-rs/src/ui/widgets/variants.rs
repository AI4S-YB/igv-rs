use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::{genomic_to_screen, Region};
use igv_core::render::RenderMode;
use igv_core::source::{GenotypeCall, VariantRecord};

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct VariantsWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for VariantsWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        draw_vcf_panel(
            area,
            buf,
            "variants",
            &[],
            &self.state.variants,
            &self.state.region,
            self.state.thresholds.classify(self.state.region.width()),
            self.theme,
        );
    }
}

pub struct VcfTrackWidget<'a> {
    pub display_name: &'a str,
    pub sample_names: &'a [String],
    pub records: &'a [VariantRecord],
    pub region: &'a Region,
    pub mode: RenderMode,
    pub theme: &'a Theme,
}

impl Widget for VcfTrackWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        draw_vcf_panel(
            area,
            buf,
            self.display_name,
            self.sample_names,
            self.records,
            self.region,
            self.mode,
            self.theme,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_vcf_panel(
    area: Rect,
    buf: &mut Buffer,
    title: &str,
    sample_names: &[String],
    records: &[VariantRecord],
    region: &Region,
    mode: RenderMode,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .style(theme.get("BORDER"))
        .title(format!("{title} ({} vars)", records.len()));
    let inner = block.inner(area);
    block.render(area, buf);

    if inner.area() == 0 || records.is_empty() || matches!(mode, RenderMode::OverviewOnly) {
        return;
    }

    let view_start_0 = region.start - 1;
    let style = theme.get("VARIANT");

    for v in records {
        let pos_0 = v.pos.saturating_sub(1);
        let Some(col) = genomic_to_screen(pos_0, view_start_0, region.width(), inner.width as u32)
        else {
            continue;
        };
        let glyph: char = match mode {
            RenderMode::PerBase => v
                .alternate_alleles
                .first()
                .and_then(|a| a.chars().next())
                .unwrap_or('▼'),
            _ => '▼',
        };
        buf[(inner.x + col as u16, inner.y)]
            .set_char(glyph)
            .set_style(style);

        for row_off in 1..inner.height {
            let s_idx = (row_off - 1) as usize;
            if s_idx >= sample_names.len() {
                break;
            }
            let Some(sg) = v.genotypes.get(s_idx) else {
                continue;
            };
            let (ch, st) = match (sg.mendelian_violation, sg.call) {
                (true, _) => (
                    '!',
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                (false, GenotypeCall::HomRef) => ('·', Style::default().fg(Color::DarkGray)),
                (false, GenotypeCall::Het) => ('◐', Style::default().fg(Color::Cyan)),
                (false, GenotypeCall::HomAlt) => ('█', Style::default().fg(Color::Red)),
                (false, GenotypeCall::OtherAlt) => ('▒', Style::default().fg(Color::Yellow)),
                (false, GenotypeCall::NoCall) => (' ', Style::default()),
            };
            buf[(inner.x + col as u16, inner.y + row_off)]
                .set_char(ch)
                .set_style(st);
        }
    }
}
