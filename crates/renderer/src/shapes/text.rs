use super::Shape;
use crate::scene::{Point, Rgba};

/// A line of text, as the coverage its font gave each physical pixel, made
/// by [`Font::text`](crate::text::Font::text) for one output scale.
///
/// Its distance is not a distance: it is the one value that makes the
/// rasteriser's coverage of each pixel the glyph's own, which keeps text as
/// the font drew it, without a second way of drawing. So an [`Outline`] of it
/// means nothing. The shader has no glyphs, so a frame with text is drawn on
/// the CPU even on the GPU, which suits small surfaces such as figures in a
/// corner, and not a large one that animates.
///
/// [`Outline`]: super::Outline
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    /// The physical pixel the coverage starts at.
    pub(crate) origin: (i32, i32),
    /// Physical pixels.
    pub(crate) size: (usize, usize),
    pub(crate) coverage: Vec<u8>,
    pub(crate) scale: f32,
    pub color: Rgba,
}

impl Text {
    /// Its width and height in logical pixels.
    pub fn extent(&self) -> (f32, f32) {
        (
            self.size.0 as f32 / self.scale,
            self.size.1 as f32 / self.scale,
        )
    }
}

impl Shape for Text {
    fn bounds(&self) -> [Point; 2] {
        let (x, y) = (self.origin.0 as f32, self.origin.1 as f32);
        [
            Point::new(x / self.scale, y / self.scale),
            Point::new(
                (x + self.size.0 as f32) / self.scale,
                (y + self.size.1 as f32) / self.scale,
            ),
        ]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    /// The rasteriser samples the centres of physical pixels, which this
    /// maps back to the pixel they belong to.
    fn distance(&self, p: Point) -> f32 {
        let x = (p.x * self.scale - 0.5).round() as i32 - self.origin.0;
        let y = (p.y * self.scale - 0.5).round() as i32 - self.origin.1;
        let (w, h) = (self.size.0 as i32, self.size.1 as i32);
        if x < 0 || y < 0 || x >= w || y >= h {
            return 1.0 / self.scale;
        }
        let c = self.coverage[(y * w + x) as usize] as f32 / 255.0;
        (0.5 - c) / self.scale
    }
}

#[cfg(test)]
mod tests {
    use crate::cpu;
    use crate::shapes::Shape;
    use crate::shapes::fixtures::text;

    #[test]
    fn the_rasteriser_draws_each_pixel_with_its_coverage() {
        // At scale 2, the fixture's coverage starts at physical (2, 2).
        let shapes: Vec<Box<dyn Shape>> = vec![Box::new(text())];
        let (w, h) = (8, 6);
        let mut px = vec![0u8; w * h * 4];
        cpu::draw(&mut px, w, h, 2.0, &shapes);
        let alpha = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        assert_eq!(alpha(2, 2), 0);
        assert_eq!(alpha(3, 2), 128);
        assert_eq!(alpha(4, 3), 255);
        assert_eq!(alpha(0, 0), 0);
    }
}
