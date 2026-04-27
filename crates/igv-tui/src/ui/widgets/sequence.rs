use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::app::state::AppState;
use crate::ui::theme::Theme;
use igv_core::region::genomic_to_screen;
use igv_core::render::RenderMode;

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

        let dim: Style = self.theme.get("BORDER");
        let view_start = region.start - 1; // 0-based
        let view_width = region.width();

        for (i, base) in self.state.reference_seq.iter().enumerate() {
            let g = view_start + i as u64;
            let col = match genomic_to_screen(g, view_start, view_width, area.width as u32) {
                Some(c) => c,
                None => continue,
            };
            let key = match base.to_ascii_uppercase() {
                b'A' => "A",
                b'C' => "C",
                b'G' => "G",
                b'T' => "T",
                _ => "N",
            };
            let style = self.theme.get(key);
            buf[(area.x + col as u16, area.y)]
                .set_char(*base as char)
                .set_style(style);
            // ignore second row of `area` (height ≥ 2 → leave it blank)
            let _ = dim;
        }
    }
}
