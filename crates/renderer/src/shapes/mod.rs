//! Shapes are signed distances to their own edge rather than pixels or
//! paths. That one quantity gives antialiasing, outlines and independence
//! from the output scale, and it is what the GPU's fragment shader evaluates,
//! so both backends rasterise the same [`Scene`](crate::Scene) the same way.
//!
//! One file per shape, so that a shape's maths and the tests of that maths
//! stay together. The tests that every shape has to pass are in this file.

mod circle;
mod outline;
mod rect;

pub use circle::Circle;
pub use outline::Outline;
pub use rect::Rect;

use crate::scene::{Point, Rgba};

/// Backends only ever call these methods, so a new kind of shape needs no
/// backend changes. `Send`, because the engine hands each scene to the
/// GPU's thread.
pub trait Shape: Send {
    /// Backends skip every pixel outside these corners, so the box may be
    /// larger than the shape, which only costs time, but never smaller,
    /// which clips it.
    fn bounds(&self) -> [Point; 2];

    /// Kept apart from the geometry so that wrappers such as [`Outline`] can
    /// draw a shape's geometry in a colour of their own.
    fn color(&self) -> Rgba;

    /// Negative inside, positive outside. It has to be the true Euclidean
    /// distance near the edge: antialiasing and [`Outline`] rely on it, and
    /// an approximation makes edges and outlines uneven in width. It runs
    /// for every pixel along a shape's edges, and for every pixel of a shape
    /// without [`Shape::inside`], so keep it cheap.
    fn distance(&self, p: Point) -> f32;

    /// The stretch of the row at height `y` that lies at least `depth`
    /// inside the shape, as the `x` of its two ends. The CPU rasteriser fills
    /// it with the colour directly instead of asking [`Shape::distance`]
    /// pixel by pixel, which is what makes large shapes cheap. It may be
    /// shorter than the true stretch, which only costs time, but never
    /// longer, which would paint over antialiased edges. `None` sends the
    /// whole row through `distance`, so a new shape draws correctly before
    /// it implements this.
    fn inside(&self, _y: f32, _depth: f32) -> Option<[f32; 2]> {
        None
    }

    /// The stretch of the row at height `y` that lies at least `depth`
    /// outside the shape, although within its bounds, such as the middle of
    /// an [`Outline`]. The CPU rasteriser skips it. Shorter is allowed and
    /// longer is not, as with [`Shape::inside`].
    fn hole(&self, _y: f32, _depth: f32) -> Option<[f32; 2]> {
        None
    }

    /// The shape in the one form the GPU's shader knows, because a shader
    /// cannot call [`Shape::distance`]. Its [`Sdf::distance`] has to match
    /// `distance`, which the every-shape tests check. `None` makes the GPU
    /// hand any frame with this shape to the CPU rasteriser, so a new shape
    /// draws correctly before it has this.
    fn sdf(&self) -> Option<Sdf> {
        None
    }

    /// Wrapping a shape, rather than giving every shape a stroke of its own,
    /// gives every current and future shape an outline at no cost.
    /// `Self: Sized` keeps the trait usable as `dyn Shape`.
    fn outline(self, width: f32, color: Rgba) -> Outline<Self>
    where
        Self: Sized,
    {
        Outline::new(self, width, color)
    }
}

/// A box with rounded corners, which with the radius at half its size is a
/// circle, folded into an outline if `outline` is above 0. One form for
/// every shape keeps the shader free of branches per shape, so the GPU draws
/// a whole scene with a single draw call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sdf {
    pub center: Point,
    /// Half the width and the height, rounded corners included.
    pub half: Point,
    pub radius: f32,
    /// The width of the band centred on the edge, as [`Outline`] draws it.
    pub outline: f32,
}

impl Sdf {
    /// What `shape.frag` computes, written out again here so that the tests
    /// can hold every shape's [`Shape::sdf`] against its [`Shape::distance`]
    /// without a GPU. A change to one of the two needs the same change in
    /// the other.
    pub fn distance(&self, p: Point) -> f32 {
        let qx = (p.x - self.center.x).abs() - self.half.x + self.radius;
        let qy = (p.y - self.center.y).abs() - self.half.y + self.radius;
        let d = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - self.radius;
        if self.outline > 0.0 {
            d.abs() - self.outline / 2.0
        } else {
            d
        }
    }
}

/// Shared by the tests of every shape, of the scene and of both backends, so
/// that the numbers in their expectations all refer to the same few shapes,
/// and both backends are held to the same random scenes.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{Circle, Rect, Shape};
    use crate::scene::{Point, Rgba};

    pub fn white() -> Rgba {
        Rgba::from_hex(0xffffff, 1.0)
    }

    pub fn orange() -> Rgba {
        Rgba::from_hex(0xff8000, 1.0)
    }

    pub fn transparent() -> Rgba {
        Rgba::from_hex(0xffffff, 0.0)
    }

    /// At the origin with round numbers, so expected distances can be
    /// checked by eye.
    pub fn square() -> Rect {
        Rect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(10.0, 10.0),
            color: orange(),
        }
    }

    /// Clear of the origin, so that its outlines stay at positive
    /// coordinates.
    pub fn circle() -> Circle {
        Circle {
            center: Point::new(10.0, 10.0),
            r: 8.0,
            color: orange(),
        }
    }

    /// xorshift, so that the scenes are random but the same on every run,
    /// and a failure can be reproduced.
    pub struct Dice(pub u32);

    impl Dice {
        pub fn roll(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 as f32 / u32::MAX as f32
        }

        pub fn between(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * self.roll()
        }

        /// Half the time on the half-pixel grid, because edges on whole
        /// pixels put pixel centres exactly at the depth the stretches are
        /// cut at, which is where rounding would show.
        pub fn coordinate(&mut self, lo: f32, hi: f32) -> f32 {
            let v = self.between(lo, hi);
            if self.roll() < 0.5 {
                (v * 2.0).round() / 2.0
            } else {
                v
            }
        }
    }

    pub fn random_shape(dice: &mut Dice, (w, h): (f32, f32)) -> Box<dyn Shape> {
        let opacity = if dice.roll() < 0.5 { 1.0 } else { dice.roll() };
        let channel = |dice: &mut Dice| (dice.roll() * 255.0) as u8;
        let color = Rgba::from_rgb(channel(dice), channel(dice), channel(dice), opacity);
        let (x, y) = (
            dice.coordinate(-5.0, w + 5.0),
            dice.coordinate(-5.0, h + 5.0),
        );
        let (dx, dy) = (
            dice.coordinate(0.0, w / 2.0),
            dice.coordinate(0.0, h / 2.0 + 2.0),
        );
        let rect = Rect {
            p0: Point::new(x - dx, y - dy),
            p1: Point::new(x + dx, y + dy),
            color,
        };
        let circle = Circle {
            center: Point::new(x, y),
            r: dice.coordinate(0.5, h.max(4.0)),
            color,
        };
        let width = dice.coordinate(0.5, 4.0);
        match (dice.roll() * 4.0) as u32 {
            0 => Box::new(rect),
            1 => Box::new(circle),
            2 => Box::new(rect.outline(width, color)),
            _ => Box::new(circle.outline(width, color)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{circle, square, white};
    use super::*;

    /// Backends skip every pixel outside `bounds`, so a covered point
    /// outside them would be clipped without any other test noticing.
    fn assert_bounds_enclose(shape: &dyn Shape) {
        fn around(lo: f32, hi: f32) -> impl Iterator<Item = f32> {
            let n = ((hi - lo + 10.0) * 4.0) as usize;
            (0..=n).map(move |i| lo - 5.0 + i as f32 * 0.25)
        }
        let [p0, p1] = shape.bounds();
        for y in around(p0.y, p1.y) {
            for x in around(p0.x, p1.x) {
                if shape.distance(Point::new(x, y)) < 0.0 {
                    assert!(
                        (p0.x..=p1.x).contains(&x) && (p0.y..=p1.y).contains(&y),
                        "({x}, {y}) is covered but outside the bounds {p0:?}..{p1:?}"
                    );
                }
            }
        }
    }

    /// The rasteriser trusts `inside` and `hole` without checking a pixel,
    /// so a stretch that reaches too far would change pixels that only these
    /// tests notice.
    fn assert_stretches_hold(shape: &dyn Shape) {
        let [p0, p1] = shape.bounds();
        let steps = |lo: f32, hi: f32| {
            let n = (((hi - lo) * 8.0) as usize).max(1);
            (0..=n).map(move |i| lo + (hi - lo) * i as f32 / n as f32)
        };
        for depth in [0.25, 0.5, 1.0] {
            for y in steps(p0.y - 1.0, p1.y + 1.0) {
                if let Some([a, b]) = shape.inside(y, depth) {
                    for x in steps(a, b) {
                        let d = shape.distance(Point::new(x, y));
                        assert!(d <= -depth + 1e-4, "({x}, {y}) is {d}, not {depth} inside");
                    }
                }
                if let Some([a, b]) = shape.hole(y, depth) {
                    for x in steps(a, b) {
                        let d = shape.distance(Point::new(x, y));
                        assert!(d >= depth - 1e-4, "({x}, {y}) is {d}, not {depth} outside");
                    }
                }
            }
        }
    }

    /// Every shape, including new ones, belongs in this list.
    #[test]
    fn bounds_enclose_every_shape() {
        assert_bounds_enclose(&square());
        assert_bounds_enclose(&circle());
        assert_bounds_enclose(&square().outline(3.0, white()));
        assert_bounds_enclose(&circle().outline(3.0, white()));
        assert_bounds_enclose(&circle().outline(2.0, white()).outline(1.0, white()));
    }

    /// The GPU evaluates `sdf` where the CPU evaluates `distance`, so any
    /// difference between the two is a difference between the backends.
    fn assert_sdf_matches(shape: &dyn Shape) {
        let Some(sdf) = shape.sdf() else {
            return;
        };
        let [p0, p1] = shape.bounds();
        for i in 0..=40 {
            for j in 0..=40 {
                let p = Point::new(
                    p0.x - 3.0 + (p1.x - p0.x + 6.0) * i as f32 / 40.0,
                    p0.y - 3.0 + (p1.y - p0.y + 6.0) * j as f32 / 40.0,
                );
                let (gpu, cpu) = (sdf.distance(p), shape.distance(p));
                assert!(
                    (gpu - cpu).abs() < 1e-4,
                    "at {p:?}: {gpu} on the GPU, {cpu}"
                );
            }
        }
    }

    /// Every shape, including new ones, belongs in this list.
    #[test]
    fn stretches_hold_for_every_shape() {
        assert_stretches_hold(&square());
        assert_stretches_hold(&circle());
        assert_stretches_hold(&square().outline(3.0, white()));
        assert_stretches_hold(&circle().outline(3.0, white()));
        assert_stretches_hold(&circle().outline(2.0, white()).outline(1.0, white()));
    }

    /// Every shape, including new ones, belongs in this list.
    #[test]
    fn sdf_matches_distance_for_every_shape() {
        assert_sdf_matches(&square());
        assert_sdf_matches(&circle());
        assert_sdf_matches(&square().outline(3.0, white()));
        assert_sdf_matches(&circle().outline(3.0, white()));
        assert_sdf_matches(&circle().outline(2.0, white()).outline(1.0, white()));
    }

    #[test]
    fn the_gpu_draws_every_shape_but_an_outline_of_an_outline() {
        assert!(square().sdf().is_some());
        assert!(circle().sdf().is_some());
        assert!(square().outline(3.0, white()).sdf().is_some());
        assert!(circle().outline(3.0, white()).sdf().is_some());
        assert!(
            circle()
                .outline(2.0, white())
                .outline(1.0, white())
                .sdf()
                .is_none()
        );
    }
}
