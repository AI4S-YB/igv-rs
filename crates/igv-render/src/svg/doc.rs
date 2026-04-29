//! SVG document builder. Lives behind a small typed API so we can swap
//! the backing string concatenation for the `svg` crate later without
//! touching per-track code.

use std::fmt::Write;

use crate::theme::Rgb;

#[derive(Debug)]
pub struct SvgDoc {
    body: String,
}

impl SvgDoc {
    pub fn new(width: u32, height: u32, bg: Rgb, font_family: &str) -> Self {
        let mut body = String::new();
        writeln!(body, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
        writeln!(
            body,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="{}">"#,
            width,
            height,
            width,
            height,
            escape_xml(font_family)
        )
        .unwrap();
        writeln!(
            body,
            r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
            width,
            height,
            bg.hex()
        )
        .unwrap();
        Self { body }
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: Rgb) {
        writeln!(
            self.body,
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"/>"#,
            x,
            y,
            w,
            h,
            fill.hex()
        )
        .unwrap();
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, stroke: Rgb, stroke_w: f64) {
        writeln!(
            self.body,
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{:.2}"/>"#,
            x1,
            y1,
            x2,
            y2,
            stroke.hex(),
            stroke_w
        )
        .unwrap();
    }

    pub fn text(
        &mut self,
        x: f64,
        y: f64,
        text: &str,
        fill: Rgb,
        font_px: u32,
        anchor: TextAnchor,
    ) {
        writeln!(
            self.body,
            r#"<text x="{:.2}" y="{:.2}" fill="{}" font-size="{}" text-anchor="{}">{}</text>"#,
            x,
            y,
            fill.hex(),
            font_px,
            anchor.as_str(),
            escape_xml(text)
        )
        .unwrap();
    }

    pub fn polygon(&mut self, points: &[(f64, f64)], fill: Rgb) {
        let mut buf = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i > 0 {
                buf.push(' ');
            }
            write!(buf, "{:.2},{:.2}", x, y).unwrap();
        }
        writeln!(self.body, r#"<polygon points="{}" fill="{}"/>"#, buf, fill.hex()).unwrap();
    }

    pub fn path(&mut self, d: &str, stroke: Rgb, stroke_w: f64, fill: Option<Rgb>) {
        let fill_attr = match fill {
            Some(f) => format!(r#"fill="{}""#, f.hex()),
            None => "fill=\"none\"".to_string(),
        };
        writeln!(
            self.body,
            r#"<path d="{}" stroke="{}" stroke-width="{:.2}" {}/>"#,
            d,
            stroke.hex(),
            stroke_w,
            fill_attr,
        )
        .unwrap();
    }

    pub fn finish(mut self) -> String {
        self.body.push_str("</svg>\n");
        self.body
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

impl TextAnchor {
    fn as_str(self) -> &'static str {
        match self {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        }
    }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}
