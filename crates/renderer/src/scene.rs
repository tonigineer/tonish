#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Rgba {
    pub fn from_rgb(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(hex: u32, a: f32) -> Self {
        let c = |s: u32| ((hex >> s) & 0xff) as u8;
        Self::from_rgb(c(16), c(8), c(0), a)
    }

    pub fn alpha(mut self, a: f32) -> Self {
        self.a = a;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Side {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub fn is_vertical(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }
}

/// A point in logical pixels: f32, since edges fall between pixels and
/// scales are fractional.
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

/// What a backend needs to know of a shape.
pub trait Shape {
    /// The corners of the box it lies in: top left, bottom right.
    fn bounds(&self) -> [Point; 2];

    fn color(&self) -> Rgba;

    fn distance(&self, p: Point) -> f32;

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
        let (cx, cy) = ((self.p0.x + self.p1.x) / 2.0, (self.p0.y + self.p1.y) / 2.0);
        let (hx, hy) = ((self.p1.x - self.p0.x) / 2.0, (self.p1.y - self.p0.y) / 2.0);
        let (qx, qy) = ((p.x - cx).abs() - hx, (p.y - cy).abs() - hy);
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)
    }
}

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

    pub fn push(&mut self, shape: impl Shape + 'static) {
        if shape.color().a <= 0.0 {
            return;
        }
        self.shapes.push(Box::new(shape));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline<S> {
    shape: S,
    width: f32,
    color: Rgba,
}

impl<S: Shape> Shape for Outline<S> {
    fn bounds(&self) -> [Point; 2] {
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
        self.shape.distance(p).abs() - self.width / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_red_from_rgb() {
        let rgba = Rgba::from_rgb(255, 0, 0, 1.0);
        assert_eq!(rgba.r, 255);
        assert_eq!(rgba.g, 0);
        assert_eq!(rgba.b, 0);
        assert_eq!(rgba.a, 1.0);
    }

    #[test]
    fn create_red_from_hex() {
        let rgba = Rgba::from_hex(0xff0000, 1.0);
        assert_eq!(rgba, Rgba::from_rgb(255, 0, 0, 1.0));
    }

    #[test]
    fn change_alpha() {
        let rgba = Rgba::from_hex(0xff0000, 1.0).alpha(0.5);
        assert_eq!(rgba.a, 0.5);
    }

    #[test]
    fn left_and_right_are_vertical() {
        assert!(Side::Left.is_vertical() && Side::Right.is_vertical());
        assert!(!Side::Top.is_vertical() && !Side::Bottom.is_vertical());
    }

    #[test]
    fn a_scene_holds_different_shapes_in_order() {
        let white = Rgba::from_hex(0xffffff, 1.0);
        let mut s = Scene::new();
        s.push(Rect {
            p0: Point::new(10.0, 20.0),
            p1: Point::new(110.0, 60.0),
            color: white,
        });
        s.push(Circle {
            center: Point::new(5.0, 5.0),
            r: 4.0,
            color: white,
        });
        assert_eq!(s.shapes.len(), 2);
        assert_eq!(
            s.shapes[0].bounds(),
            [Point::new(10.0, 20.0), Point::new(110.0, 60.0)]
        );
        assert_eq!(
            s.shapes[1].bounds(),
            [Point::new(1.0, 1.0), Point::new(9.0, 9.0)]
        );
        // Inside below 0, on the edge 0, outside above 0.
        assert_eq!(s.shapes[1].distance(Point::new(5.0, 5.0)), -4.0);
        assert_eq!(s.shapes[1].distance(Point::new(9.0, 5.0)), 0.0);
        assert_eq!(s.shapes[0].distance(Point::new(0.0, 40.0)), 10.0);
    }

    #[test]
    fn what_cannot_be_seen_is_not_drawn() {
        let mut s = Scene::new();
        s.push(Circle {
            center: Point::new(5.0, 5.0),
            r: 4.0,
            color: Rgba::from_hex(0xffffff, 0.0),
        });
        s.push(square().outline(2.0, Rgba::from_hex(0xffffff, 0.0)));
        assert!(s.shapes.is_empty());
    }

    fn white() -> Rgba {
        Rgba::from_hex(0xffffff, 1.0)
    }

    fn square() -> Rect {
        Rect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(10.0, 10.0),
            color: Rgba::from_hex(0xff8000, 1.0),
        }
    }

    fn circle() -> Circle {
        Circle {
            center: Point::new(10.0, 10.0),
            r: 8.0,
            color: Rgba::from_hex(0xff8000, 1.0),
        }
    }

    #[test]
    fn a_rect_corner_is_as_far_as_the_diagonal() {
        assert_eq!(square().distance(Point::new(5.0, 5.0)), -5.0);
        assert_eq!(square().distance(Point::new(10.0, 5.0)), 0.0);
        assert_eq!(square().distance(Point::new(13.0, 14.0)), 5.0);
    }

    #[test]
    fn a_ring_is_a_band_around_the_edge() {
        let ring = circle().outline(2.0, white());
        assert_eq!(ring.distance(Point::new(10.0, 10.0)), 7.0);
        assert_eq!(ring.distance(Point::new(10.0, 2.0)), -1.0);
        assert_eq!(ring.distance(Point::new(10.0, 1.0)), 0.0);
        assert_eq!(ring.distance(Point::new(10.0, 3.0)), 0.0);
        assert_eq!(
            ring.bounds(),
            [Point::new(1.0, 1.0), Point::new(19.0, 19.0)]
        );
    }

    #[test]
    fn a_rect_outline_is_hollow_and_reaches_past_the_rect() {
        let frame = square().outline(2.0, white());
        assert_eq!(frame.distance(Point::new(0.0, 5.0)), -1.0);
        assert_eq!(frame.distance(Point::new(11.0, 5.0)), 0.0);
        assert_eq!(frame.distance(Point::new(5.0, 5.0)), 4.0);
        assert_eq!(
            frame.bounds(),
            [Point::new(-1.0, -1.0), Point::new(11.0, 11.0)]
        );
    }

    #[test]
    fn an_outline_has_its_own_colour() {
        let frame = square().outline(1.0, white());
        assert_eq!(frame.color(), white());
        assert_ne!(frame.color(), square().color());
    }

    #[test]
    fn an_outline_is_drawn_even_when_its_shape_is_not() {
        let mut s = Scene::new();
        let hidden = Rect {
            color: Rgba::from_hex(0xffffff, 0.0),
            ..square()
        };
        s.push(hidden);
        s.push(hidden.outline(2.0, white()));
        assert_eq!(s.shapes.len(), 1);
        assert_eq!(s.shapes[0].color(), white());
    }

    #[test]
    fn an_outline_of_an_outline_is_two_lines() {
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
    fn a_fill_and_its_outline_are_drawn_in_order() {
        let mut s = Scene::new();
        s.push(square());
        s.push(square().outline(2.0, white()));
        assert_eq!(s.shapes.len(), 2);
        assert_eq!(s.shapes[0].color(), square().color());
        assert_eq!(s.shapes[1].color(), white());
        assert_eq!(s.shapes[1].bounds()[0], Point::new(-1.0, -1.0));
    }
}
