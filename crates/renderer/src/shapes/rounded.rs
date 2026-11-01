use super::{Sdf, Shape};
use crate::scene::{Point, Rgba};

/// `r` has to be at most half the shorter side, and `p0` and `p1` are the
/// top-left and bottom-right corners, as for [`Rect`](super::Rect).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedRect {
    pub p0: Point,
    pub p1: Point,
    pub r: f32,
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
        let qx = (p.x - c.x).abs() - h.x + self.r;
        let qy = (p.y - c.y).abs() - h.y + self.r;
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - self.r
    }

    /// The points `depth` inside form the rectangle shrunk by `depth`, with
    /// its corners rounded by what is left of the radius: the corners' arcs
    /// keep their centres, and past the radius the corners are square.
    fn inside(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let (c, h) = self.center_half();
        let (hx, hy, r) = (h.x - depth, h.y - depth, (self.r - depth).max(0.0));
        let dy = (y - c.y).abs();
        if dy > hy || hx < 0.0 {
            return None;
        }
        let straight = hy - r;
        let half = if dy <= straight {
            hx
        } else {
            let ey = dy - straight;
            hx - r + (r * r - ey * ey).max(0.0).sqrt()
        };
        Some([c.x - half, c.x + half])
    }

    fn sdf(&self) -> Option<Sdf> {
        let (center, half) = self.center_half();
        Some(Sdf {
            center,
            half,
            radius: self.r,
            ..Sdf::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::rounded;

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
}
