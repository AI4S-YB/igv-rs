//! Graphical (SVG / PNG / Circular / Inline Terminal Graphics) renderer for igv-rs snapshots.

#![forbid(unsafe_code)]

pub mod circview;
pub mod error;
pub mod layout;
pub mod options;
pub mod png;
pub mod svg;
pub mod term_gfx;
pub mod theme;

pub use circview::{render_circular_png, render_circular_svg, CircularOptions};
pub use error::RenderError;
pub use options::{SvgOptions, TrackHeights};
pub use term_gfx::{
    detect_protocol, encode_iterm2_png, encode_kitty_png, encode_sixel_rgba,
    render_terminal_escape, TerminalGraphicsProtocol,
};
pub use theme::GraphicalTheme;

pub fn render_svg(inputs: &igv_core::render_inputs::RenderInputs, opts: &SvgOptions) -> String {
    svg::render(inputs, opts)
}

pub fn render_png(
    inputs: &igv_core::render_inputs::RenderInputs,
    opts: &SvgOptions,
) -> Result<Vec<u8>, RenderError> {
    png::render(inputs, opts)
}
