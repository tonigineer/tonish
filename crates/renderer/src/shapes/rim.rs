use super::{Shape, clipped, round_union, rounded_box};
use crate::scene::{Point, Rgba};

/// The space between a rectangle's edge and a rounded rectangle inside it,
/// as a frame round a monitor, with a rounded box joined into it, as a bar
/// that is part of that frame. Where the box meets the rim, concave corners
/// of radius `blend` join them, exact where their edges meet square.
///
/// It is one shape, however many surfaces draw it, so that each draws its
/// own part, cut out with `clip`, and the parts meet without a seam and
/// without a pixel drawn twice, which a translucent colour would show. For
/// the same reason a border is the shape `inset` by half its width, folded
/// into a band as wide as the border by `stroke`, and the fill inside it is
/// the shape inset by the whole width: the two never overlap. The fold comes
/// before the clip, where an [`Outline`](super::Outline) round a clipped
/// shape would draw a line along every cut too.
///
/// The shader has no form for it, so the CPU draws it: it is meant for
/// surfaces that are drawn once, and for those the CPU is the cheaper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rim {
    /// Top-left and bottom-right corners.
    pub outer: [Point; 2],
    pub inner: [Point; 2],
    /// Of `inner`'s corners, in the order of
    /// [`RoundedRect::radii`](super::RoundedRect::radii).
    pub radii: [f32; 4],
    pub joined: Option<Joined>,
    /// How far in from its edge the shape is drawn.
    pub inset: f32,
    /// Above 0, only a band this wide round its edge.
    pub stroke: f32,
    pub clip: [Point; 2],
    pub color: Rgba,
}

/// A box joined into a [`Rim`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joined {
    pub p0: Point,
    pub p1: Point,
    pub radii: [f32; 4],
    /// The radius of the concave corners where it meets the rim. 0 joins
    /// them square.
    pub blend: f32,
}

fn center_half([p0, p1]: [Point; 2]) -> (Point, Point) {
    (
        Point::new((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0),
        Point::new((p1.x - p0.x) / 2.0, (p1.y - p0.y) / 2.0),
    )
}

fn longer(a: Option<[f32; 2]>, b: Option<[f32; 2]>) -> Option<[f32; 2]> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if a[1] - a[0] >= b[1] - b[0] { a } else { b }),
        (a, b) => a.or(b),
    }
}

fn stretch(a: f32, b: f32) -> Option<[f32; 2]> {
    (a <= b).then_some([a, b])
}

impl Rim {
    fn ring(&self, p: Point) -> f32 {
        let (oc, oh) = center_half(self.outer);
        let (ic, ih) = center_half(self.inner);
        rounded_box(oc, oh, [0.0; 4], p).max(-rounded_box(ic, ih, self.radii, p))
    }

    /// The stretch of row `y` at least `d` inside the shape before inset,
    /// stroke and clip. The rounded union takes in both parts whole, so a
    /// stretch inside the rim, or inside the joined box away from its
    /// rounded corners, is inside the shape, and so are both together where
    /// they overlap.
    fn solid(&self, y: f32, d: f32) -> Option<[f32; 2]> {
        let [o0, o1] = self.outer;
        let [i0, i1] = self.inner;
        let ring = if y < o0.y + d || y > o1.y - d {
            None
        } else if y <= i0.y - d || y >= i1.y + d {
            stretch(o0.x + d, o1.x - d)
        } else {
            longer(stretch(o0.x + d, i0.x - d), stretch(i1.x + d, o1.x - d))
        };
        let joined = self.joined.and_then(|j| {
            let r = j.radii.iter().fold(0.0f32, |m, &r| m.max(r));
            if y < j.p0.y + r + d || y > j.p1.y - r - d {
                return None;
            }
            stretch(j.p0.x + d, j.p1.x - d)
        });
        match (ring, joined) {
            (Some([a, b]), Some([e, f])) if a <= f && e <= b => Some([a.min(e), b.max(f)]),
            (s, t) => longer(s, t),
        }
    }

    /// The stretch of row `y` at least `h` outside the shape before inset,
    /// stroke and clip: the middle of the rim, away from the inner corners'
    /// rounding and from the joined box with its concave corners.
    fn void(&self, y: f32, h: f32) -> Option<[f32; 2]> {
        let [i0, i1] = self.inner;
        let r = self.radii.iter().fold(0.0f32, |m, &r| m.max(r));
        if y < i0.y + r + h || y > i1.y - r - h {
            return None;
        }
        let mut s = stretch(i0.x + h, i1.x - h)?;
        if let Some(j) = self.joined {
            let reach = j.blend + h + 1.0;
            if y >= j.p0.y - reach && y <= j.p1.y + reach {
                s = longer(
                    stretch(s[0], s[1].min(j.p0.x - reach)),
                    stretch(s[0].max(j.p1.x + reach), s[1]),
                )?;
            }
        }
        Some(s)
    }
}

impl Shape for Rim {
    fn bounds(&self) -> [Point; 2] {
        self.clip
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        let mut d = self.ring(p);
        if let Some(j) = self.joined {
            let (c, h) = center_half([j.p0, j.p1]);
            d = round_union(d, rounded_box(c, h, j.radii, p), j.blend);
        }
        let d = d + self.inset;
        let d = if self.stroke > 0.0 {
            d.abs() - self.stroke / 2.0
        } else {
            d
        };
        clipped(d, self.clip, p)
    }

    /// A band has no solid stretch of any length worth the trouble.
    fn inside(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let [c0, c1] = self.clip;
        if self.stroke > 0.0 || y < c0.y + depth || y > c1.y - depth {
            return None;
        }
        let s = self.solid(y, depth + self.inset)?;
        stretch(s[0].max(c0.x + depth), s[1].min(c1.x - depth))
    }

    /// A band's middle is the shape's inside, as for an
    /// [`Outline`](super::Outline), which is what makes a border round a
    /// large shape cheap.
    fn hole(&self, y: f32, depth: f32) -> Option<[f32; 2]> {
        let s = if self.stroke > 0.0 {
            let half = self.stroke / 2.0 + depth;
            longer(
                self.solid(y, self.inset + half),
                self.void(y, (half - self.inset).max(0.0)),
            )?
        } else {
            self.void(y, (depth - self.inset).max(0.0))?
        };
        let [c0, c1] = self.clip;
        stretch(s[0].max(c0.x), s[1].min(c1.x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::rim;

    #[test]
    fn it_is_the_space_between_the_two_rectangles() {
        // A rim 4 wide round a box 60 by 40.
        let r = Rim {
            joined: None,
            ..rim()
        };
        assert_eq!(r.distance(Point::new(2.0, 20.0)), -2.0);
        assert_eq!(r.distance(Point::new(30.0, 30.0)), 6.0);
        assert_eq!(r.distance(Point::new(-3.0, 20.0)), 3.0);
    }

    #[test]
    fn a_joined_box_melts_into_it_with_concave_corners() {
        // The box from 20 to 40 hangs from the rim's top side, 4 deep, down
        // to 14, with corners of radius 3 where they meet.
        let r = rim();
        assert!(r.distance(Point::new(30.0, 10.0)) < 0.0);
        assert!(r.distance(Point::new(19.5, 4.5)) < 0.0);
        assert!(r.distance(Point::new(17.0, 7.0)) > 0.0);
    }

    #[test]
    fn a_stroke_is_a_band_inside_its_edge_and_none_along_the_clip() {
        // A border 2 wide: the band from the edge to 2 in.
        let r = Rim {
            inset: 1.0,
            stroke: 2.0,
            clip: [Point::new(0.0, 0.0), Point::new(30.0, 40.0)],
            ..rim()
        };
        // Within 2 of the rim's inner edge at 4, and outside the rim.
        assert!(r.distance(Point::new(3.0, 20.0)) < 0.0);
        assert!(r.distance(Point::new(5.0, 20.0)) > 0.0);
        // Deep inside the joined box, and no line along the cut at 30.
        assert!(r.distance(Point::new(29.5, 10.0)) > 0.0);
    }

    #[test]
    fn inset_and_clip_cut_it_down() {
        let r = Rim {
            inset: 1.0,
            clip: [Point::new(0.0, 0.0), Point::new(30.0, 40.0)],
            joined: None,
            ..rim()
        };
        assert_eq!(r.distance(Point::new(2.0, 20.0)), -1.0);
        assert!(r.distance(Point::new(58.0, 20.0)) > 0.0);
    }
}
