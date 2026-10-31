use crate::shapes::Shape;

/// 8-bit channels, because that is the precision of the buffers a colour
/// ends up in. Fractional work such as antialiasing coverage happens per
/// pixel in the backends, and animations derive a colour from their own
/// `f32` progress every frame rather than stepping a stored one, so rounding
/// never accumulates.
///
/// Constructors take the opacity as a fraction, because that is how designs
/// and animations express it. They clamp it, since eased animations
/// overshoot both ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

fn opacity(a: f32) -> u8 {
    (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

impl Rgba {
    pub fn from_rgb(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self {
            r,
            g,
            b,
            a: opacity(a),
        }
    }

    /// Takes the opacity separately so that a `0xRRGGBB` literal copied from
    /// a design tool is never mistaken for `0xAARRGGBB`.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Orientation {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl Orientation {
    pub fn is_vertical(self) -> bool {
        matches!(self, Orientation::Left | Orientation::Right)
    }
}

/// In logical pixels, so that layout never depends on the output's scale.
/// `f32`, because edges fall between physical pixels, scales can be
/// fractional, and animated positions move by fractions of a pixel.
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

/// Back to front. Clear and refill a surface's scene every frame instead of
/// replacing it, so that its allocation is reused.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::fixtures::{circle, orange, square, transparent, white};
    use crate::shapes::{Circle, Rect};

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
        assert!(Orientation::Left.is_vertical() && Orientation::Right.is_vertical());
        assert!(!Orientation::Top.is_vertical() && !Orientation::Bottom.is_vertical());
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
