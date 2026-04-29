//! RGB-based theme. Independent of crossterm `Style` — the SVG world
//! does not have ANSI/named colors, only hex.

#[derive(Debug, Clone, Copy)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone)]
pub struct GraphicalTheme {
    pub bg: Rgb,
    pub fg: Rgb,
    pub muted: Rgb,
    pub ruler_text: Rgb,
    pub transcript_exon: Rgb,
    pub transcript_intron: Rgb,
    pub transcript_label: Rgb,
    pub variant_snv: Rgb,
    pub variant_indel: Rgb,
    pub coverage_bar: Rgb,
    pub signal_bar: Rgb,
    pub link_color: Rgb,
    pub link_gradient: [Rgb; 5],
    pub read_forward: Rgb,
    pub read_reverse: Rgb,
    pub mismatch_a: Rgb,
    pub mismatch_c: Rgb,
    pub mismatch_g: Rgb,
    pub mismatch_t: Rgb,
    pub mismatch_n: Rgb,
    pub font_family: &'static str,
    pub font_px_small: u32,
    pub font_px_normal: u32,
    pub font_px_label: u32,
}

impl GraphicalTheme {
    pub fn igv_light() -> Self {
        Self {
            bg: Rgb(0xff, 0xff, 0xff),
            fg: Rgb(0x1a, 0x1a, 0x1a),
            muted: Rgb(0x88, 0x88, 0x88),
            ruler_text: Rgb(0x44, 0x44, 0x44),
            transcript_exon: Rgb(0x1f, 0x3b, 0x73),
            transcript_intron: Rgb(0x77, 0x77, 0x77),
            transcript_label: Rgb(0x1a, 0x1a, 0x1a),
            variant_snv: Rgb(0xc0, 0x39, 0x2b),
            variant_indel: Rgb(0x7d, 0x3c, 0x98),
            coverage_bar: Rgb(0x88, 0x88, 0x88),
            signal_bar: Rgb(0x1f, 0x4e, 0x79),
            link_color: Rgb(0x6a, 0x3d, 0x9a),
            link_gradient: [
                Rgb(0xfd, 0xe7, 0x25), // viridis-low (yellow)
                Rgb(0x7a, 0xd1, 0x51),
                Rgb(0x21, 0x90, 0x8d),
                Rgb(0x44, 0x47, 0x8c),
                Rgb(0x44, 0x01, 0x54), // viridis-high (purple)
            ],
            read_forward: Rgb(0x9e, 0xc3, 0xe0),
            read_reverse: Rgb(0xe8, 0xb6, 0xb6),
            mismatch_a: Rgb(0x2c, 0xa0, 0x2c),
            mismatch_c: Rgb(0x1f, 0x77, 0xb4),
            mismatch_g: Rgb(0xff, 0x7f, 0x0e),
            mismatch_t: Rgb(0xd6, 0x27, 0x28),
            mismatch_n: Rgb(0x88, 0x88, 0x88),
            font_family: "DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif",
            font_px_small: 10,
            font_px_normal: 12,
            font_px_label: 14,
        }
    }

    /// Color for a mismatch base. Returns `mismatch_n` for unknown bases.
    pub fn mismatch_color(&self, base: u8) -> Rgb {
        match base.to_ascii_uppercase() {
            b'A' => self.mismatch_a,
            b'C' => self.mismatch_c,
            b'G' => self.mismatch_g,
            b'T' => self.mismatch_t,
            _ => self.mismatch_n,
        }
    }

    /// Sample the `link_gradient` at `t ∈ [0, 1]`. Clamps; intermediate
    /// values lerp between adjacent stops.
    pub fn link_color_at(&self, t: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let stops = self.link_gradient.len();
        let scaled = t * (stops as f64 - 1.0);
        let idx = scaled.floor() as usize;
        let frac = scaled - idx as f64;
        if idx + 1 >= stops {
            return self.link_gradient[stops - 1];
        }
        let a = self.link_gradient[idx];
        let b = self.link_gradient[idx + 1];
        let lerp = |x: u8, y: u8| -> u8 {
            (x as f64 + (y as f64 - x as f64) * frac).round() as u8
        };
        Rgb(lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_pads_short_components() {
        assert_eq!(Rgb(0, 1, 254).hex(), "#0001fe");
    }

    #[test]
    fn igv_light_returns_white_bg() {
        let t = GraphicalTheme::igv_light();
        assert_eq!(t.bg.hex(), "#ffffff");
    }

    #[test]
    fn mismatch_color_falls_back_to_n_for_unknown() {
        let t = GraphicalTheme::igv_light();
        assert_eq!(t.mismatch_color(b'X').hex(), t.mismatch_n.hex());
        assert_eq!(t.mismatch_color(b'a').hex(), t.mismatch_a.hex());
    }

    #[test]
    fn link_gradient_endpoints_match() {
        let t = GraphicalTheme::igv_light();
        assert_eq!(t.link_color_at(0.0).hex(), t.link_gradient[0].hex());
        assert_eq!(t.link_color_at(1.0).hex(), t.link_gradient[4].hex());
    }

    #[test]
    fn link_gradient_midpoint_lerps_between_stops() {
        let t = GraphicalTheme::igv_light();
        // t=0.5 lands on a 5-stop ramp's middle stop (index 2).
        assert_eq!(t.link_color_at(0.5).hex(), t.link_gradient[2].hex());
    }
}
