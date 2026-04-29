//! Link-track widget. Renders pairwise interactions (BEDPE) as adaptive
//! arcs / heatmap. See `docs/superpowers/specs/2026-04-29-bedpe-link-design.md`.
//!
//! Task 18: real heatmap rendering, partial-cis arrow, trans markers,
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
        // Compute mode first so it can label the title.
        let arc_count = self
            .visible
            .iter()
            .filter(|v| matches!(v.scope, LinkScope::BothIn | LinkScope::PartialCis { .. }))
            .count();
        let arc_budget_estimate = (area.height.saturating_sub(3)) as usize; // rough: -2 borders -1 anchor strip
        let mode_estimate = if arc_count <= arc_budget_estimate {
            Mode::Arc
        } else {
            Mode::Heatmap
        };

        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .style(self.theme.get("BORDER"))
            .title_bottom(format_title(
                self.display_name,
                self.visible.len(),
                mode_estimate,
                self.total_record_count,
            ));
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.area() == 0 {
            return;
        }

        // Now compute the real arc budget (after we know inner.height).
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
            Mode::Heatmap => paint_heatmap(buf, inner, region, self.visible, style),
        }
    }
}

fn format_title(name: &str, count: usize, mode: Mode, total: usize) -> String {
    let suffix_word = if count == 1 { "loop" } else { "loops" };
    match mode {
        Mode::Arc => format!("link[{}]  {} {}", name, count, suffix_word),
        Mode::Heatmap => format!(
            "link[{}] · heatmap  {} {} in window (of {})",
            name, count, suffix_word, total
        ),
    }
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

    // PartialCis: paint the in-window anchor + an edge arrow with distance label.
    for v in visible {
        if let LinkScope::PartialCis { off_anchor_mid, off_to_left } = v.scope {
            let style = bucket_style_for(&bucket_styles, v.record.score);
            // Determine which anchor is in-window.
            let (in_s, in_e) = if v.record.end_a >= region.start && v.record.start_a <= region.end {
                (v.record.start_a, v.record.end_a)
            } else {
                (v.record.start_b, v.record.end_b)
            };
            paint_anchor_block(buf, inner, region, in_s, in_e, anchor_y, style);
            // Edge arrow on the row immediately above the anchor strip.
            let edge_y = anchor_y.saturating_sub(1);
            let dist_bp = if off_to_left {
                region.start.saturating_sub(off_anchor_mid)
            } else {
                off_anchor_mid.saturating_sub(region.end)
            };
            let label = format!("{} {}", arrow_label(off_to_left), human_bp(dist_bp));
            let lx = if off_to_left {
                inner.x
            } else {
                inner.x + width.saturating_sub(label.chars().count() as u16)
            };
            paint_str(buf, lx, edge_y, &label, style, inner.x + width);
        }
    }

    // Trans: paint the in-window anchor + a ⤴ chrom:pos marker.
    for v in visible {
        if let LinkScope::Trans { ref off_chrom, off_anchor_mid } = v.scope {
            let style = bucket_style_for(&bucket_styles, v.record.score);
            let (in_s, in_e) = if v.record.chrom_a.as_ref() == region.chrom.as_str() {
                (v.record.start_a, v.record.end_a)
            } else {
                (v.record.start_b, v.record.end_b)
            };
            paint_anchor_block(buf, inner, region, in_s, in_e, anchor_y, style);
            let mid_in_clamped = in_s.max(region.start);
            let mid_in_clamped_e = in_e.min(region.end);
            if mid_in_clamped_e < mid_in_clamped {
                continue;
            }
            let mid_in = midpoint_u64(mid_in_clamped, mid_in_clamped_e);
            let label = format!("\u{2934} {}:{}", off_chrom, human_bp_pos(off_anchor_mid));
            if let Some(c) = bp_to_col_helper(region, inner, mid_in) {
                paint_str(buf, c, anchor_y.saturating_sub(1), &label, style, inner.x + width);
            }
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

/// Quartile bucket styles, computed only when ≥4 scored records are
/// visible. Returns `None` for the degenerate-low-data case so callers
/// fall back to the default style.
fn compute_bucket_styles(
    visible: &[VisibleLink],
    base: ratatui::style::Style,
) -> Option<([f64; 3], [ratatui::style::Style; 4])> {
    let mut scored: Vec<f64> = visible
        .iter()
        .filter_map(|v| v.record.score)
        .collect();
    scored.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if scored.len() < 4 {
        return None;
    }
    let n = scored.len();
    let qs = [
        scored[n / 4],
        scored[n / 2],
        scored[(3 * n) / 4],
    ];
    let styles = [
        base.add_modifier(Modifier::DIM),
        base,
        base.add_modifier(Modifier::BOLD),
        base.add_modifier(Modifier::BOLD),
    ];
    Some((qs, styles))
}

fn bucket_style_for(
    buckets: &Option<([f64; 3], [ratatui::style::Style; 4])>,
    score: Option<f64>,
) -> ratatui::style::Style {
    match (buckets, score) {
        (Some((qs, styles)), Some(s)) => {
            let bucket = if s < qs[0] { 0 }
                else if s < qs[1] { 1 }
                else if s < qs[2] { 2 }
                else { 3 };
            styles[bucket]
        }
        (Some((_, styles)), None) => styles[1],
        (None, _) => ratatui::style::Style::default(),
    }
}

fn paint_heatmap(
    buf: &mut Buffer,
    inner: Rect,
    region: &Region,
    visible: &[VisibleLink],
    base: ratatui::style::Style,
) {
    let cols = inner.width as usize;
    if cols == 0 {
        return;
    }
    let scored: Vec<f64> = visible.iter().filter_map(|v| v.record.score).collect();
    let use_count_fallback = scored.len() < 4;

    // Per-column accumulator: max-of-scores in normal mode,
    // count-of-overlapping-anchors in fallback mode.
    let mut col_value: Vec<f64> = vec![0.0; cols];
    let span = (region.end - region.start).max(1);

    let q25 = if !use_count_fallback {
        let mut s = scored.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        s[s.len() / 4]
    } else {
        0.0
    };

    for v in visible {
        for (s, e) in anchors_in_window(v, region) {
            if e < region.start || s > region.end {
                continue;
            }
            let s = s.max(region.start);
            let e = e.min(region.end);
            let lo = ((s - region.start) as f64 / span as f64
                * (cols as f64 - 1.0))
                .floor() as usize;
            let hi = ((e - region.start) as f64 / span as f64
                * (cols as f64 - 1.0))
                .ceil() as usize;
            let end = hi.min(cols.saturating_sub(1)).saturating_add(1);
            for val in &mut col_value[lo..end] {
                if use_count_fallback {
                    *val += 1.0;
                } else {
                    let score = v.record.score.unwrap_or(q25);
                    if score > *val {
                        *val = score;
                    }
                }
            }
        }
    }

    let max = col_value.iter().cloned().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        return;
    }
    for (c, &v) in col_value.iter().enumerate() {
        let q = (v / max).clamp(0.0, 1.0);
        let ch = if q == 0.0 { ' ' }
            else if q < 0.25 { '\u{2591}' }   // ░
            else if q < 0.50 { '\u{2592}' }   // ▒
            else if q < 0.75 { '\u{2593}' }   // ▓
            else { '\u{2588}' };               // █
        if ch == ' ' {
            continue;
        }
        let x = inner.x + c as u16;
        for row in 0..inner.height {
            let y = inner.y + row;
            buf[(x, y)].set_char(ch).set_style(base);
        }
    }
}

fn anchors_in_window(
    v: &VisibleLink,
    region: &Region,
) -> Vec<(u64, u64)> {
    let mut out = Vec::with_capacity(2);
    if v.record.chrom_a.as_ref() == region.chrom.as_str()
        && v.record.end_a >= region.start
        && v.record.start_a <= region.end
    {
        out.push((v.record.start_a, v.record.end_a));
    }
    if v.record.chrom_b.as_ref() == region.chrom.as_str()
        && v.record.end_b >= region.start
        && v.record.start_b <= region.end
    {
        out.push((v.record.start_b, v.record.end_b));
    }
    out
}

fn arrow_label(left: bool) -> &'static str {
    if left { "\u{25c0}\u{2500}" } else { "\u{2500}\u{25b6}" } // ◄─ / ─►
}

fn human_bp(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}Mb", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}kb", n / 1_000)
    } else {
        format!("{}b", n)
    }
}

fn human_bp_pos(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{}", n)
    }
}

/// Paint a string into the buffer starting at (x, y). Each character takes
/// one cell. Stops at `right_bound` (exclusive) so the string never overflows
/// the inner area.
fn paint_str(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    s: &str,
    style: ratatui::style::Style,
    right_bound: u16,
) {
    for (i, ch) in s.chars().enumerate() {
        let cx = x.saturating_add(i as u16);
        if cx >= right_bound {
            break;
        }
        buf[(cx, y)].set_char(ch).set_style(style);
    }
}

fn bp_to_col_helper(region: &Region, inner: Rect, bp: u64) -> Option<u16> {
    let width = inner.width;
    if bp < region.start || bp > region.end || width == 0 {
        return None;
    }
    let off = bp - region.start;
    let span = (region.end - region.start).max(1);
    let frac = off as f64 / span as f64;
    let c = (frac * (width as f64 - 1.0)).round() as u16;
    Some(inner.x + c.min(width.saturating_sub(1)))
}
