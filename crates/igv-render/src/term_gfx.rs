//! Pure-Rust inline terminal pixel graphics encoders: Kitty, iTerm2, and Sixel.

use std::fmt::Write;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use igv_core::render_inputs::RenderInputs;

use crate::error::RenderError;
use crate::options::SvgOptions;
use crate::png::{svg_to_pixmap, svg_to_png};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalGraphicsProtocol {
    #[default]
    Auto,
    None,
    Kitty,
    Iterm2,
    Sixel,
}

impl TerminalGraphicsProtocol {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "none" | "off" => Some(Self::None),
            "kitty" => Some(Self::Kitty),
            "iterm2" | "iterm" => Some(Self::Iterm2),
            "sixel" => Some(Self::Sixel),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Kitty => "kitty",
            Self::Iterm2 => "iterm2",
            Self::Sixel => "sixel",
        }
    }
}

/// Inspect terminal environment variables to detect inline graphics support.
pub fn detect_protocol() -> TerminalGraphicsProtocol {
    if std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var("TERM")
            .ok()
            .is_some_and(|t| t.contains("kitty"))
    {
        return TerminalGraphicsProtocol::Kitty;
    }
    if let Ok(tp) = std::env::var("TERM_PROGRAM") {
        if matches!(tp.as_str(), "iTerm.app" | "WezTerm" | "ghostty") {
            return TerminalGraphicsProtocol::Iterm2;
        }
    }
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("mlterm") || term.contains("foot") || term.contains("yaft") {
            return TerminalGraphicsProtocol::Sixel;
        }
    }
    TerminalGraphicsProtocol::None
}

/// Encode PNG bytes using the Kitty graphics protocol (4096-byte chunked base64).
pub fn encode_kitty_png(png_bytes: &[u8]) -> String {
    let b64 = STANDARD.encode(png_bytes);
    let bytes = b64.as_bytes();
    let chunks: Vec<&[u8]> = bytes.chunks(4096).collect();
    let mut out = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = if i + 1 < chunks.len() { 1 } else { 0 };
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            let _ = write!(out, "\x1b_Ga=T,f=100,m={more};{chunk_str}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more};{chunk_str}\x1b\\");
        }
    }
    if chunks.is_empty() {
        out.push_str("\x1b_Ga=T,f=100,m=0;\x1b\\");
    }
    out
}

/// Encode PNG bytes using iTerm2 OSC 1337 inline image escape.
pub fn encode_iterm2_png(png_bytes: &[u8], cols: u16, rows: u16) -> String {
    let b64 = STANDARD.encode(png_bytes);
    format!(
        "\x1b]1337;File=inline=1;width={cols};height={rows};preserveAspectRatio=1:{b64}\x07"
    )
}

/// Pure-Rust Sixel encoder using a 6×6×6 RGB cube palette (216 colors) and
/// 6-row band RLE (`!...` repeat compression). Expects RGBA8 pixel buffer.
pub fn encode_sixel_rgba(rgba: &[u8], width: u32, height: u32) -> String {
    if width == 0 || height == 0 || rgba.len() < (width as usize * height as usize * 4) {
        return String::new();
    }

    let w = width as usize;
    let h = height as usize;

    // Map pixel RGB to 6×6×6 color index (0..=215)
    let mut indexed = vec![0u8; w * h];
    let mut used = [false; 216];
    for (idx, px) in indexed.iter_mut().enumerate() {
        let o = idx * 4;
        let a = rgba[o + 3] as u32;
        let (r, g, b) = if a == 255 {
            (rgba[o], rgba[o + 1], rgba[o + 2])
        } else if a == 0 {
            (255, 255, 255)
        } else {
            (
                ((rgba[o] as u32 * a + 255 * (255 - a)) / 255) as u8,
                ((rgba[o + 1] as u32 * a + 255 * (255 - a)) / 255) as u8,
                ((rgba[o + 2] as u32 * a + 255 * (255 - a)) / 255) as u8,
            )
        };
        let ri = ((r as u16 * 5 + 127) / 255) as u8;
        let gi = ((g as u16 * 5 + 127) / 255) as u8;
        let bi = ((b as u16 * 5 + 127) / 255) as u8;
        let ci = ri * 36 + gi * 6 + bi;
        *px = ci;
        used[ci as usize] = true;
    }

    let mut out = String::from("\x1bPq");
    let _ = write!(out, "\"1;1;{w};{h}");

    // Emit used palette entries (Sixel percentages 0..100)
    for (ci, &is_used) in used.iter().enumerate() {
        if !is_used {
            continue;
        }
        let r_pct = ((ci / 36) * 20) as u32;
        let g_pct = (((ci / 6) % 6) * 20) as u32;
        let b_pct = ((ci % 6) * 20) as u32;
        let _ = write!(out, "#{ci};2;{r_pct};{g_pct};{b_pct}");
    }

    // Emit 6-pixel vertical bands
    let mut y = 0usize;
    while y < h {
        let mut first_in_band = true;
        for (ci, &is_used) in used.iter().enumerate() {
            if !is_used {
                continue;
            }
            let c_u8 = ci as u8;
            let mut row_masks = vec![0u8; w];
            let mut any_hit = false;
            for bit in 0..6 {
                let py = y + bit;
                if py >= h {
                    break;
                }
                let row_off = py * w;
                for x in 0..w {
                    if indexed[row_off + x] == c_u8 {
                        row_masks[x] |= 1 << bit;
                        any_hit = true;
                    }
                }
            }
            if !any_hit {
                continue;
            }
            if !first_in_band {
                out.push('$'); // carriage return within same 6-row band
            }
            first_in_band = false;
            let _ = write!(out, "#{ci}");
            emit_rle_sixel(&mut out, &row_masks);
        }
        y += 6;
        if y < h {
            out.push('-'); // graphics newline (advance 6 rows)
        }
    }
    out.push_str("\x1b\\");
    out
}

fn emit_rle_sixel(out: &mut String, masks: &[u8]) {
    let mut i = 0;
    while i < masks.len() {
        let v = masks[i];
        let mut run = 1usize;
        while i + run < masks.len() && masks[i + run] == v {
            run += 1;
        }
        let ch = (v + 63) as char;
        if run >= 4 {
            let _ = write!(out, "!{run}{ch}");
        } else {
            for _ in 0..run {
                out.push(ch);
            }
        }
        i += run;
    }
}

/// Render `inputs` to an inline terminal escape sequence according to `proto`
/// (`None` if disabled or unresolved under `Auto`).
pub fn render_terminal_escape(
    inputs: &RenderInputs,
    opts: &SvgOptions,
    proto: TerminalGraphicsProtocol,
    cols: u16,
    rows: u16,
) -> Result<Option<String>, RenderError> {
    let resolved = match proto {
        TerminalGraphicsProtocol::Auto => detect_protocol(),
        other => other,
    };
    match resolved {
        TerminalGraphicsProtocol::None | TerminalGraphicsProtocol::Auto => Ok(None),
        TerminalGraphicsProtocol::Kitty => {
            let svg = crate::svg::render(inputs, opts);
            let png = svg_to_png(&svg)?;
            Ok(Some(encode_kitty_png(&png)))
        }
        TerminalGraphicsProtocol::Iterm2 => {
            let svg = crate::svg::render(inputs, opts);
            let png = svg_to_png(&svg)?;
            Ok(Some(encode_iterm2_png(&png, cols, rows)))
        }
        TerminalGraphicsProtocol::Sixel => {
            let svg = crate::svg::render(inputs, opts);
            let pm = svg_to_pixmap(&svg)?;
            Ok(Some(encode_sixel_rgba(pm.data(), pm.width(), pm.height())))
        }
    }
}
