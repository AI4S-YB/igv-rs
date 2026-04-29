//! Link-track widget. Renders pairwise interactions (BEDPE) as adaptive
//! arcs / heatmap. See `docs/superpowers/specs/2026-04-29-bedpe-link-design.md`.
//!
//! This task (16) ships the arc-mode skeleton for `BothIn` records.
//! Task 18 adds: real heatmap rendering, partial-cis arrow, trans markers,
//! score-quartile coloring.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::widgets::{Block, Borders, Widget};

use igv_core::region::Region;
use igv_core::source::link::{LinkScope, VisibleLink};

use crate::ui::theme::Theme;

pub struct LinkWidget<'a> {
    pub display_name: &'a str,
    pub region: &'a Region,
    pub theme: &'a Theme,
    pub visible: &'a [VisibleLink],
    /// Total records loaded for this track (for "N of M" labeling in heatmap mode).
    pub total_record_count: usize,
    pub height_rows: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Arc,
    Heatmap,
}

impl Widget for LinkWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Compute mode BEFORE the title so it can label the band.
        let arc_count = self
            .visible
            .iter()
            .filter(|v| matches!(v.scope, LinkScope::BothIn | LinkScope::PartialCis { .. }))
            .count();

        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title_bottom(format_title(self.display_name, self.visible.len()));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 {
            return;
        }

        let arc_budget = inner.height.saturating_sub(1) as usize; // -1 for anchor strip
        let mode = if arc_count <= arc_budget {
            Mode::Arc
        } else {
            Mode::Heatmap
        };

        let style = self.theme.get("LINK");
        let region = self.region;
        let cols = inner.width as u32;
        if cols == 0 {
            return;
        }

        match mode {
            Mode::Arc => paint_arc_mode(buf, inner, region, self.visible, style),
            Mode::Heatmap => paint_heatmap_placeholder(buf, inner, style),
        }
    }
}

fn format_title(name: &str, count: usize) -> String {
    let suffix = if count == 1 { "loop" } else { "loops" };
    format!("link[{}]  {} {}", name, count, suffix)
}

fn paint_arc_mode(
    buf: &mut Buffer,
    inner: Rect,
    region: &Region,
    visible: &[VisibleLink],
    base: ratatui::style::Style,
) {
    if visible.is_empty() {
        return;
    }
    let width = inner.width;
    let anchor_y = inner.y + inner.height.saturating_sub(1);

    let bp_to_col = |bp: u64| -> Option<u16> {
        if bp < region.start || bp > region.end || width == 0 {
            return None;
        }
        let off = bp - region.start;
        let span = region.end - region.start;
        if span == 0 {
            return Some(inner.x);
        }
        let frac = off as f64 / span as f64;
        let c = (frac * (width as f64 - 1.0)).round() as u16;
        Some(inner.x + c.min(width.saturating_sub(1)))
    };

    let bucket_styles = compute_bucket_styles(visible, base);

    // Greedy arc-row placement: sort by left anchor end, place each arc
    // into the lowest row whose latest occupied column is left of new start.
    let mut arcs: Vec<(u16, u16, ratatui::style::Style)> = Vec::new();
    for v in visible {
        if let LinkScope::BothIn = v.scope {
            let mid_a = midpoint_u64(v.record.start_a, v.record.end_a);
            let mid_b = midpoint_u64(v.record.start_b, v.record.end_b);
            if let (Some(ca), Some(cb)) = (bp_to_col(mid_a), bp_to_col(mid_b)) {
                let (lo, hi) = if ca <= cb { (ca, cb) } else { (cb, ca) };
                let style = bucket_style_for(&bucket_styles, v.record.score);
                arcs.push((lo, hi, style));
            }
        }
    }
    arcs.sort_by_key(|(lo, hi, _)| (*lo, *hi));

    let mut row_last_end: Vec<u16> = Vec::new();
    let arc_band_top = inner.y;
    let arc_band_bot = anchor_y.saturating_sub(1);

    for (lo, hi, style) in arcs {
        let row_idx = row_last_end
            .iter()
            .position(|&end| end < lo)
            .unwrap_or_else(|| {
                row_last_end.push(0);
                row_last_end.len() - 1
            });
        if arc_band_bot < arc_band_top {
            // No arc band space at all.
            break;
        }
        let arc_band_h = arc_band_bot - arc_band_top;
        if (row_idx as u16) > arc_band_h {
            // Out of vertical space.
            break;
        }
        row_last_end[row_idx] = hi;
        let y = arc_band_bot.saturating_sub(row_idx as u16);
        if y >= arc_band_top && lo < inner.x + width {
            buf[(lo, y)].set_char('\u{256d}').set_style(style); // ╭
        }
        if y >= arc_band_top && hi < inner.x + width {
            buf[(hi, y)].set_char('\u{256e}').set_style(style); // ╮
        }
        for x in (lo + 1)..hi {
            if x < inner.x + width {
                buf[(x, y)].set_char('\u{2500}').set_style(style); // ─
            }
        }
    }

    // Anchor strip — paint a █ for every column an anchor covers.
    for v in visible {
        if let LinkScope::BothIn = v.scope {
            let style = bucket_style_for(&bucket_styles, v.record.score);
            paint_anchor_block(buf, inner, region, v.record.start_a, v.record.end_a, anchor_y, style);
            paint_anchor_block(buf, inner, region, v.record.start_b, v.record.end_b, anchor_y, style);
        }
    }
}

fn paint_anchor_block(
    buf: &mut Buffer,
    inner: Rect,
    region: &Region,
    s: u64,
    e: u64,
    y: u16,
    style: ratatui::style::Style,
) {
    let width = inner.width;
    if width == 0 || s > region.end || e < region.start {
        return;
    }
    let span = (region.end - region.start).max(1);
    let s_clamped = s.max(region.start);
    let e_clamped = e.min(region.end);
    let lo = ((s_clamped - region.start) as f64 / span as f64
        * (width as f64 - 1.0))
        .round() as u16;
    let hi = ((e_clamped - region.start) as f64 / span as f64
        * (width as f64 - 1.0))
        .round() as u16;
    let lo = inner.x + lo.min(width.saturating_sub(1));
    let hi = inner.x + hi.min(width.saturating_sub(1));
    for x in lo..=hi {
        buf[(x, y)].set_char('\u{2588}').set_style(style); // █
    }
}

fn midpoint_u64(s: u64, e: u64) -> u64 {
    s + (e - s) / 2
}

/// Heatmap placeholder — a single dim divider so density-mode is visually
/// distinct. Real heatmap implementation lands in Task 18.
fn paint_heatmap_placeholder(
    buf: &mut Buffer,
    inner: Rect,
    base: ratatui::style::Style,
) {
    for x in inner.x..(inner.x + inner.width) {
        buf[(x, inner.y + inner.height / 2)]
            .set_char('\u{2500}')
            .set_style(base);
    }
}

/// Quartile bucket styles, computed only when ≥4 scored records are
/// visible. Returns `None` for the degenerate-low-data case so callers
/// fall back to the default style.
fn compute_bucket_styles(
    visible: &[VisibleLink],
    base: ratatui::style::Style,
) -> Option<[ratatui::style::Style; 4]> {
    let mut scored: Vec<f64> = visible
        .iter()
        .filter_map(|v| v.record.score)
        .collect();
    scored.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if scored.len() < 4 {
        return None;
    }
    Some([
        base.add_modifier(Modifier::DIM),
        base,
        base.add_modifier(Modifier::BOLD),
        base.add_modifier(Modifier::BOLD),
    ])
}

/// Task 16: simple bucket-1 (default) styling for any record.
/// Task 18 will replace with real per-record quartile lookup.
fn bucket_style_for(
    buckets: &Option<[ratatui::style::Style; 4]>,
    _score: Option<f64>,
) -> ratatui::style::Style {
    match buckets {
        Some(b) => b[1],
        None => ratatui::style::Style::default(),
    }
}
