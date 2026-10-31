//! The renderer's description of a frame.
//!
//! Shapes are described by their signed distance to their own edge rather
//! than by pixels or paths. That single quantity yields antialiasing,
//! outlines and independence from the output scale, and it is exactly what a
//! GPU fragment shader evaluates, so every backend can rasterise the same
//! [`Scene`] the same way.

/// A colour with 8-bit channels, opacity included.
///
/// Every channel matches the precision of the buffers it ends up in, so
/// storing more would gain nothing visible. Fractional work such as
/// antialiasing coverage happens per pixel in the backends, and animations
/// derive a colour from their own `f32` progress every frame rather than
/// stepping a stored one, so rounding never accumulates.
///
/// Constructors take the opacity as a fraction in `0.0..=1.0`, which is how
/// designs and animations express it; values outside are clamped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Converts a fractional opacity to a byte, rounding to the nearest step.
fn opacity(a: f32) -> u8 {
    (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

impl Rgba {
    pub fn from_rgb(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a: opacity(a) }
    }

    /// Takes the opacity separately so that a `0xRRGGBB` literal copied from
    /// a design tool is never mistaken for `0xAARRGGBB`. Bits above the
    /// lowest 24 are ignored.
    pub fn from_hex(hex: u32, a: f32) -> Self {
        let c = |s: u32| ((hex >> s) & 0xff) as u8;
        Self::from_rgb(c(16), c(8), c(0), a)
    }

    /// Replaces the opacity instead of scaling it, so the result does not
    /// depend on the opacity the colour was defined with. To fade a colour
    /// that is already translucent, scale `a` directly.
    pub fn alpha(mut self, a: f32) -> Self {
        self.a = opacity(a);
        self
    }
}

/// An edge of an output that a bar can be attached to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Side {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// Bars on the left and right run vertically; layout uses this to decide
    /// which axis is a bar's length and which its thickness.
    pub fn is_vertical(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }
}

/// A position in logical pixels.
///
/// Coordinates are `f32` because edges fall between physical pixels, output
/// scales can be fractional, and animated positions move by fractions of a
/// pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Anything a backend can rasterise.
///
/// Backends only ever call these methods, so a new kind of shape needs no
/// backend changes. `distance` runs once for every pixel a shape may cover,
/// so implementations should keep it cheap.
pub trait Shape {
    /// The top-left and bottom-right corners of a box enclosing the shape,
    /// used to skip pixels it cannot touch. It may be larger than the shape,
    /// which only costs time, but never smaller, which would clip it.
    fn bounds(&self) -> [Point; 2];

    /// Kept apart from the geometry so that wrappers such as [`Outline`] can
    /// draw a shape's geometry in a colour of their own.
    fn color(&self) -> Rgba;

    /// The signed distance from `p` to the edge, in logical pixels: negative
    /// inside, zero on the edge, positive outside. Antialiasing and
    /// [`Outline`] rely on it being the true Euclidean distance near the
    /// edge; an approximation makes edges and outlines uneven in width.
    fn distance(&self, p: Point) -> f32;

    /// Wrapping a shape, rather than giving every shape a stroke of its own,
    /// gives every current and future shape an outline at no cost.
    /// `Self: Sized` keeps the trait usable as `dyn Shape`.
    fn outline(self, width: f32, color: Rgba) -> Outline<Self>
    where
        Self: Sized,
    {
        Outline {
            shape: self,
            width,
            color,
        }
    }
}

/// An axis-aligned rectangle from its top-left corner `p0` to its
/// bottom-right corner `p1`.
///
/// The corners are not normalised: every caller already knows their order,
/// and `distance` runs once per pixel. Swapped corners give wrong bounds and
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
        // The exact box distance: Euclidean beyond the corners, so that
        // antialiasing and outlines stay round there.
        let (cx, cy) = ((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0);
        let (hx, hy) = ((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0);
        let (qx, qy) = ((p.x - cx).abs() - hx, (p.y - cy).abs() - hy);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)
    }
}

/// A filled circle; `r` is its radius in logical pixels.
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
}

/// The shapes to draw, back to front.
///
/// A surface's scene is meant to be cleared and refilled for every frame
/// rather than replaced, so that its allocation is reused.
#[derive(Default)]
pub struct Scene {
    pub shapes: Vec<Box<dyn Shape>>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
    }

    /// Invisible shapes are dropped here, once, so that backends never spend
    /// per-pixel work on them and animations can fade a shape to zero, or
    /// past it, without special cases.
    pub fn push(&mut self, shape: impl Shape + 'static) {
        if shape.color().a == 0 {
            return;
        }
        self.shapes.push(Box::new(shape));
    }
}

/// The outline of any shape, centred on its edge. Built with
/// [`Shape::outline`].
///
/// Centring the line on the edge means an outline drawn over its own fill
/// hides the fill's antialiased fringe instead of leaving a seam. The line
/// has a colour of its own and ignores the wrapped shape's, which is how a
/// border without a fill is drawn: outline a transparent shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline<S> {
    shape: S,
    width: f32,
    color: Rgba,
}

impl<S: Shape> Shape for Outline<S> {
    fn bounds(&self) -> [Point; 2] {
        // The line reaches half its width past the shape's edge.
        let [p0, p1] = self.shape.bounds();
        let w = self.width / 2.0;
        [
            Point::new(p0.x - w, p0.y - w),
            Point::new(p1.x + w, p1.y + w),
        ]
    }

    fn color(&self) -> Rgba {
        self.color
    }

    fn distance(&self, p: Point) -> f32 {
        // Folding the distance turns the edge into a band, which is why
        // `Shape::distance` has to be exact.
        self.shape.distance(p).abs() - self.width / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white() -> Rgba {
        Rgba::from_hex(0xffffff, 1.0)
    }

    fn orange() -> Rgba {
        Rgba::from_hex(0xff8000, 1.0)
    }

    fn transparent() -> Rgba {
        Rgba::from_hex(0xffffff, 0.0)
    }

    /// At the origin with round numbers, so expected distances can be
    /// checked by eye.
    fn square() -> Rect {
        Rect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(10.0, 10.0),
            color: orange(),
        }
    }

    /// Clear of the origin, so that its outlines stay at positive
    /// coordinates.
    fn circle() -> Circle {
        Circle {
            center: Point::new(10.0, 10.0),
            r: 8.0,
            color: orange(),
        }
    }

    /// Backends skip every pixel outside `bounds`, so a covered point
    /// outside them would silently be clipped. Sampled on a quarter-pixel
    /// grid reaching well past the bounds.
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

    #[test]
    fn from_hex_reads_rrggbb() {
        assert_eq!(
            Rgba::from_hex(0xff8000, 0.5),
            Rgba::from_rgb(255, 128, 0, 0.5)
        );
    }

    #[test]
    fn from_hex_cannot_set_the_opacity() {
        assert_eq!(Rgba::from_hex(0x80ff8000, 1.0), orange());
    }

    #[test]
    fn alpha_replaces_the_opacity_and_keeps_the_colour() {
        let c = Rgba::from_hex(0xff8000, 0.2).alpha(0.5);
        assert_eq!(c, Rgba::from_rgb(255, 128, 0, 0.5));
    }

    #[test]
    fn opacity_rounds_to_the_nearest_byte() {
        assert_eq!(white().a, 255);
        assert_eq!(transparent().a, 0);
        // 127.5 rounds up, so half opacity sits a hair above half.
        assert_eq!(white().alpha(0.5).a, 128);
    }

    #[test]
    fn opacity_outside_zero_to_one_is_clamped() {
        // Animations with easing overshoot both ends.
        assert_eq!(white().alpha(1.25).a, 255);
        assert_eq!(white().alpha(-0.25).a, 0);
        assert_eq!(Rgba::from_rgb(1, 2, 3, 2.0), Rgba::from_rgb(1, 2, 3, 1.0));
    }

    #[test]
    fn only_left_and_right_are_vertical() {
        assert!(Side::Left.is_vertical() && Side::Right.is_vertical());
        assert!(!Side::Top.is_vertical() && !Side::Bottom.is_vertical());
    }

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
    fn circle_distance_is_negative_inside_zero_on_the_edge_positive_outside() {
        assert_eq!(circle().distance(Point::new(10.0, 10.0)), -8.0);
        assert_eq!(circle().distance(Point::new(18.0, 10.0)), 0.0);
        assert_eq!(circle().distance(Point::new(10.0, 21.0)), 3.0);
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

    #[test]
    fn an_outline_is_a_band_centred_on_the_edge() {
        let ring = circle().outline(2.0, white());
        assert_eq!(ring.distance(Point::new(10.0, 2.0)), -1.0);
        assert_eq!(ring.distance(Point::new(10.0, 1.0)), 0.0);
        assert_eq!(ring.distance(Point::new(10.0, 3.0)), 0.0);
        assert_eq!(
            ring.bounds(),
            [Point::new(1.0, 1.0), Point::new(19.0, 19.0)]
        );
    }

    #[test]
    fn an_outline_is_hollow() {
        let ring = circle().outline(2.0, white());
        let frame = square().outline(2.0, white());
        assert_eq!(ring.distance(Point::new(10.0, 10.0)), 7.0);
        assert_eq!(frame.distance(Point::new(5.0, 5.0)), 4.0);
    }

    #[test]
    fn a_rect_outline_reaches_half_its_width_past_the_rect() {
        let frame = square().outline(2.0, white());
        assert_eq!(frame.distance(Point::new(0.0, 5.0)), -1.0);
        assert_eq!(frame.distance(Point::new(11.0, 5.0)), 0.0);
        assert_eq!(
            frame.bounds(),
            [Point::new(-1.0, -1.0), Point::new(11.0, 11.0)]
        );
    }

    #[test]
    fn an_outline_uses_its_own_colour() {
        assert_eq!(square().outline(1.0, white()).color(), white());
    }

    #[test]
    fn an_outline_of_a_transparent_shape_is_a_border_without_a_fill() {
        let mut s = Scene::new();
        let hidden = Rect {
            color: transparent(),
            ..square()
        };
        s.push(hidden);
        s.push(hidden.outline(2.0, white()));
        assert_eq!(s.shapes.len(), 1);
        assert_eq!(s.shapes[0].color(), white());
    }

    #[test]
    fn outlines_nest_into_parallel_lines() {
        let double = circle().outline(2.0, white()).outline(1.0, white());
        assert_eq!(double.distance(Point::new(10.0, 1.0)), -0.5);
        assert_eq!(double.distance(Point::new(10.0, 3.0)), -0.5);
        assert_eq!(double.distance(Point::new(10.0, 2.0)), 0.5);
        assert_eq!(
            double.bounds(),
            [Point::new(0.5, 0.5), Point::new(19.5, 19.5)]
        );
    }

    #[test]
    fn a_scene_keeps_shapes_of_any_kind_in_push_order() {
        let mut s = Scene::new();
        s.push(square());
        s.push(circle());
        s.push(square().outline(2.0, white()));
        let corners: Vec<Point> = s.shapes.iter().map(|shape| shape.bounds()[0]).collect();
        assert_eq!(
            corners,
            [
                Point::new(0.0, 0.0),
                Point::new(2.0, 2.0),
                Point::new(-1.0, -1.0)
            ]
        );
    }

    #[test]
    fn a_scene_drops_what_cannot_be_seen() {
        let mut s = Scene::new();
        s.push(Rect {
            color: transparent(),
            ..square()
        });
        // A fade that overshoots below zero.
        s.push(Circle {
            color: orange().alpha(-0.25),
            ..circle()
        });
        s.push(square().outline(2.0, transparent()));
        assert!(s.shapes.is_empty());
    }

    #[test]
    fn clearing_a_scene_keeps_its_allocation() {
        let mut s = Scene::new();
        s.push(square());
        s.push(circle());
        let capacity = s.shapes.capacity();
        s.clear();
        assert!(s.shapes.is_empty());
        assert_eq!(s.shapes.capacity(), capacity);
    }
}
