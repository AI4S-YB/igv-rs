//! Mouse hit-testing for ruler click/drag-zoom and per-feature click inspection.

use ratatui::layout::Rect;

use igv_core::region::Region;

use crate::app::state::{AppState, InspectorPopup};
use crate::ui::layout::LayoutAreas;

#[derive(Debug, Clone, Default)]
pub struct FrameHitMap {
    pub panes: Vec<(Rect, LayoutAreas)>,
}

fn inside(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x.saturating_add(r.width) && y >= r.y && y < r.y.saturating_add(r.height)
}

fn col_to_bp(col_x: u16, area: Rect, region: &Region) -> u64 {
    let inner_w = area.width.max(1) as u64;
    let rel = col_x.saturating_sub(area.x) as u64;
    let offset = (rel.saturating_mul(region.width()) / inner_w).min(region.width().saturating_sub(1));
    region.start.saturating_add(offset)
}

pub fn handle_drag(
    hit: &FrameHitMap,
    state: &AppState,
    start_x: u16,
    end_x: u16,
    y: u16,
) -> Option<Region> {
    let (_pane_rect, areas) = hit.panes.get(state.active_pane)?;
    if inside(areas.ruler, start_x, y) {
        let b0 = col_to_bp(start_x.min(end_x), areas.ruler, &state.region);
        let b1 = col_to_bp(start_x.max(end_x), areas.ruler, &state.region);
        let hi = b1.max(b0.saturating_add(9));
        return Region::new(state.region.chrom.clone(), b0, hi).ok();
    }
    None
}

pub fn handle_click(hit: &FrameHitMap, state: &mut AppState, x: u16, y: u16) -> Option<Region> {
    // Allow clicking any split pane to switch active pane
    for (p_idx, (pane_rect, _)) in hit.panes.iter().enumerate() {
        if p_idx != state.active_pane && inside(*pane_rect, x, y) {
            state.active_pane = p_idx;
            return state.panes.get(p_idx).cloned();
        }
    }

    let (_pane_rect, areas) = hit.panes.get(state.active_pane)?;
    if inside(areas.ruler, x, y) {
        let bp = col_to_bp(x, areas.ruler, &state.region);
        let half = state.region.width() / 2;
        let s = bp.saturating_sub(half).max(1);
        let e = s.saturating_add(state.region.width().saturating_sub(1));
        return Region::new(state.region.chrom.clone(), s, e).ok();
    }

    if inside(areas.overview, x, y) {
        let chrom_len = state
            .references
            .iter()
            .find(|r| r.name == state.region.chrom)
            .map(|r| r.length)
            .unwrap_or(state.region.end)
            .max(1);
        let inner_w = areas.overview.width.saturating_sub(2).max(1) as u128;
        let rel = x.saturating_sub(areas.overview.x + 1) as u128;
        let bp = ((rel * chrom_len as u128) / inner_w) as u64 + 1;
        let half = state.region.width() / 2;
        let s = bp.saturating_sub(half).max(1);
        let e = s.saturating_add(state.region.width().saturating_sub(1)).min(chrom_len);
        return Region::new(state.region.chrom.clone(), s, e.max(s)).ok();
    }

    // Alignments read click
    for (ti, rect) in areas.alignments.iter().enumerate() {
        if inside(*rect, x, y) && y > rect.y && y + 1 < rect.y + rect.height {
            let inner = Rect::new(rect.x, rect.y + 1, rect.width, rect.height - 2);
            let lane_target = (y - inner.y) as u32 + state.bam_scroll as u32;
            let bp = col_to_bp(x, inner, &state.region);
            if let (Some(rows), Some(lanes)) = (state.bam_rows.get(ti), state.bam_lanes.get(ti)) {
                if let Some((row, _)) = rows
                    .iter()
                    .zip(lanes.iter())
                    .find(|(r, &ln)| ln == lane_target && bp >= r.ref_start && bp <= r.ref_end)
                {
                    let cigar_str: String = row
                        .cigar
                        .iter()
                        .map(|op| format!("{}{:?}", op.len, op.kind))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let mut info = vec![
                        ("QNAME".into(), row.query_name.clone()),
                        ("Region".into(), format!("{}:{}-{}", state.region.chrom, row.ref_start, row.ref_end)),
                        ("FLAG".into(), format!("0x{:x} ({})", row.flag, if row.is_reverse { "-" } else { "+" })),
                        ("MAPQ".into(), row.mapq.to_string()),
                        ("CIGAR".into(), cigar_str),
                        ("InsertSize".into(), row.template_len.to_string()),
                    ];
                    if let Some(mc) = &row.mate_chrom {
                        info.push(("Mate".into(), format!("{mc}:{}", row.mate_pos.unwrap_or(0))));
                    }
                    if let Some((k, v)) = &row.tag {
                        info.push((format!("Tag:{k}"), v.clone()));
                    }
                    if !row.base_mods.is_empty() {
                        info.push(("BaseMods".into(), format!("{} calls (MM/ML)", row.base_mods.len())));
                    }
                    state.inspector = Some(InspectorPopup {
                        title: "Alignment Read".into(),
                        rows: info,
                    });
                    return None;
                }
            }
        }
    }

    // Variant click
    let var_rects = areas
        .vcf_tracks
        .iter()
        .enumerate()
        .chain(areas.variants.iter().map(|r| (0usize, r)));
    for (vi, rect) in var_rects {
        if inside(*rect, x, y) {
            let bp = col_to_bp(x, *rect, &state.region);
            let recs = if !state.vcf_track_rows.is_empty() {
                state.vcf_track_rows.get(vi).map(Vec::as_slice).unwrap_or(&[])
            } else {
                state.variants.as_slice()
            };
            if let Some(v) = recs.iter().min_by_key(|v| v.pos.abs_diff(bp)) {
                let mut rows = vec![
                    ("Locus".into(), format!("{}:{}", v.chrom, v.pos)),
                    ("ID".into(), v.id.clone().unwrap_or_else(|| ".".into())),
                    (
                        "Alleles".into(),
                        format!("{} -> {}", v.reference_allele, v.alternate_alleles.join(",")),
                    ),
                    (
                        "QUAL/Filter".into(),
                        format!(
                            "{} ({})",
                            v.quality.map(|q| format!("{q:.1}")).unwrap_or_else(|| ".".into()),
                            if v.passes_filter { "PASS" } else { "FILTERED" }
                        ),
                    ),
                ];
                if let Some(af) = v.allele_freq {
                    rows.push(("AF".into(), format!("{af:.4}")));
                }
                for sg in v.genotypes.iter().take(6) {
                    let mv = if sg.mendelian_violation { " [MENDELIAN VIOLATION]" } else { "" };
                    rows.push((sg.sample.clone(), format!("{:?}{mv}", sg.call)));
                }
                state.inspector = Some(InspectorPopup {
                    title: "Variant Record".into(),
                    rows,
                });
                return None;
            }
        }
    }

    // Annotation transcript click
    for (ai, rect) in areas.annotations.iter().enumerate() {
        if inside(*rect, x, y) {
            let bp = col_to_bp(x, *rect, &state.region);
            if let Some(txs) = state.annotation_rows.get(ai) {
                if let Some(tx) = txs.iter().find(|t| {
                    t.span().is_some_and(|(s, e)| bp >= s && bp <= e)
                }) {
                    let (s, e) = tx.span().unwrap_or((0, 0));
                    state.inspector = Some(InspectorPopup {
                        title: "Annotation Feature".into(),
                        rows: vec![
                            ("Name".into(), tx.name.clone()),
                            ("ID".into(), tx.id.clone()),
                            ("GeneID".into(), tx.gene_id.clone().unwrap_or_else(|| "-".into())),
                            ("Span".into(), format!("{}:{s}-{e}", state.region.chrom)),
                            ("Blocks".into(), tx.blocks.len().to_string()),
                        ],
                    });
                    return None;
                }
            }
        }
    }

    // GWAS SNP click
    for (gi, rect) in areas.gwas.iter().enumerate() {
        if inside(*rect, x, y) {
            let bp = col_to_bp(x, *rect, &state.region);
            if let Some(pts) = state.gwas_points.get(gi) {
                if let Some(pt) = pts.iter().min_by_key(|p| p.pos.abs_diff(bp)) {
                    state.inspector = Some(InspectorPopup {
                        title: "GWAS Marker".into(),
                        rows: vec![
                            ("SNP".into(), pt.snp_id.clone()),
                            ("Pos".into(), format!("{}:{}", state.region.chrom, pt.pos)),
                            ("p-value".into(), format!("{:.3e}", pt.p_raw)),
                            ("-log10(p)".into(), format!("{:.2}", pt.neg_log10_p)),
                        ],
                    });
                    return None;
                }
            }
        }
    }

    None
}
