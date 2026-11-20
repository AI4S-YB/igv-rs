//! Build a RenderInputs from AppState and write SVG/PNG/Circular snapshots.

use std::path::Path;

use anyhow::{Context, Result};
use igv_core::render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, GwasTrackSnapshot, LinkTrackSnapshot, RenderInputs,
    SegTrackSnapshot, SignalTrackSnapshot, VcfTrackSnapshot,
};
use igv_core::source::link::LinkRecord;
use igv_core::source::seg::SegInterval;
use igv_render::{
    render_circular_png, render_circular_svg, render_png, render_svg, CircularOptions, SvgOptions,
};

use crate::app::action::SnapshotFormat;
use crate::app::state::AppState;

/// Build a `RenderInputs` snapshot from the current TUI state.
pub fn inputs_from_state(state: &AppState) -> RenderInputs {
    let bams = state
        .bams
        .iter()
        .enumerate()
        .map(|(i, t)| BamTrackSnapshot {
            display: t.display.clone(),
            rows: state.bam_rows.get(i).cloned().unwrap_or_default(),
            lanes: state.bam_lanes.get(i).cloned().unwrap_or_default(),
            total_lanes: state.bam_total_lanes.get(i).copied().unwrap_or(0),
            bands: state.bam_bands.get(i).cloned().unwrap_or_default(),
            mate_links: state.bam_mate_links.get(i).cloned().unwrap_or_default(),
            downsampled: state.bam_downsampled.get(i).copied().unwrap_or(0),
            junctions: state.bam_junctions.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    let vcf_tracks = state
        .vcf_tracks
        .iter()
        .enumerate()
        .map(|(i, t)| VcfTrackSnapshot {
            display: t.display.clone(),
            sample_names: t.source.sample_names(),
            records: state.vcf_track_rows.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    let annotations = state
        .annotations
        .iter()
        .enumerate()
        .map(|(i, t)| AnnotationTrackSnapshot {
            display: t.display.clone(),
            transcripts: state.annotation_rows.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    let signals = state
        .signals
        .iter()
        .enumerate()
        .map(|(i, t)| SignalTrackSnapshot {
            display: t.display.clone(),
            bins: state.signal_bins.get(i).cloned().unwrap_or_default(),
        })
        .collect();
    let links = state
        .links
        .iter()
        .enumerate()
        .map(|(i, t)| LinkTrackSnapshot {
            display: t.display.clone(),
            visible: state.link_records.get(i).cloned().unwrap_or_default(),
            total_record_count: t.source.record_count(),
        })
        .collect();
    let segs = state
        .segs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let data = state.seg_data.get(i).cloned().unwrap_or_default();
            SegTrackSnapshot {
                display: t.display.clone(),
                samples: if data.samples.is_empty() {
                    t.source.samples()
                } else {
                    data.samples
                },
                segments: data.segments,
            }
        })
        .collect();
    let gwas = state
        .gwas
        .iter()
        .enumerate()
        .map(|(i, t)| GwasTrackSnapshot {
            display: t.display.clone(),
            points: state.gwas_points.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    RenderInputs {
        region: state.region.clone(),
        references: state.references.clone(),
        cytobands: state.cytobands.clone(),
        reference_seq: state.reference_seq.clone(),
        sequence_opts: state.sequence_opts.clone(),
        variants: state.variants.clone(),
        vcf_tracks,
        bams,
        alignment_opts: state.alignment_opts.clone(),
        annotations,
        signals,
        links,
        segs,
        gwas,
        render_mode: state.render_mode(),
    }
}

fn signal_shared_max(state: &AppState) -> Option<f32> {
    if !state.signal_shared_scale {
        return None;
    }
    let m = state
        .signal_bins
        .iter()
        .flatten()
        .map(|b| b.value)
        .fold(0.0_f32, f32::max);
    Some(m)
}

pub fn svg_options_for_state(state: &AppState) -> SvgOptions {
    SvgOptions {
        signal_shared_max: signal_shared_max(state),
        show_ideogram: !state.cytobands.is_empty(),
        show_sequence: state.sequence_opts.show_translation
            || state.sequence_opts.show_complement
            || !state.sequence_opts.motif_hits.is_empty(),
        show_coverage_snv: true,
        show_sashimi: state.show_sashimi,
        ..Default::default()
    }
}

pub fn write_snapshot(state: &AppState, path: &Path, format: SnapshotFormat) -> Result<()> {
    let inputs = inputs_from_state(state);
    let opts = svg_options_for_state(state);
    match format {
        SnapshotFormat::Svg => {
            let svg = render_svg(&inputs, &opts);
            std::fs::write(path, svg).with_context(|| format!("write {}", path.display()))?;
        }
        SnapshotFormat::Png => {
            let bytes = render_png(&inputs, &opts).with_context(|| "render PNG")?;
            std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
        }
    }
    Ok(())
}

pub fn write_circular_snapshot(state: &AppState, path: &Path) -> Result<()> {
    let mut all_links: Vec<LinkRecord> = Vec::new();
    for t in &state.links {
        all_links.extend(t.source.all_records());
    }
    let mut all_segs: Vec<SegInterval> = Vec::new();
    for d in &state.seg_data {
        all_segs.extend(d.segments.iter().cloned());
    }
    let opts = CircularOptions {
        title: state.genome_id.clone(),
        ..Default::default()
    };
    let fmt = SnapshotFormat::from_path(path);
    match fmt {
        SnapshotFormat::Svg => {
            let svg = render_circular_svg(&state.references, &all_links, &all_segs, &opts);
            std::fs::write(path, svg).with_context(|| format!("write {}", path.display()))?;
        }
        SnapshotFormat::Png => {
            let bytes = render_circular_png(&state.references, &all_links, &all_segs, &opts)?;
            std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
        }
    }
    Ok(())
}
