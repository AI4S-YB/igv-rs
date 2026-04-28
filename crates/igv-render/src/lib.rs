//! Graphical (SVG / PNG) renderer for igv-rs snapshots.
//!
//! Consumes `igv_core::render::RenderInputs` and emits an SVG string or a
//! PNG byte buffer. The same data shape feeds both the interactive
//! snapshot key (`S`) and the headless batch CLI (`--snapshot-bed`,
//! `--snapshot-genes`).

#![forbid(unsafe_code)]
