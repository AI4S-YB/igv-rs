use igv_core::render_inputs::RenderInputs;

use crate::layout::{PlotMetrics, Rect};
use crate::svg::doc::{SvgDoc, TextAnchor};
use crate::theme::GraphicalTheme;

pub fn draw(
    doc: &mut SvgDoc,
    area: Rect,
    plot: &PlotMetrics,
    inputs: &RenderInputs,
    theme: &GraphicalTheme,
) {
    let baseline_y = (area.y + area.h - 6) as f64;
    doc.line(
        plot.plot_x0 as f64,
        baseline_y,
        plot.plot_x1 as f64,
        baseline_y,
        theme.muted,
        1.0,
    );
    let region = &inputs.region;
    let step = nice_step_bp(region.width());
    let first = region.start.div_ceil(step) * step;
    let mut tick = first;
    while tick <= region.end {
        let x = plot.bp_to_px(tick);
        doc.line(x, baseline_y - 4.0, x, baseline_y, theme.muted, 1.0);
        let label = format_bp(tick);
        doc.text(
            x,
            baseline_y - 6.0,
            &label,
            theme.ruler_text,
            theme.font_px_small,
            TextAnchor::Middle,
        );
        tick = tick.saturating_add(step);
        if tick == 0 {
            break;
        }
    }
}

/// Pick a "nice" tick interval (1, 2, 5 × 10^k) for a given region width
/// such that we get roughly 6–10 ticks across the plot.
pub fn nice_step_bp(region_width: u64) -> u64 {
    if region_width == 0 {
        return 1;
    }
    let target = (region_width / 8).max(1);
    let mag = 10u64.pow((target as f64).log10().floor() as u32);
    for &m in &[1u64, 2, 5, 10] {
        if mag * m >= target {
            return mag * m;
        }
    }
    mag * 10
}

fn format_bp(bp: u64) -> String {
    let s = bp.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_step_for_typical_widths() {
        assert!(nice_step_bp(100) <= 25);
        assert!(nice_step_bp(1_000) <= 250);
        assert!(nice_step_bp(50_000) >= 5_000);
        assert!(nice_step_bp(50_000) <= 10_000);
    }

    #[test]
    fn format_bp_inserts_thousand_separators() {
        assert_eq!(format_bp(0), "0");
        assert_eq!(format_bp(123), "123");
        assert_eq!(format_bp(1_234), "1,234");
        assert_eq!(format_bp(1_234_567), "1,234,567");
    }
}
