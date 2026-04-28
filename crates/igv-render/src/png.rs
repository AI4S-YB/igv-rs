//! PNG output: render the SVG, parse with usvg, raster with resvg into
//! a tiny-skia Pixmap, encode PNG.

use igv_core::render_inputs::RenderInputs;

use crate::error::RenderError;
use crate::options::SvgOptions;

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> Result<Vec<u8>, RenderError> {
    let svg = crate::svg::render(inputs, opts);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())
        .map_err(|e| RenderError::UsvgParse(e.to_string()))?;
    let size = tree.size();
    let w = size.width().ceil() as u32;
    let h = size.height().ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| RenderError::PngEncode("pixmap alloc failed".into()))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|e| RenderError::PngEncode(e.to_string()))
}
