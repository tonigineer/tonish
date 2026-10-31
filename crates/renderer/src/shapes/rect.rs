use super::Shape;
use crate::scene::{Point, Rgba};

/// `p0` has to be the top-left corner and `p1` the bottom-right one. They are
/// not normalised, because every caller already knows their order and
/// `distance` runs once per pixel; swapped corners give wrong bounds and
/// distances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub p0: Point,
    pub p1: Point,
    pub color: Rgba,
}

impl Shape for Rect {
    fn bounds(&self) -> [Point; 2] {
        [self.p0, self.p1]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        // Euclidean beyond the corners rather than the cheaper max-norm, so
        // that antialiased and outlined corners stay round.
        let (cx, cy) = ((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0);
        let (hx, hy) = ((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0);
        let (qx, qy) = ((p.x - cx).abs() - hx, (p.y - cy).abs() - hy);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)
    }

    /// Inside, the distance is the one to the nearest side, so the points
    /// `depth` inside form the rectangle shrunk by `depth` on every side.
    fn inside(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let (x0, x1) = (self.p0.x + depth, self.p1.x - depth);
        let rows = self.p0.y + depth..=self.p1.y - depth;
        (rows.contains(&y) && x0 <= x1).then_some([x0, x1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::square;

    #[test]
    fn rect_distance_is_negative_inside_zero_on_the_edge_positive_outside() {
        assert_eq!(square().distance(Point::new(5.0, 5.0)), -5.0);
        assert_eq!(square().distance(Point::new(2.0, 5.0)), -2.0);
        assert_eq!(square().distance(Point::new(10.0, 5.0)), 0.0);
        assert_eq!(square().distance(Point::new(5.0, 13.0)), 3.0);
    }

    #[test]
    fn rect_distance_past_a_corner_is_euclidean() {
        // A max-norm distance would give 4 here and square off antialiased
        // and outlined corners.
        assert_eq!(square().distance(Point::new(13.0, 14.0)), 5.0);
    }

    #[test]
    fn a_rects_inside_is_the_rect_shrunk_by_the_depth() {
        assert_eq!(square().inside(5.0, 1.0), Some([1.0, 9.0]));
        assert_eq!(square().inside(0.5, 1.0), None);
        assert_eq!(square().inside(5.0, 6.0), None);
    }

    #[test]
    fn a_rect_has_no_hole() {
        assert_eq!(square().hole(5.0, 0.5), None);
    }
}
