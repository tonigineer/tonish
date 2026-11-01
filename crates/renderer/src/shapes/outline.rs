use super::{Sdf, Shape};
use crate::scene::{Point, Rgba};

/// Centred on the edge, so that an outline drawn over its own fill hides the
/// fill's antialiased fringe instead of leaving a seam. It ignores the
/// wrapped shape's colour, which is how a border without a fill is drawn:
/// outline a transparent shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline<S> {
    shape: S,
    width: f32,
    color: Rgba,
}

impl<S> Outline<S> {
    pub(super) fn new(shape: S, width: f32, color: Rgba) -> Self {
        Self {
            shape,
            width,
            color,
        }
    }
}

impl<S: Shape> Shape for Outline<S> {
    fn bounds(&self) -> [Point; 2] {
        let [p0, p1] = self.shape.bounds();
        let w = self.width / 2.0;
        [
            Point::new(p0.x - w, p0.y - w),
            Point::new(p1.x + w, p1.y + w),
        ]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        // Folding the distance turns the edge into a band, which is why
        // `Shape::distance` has to be exact.
        self.shape.distance(p).abs() - self.width / 2.0
    }

    /// The band's own solid part would be two stretches on most rows, left
    /// and right of the middle, so it keeps the default. Its middle is what
    /// matters: an outline around a whole panel would otherwise send every
    /// pixel of the panel through `distance`.
    fn hole(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        self.shape.inside(y, self.width / 2.0 + depth)
    }

    /// The shader folds the distance once, so an outline of an outline is
    /// left to the CPU.
    fn sdf(&self) -> Option<Sdf> {
        let sdf = self.shape.sdf().filter(|s| s.outline == 0.0)?;
        Some(Sdf {
            outline: self.width,
            ..sdf
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use crate::shapes::Rect;
    use crate::shapes::fixtures::{circle, square, transparent, white};

    #[test]
    fn an_outline_is_a_band_centred_on_the_edge() {
        let ring = circle().outline(2.0, white());
        assert_eq!(ring.distance(Point::new(10.0, 2.0)), -1.0);
        assert_eq!(ring.distance(Point::new(10.0, 1.0)), 0.0);
        assert_eq!(ring.distance(Point::new(10.0, 3.0)), 0.0);
        assert_eq!(
            ring.bounds(),
            [Point::new(1.0, 1.0), Point::new(19.0, 19.0)]
        );
    }

    #[test]
    fn an_outline_is_hollow() {
        let ring = circle().outline(2.0, white());
        let frame = square().outline(2.0, white());
        assert_eq!(ring.distance(Point::new(10.0, 10.0)), 7.0);
        assert_eq!(frame.distance(Point::new(5.0, 5.0)), 4.0);
    }

    #[test]
    fn an_outlines_hole_is_its_middle() {
        let ring = circle().outline(2.0, white());
        assert_eq!(ring.inside(10.0, 0.5), None);
        assert_eq!(ring.hole(10.0, 0.5), Some([3.5, 16.5]));
    }

    #[test]
    fn a_rect_outline_reaches_half_its_width_past_the_rect() {
        let frame = square().outline(2.0, white());
        assert_eq!(frame.distance(Point::new(0.0, 5.0)), -1.0);
        assert_eq!(frame.distance(Point::new(11.0, 5.0)), 0.0);
        assert_eq!(
            frame.bounds(),
            [Point::new(-1.0, -1.0), Point::new(11.0, 11.0)]
        );
    }

    #[test]
    fn an_outline_uses_its_own_colour() {
        assert_eq!(square().outline(1.0, white()).color(), white());
    }

    #[test]
    fn an_outline_of_a_transparent_shape_is_a_border_without_a_fill() {
        let mut s = Scene::new();
        let hidden = Rect {
            color: transparent(),
            ..square()
        };
        s.push(hidden);
        s.push(hidden.outline(2.0, white()));
        assert_eq!(s.shapes.len(), 1);
        assert_eq!(s.shapes[0].color(), white());
    }

    #[test]
    fn outlines_nest_into_parallel_lines() {
        let double = circle().outline(2.0, white()).outline(1.0, white());
        assert_eq!(double.distance(Point::new(10.0, 1.0)), -0.5);
        assert_eq!(double.distance(Point::new(10.0, 3.0)), -0.5);
        assert_eq!(double.distance(Point::new(10.0, 2.0)), 0.5);
        assert_eq!(
            double.bounds(),
            [Point::new(0.5, 0.5), Point::new(19.5, 19.5)]
        );
    }
}
