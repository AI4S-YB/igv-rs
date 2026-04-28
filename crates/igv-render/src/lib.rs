//! Graphical (SVG / PNG) renderer for igv-rs snapshots.

#![forbid(unsafe_code)]

pub mod error;
pub mod layout;
pub mod options;
pub mod svg;
pub mod theme;

pub use error::RenderError;
pub use options::{SvgOptions, TrackHeights};
pub use theme::GraphicalTheme;

pub fn render_svg(inputs: &igv_core::render_inputs::RenderInputs, opts: &SvgOptions) -> String {
    svg::render(inputs, opts)
}
