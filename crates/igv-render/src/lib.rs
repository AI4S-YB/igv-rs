//! Graphical (SVG / PNG) renderer for igv-rs snapshots.

#![forbid(unsafe_code)]

pub mod error;
pub mod options;
pub mod theme;

pub use error::RenderError;
pub use options::{SvgOptions, TrackHeights};
pub use theme::GraphicalTheme;
