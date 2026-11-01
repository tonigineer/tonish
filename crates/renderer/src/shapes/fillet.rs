use super::{Sdf, Shape};
use crate::scene::{Point, Rgba};

/// A rectangle with a circle cut out of it. Laid in the corner where two
/// shapes of the same colour meet at a right angle, with the circle centred
/// on the corner's far side, it joins them in a concave curve. The circle is
/// free of the rectangle, rather than always reaching its far corner, so
/// that a border's fill can share the line's centre with a larger radius,
/// which keeps the line as wide along the curve as along the sides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fillet {
    /// The rectangle's top-left and bottom-right corners.
    pub p0: Point,
    pub p1: Point,
    pub center: Point,
    pub r: f32,
    pub color: Rgba,
}

impl Shape for Fillet {
    fn bounds(&self) -> [Point; 2] {
        [self.p0, self.p1]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    /// The rectangle's distance, with the circle's taken away: exact near
    /// the arc and near the rectangle's edges, which is all antialiasing
    /// needs.
    fn distance(&self, p: Point) -> f32 {
        let (cx, cy) = ((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0);
        let (hx, hy) = ((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0);
        let (qx, qy) = ((p.x - cx).abs() - hx, (p.y - cy).abs() - hy);
        let rect = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0);
        rect.max(self.r - (p.x - self.center.x).hypot(p.y - self.center.y))
    }

    fn sdf(&self) -> Option<Sdf> {
        Some(Sdf {
            center: Point::new((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0),
            half: Point::new((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0),
            cut: self.center,
            cut_radius: self.r,
            ..Sdf::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::fillet;

    #[test]
    fn it_fills_the_corner_and_leaves_the_circle_empty() {
        // The rectangle is (2, 0) to (10, 8); the circle is centred on (2, 8).
        assert!(fillet().distance(Point::new(9.5, 0.5)) < 0.0);
        assert!(fillet().distance(Point::new(3.0, 7.0)) > 0.0);
    }

    #[test]
    fn near_the_arc_it_is_as_far_as_the_arc() {
        let d = fillet().distance(Point::new(6.5, 1.5));
        let want = 8.0 - 4.5f32.hypot(6.5);
        assert!((d - want).abs() < 1e-5, "{d}, not {want}");
    }

    #[test]
    fn a_circle_larger_than_the_rectangle_cuts_further_in() {
        let wider = Fillet { r: 9.0, ..fillet() };
        let p = Point::new(6.5, 1.5);
        assert!(wider.distance(p) > fillet().distance(p));
    }
}
