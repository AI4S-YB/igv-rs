//! Core data layer for igv-rs: region types, async data sources, alignment
//! processing, coverage, and rendering thresholds. UI-free.

#![warn(rust_2018_idioms, missing_debug_implementations)]

pub mod alignment;
pub mod collect;
pub mod coverage;
pub mod error;
pub mod region;
pub mod render;
pub mod render_inputs;
pub mod source;

pub use collect::{collect_render_inputs, CollectOpts, Sources};
pub use error::{IgvError, Result};
pub use region::Region;
pub use render_inputs::{
    AnnotationTrackSnapshot, BamTrackSnapshot, LinkTrackSnapshot, RenderInputs,
    SignalTrackSnapshot,
};
