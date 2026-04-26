use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::app::state::AppState;
use crate::ui::theme::Theme;

pub struct RulerWidget<'a> {
    pub state: &'a AppState,
    pub theme: &'a Theme,
}

fn pretty_pos(p: u64) -> String {
    if p >= 1_000_000 {
        format!("{:.1}Mb", p as f64 / 1_000_000.0)
    } else if p >= 1_000 {
        format!("{:.1}kb", p as f64 / 1_000.0)
    } else {
        format!("{}", p)
    }
}

impl Widget for RulerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let style: Style = self.theme.get("BORDER");
        let region = &self.state.region;
        let width = area.width.max(1) as u64;
        let span = region.width();
        if span == 0 || width == 0 {
            return;
        }

        // Choose tick count by area width.
        let target_ticks = (area.width / 12).max(2) as u64;
        let raw_step = span / target_ticks.max(1);
        let step = nice_step(raw_step.max(1));

        let mut col = 0;
        let mut pos = ((region.start + step - 1) / step) * step; // round up
        while pos <= region.end && col < area.width {
            let rel = pos.saturating_sub(region.start);
            let screen_col =
                (rel as u128 * area.width as u128 / span as u128) as u16;
            if screen_col < area.width {
                let label = pretty_pos(pos);
                let max_len = (area.width - screen_col) as usize;
                let cut = &label[..label.len().min(max_len)];
                for (i, ch) in cut.chars().enumerate() {
                    buf.get_mut(area.x + screen_col + i as u16, area.y)
                        .set_char(ch)
                        .set_style(style);
                }
            }
            pos += step;
            col += 1;
        }
    }
}

fn nice_step(raw: u64) -> u64 {
    // 1, 2, 5, 10, 20, 50, 100 …
    let mut step = 1u64;
    while step < raw {
        if step.saturating_mul(2) >= raw {
            return step * 2;
        }
        if step.saturating_mul(5) >= raw {
            return step * 5;
        }
        step = step.saturating_mul(10);
    }
    step
}
