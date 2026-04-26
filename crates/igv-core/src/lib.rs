//! Core data layer for igv-rs: region types, async data sources, alignment
//! processing, coverage, and rendering thresholds. UI-free.

#![warn(rust_2018_idioms, missing_debug_implementations)]

pub mod alignment;
pub mod coverage;
pub mod error;
pub mod region;
pub mod render;
pub mod source;

pub use error::{IgvError, Result};
// TODO: re-enable once underlying types are introduced in later tasks.
// pub use region::Region;
