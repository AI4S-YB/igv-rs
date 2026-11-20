//! PNG rasterization pipeline: render SVG, parse with usvg, raster with resvg into
//! a tiny-skia Pixmap, and encode PNG (also shared by circular view & Sixel/Kitty/iTerm2).

use igv_core::render_inputs::RenderInputs;

use crate::error::RenderError;
use crate::options::SvgOptions;

pub fn svg_to_pixmap(svg: &str) -> Result<tiny_skia::Pixmap, RenderError> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())
        .map_err(|e| RenderError::UsvgParse(e.to_string()))?;
    let size = tree.size();
    let w = size.width().ceil().max(1.0) as u32;
    let h = size.height().ceil().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| RenderError::PngEncode("pixmap alloc failed".into()))?;
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Ok(pixmap)
}

pub fn svg_to_png(svg: &str) -> Result<Vec<u8>, RenderError> {
    let pixmap = svg_to_pixmap(svg)?;
    pixmap
        .encode_png()
        .map_err(|e| RenderError::PngEncode(e.to_string()))
}

pub fn render(inputs: &RenderInputs, opts: &SvgOptions) -> Result<Vec<u8>, RenderError> {
    let svg = crate::svg::render(inputs, opts);
    svg_to_png(&svg)
}
