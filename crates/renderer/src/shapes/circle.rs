use super::{Sdf, Shape};
use crate::scene::{Point, Rgba};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Circle {
    pub center: Point,
    pub r: f32,
    pub color: Rgba,
}

impl Shape for Circle {
    fn bounds(&self) -> [Point; 2] {
        let Point { x, y } = self.center;
        [
            Point::new(x - self.r, y - self.r),
            Point::new(x + self.r, y + self.r),
        ]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        (p.x - self.center.x).hypot(p.y - self.center.y) - self.r
    }

    fn inside(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let (r, dy) = (self.r - depth, y - self.center.y);
        if r <= 0.0 || dy.abs() > r {
            return None;
        }
        let half = (r * r - dy * dy).sqrt();
        Some([self.center.x - half, self.center.x + half])
    }

    fn sdf(&self) -> Option<Sdf> {
        Some(Sdf {
            center: self.center,
            half: Point::new(self.r, self.r),
            radius: self.r,
            ..Sdf::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::circle;

    #[test]
    fn circle_distance_is_negative_inside_zero_on_the_edge_positive_outside() {
        assert_eq!(circle().distance(Point::new(10.0, 10.0)), -8.0);
        assert_eq!(circle().distance(Point::new(18.0, 10.0)), 0.0);
        assert_eq!(circle().distance(Point::new(10.0, 21.0)), 3.0);
    }

    #[test]
    fn a_circles_inside_is_a_chord() {
        assert_eq!(circle().inside(10.0, 1.0), Some([3.0, 17.0]));
        assert_eq!(circle().inside(13.0, 3.0), Some([6.0, 14.0]));
        assert_eq!(circle().inside(17.5, 1.0), None);
    }
}
