use super::{Sdf, Shape};
use crate::scene::{Point, Rgba};

/// A concave corner: the `r` × `r` square at the corner where two edges
/// meet, less the quarter circle that rounds the one into the other. Laid in
/// the corner between two shapes of the same colour, it joins them in a
/// curve. `at` is the square's filled corner, and `corner` says which of the
/// square's corners that is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fillet {
    pub at: Point,
    pub corner: Corner,
    pub r: f32,
    pub color: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Fillet {
    /// The square's corners, and the centre of the circle cut out of it,
    /// which is the corner opposite `at`.
    fn geometry(&self) -> ([Point; 2], Point) {
        let r = self.r;
        let (dx, dy) = match self.corner {
            Corner::TopLeft => (r, r),
            Corner::TopRight => (-r, r),
            Corner::BottomLeft => (r, -r),
            Corner::BottomRight => (-r, -r),
        };
        let far = Point::new(self.at.x + dx, self.at.y + dy);
        let p0 = Point::new(self.at.x.min(far.x), self.at.y.min(far.y));
        let p1 = Point::new(self.at.x.max(far.x), self.at.y.max(far.y));
        ([p0, p1], far)
    }
}

impl Shape for Fillet {
    fn bounds(&self) -> [Point; 2] {
        self.geometry().0
    }

    fn color(&self) -> Rgba {
        self.color
    }

    /// The square's distance, with the circle's taken away: exact near the
    /// arc and near the square's edges, which is all antialiasing needs.
    fn distance(&self, p: Point) -> f32 {
        let ([p0, p1], cut) = self.geometry();
        let (cx, cy) = ((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
        let h = self.r / 2.0;
        let (qx, qy) = ((p.x - cx).abs() - h, (p.y - cy).abs() - h);
        let square = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0);
        square.max(self.r - (p.x - cut.x).hypot(p.y - cut.y))
    }

    fn sdf(&self) -> Option<Sdf> {
        let ([p0, p1], cut) = self.geometry();
        Some(Sdf {
            center: Point::new((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0),
            half: Point::new(self.r / 2.0, self.r / 2.0),
            cut,
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
        // Its filled corner is (10, 0); the circle is centred on (2, 8).
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
    fn its_bounds_are_the_square() {
        let [p0, p1] = fillet().bounds();
        assert_eq!((p0, p1), (Point::new(2.0, 0.0), Point::new(10.0, 8.0)));
    }
}
