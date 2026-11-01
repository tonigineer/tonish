//! Text on one line, without shaping: each character is its font's glyph
//! for it, set at its advance. That is right for the Latin figures and
//! labels a panel shows, not for scripts that join or reorder their letters.

use std::collections::HashMap;

use crate::scene::{Point, Rgba};
use crate::shapes::Text;

pub struct Font {
    font: fontdue::Font,
    /// Rasterised once per character and pixel size, because text that is
    /// drawn again, such as figures that change every second, is mostly the
    /// same characters.
    glyphs: HashMap<(char, u32), (fontdue::Metrics, Vec<u8>)>,
}

impl Font {
    /// `bytes` is a TrueType or OpenType font.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())?;
        Ok(Self {
            font,
            glyphs: HashMap::new(),
        })
    }

    /// The height of a line of `size`, in logical pixels.
    pub fn line_height(&self, size: f32) -> f32 {
        self.font
            .horizontal_line_metrics(size)
            .map_or(size * 1.2, |m| m.new_line_size)
    }

    /// `text` with its top-left corner at `at`, `size` logical pixels to the
    /// em, rasterised at `scale` and on whole physical pixels, so that it is
    /// as sharp as the font makes it.
    pub fn text(&mut self, text: &str, at: Point, size: f32, scale: f32, color: Rgba) -> Text {
        let px = size * scale;
        let (ascent, height) = match self.font.horizontal_line_metrics(px) {
            Some(m) => (
                m.ascent.round() as i32,
                (m.ascent - m.descent).ceil() as i32,
            ),
            None => (px.round() as i32, (px * 1.2).ceil() as i32),
        };
        let mut placed = Vec::new();
        let mut pen = 0.0f32;
        for ch in text.chars() {
            let (metrics, _) = self.glyph(ch, px);
            let x = pen.round() as i32 + metrics.xmin;
            let y = ascent - metrics.ymin - metrics.height as i32;
            placed.push((ch, x, y));
            pen += metrics.advance_width;
        }
        let width = (pen.ceil() as i32).max(1) as usize;
        let height = height.max(1) as usize;
        let mut coverage = vec![0u8; width * height];
        for (ch, x0, y0) in placed {
            let (metrics, bitmap) = self.glyph(ch, px);
            for gy in 0..metrics.height {
                for gx in 0..metrics.width {
                    let (x, y) = (x0 + gx as i32, y0 + gy as i32);
                    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
                        continue;
                    }
                    let c = &mut coverage[y as usize * width + x as usize];
                    // The larger, where two glyphs overlap, as with a
                    // letter's tail under the next one.
                    *c = (*c).max(bitmap[gy * metrics.width + gx]);
                }
            }
        }
        Text {
            origin: ((at.x * scale).round() as i32, (at.y * scale).round() as i32),
            size: (width, height),
            coverage,
            scale,
            color,
        }
    }

    fn glyph(&mut self, ch: char, px: f32) -> &(fontdue::Metrics, Vec<u8>) {
        let font = &self.font;
        self.glyphs
            .entry((ch, px.to_bits()))
            .or_insert_with(|| font.rasterize(ch, px))
    }
}
