//! Core data layer for igv-rs: region types, async data sources, alignment
//! processing, coverage, and rendering thresholds. UI-free.

#![warn(rust_2018_idioms, missing_debug_implementations)]

pub mod alignment;
pub mod coverage;
pub mod error;
pub mod region;
pub mod render;
pub mod source;

// TODO: re-enable once underlying types are introduced in later tasks.
// pub use error::{IgvError, Result};
// pub use region::Region;
