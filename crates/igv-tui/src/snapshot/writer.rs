//! Build a RenderInputs from AppState and write SVG/PNG.

use std::path::Path;

use anyhow::{Context, Result};
use igv_core::render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, RenderInputs, SignalTrackSnapshot,
};
use igv_render::{render_png, render_svg, SvgOptions};

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
    RenderInputs {
        region: state.region.clone(),
        references: state.references.clone(),
        reference_seq: state.reference_seq.clone(),
        variants: state.variants.clone(),
        bams,
        annotations,
        signals,
        links: Vec::new(),
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

pub fn write_snapshot(state: &AppState, path: &Path, format: SnapshotFormat) -> Result<()> {
    let inputs = inputs_from_state(state);
    let mut opts = SvgOptions::default();
    opts.signal_shared_max = signal_shared_max(state);
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
