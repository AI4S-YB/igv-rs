use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::genome::CytoStain;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct OverviewWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

impl Widget for OverviewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let pane_tag = if self.state.panes.len() > 1 {
            format!(
                "chromosome {} [{}/{}]",
                self.state.region.chrom,
                self.state.active_pane + 1,
                self.state.panes.len()
            )
        } else {
            format!("chromosome {}", self.state.region.chrom)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .style(self.theme.get("BORDER"))
            .title(pane_tag);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 {
            return;
        }

        let chrom_len = self
            .state
            .references
            .iter()
            .find(|r| r.name == self.state.region.chrom)
            .map(|r| r.length)
            .unwrap_or(self.state.region.end)
            .max(1);

        let bar_y = inner.y;
        let bands: Vec<_> = self
            .state
            .cytobands
            .iter()
            .filter(|b| b.chrom == self.state.region.chrom)
            .collect();

        if bands.is_empty() {
            let style = self.theme.get("OVERVIEW");
            for x in 0..inner.width {
                buf[(inner.x + x, bar_y)].set_char('─').set_style(style);
            }
        } else {
            for col in 0..inner.width {
                let bp = ((col as u128 * chrom_len as u128) / inner.width.max(1) as u128) as u64 + 1;
                let band = bands.iter().find(|b| bp >= b.start && bp <= b.end);
                let (ch, col_fg) = match band.map(|b| b.stain) {
                    Some(CytoStain::Gneg) | None => ('░', Color::Gray),
                    Some(CytoStain::Gpos25) => ('▒', Color::DarkGray),
                    Some(CytoStain::Gpos50) => ('▓', Color::Gray),
                    Some(CytoStain::Gpos75) => ('█', Color::DarkGray),
                    Some(CytoStain::Gpos100) => ('█', Color::White),
                    Some(CytoStain::Acen) => ('◆', Color::Red),
                    Some(CytoStain::Gvar) => ('▒', Color::Cyan),
                    Some(CytoStain::Stalk) => ('│', Color::Blue),
                };
                buf[(inner.x + col, bar_y)]
                    .set_char(ch)
                    .set_style(Style::default().fg(col_fg));
            }
        }

        let start_col = ((self.state.region.start as u128 * inner.width as u128 / chrom_len as u128)
            .min(inner.width as u128 - 1)) as u16;
        let end_col = ((self.state.region.end as u128 * inner.width as u128 / chrom_len as u128)
            .min(inner.width as u128 - 1)) as u16;
        let vp_style = Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD);
        for x in start_col..=end_col {
            let ch = if x == start_col && end_col > start_col {
                '['
            } else if x == end_col && end_col > start_col {
                ']'
            } else {
                '█'
            };
            buf[(inner.x + x, bar_y)].set_char(ch).set_style(vp_style);
        }
    }
}
