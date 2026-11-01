use super::rounded::inside_box;
use super::{
    Bulge, Melt, Neck, Sdf, Shape, Wave, clipped, edge_distance, room, round_union, rounded_box,
    with_neck,
};
use crate::scene::{Point, Rgba};

/// A rounded box melted into a straight edge, as a panel into the bar it
/// opens out of: where the box meets the edge, concave corners of radius
/// `blend` join the two. The edge's side is a half-plane, drawn only within
/// `bounds`, which clip the whole shape.
///
/// The concave corners are exact circular arcs, so the shape with its box and
/// edge moved in by a border's width and `blend` grown by it is the inside of
/// that border, and the border keeps its width round the corners. An
/// approximate smooth minimum would thicken it there by up to √2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Liquid {
    /// The box's top-left and bottom-right corners. It may reach into the
    /// edge's side, which hides its corners there.
    pub p0: Point,
    pub p1: Point,
    /// In the order of [`RoundedRect::radii`](super::RoundedRect::radii).
    pub radii: [f32; 4],
    /// Of length 1, pointing away from the edge's side: a point is on that
    /// side when its position along `normal` is below `offset`.
    pub normal: Point,
    pub offset: f32,
    /// 0 meets the edge in square corners. Within `blend` of the edge's
    /// ends, a corner curves into edge that isn't there, which the bounds
    /// cut off square.
    pub blend: f32,
    pub wave: Option<Wave>,
    pub bulge: Option<Bulge>,
    pub neck: Option<Neck>,
    /// Where the edge runs, along it: x for an edge along the x axis, y for
    /// one along the y axis. Past its ends the edge's side is empty, and a
    /// box beside it melts into its end as into its edge.
    pub extent: [f32; 2],
    pub bounds: [Point; 2],
    pub color: Rgba,
}

impl Liquid {
    /// An edge without ends. Not infinite, which the shader would turn
    /// into NaN in its sums.
    pub const ENDLESS: [f32; 2] = [-1e9, 1e9];

    fn center_half(&self) -> (Point, Point) {
        (
            Point::new((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0),
            Point::new((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0),
        )
    }

    /// The stretch of the row at `y` that lies `depth` into the edge's side,
    /// unbounded, for a straight edge along either axis. A slanted edge or one
    /// with waves gives none, which only costs time.
    fn edge_side(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        if self.wave.is_some() || self.bulge.is_some() {
            return None;
        }
        let Point { x: nx, y: ny } = self.normal;
        let [lo, hi] = [self.extent[0] + depth, self.extent[1] - depth];
        let across = match (nx, ny) {
            (0.0, _) => {
                return (y * ny - self.offset <= -depth && lo <= hi).then_some([lo, hi]);
            }
            (1.0, 0.0) => [f32::NEG_INFINITY, self.offset - depth],
            (-1.0, 0.0) => [depth - self.offset, f32::INFINITY],
            _ => return None,
        };
        (lo..=hi).contains(&y).then_some(across)
    }
}

impl Shape for Liquid {
    fn bounds(&self) -> [Point; 2] {
        self.bounds
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        let (c, h) = self.center_half();
        let edge = edge_distance(
            self.normal,
            self.offset,
            self.wave,
            self.bulge,
            self.extent,
            p,
        );
        // The neck melts in after the edge's exact join, so that the box's
        // corners at the edge keep their radius however gooey the neck is.
        let room = room(self.normal, self.extent, p);
        let d = round_union(edge, rounded_box(c, h, self.radii, p), self.blend);
        clipped(
            with_neck(d, self.neck, self.normal, room, p),
            self.bounds,
            p,
        )
    }

    /// The unions never lie outside any part, so a stretch inside the
    /// box or inside the edge's side is inside the shape, and so are both
    /// together where they overlap. The clip takes `depth` off every side.
    fn inside(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let [b0, b1] = self.bounds;
        if y < b0.y + depth || y > b1.y - depth {
            return None;
        }
        let (c, h) = self.center_half();
        let stretch = match (
            inside_box(c, h, self.radii, y, depth),
            self.edge_side(y, depth),
        ) {
            (Some([a, b]), Some([e, f])) if a <= f && e <= b => [a.min(e), b.max(f)],
            (Some(s), Some(t)) => {
                if s[1] - s[0] >= t[1] - t[0] {
                    s
                } else {
                    t
                }
            }
            (s, t) => s.or(t)?,
        };
        let (a, b) = (stretch[0].max(b0.x + depth), stretch[1].min(b1.x - depth));
        (a <= b).then_some([a, b])
    }

    fn sdf(&self) -> Option<Sdf> {
        let (center, half) = self.center_half();
        Some(Sdf {
            center,
            half,
            radii: self.radii,
            melt: Some(Melt {
                normal: self.normal,
                offset: self.offset,
                radius: self.blend,
                clip: self.bounds,
                wave: self.wave,
                bulge: self.bulge,
                neck: self.neck,
                extent: self.extent,
            }),
            ..Sdf::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::liquid;

    /// An edge from x 15 to 45, at y 10, the box from 20 to 40 below it.
    fn ended() -> Liquid {
        Liquid {
            extent: [15.0, 45.0],
            bounds: [Point::new(0.0, 0.0), Point::new(60.0, 35.0)],
            ..liquid()
        }
    }

    #[test]
    fn past_its_extent_the_edge_has_no_side() {
        let edge = |x, y| {
            let down = Point::new(0.0, 1.0);
            edge_distance(down, 10.0, None, None, [15.0, 45.0], Point::new(x, y))
        };
        // In the edge's side: inside within the extent, and outside past
        // it, by the distance to the edge's end.
        assert_eq!(edge(30.0, 4.0), -6.0);
        assert_eq!(edge(5.0, 4.0), 10.0);
        assert_eq!(edge(55.0, 4.0), 10.0);
        // Past the end and below the edge, from the end's corner.
        assert_eq!(edge(8.0, 14.0), 65f32.sqrt());
    }

    #[test]
    fn waves_flatten_at_the_ends_of_the_edge() {
        let wavy = Liquid {
            wave: Some(super::super::Wave {
                center: 15.0,
                amplitude: 6.0,
                group: 0.0,
                phase: 0.0,
            }),
            p0: Point::new(30.0, 0.0),
            p1: Point::new(30.0, 2.0),
            ..ended()
        };
        // At the end itself the crest at 15 has no height left: the edge
        // is where it would be without it.
        assert!((wavy.distance(Point::new(15.0, 10.0))).abs() < 1e-4);
        // Further in, the waves rise out of it again.
        assert!(wavy.distance(Point::new(35.0, 10.5)) < 0.0);
    }

    #[test]
    fn far_from_the_corners_it_is_the_edge_or_the_box() {
        // The edge's side is y < 10; the box hangs below it, from x 20 to 40.
        assert_eq!(liquid().distance(Point::new(5.0, 6.0)), -4.0);
        assert_eq!(liquid().distance(Point::new(30.0, 30.0)), -5.0);
        assert_eq!(liquid().distance(Point::new(5.0, 20.0)), 10.0);
    }

    #[test]
    fn a_concave_corner_is_an_arc_round_its_centre() {
        // The left corner's arc has radius 6, centred 6 left of the box and
        // 6 below the edge: (14, 16).
        let at = |p: Point| liquid().distance(p);
        let d = at(Point::new(17.0, 13.0));
        assert!((d - (6.0 - 18f32.sqrt())).abs() < 1e-5, "{d}");
        // A border's inner edge at depth 2 is the arc of radius 8 round the
        // same centre.
        let inner = Point::new(14.0 + 8.0 / 2f32.sqrt(), 16.0 - 8.0 / 2f32.sqrt());
        assert!((at(inner) + 2.0).abs() < 1e-4, "{}", at(inner));
    }

    #[test]
    fn nothing_outside_its_bounds_is_covered() {
        assert!(liquid().distance(Point::new(-1.0, 5.0)) > 0.0);
        assert!(liquid().distance(Point::new(30.0, 36.0)) > 0.0);
    }
}
