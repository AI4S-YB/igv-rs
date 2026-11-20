//! Runtime `:` command palette parser & executor + UCSC BLAT JSON query.

use std::path::PathBuf;

use igv_core::alignment::{AlignmentColorMode, AlignmentGroupMode, AlignmentSortMode};
use igv_core::region::Region;
use igv_core::render_inputs::RenderInputs;
use igv_core::source::SignalSummary;
use igv_render::TerminalGraphicsProtocol;

use crate::app::action::SnapshotFormat;
use crate::app::loader::LoadRequest;
use crate::app::state::{parse_snapshot_command, AppState, AsyncCommand, InspectorPopup, SnapshotJob, StatusKind};

pub fn execute_command(state: &mut AppState, raw: &str) -> Option<LoadRequest> {
    let trimmed = raw.trim().strip_prefix(':').unwrap_or(raw.trim()).trim();
    if trimmed.is_empty() {
        return None;
    }

    if matches!(trimmed, "q" | "quit" | "exit") {
        state.should_quit = true;
        return None;
    }
    if trimmed == "help" {
        state.help_open = !state.help_open;
        return None;
    }

    if let Some((path, format)) = parse_snapshot_command(trimmed) {
        if state.loading {
            state.set_status(StatusKind::Warning, "snapshot: still loading, try again");
        } else {
            state.pending_snapshot = Some(SnapshotJob {
                path: Some(path),
                format,
            });
        }
        return None;
    }

    if trimmed == "snapshot" || trimmed == "snap" {
        if state.loading {
            state.set_status(StatusKind::Warning, "snapshot: still loading, try again");
        } else {
            state.pending_snapshot = Some(SnapshotJob {
                path: None,
                format: SnapshotFormat::Svg,
            });
        }
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next().unwrap_or("").trim();

    match cmd {
        "circular" => {
            let p = if arg.is_empty() {
                PathBuf::from("circular.svg")
            } else {
                PathBuf::from(arg)
            };
            state.pending_circular = Some(p);
            return None;
        }
        "sashimi" => {
            state.show_sashimi = match arg {
                "on" | "true" | "1" => true,
                "off" | "false" | "0" => false,
                _ => !state.show_sashimi,
            };
            state.set_status(
                StatusKind::Info,
                format!("sashimi: {}", if state.show_sashimi { "on" } else { "off" }),
            );
            return None;
        }
        "sort" => {
            if let Some(m) = AlignmentSortMode::parse(arg) {
                state.alignment_opts.sort_mode = m;
                state.recompute_all_bams();
                state.set_status(StatusKind::Info, format!("sort: {}", m.name()));
            } else {
                state.set_status(StatusKind::Error, format!("unknown sort mode: {arg}"));
            }
            return None;
        }
        "group" => {
            if let Some(m) = AlignmentGroupMode::parse(arg) {
                state.alignment_opts.group_mode = m;
                state.recompute_all_bams();
                state.set_status(StatusKind::Info, format!("group: {}", m.name()));
            } else {
                state.set_status(StatusKind::Error, format!("unknown group mode: {arg}"));
            }
            return None;
        }
        "color" | "colorBy" | "colorby" => {
            let mut sub = arg.split_whitespace();
            let mode_s = sub.next().unwrap_or("");
            if let Some(m) = AlignmentColorMode::parse(mode_s) {
                state.alignment_opts.color_mode = m;
                if let Some(tag) = sub.next() {
                    state.bam_tag = Some(tag.to_string());
                }
                state.set_status(StatusKind::Info, format!("color: {}", m.name()));
            } else {
                state.set_status(StatusKind::Error, format!("unknown color mode: {arg}"));
            }
            return None;
        }
        "pairs" | "viewaspairs" => {
            state.alignment_opts.view_as_pairs = match arg {
                "on" | "true" | "1" => true,
                "off" | "false" | "0" => false,
                _ => !state.alignment_opts.view_as_pairs,
            };
            state.recompute_all_bams();
            state.set_status(
                StatusKind::Info,
                format!(
                    "view-as-pairs: {}",
                    if state.alignment_opts.view_as_pairs { "on" } else { "off" }
                ),
            );
            return None;
        }
        "softclip" | "softclips" => {
            state.alignment_opts.show_soft_clips = match arg {
                "on" | "true" | "1" => true,
                "off" | "false" | "0" => false,
                _ => !state.alignment_opts.show_soft_clips,
            };
            state.set_status(
                StatusKind::Info,
                format!(
                    "soft-clips: {}",
                    if state.alignment_opts.show_soft_clips { "on" } else { "off" }
                ),
            );
            return None;
        }
        "downsample" => {
            if arg.eq_ignore_ascii_case("off") || arg == "0" {
                state.alignment_opts.max_depth = None;
                state.recompute_all_bams();
                state.set_status(StatusKind::Info, "downsampling: off");
            } else if let Ok(n) = arg.parse::<usize>() {
                state.alignment_opts.max_depth = Some(n);
                state.recompute_all_bams();
                state.set_status(StatusKind::Info, format!("max-depth: {n}"));
            } else {
                state.set_status(StatusKind::Error, format!("bad downsample depth: {arg}"));
            }
            return None;
        }
        "summary" => {
            if let Some(s) = SignalSummary::parse(arg) {
                state.signal_summary = s;
                state.set_status(StatusKind::Info, format!("signal summary: {arg}"));
                let r = state.region.clone();
                return state.set_region_pending(r);
            }
            state.set_status(StatusKind::Error, format!("unknown signal summary: {arg}"));
            return None;
        }
        "translate" => {
            match arg.to_ascii_lowercase().as_str() {
                "off" => state.sequence_opts.show_translation = false,
                "rev" | "reverse" => {
                    state.sequence_opts.show_translation = true;
                    state.sequence_opts.reverse_translation = true;
                }
                "fwd" | "forward" | "on" => {
                    state.sequence_opts.show_translation = true;
                    state.sequence_opts.reverse_translation = false;
                }
                _ => {
                    state.sequence_opts.show_translation = !state.sequence_opts.show_translation;
                }
            }
            state.set_status(
                StatusKind::Info,
                format!(
                    "translation: {}",
                    if state.sequence_opts.show_translation {
                        if state.sequence_opts.reverse_translation {
                            "3-frame (rev)"
                        } else {
                            "3-frame (fwd)"
                        }
                    } else {
                        "off"
                    }
                ),
            );
            return None;
        }
        "strand" => {
            state.sequence_opts.show_complement = match arg {
                "both" | "2" | "on" => true,
                "fwd" | "1" | "off" => false,
                _ => !state.sequence_opts.show_complement,
            };
            state.set_status(
                StatusKind::Info,
                format!(
                    "complement strand: {}",
                    if state.sequence_opts.show_complement { "on" } else { "off" }
                ),
            );
            return None;
        }
        "motif" => {
            if arg.is_empty() || arg.eq_ignore_ascii_case("off") || arg.eq_ignore_ascii_case("clear") {
                state.sequence_opts.motif_pattern = None;
                state.sequence_opts.motif_hits.clear();
                state.set_status(StatusKind::Info, "motif highlight cleared");
            } else {
                state.sequence_opts.motif_pattern = Some(arg.to_string());
                state.recompute_motif_hits();
                let count = state.sequence_opts.motif_hits.len();
                state.set_status(
                    StatusKind::Info,
                    format!("motif '{arg}': {count} hit(s) in window"),
                );
            }
            return None;
        }
        "split" => {
            if state.panes.len() >= 4 {
                state.set_status(StatusKind::Warning, "max 4 split panes supported");
                return None;
            }
            let target = if arg.is_empty() {
                state.region.clone()
            } else {
                match Region::parse(arg) {
                    Ok(r) => r,
                    Err(_) => match state.find_gene_region(arg) {
                        Some((r, _)) => r,
                        None => {
                            state.set_status(StatusKind::Error, format!("bad split locus: {arg}"));
                            return None;
                        }
                    },
                }
            };
            let snap = crate::snapshot::writer::inputs_from_state(state);
            if let Some(slot) = state.pane_snapshots.get_mut(state.active_pane) {
                *slot = Some(snap);
            }
            state.panes.push(target.clone());
            state.pane_snapshots.push(None);
            state.active_pane = state.panes.len() - 1;
            state.set_status(
                StatusKind::Info,
                format!("split pane {}/{}: {target}", state.active_pane + 1, state.panes.len()),
            );
            return state.set_region_pending(target);
        }
        "unsplit" => {
            if state.panes.len() <= 1 {
                state.set_status(StatusKind::Warning, "only 1 pane active");
                return None;
            }
            state.panes.remove(state.active_pane);
            if self_has_slot(&state.pane_snapshots, state.active_pane) {
                state.pane_snapshots.remove(state.active_pane);
            }
            if state.active_pane >= state.panes.len() {
                state.active_pane = state.panes.len() - 1;
            }
            let r = state.panes[state.active_pane].clone();
            state.set_status(
                StatusKind::Info,
                format!("closed pane → {} panes left", state.panes.len()),
            );
            return state.set_region_pending(r);
        }
        "unload" => {
            if arg.is_empty() {
                state.set_status(StatusKind::Error, "usage: :unload <name>");
                return None;
            }
            let removed = unload_by_name(state, arg);
            if removed {
                state.set_status(StatusKind::Info, format!("unloaded matching '{arg}'"));
            } else {
                state.set_status(StatusKind::Warning, format!("no track matched '{arg}'"));
            }
            return None;
        }
        "load" => {
            if arg.is_empty() {
                state.set_status(StatusKind::Error, "usage: :load <path_or_url>");
            } else {
                state.pending_async = Some(AsyncCommand::LoadTrack(arg.to_string()));
            }
            return None;
        }
        "genome" => {
            if arg.is_empty() {
                state.set_status(StatusKind::Error, "usage: :genome <id_or_path>");
            } else {
                state.pending_async = Some(AsyncCommand::LoadGenome(arg.to_string()));
            }
            return None;
        }
        "save-session" | "savesession" => {
            let p = if arg.is_empty() {
                PathBuf::from("igv-session.toml")
            } else {
                PathBuf::from(arg)
            };
            state.pending_async = Some(AsyncCommand::SaveSession(p));
            return None;
        }
        "load-session" | "loadsession" => {
            if arg.is_empty() {
                state.set_status(StatusKind::Error, "usage: :load-session <path>");
            } else {
                state.pending_async = Some(AsyncCommand::LoadSession(PathBuf::from(arg)));
            }
            return None;
        }
        "blat" => {
            if arg.len() < 15 {
                state.set_status(StatusKind::Error, "BLAT requires >= 15 bp DNA sequence");
            } else {
                state.pending_async = Some(AsyncCommand::BlatQuery(arg.to_string()));
            }
            return None;
        }
        "graphics" => {
            if arg.eq_ignore_ascii_case("toggle") {
                state.graphics_overlay = !state.graphics_overlay;
            } else if let Some(p) = TerminalGraphicsProtocol::parse(arg) {
                state.graphics_protocol = p;
                state.graphics_overlay = !matches!(p, TerminalGraphicsProtocol::None);
            } else {
                state.set_status(
                    StatusKind::Error,
                    format!("unknown graphics protocol: {arg} (auto|none|kitty|iterm2|sixel)"),
                );
                return None;
            }
            state.set_status(
                StatusKind::Info,
                format!(
                    "graphics: {} ({})",
                    state.graphics_protocol.name(),
                    if state.graphics_overlay { "on" } else { "off" }
                ),
            );
            return None;
        }
        _ => {}
    }

    // Fallback: chromosome alias or locus OR gene name
    let avail: Vec<String> = state.references.iter().map(|r| r.name.clone()).collect();
    let parse = Region::parse(trimmed);
    let resolved_known = parse
        .as_ref()
        .ok()
        .and_then(|r| state.chrom_aliases.resolve(&r.chrom, &avail).map(|_| r.clone()));
    match (parse, resolved_known) {
        (Ok(_), Some(r)) => state.set_region_pending(r),
        (parse_result, _) => match state.find_gene_region(trimmed) {
            Some((region, label)) => {
                state.set_status(StatusKind::Info, format!("jump → {label}"));
                state.set_region_pending(region)
            }
            None => {
                let msg = match parse_result {
                    Ok(r) => format!("unknown chrom, gene, or command: {}", r.chrom),
                    Err(_) if state.annotations.is_empty() => format!("invalid command or region: {trimmed}"),
                    Err(_) => format!("not a region, gene, or command: {trimmed}"),
                };
                state.set_status(StatusKind::Error, msg);
                None
            }
        },
    }
}

fn self_has_slot(v: &[Option<RenderInputs>], idx: usize) -> bool {
    idx < v.len()
}

fn unload_by_name(state: &mut AppState, needle: &str) -> bool {
    let n = needle.to_ascii_lowercase();
    let mut removed = false;
    if let Some(i) = state
        .bams
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.bams.remove(i);
        state.ensure_track_vecs();
        removed = true;
    } else if let Some(i) = state
        .vcf_tracks
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.vcf_tracks.remove(i);
        if state.vcf_tracks.is_empty() {
            state.vcf = None;
            state.variants.clear();
        }
        state.ensure_track_vecs();
        removed = true;
    } else if let Some(i) = state
        .annotations
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.annotations.remove(i);
        state.ensure_track_vecs();
        removed = true;
    } else if let Some(i) = state
        .signals
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.signals.remove(i);
        state.ensure_track_vecs();
        removed = true;
    } else if let Some(i) = state
        .links
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.links.remove(i);
        state.ensure_track_vecs();
        removed = true;
    } else if let Some(i) = state
        .segs
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.segs.remove(i);
        state.ensure_track_vecs();
        removed = true;
    } else if let Some(i) = state
        .gwas
        .iter()
        .position(|t| t.display.to_ascii_lowercase().contains(&n))
    {
        state.gwas.remove(i);
        state.ensure_track_vecs();
        removed = true;
    }
    removed
}

/// Query UCSC hgBlat JSON web API and return `(top_region, summary_popup)`.
pub async fn run_blat_query(
    db: &str,
    user_seq: &str,
) -> anyhow::Result<(Option<Region>, InspectorPopup)> {
    let url = format!(
        "https://genome.ucsc.edu/cgi-bin/hgBlat?userSeq={}&type=DNA&db={}&output=json",
        user_seq.trim(),
        db
    );
    let resp = reqwest::get(&url).await?;
    let val: serde_json::Value = resp.json().await?;
    let mut rows = Vec::new();
    let mut top_region = None;
    if let Some(arr) = val.get("blat").and_then(|v| v.as_array()) {
        for (idx, hit) in arr.iter().take(8).enumerate() {
            if let Some(cols) = hit.as_array() {
                let matches = cols.first().and_then(|v| v.as_u64()).unwrap_or(0);
                let chrom = cols.get(13).and_then(|v| v.as_str()).unwrap_or("");
                let t_start = cols.get(15).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
                let t_end = cols.get(16).and_then(|v| v.as_u64()).unwrap_or(t_start);
                if idx == 0 && !chrom.is_empty() {
                    top_region = Region::new(chrom, t_start, t_end.max(t_start)).ok();
                }
                rows.push((
                    format!("Hit #{}", idx + 1),
                    format!("{chrom}:{t_start}-{t_end} ({matches} matches)"),
                ));
            }
        }
    }
    if rows.is_empty() {
        rows.push(("Result".into(), format!("No BLAT hits returned for {db}")));
    }
    Ok((
        top_region,
        InspectorPopup {
            title: format!("UCSC BLAT ({db})"),
            rows,
        },
    ))
}
