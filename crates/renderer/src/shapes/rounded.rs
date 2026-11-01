use super::{Sdf, Shape, rounded_box};
use crate::scene::{Point, Rgba};

/// `p0` and `p1` are the top-left and bottom-right corners, as for
/// [`Rect`](super::Rect).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    pub p0: Point,
    pub p1: Point,
    /// Top left, top right, bottom right, bottom left, as CSS orders them.
    /// Each has to be at most half the shorter side.
    pub radii: [f32; 4],
    pub color: Rgba,
}

impl RoundedRect {
    fn center_half(&self) -> (Point, Point) {
        (
            Point::new((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0),
            Point::new((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0),
        )
    }
}

impl Shape for RoundedRect {
    fn bounds(&self) -> [Point; 2] {
        [self.p0, self.p1]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        let (c, h) = self.center_half();
        rounded_box(c, h, self.radii, p)
    }

    /// The points `depth` inside form the rectangle shrunk by `depth`, with
    /// its corners rounded by what is left of their radii: the corners' arcs
    /// keep their centres, and past a radius the corner is square. Each end
    /// of a row follows the corner of its half.
    fn inside(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let (c, h) = self.center_half();
        let (hx, hy, dy) = (h.x - depth, h.y - depth, y - c.y);
        if dy.abs() > hy || hx < 0.0 {
            return None;
        }
        let reach = |radius: f32| {
            let r = (radius - depth).max(0.0);
            let straight = hy - r;
            if dy.abs() <= straight {
                hx
            } else {
                let ey = dy.abs() - straight;
                hx - r + (r * r - ey * ey).max(0.0).sqrt()
            }
        };
        let [tl, tr, br, bl] = self.radii;
        let (left, right) = if dy > 0.0 { (bl, br) } else { (tl, tr) };
        Some([c.x - reach(left), c.x + reach(right)])
    }

    fn sdf(&self) -> Option<Sdf> {
        let (center, half) = self.center_half();
        Some(Sdf {
            center,
            half,
            radii: self.radii,
            ..Sdf::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::{rounded, uneven};

    #[test]
    fn along_its_sides_it_is_as_far_as_a_rect() {
        assert_eq!(rounded().distance(Point::new(15.0, 2.0)), -2.0);
        assert_eq!(rounded().distance(Point::new(15.0, -3.0)), 3.0);
    }

    #[test]
    fn past_a_corner_it_is_as_far_as_the_corners_circle() {
        // The top-left corner's arc is centred on (4, 4) with radius 4.
        let d = rounded().distance(Point::new(0.0, 0.0));
        assert!((d - (32f32.sqrt() - 4.0)).abs() < 1e-5, "{d}");
    }

    #[test]
    fn its_inside_is_narrower_in_the_rows_of_the_corners() {
        assert_eq!(rounded().inside(10.0, 1.0), Some([1.0, 29.0]));
        // 1 inside, the arc has radius 3 around (4, 4): at y = 1 it is a point.
        assert_eq!(rounded().inside(1.0, 1.0), Some([4.0, 26.0]));
        assert_eq!(rounded().inside(0.5, 1.0), None);
    }

    #[test]
    fn each_corner_is_as_round_as_its_own_radius() {
        // The top right is square, so past it the distance is the rect's.
        assert_eq!(uneven().distance(Point::new(33.0, -4.0)), 5.0);
        // The bottom left has radius 8, its arc centred on (8, 12).
        let d = uneven().distance(Point::new(0.0, 20.0));
        assert!((d - (128f32.sqrt() - 8.0)).abs() < 1e-5, "{d}");
    }

    #[test]
    fn a_rows_ends_follow_the_corners_of_its_half() {
        // 1 inside, at y = 1: the top left (6) is a curve, the top right a
        // square corner.
        let [a, b] = uneven().inside(1.0, 1.0).unwrap();
        assert!(a > 1.0 && b == 29.0, "{a} {b}");
    }
}
