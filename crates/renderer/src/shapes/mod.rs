//! Shapes are signed distances to their own edge rather than pixels or
//! paths. That one quantity gives antialiasing, outlines and independence
//! from the output scale, and it is what the GPU's fragment shader evaluates,
//! so both backends rasterise the same [`Scene`](crate::Scene) the same way.
//!
//! One file per shape, so that a shape's maths and the tests of that maths
//! stay together. The tests that every shape has to pass are in this file.

mod circle;
mod fillet;
mod liquid;
mod outline;
mod rect;
mod rounded;
mod text;

pub use circle::Circle;
pub use fillet::Fillet;
pub use liquid::Liquid;
pub use outline::Outline;
pub use rect::Rect;
pub use rounded::RoundedRect;
pub use text::Text;

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

/// A box with rounded corners, which with every radius at half its size is
/// a circle, melted into an edge if `melt` is set, as for a [`Liquid`], with a
/// circle cut out of it if `cut_radius` is above 0, as for a [`Fillet`], and
/// folded into an outline if `outline` is above 0. One form for every shape
/// keeps the shader free of branches per shape, so the GPU draws a whole
/// scene with a single draw call.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sdf {
    pub center: Point,
    /// Half the width and the height, rounded corners included.
    pub half: Point,
    /// In the order of [`RoundedRect::radii`].
    pub radii: [f32; 4],
    /// The width of the band centred on the edge, as [`Outline`] draws it.
    pub outline: f32,
    pub cut: Point,
    pub cut_radius: f32,
    pub melt: Option<Melt>,
}

/// The edge a [`Liquid`] melts into, its concave corners' radius, and the
/// box it is clipped to, which the GPU also draws it within.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Melt {
    pub normal: Point,
    pub offset: f32,
    pub radius: f32,
    pub clip: [Point; 2],
    pub wave: Option<Wave>,
    pub bulge: Option<Bulge>,
    pub neck: Option<Neck>,
    /// Where the edge runs, along it, as [`Liquid::extent`].
    pub extent: [f32; 2],
}

/// A neck of liquid melted into the box, as between a card pulled off a bar
/// and the bar: a curve from `from` to `to`, wide at its ends and thin in
/// its middle, where a liquid bridge thins first and breaks. Its width runs
/// along a catenary, `waist · cosh(u · acosh(end / waist))` for `u` from -1
/// to 1, the shape a soap film takes between two rings. It leaves
/// `from` and reaches `to` along the edge's normal, square to the edge and
/// to the box's side facing it, as liquid pulled out of a surface does, and
/// bends in between where the two lie apart along the edge, as a smoothstep.
/// It melts in with the quadratic smooth minimum, the gooey join pleama's
/// shapes have, rather than exact arcs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Neck {
    pub from: Point,
    pub to: Point,
    /// Half its width at its ends, and in its middle. A waist wider than
    /// the ends makes it as wide as the waist all along.
    pub end: f32,
    pub waist: f32,
    /// How far it melts into the box. 0 joins them sharply.
    pub blend: f32,
    /// Taken off its width all along, so that with `waist` at most `cut` it
    /// parts in the middle into two horns, one on each end, as a neck does
    /// once it has collapsed. A waist near 0 alone would leave a hairline.
    /// Below 0 it counts as 0, which the shader packs it as.
    pub cut: f32,
}

impl Neck {
    /// The distance across the curve at `p`'s height over the edge, divided
    /// by the length of the curve's normal as for a wave, less the half
    /// width there. Close enough to the true distance near its edge for
    /// antialiasing while it bends no steeper than it rises and narrows no
    /// steeper than a catenary of a sane waist; past its ends, the distance
    /// to the end.
    fn distance(&self, normal: Point, p: Point) -> f32 {
        let (dx, dy) = (self.to.x - self.from.x, self.to.y - self.from.y);
        let (qx, qy) = (p.x - self.from.x, p.y - self.from.y);
        let rise = dx * normal.x + dy * normal.y;
        let (wx, wy) = (dx - normal.x * rise, dy - normal.y * rise);
        let flat = rise.abs() <= 1e-6;
        let t = if flat {
            0.0
        } else {
            ((qx * normal.x + qy * normal.y) / rise).clamp(0.0, 1.0)
        };
        let s = t * t * (3.0 - 2.0 * t);
        let (rx, ry) = (
            qx - normal.x * rise * t - wx * s,
            qy - normal.y * rise * t - wy * s,
        );
        let up = rx * normal.x + ry * normal.y;
        let across = (ry * normal.x - rx * normal.y).abs();
        let slope = if flat {
            0.0
        } else {
            wx.hypot(wy) * 6.0 * t * (1.0 - t) / rise.abs()
        };
        let cut = self.cut.max(0.0);
        (across / (1.0 + slope * slope).sqrt()).hypot(up) - (self.half(2.0 * t - 1.0) - cut)
    }

    /// Half its width at `u`, from -1 at `from` to 1 at `to`. A waist of 0
    /// counts as a ten-thousandth of the ends, a cusp, rather than nothing,
    /// which acosh can't take.
    fn half(&self, u: f32) -> f32 {
        let w = self.waist.max(self.end * 1e-4).max(1e-6);
        w * (u * (self.end / w).max(1.0).acosh()).cosh()
    }
}

/// A swelling of an edge at `center` along it, `height` out of it, as a
/// bar's edge rising after a card pulled off it: a bell some 60 pixels wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bulge {
    pub center: f32,
    pub height: f32,
}

impl Bulge {
    pub const SPREAD: f32 = 20.0;

    fn height(&self, s: f32) -> (f32, f32) {
        let r = (s - self.center) / Self::SPREAD;
        let h = self.height * (-r * r).exp();
        (h, h * -2.0 * r / Self::SPREAD)
    }
}

/// pleama's smooth minimum: within `k` of each other, the two melt.
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// The box, with the neck melted into it. `normal` is the edge's, and
/// `room` caps the neck's blend.
fn with_neck(d: f32, neck: Option<Neck>, normal: Point, room: f32, p: Point) -> f32 {
    match neck {
        Some(n) => smooth_min(d, n.distance(normal, p), n.blend.min(room)),
        None => d,
    }
}

/// How far `p` is from the nearer end of the edge, along it: the neck
/// blends no wider than that, so that near an end it never curves into edge
/// that isn't there, which the clip would cut into a spike. The box's own
/// join keeps its radius, exact, which the box's owner fits to the room it
/// has.
fn room(normal: Point, [lo, hi]: [f32; 2], p: Point) -> f32 {
    let s = if normal.y.abs() > 0.5 { p.x } else { p.y };
    (s - lo).min(hi - s).max(0.0)
}

/// Waves on an edge, as on water after a drop falls in: two groups of
/// crests running apart from `center`, along the edge, `group` from it by
/// now, with crests `phase` from it, which run twice as fast as their group
/// on deep water, so that a crest rises at the back of its group and fades at
/// its front. Crests only rise out of the edge's side, never into it: a
/// surface drawn over the bar can add to the bar, not take from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wave {
    /// Along the edge: x for an edge along the x axis, y for one along the
    /// y axis.
    pub center: f32,
    /// The highest a crest rises, in logical pixels.
    pub amplitude: f32,
    pub group: f32,
    pub phase: f32,
}

impl Wave {
    /// Logical pixels from one crest to the next, and the width of a group,
    /// which together give it about three crests.
    pub const LENGTH: f32 = 20.0;
    pub const SPREAD: f32 = 24.0;

    /// Its height at `s` along the edge, and that height's slope.
    fn height(&self, s: f32) -> (f32, f32) {
        let r = s - self.center;
        // The phase within one wavelength, where a GPU's sine and cosine are
        // as precise as the CPU's: far from 0, NVIDIA's lose digits the CPU
        // keeps, which shows as a few levels of difference on steep crests.
        let u = r.abs() - self.group;
        let v = (r.abs() - self.phase).rem_euclid(Self::LENGTH);
        let envelope = (-(u / Self::SPREAD).powi(2)).exp();
        let k = std::f32::consts::PI * 2.0 / Self::LENGTH;
        let crest = 0.5 + 0.5 * (k * v).cos();
        let h = self.amplitude * envelope * crest;
        // d/d|r|, then the sign of r for d/ds.
        let slope = self.amplitude
            * (envelope * (-2.0 * u / (Self::SPREAD * Self::SPREAD)) * crest
                + envelope * -0.5 * k * (k * v).sin());
        // GLSL's sign, which is 0 at 0 where Rust's signum is 1: exactly at
        // the centre, where the two groups meet in a kink, the two backends
        // would otherwise tilt the edge differently.
        let side = if r == 0.0 { 0.0 } else { r.signum() };
        (h, slope * side)
    }
}

/// Within this of the edge's ends, waves and a bulge flatten to nothing,
/// so that they never rise out of the end of a bar.
const TAPER: f32 = 24.0;

/// 0 at either end of the extent, 1 from `TAPER` in, as smoothsteps, and
/// its slope.
fn taper(s: f32, [lo, hi]: [f32; 2]) -> (f32, f32) {
    let u = ((s - lo) / TAPER).clamp(0.0, 1.0);
    let v = ((hi - s) / TAPER).clamp(0.0, 1.0);
    let (su, sv) = (u * u * (3.0 - 2.0 * u), v * v * (3.0 - 2.0 * v));
    let (du, dv) = (6.0 * u * (1.0 - u) / TAPER, 6.0 * v * (1.0 - v) / TAPER);
    (su * sv, du * sv - su * dv)
}

/// How far `p` is out of the edge's side, along `normal`, its waves and
/// bulge included: the height over the edge divided by the length of the
/// slope's normal, which is exact on a straight edge, and near a wave close
/// enough for the edge's antialiasing and a border's width. The side ends at
/// the extent's ends, square, as a box does.
fn edge_distance(
    normal: Point,
    offset: f32,
    wave: Option<Wave>,
    bulge: Option<Bulge>,
    extent: [f32; 2],
    p: Point,
) -> f32 {
    let e = p.x * normal.x + p.y * normal.y - offset;
    let s = if normal.y.abs() > 0.5 { p.x } else { p.y };
    let across = if wave.is_none() && bulge.is_none() {
        e
    } else {
        let (hw, sw) = wave.map_or((0.0, 0.0), |w| w.height(s));
        let (hb, sb) = bulge.map_or((0.0, 0.0), |b| b.height(s));
        let (t, st) = taper(s, extent);
        let h = (hw + hb) * t;
        let slope = (sw + sb) * t + (hw + hb) * st;
        (e - h) / (1.0 + slope * slope).sqrt()
    };
    let along = (extent[0] - s).max(s - extent[1]);
    if along > 0.0 && across > 0.0 {
        along.hypot(across)
    } else {
        along.max(across)
    }
}

impl Sdf {
    /// What `shape.frag` computes, written out again here so that the tests
    /// can hold every shape's [`Shape::sdf`] against its [`Shape::distance`]
    /// without a GPU. A change to one of the two needs the same change in
    /// the other.
    pub fn distance(&self, p: Point) -> f32 {
        let mut d = rounded_box(self.center, self.half, self.radii, p);
        if let Some(m) = self.melt {
            let edge = edge_distance(m.normal, m.offset, m.wave, m.bulge, m.extent, p);
            let room = room(m.normal, m.extent, p);
            d = round_union(edge, d, m.radius);
            d = clipped(with_neck(d, m.neck, m.normal, room, p), m.clip, p);
        }
        // Before the outline, so that an outline follows the cut too.
        if self.cut_radius > 0.0 {
            d = d.max(self.cut_radius - (p.x - self.cut.x).hypot(p.y - self.cut.y));
        }
        if self.outline > 0.0 {
            d.abs() - self.outline / 2.0
        } else {
            d
        }
    }
}

/// The distance to a box with a radius of its own at each corner: each
/// quarter of the plane is as far as the box rounded by its corner's
/// radius, which is exact as long as no radius exceeds half the shorter
/// side.
fn rounded_box(center: Point, half: Point, radii: [f32; 4], p: Point) -> f32 {
    let (dx, dy) = (p.x - center.x, p.y - center.y);
    // y grows downwards, so a positive dy is the bottom half.
    let r = match (dx > 0.0, dy > 0.0) {
        (false, false) => radii[0],
        (true, false) => radii[1],
        (true, true) => radii[2],
        (false, true) => radii[3],
    };
    let (qx, qy) = (dx.abs() - half.x + r, dy.abs() - half.y + r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// Two shapes joined by concave corners of radius `r`, which are exact
/// circular arcs where their edges meet at a right angle, as hg_sdf's round
/// union has them. At 0 the corner is sharp, and still exact inside it,
/// where `min` is not: there, the nearest edge is the corner itself.
fn round_union(a: f32, b: f32, r: f32) -> f32 {
    r.max(a.min(b)) - (r - a).max(0.0).hypot((r - b).max(0.0))
}

/// `d` cut off at the edges of `clip`, a box without rounded corners. The
/// cut is exact only where it crosses a straight part of the shape, which
/// is where a [`Liquid`] is cut.
fn clipped(d: f32, [c0, c1]: [Point; 2], p: Point) -> f32 {
    let center = Point::new((c0.x + c1.x) / 2.0, (c0.y + c1.y) / 2.0);
    let half = Point::new((c1.x - c0.x) / 2.0, (c1.y - c0.y) / 2.0);
    d.max(rounded_box(center, half, [0.0; 4], p))
}

/// Shared by the tests of every shape, of the scene and of both backends, so
/// that the numbers in their expectations all refer to the same few shapes,
/// and both backends are held to the same random scenes.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{Circle, Fillet, Liquid, Rect, RoundedRect, Shape, Text};
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

    /// Wider than tall, so that the arcs of its corners meet straight sides
    /// on every side.
    pub fn rounded() -> RoundedRect {
        RoundedRect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(30.0, 20.0),
            radii: [4.0; 4],
            color: orange(),
        }
    }

    /// A different radius at every corner, one of them square.
    pub fn uneven() -> RoundedRect {
        RoundedRect {
            radii: [6.0, 0.0, 3.0, 8.0],
            ..rounded()
        }
    }

    /// Filled at (10, 0), reaching left and down, as beside the left edge of
    /// a card that hangs from a bar.
    pub fn fillet() -> Fillet {
        Fillet {
            p0: Point::new(2.0, 0.0),
            p1: Point::new(10.0, 8.0),
            center: Point::new(2.0, 8.0),
            r: 8.0,
            color: orange(),
        }
    }

    /// Hanging below an edge at y = 10 from x 20 to 40, as a panel below a
    /// bar along the top, with a bar's strip as wide again on either side.
    pub fn liquid() -> Liquid {
        Liquid {
            p0: Point::new(20.0, 4.0),
            p1: Point::new(40.0, 35.0),
            radii: [4.0; 4],
            normal: Point::new(0.0, 1.0),
            offset: 10.0,
            blend: 6.0,
            wave: None,
            bulge: None,
            neck: None,
            extent: Liquid::ENDLESS,
            bounds: [Point::new(0.0, 0.0), Point::new(60.0, 35.0)],
            color: orange(),
        }
    }

    /// [`liquid`] pulled off its edge, a neck joining the two, the edge
    /// swelling under it.
    pub fn pulled() -> Liquid {
        Liquid {
            p0: Point::new(20.0, 16.0),
            p1: Point::new(40.0, 34.0),
            bulge: Some(super::Bulge {
                center: 30.0,
                height: 3.0,
            }),
            neck: Some(super::Neck {
                from: Point::new(26.0, 10.0),
                to: Point::new(30.0, 16.0),
                end: 5.0,
                waist: 3.0,
                blend: 6.0,
                cut: 0.0,
            }),
            // The bar ends 10 past the box on either side.
            extent: [10.0, 50.0],
            bounds: [Point::new(0.0, 0.0), Point::new(60.0, 36.0)],
            ..liquid()
        }
    }

    /// The edge of [`liquid`] with waves on it, the box sunk into the edge's
    /// side, as after the card has folded back.
    pub fn waves() -> Liquid {
        Liquid {
            p0: Point::new(30.0, 0.0),
            p1: Point::new(30.0, 2.0),
            wave: Some(super::Wave {
                center: 30.0,
                amplitude: 6.0,
                group: 8.0,
                phase: 16.0,
            }),
            // Ending close enough that the waves flatten towards both ends.
            extent: [5.0, 55.0],
            ..liquid()
        }
    }

    /// Three by two physical pixels of coverage from (2, 2), at scale 2, so
    /// from (1, 1) in logical pixels.
    pub fn text() -> Text {
        Text {
            origin: (2, 2),
            size: (3, 2),
            coverage: vec![0, 128, 64, 32, 200, 255],
            scale: 2.0,
            color: white(),
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
        let most = dx.min(dy);
        let rounded = RoundedRect {
            p0: rect.p0,
            p1: rect.p1,
            radii: [(); 4].map(|()| dice.between(0.0, most)),
            color,
        };
        let r = dice.coordinate(0.5, h.max(4.0) / 2.0);
        let corner = Point::new(x + dice.between(-r, r), y + dice.between(-r, r));
        let fillet = Fillet {
            p0: rect.p0,
            p1: rect.p1,
            center: corner,
            r: r + dice.between(0.0, 2.0),
            color,
        };
        // An edge along one of the four sides, as a bar's, through the
        // rectangle, with the bounds reaching past it into the edge's side.
        let normal =
            [(0.0, 1.0), (0.0, -1.0), (1.0, 0.0), (-1.0, 0.0)][(dice.roll() * 4.0) as usize % 4];
        let normal = Point::new(normal.0, normal.1);
        let into = dice.between(0.0, 2.0 * dx.min(dy).max(1.0));
        let offset = (x * normal.x + y * normal.y) - dx.min(dy) + into / 2.0;
        let reach = dice.between(0.0, 3.0 * r);
        let liquid = Liquid {
            p0: rect.p0,
            p1: rect.p1,
            radii: rounded.radii,
            normal,
            offset,
            blend: dice.between(0.0, r),
            wave: (dice.roll() < 0.5).then(|| super::Wave {
                center: dice.coordinate(-5.0, w + 5.0),
                amplitude: dice.between(-4.0, 8.0),
                group: dice.between(0.0, 40.0),
                phase: dice.between(0.0, 80.0),
            }),
            bulge: (dice.roll() < 0.4).then(|| super::Bulge {
                center: dice.coordinate(-5.0, w + 5.0),
                height: dice.between(-6.0, 10.0),
            }),
            neck: (dice.roll() < 0.4).then(|| super::Neck {
                from: Point::new(x + dice.between(-r, r), y + dice.between(-r, r)),
                to: Point::new(x + dice.between(-r, r), y + dice.between(-2.0 * r, 2.0 * r)),
                end: dice.between(0.5, r),
                waist: dice.between(0.0, r / 2.0),
                blend: dice.between(0.0, 12.0),
                cut: if dice.roll() < 0.5 {
                    0.0
                } else {
                    dice.between(0.0, r / 2.0)
                },
            }),
            extent: if dice.roll() < 0.5 {
                Liquid::ENDLESS
            } else {
                let s = if normal.y != 0.0 { x } else { y };
                let half = dx.max(dy);
                [
                    s - dice.between(-half, 2.0 * half),
                    s + dice.between(-half, 2.0 * half),
                ]
            },
            bounds: [
                Point::new(rect.p0.x - reach, rect.p0.y - reach),
                Point::new(rect.p1.x + reach, rect.p1.y + reach),
            ],
            color,
        };
        match (dice.roll() * 9.0) as u32 {
            0 => Box::new(rect),
            1 => Box::new(circle),
            2 => Box::new(rect.outline(width, color)),
            3 => Box::new(circle.outline(width, color)),
            4 => Box::new(rounded),
            5 => Box::new(rounded.outline(width, color)),
            6 => Box::new(liquid),
            7 => Box::new(liquid.outline(width, color)),
            _ => Box::new(fillet),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{
        circle, fillet, liquid, pulled, rounded, square, text, uneven, waves, white,
    };
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
        assert_bounds_enclose(&rounded());
        assert_bounds_enclose(&uneven());
        assert_bounds_enclose(&rounded().outline(2.0, white()));
        assert_bounds_enclose(&fillet());
        assert_bounds_enclose(&fillet().outline(1.5, white()));
        assert_bounds_enclose(&liquid());
        assert_bounds_enclose(&liquid().outline(1.5, white()));
        assert_bounds_enclose(&text());
        assert_bounds_enclose(&waves());
        assert_bounds_enclose(&pulled());
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
        assert_stretches_hold(&rounded());
        assert_stretches_hold(&uneven());
        assert_stretches_hold(&rounded().outline(2.0, white()));
        assert_stretches_hold(&fillet());
        assert_stretches_hold(&fillet().outline(1.5, white()));
        assert_stretches_hold(&liquid());
        assert_stretches_hold(&liquid().outline(1.5, white()));
        assert_stretches_hold(&text());
        assert_stretches_hold(&waves());
        assert_stretches_hold(&pulled());
    }

    /// However far apart its ends lie along the edge, a neck leaves the
    /// edge and reaches the box square to the edge, and is as wide as its
    /// waist across its slope in the middle.
    #[test]
    fn a_neck_stands_square_on_its_edge_at_both_ends() {
        let neck = super::Neck {
            from: Point::new(0.0, 0.0),
            to: Point::new(40.0, 30.0),
            end: 4.0,
            waist: 2.0,
            blend: 0.0,
            cut: 0.0,
        };
        let down = Point::new(0.0, 1.0);
        for ((x, y), want) in [
            ((0.0, 0.0), -4.0),
            ((4.0, 0.0), 0.0),
            ((-4.0, 0.0), 0.0),
            ((0.0, -3.0), -1.0),
            ((40.0, 30.0), -4.0),
            ((44.0, 30.0), 0.0),
            ((36.0, 30.0), 0.0),
            ((40.0, 33.0), -1.0),
            // Half way up, it runs 2 along for 1 up.
            ((20.0 + 2.0 * 5f32.sqrt(), 15.0), 0.0),
        ] {
            let got = neck.distance(down, Point::new(x, y));
            assert!((got - want).abs() < 1e-4, "at ({x}, {y}): {got}");
        }
    }

    /// Cut by more than its waist, a neck has nothing left in its middle,
    /// not even a hairline, and horns at its ends.
    #[test]
    fn a_neck_cut_past_its_waist_parts_in_the_middle() {
        let neck = super::Neck {
            from: Point::new(0.0, 0.0),
            to: Point::new(0.0, 40.0),
            end: 8.0,
            waist: 4.0,
            blend: 0.0,
            cut: 5.0,
        };
        let down = Point::new(0.0, 1.0);
        assert_eq!(neck.distance(down, Point::new(0.0, 20.0)), 1.0);
        assert!(neck.distance(down, Point::new(0.0, 1.0)) < 0.0);
        assert!(neck.distance(down, Point::new(0.0, 39.0)) < 0.0);
        // At its ends, as wide as the ends less the cut.
        assert!((neck.distance(down, Point::new(3.0, 0.0))).abs() < 1e-4);
    }

    /// Every shape, including new ones, belongs in this list.
    #[test]
    fn sdf_matches_distance_for_every_shape() {
        assert_sdf_matches(&square());
        assert_sdf_matches(&circle());
        assert_sdf_matches(&square().outline(3.0, white()));
        assert_sdf_matches(&circle().outline(3.0, white()));
        assert_sdf_matches(&circle().outline(2.0, white()).outline(1.0, white()));
        assert_sdf_matches(&rounded());
        assert_sdf_matches(&uneven());
        assert_sdf_matches(&rounded().outline(2.0, white()));
        assert_sdf_matches(&fillet());
        assert_sdf_matches(&fillet().outline(1.5, white()));
        assert_sdf_matches(&liquid());
        assert_sdf_matches(&liquid().outline(1.5, white()));
        assert_sdf_matches(&text());
        assert_sdf_matches(&waves());
        assert_sdf_matches(&pulled());
    }

    #[test]
    fn the_gpu_draws_every_shape_but_text_and_an_outline_of_an_outline() {
        assert!(square().sdf().is_some());
        assert!(circle().sdf().is_some());
        assert!(square().outline(3.0, white()).sdf().is_some());
        assert!(circle().outline(3.0, white()).sdf().is_some());
        assert!(rounded().sdf().is_some());
        assert!(uneven().sdf().is_some());
        assert!(rounded().outline(2.0, white()).sdf().is_some());
        assert!(fillet().sdf().is_some());
        assert!(liquid().sdf().is_some());
        assert!(
            circle()
                .outline(2.0, white())
                .outline(1.0, white())
                .sdf()
                .is_none()
        );
        assert!(text().sdf().is_none());
    }
}
