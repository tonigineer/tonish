//! Where a card is and what its liquid is doing, at one moment.

use engine::Side;
use renderer::Point;

use super::Frame;
use super::draw::smoothstep;

/// The grip: its size along the bar and across it, how far it sits in from
/// the card's edge nearest the bar, and the larger stretch that takes the
/// pointer, as pleama's.
pub(super) const GRIP: (f32, f32) = (30.0, 3.0);
pub(super) const GRIP_IN: f32 = 9.0;
const GRIP_HIT: (f32, f32) = (132.0, 18.0);

/// Where the card starts to leave the bar: its middle this many pixels
/// further out than where it hangs, as pleama's.
const LEAVE: f32 = 14.0;

/// A neck reaches the card while its foot, under the card's middle, is at
/// least `NECK_ROOM` and `OFF_BAR` in from the ends of the bar's straight
/// edge, and thins away to nothing over `OFF_BAR` closer to them, so that it
/// never hangs from the bar's very end, or from nothing.
const NECK_ROOM: f32 = 40.0;
const OFF_BAR: f32 = 40.0;

fn length((w, h): (f32, f32), side: Side) -> f32 {
    match side {
        Side::Above | Side::Below => w,
        Side::Left | Side::Right => h,
    }
}

fn depth((w, h): (f32, f32), side: Side) -> f32 {
    match side {
        Side::Above | Side::Below => h,
        Side::Left | Side::Right => w,
    }
}

/// Where the card is and how it moves, in the [`Frame`]'s terms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// The middle of the card, along the bar and away from its edge. It
    /// hangs from the bar with its middle half its depth out.
    pub along: f32,
    pub across: f32,
    /// Its width and height on the monitor, in logical pixels.
    pub size: (f32, f32),
    /// 0 shut, 1 open, past 1 while it bounces.
    pub grow: f32,
    /// How much it stretches along and across, from how fast it moves.
    pub stretch: (f32, f32),
    /// The bar's edge kicked when the neck breaks or joins, in pixels out
    /// of it.
    pub bounce: f32,
    /// Waves on the bar's edge: where they started along it, and seconds
    /// since.
    pub ripple: Option<(f32, f32)>,
    /// The bead a collapsing neck leaves, which falls into the card: where
    /// along the bar the neck broke, and how much of the bead is left, from 1
    /// to 0.
    pub drop: Option<(f32, f32)>,
    /// 0 to 1, as the pointer is over the grip or holds it.
    pub grip: f32,
    /// How far its middle goes out from where it hangs before the neck
    /// breaks.
    pub reach: f32,
    /// Along the bar, where its middle may hang.
    pub dock: (f32, f32),
    /// As [`Frame::bar`].
    pub bar: (f32, f32),
    /// Whether a neck joins it to the bar: not from when the neck breaks
    /// until the card comes close enough again to reach for.
    pub joined: bool,
    /// The neck since it broke: where along the bar, and how much of the
    /// two stubs it left is still out, from 1 as it breaks to 0 once they
    /// have snapped back into the bar and the card.
    pub collapse: Option<(f32, f32)>,
}

impl Pose {
    /// Its length along the bar.
    pub fn length(&self, side: Side) -> f32 {
        length(self.size, side)
    }

    /// Its depth away from the bar.
    pub fn depth(&self, side: Side) -> f32 {
        depth(self.size, side)
    }

    /// Half its length and depth, as [`Frame::keep`] takes them.
    pub fn half(&self, side: Side) -> (f32, f32) {
        (self.length(side) / 2.0, self.depth(side) / 2.0)
    }

    /// Where a card of `size` hangs from the bar, `along` it.
    pub fn hanging(along: f32, size: (f32, f32), side: Side) -> Self {
        Self {
            along,
            across: depth(size, side) / 2.0,
            size,
            grow: 0.0,
            stretch: (0.0, 0.0),
            bounce: 0.0,
            ripple: None,
            drop: None,
            grip: 0.0,
            reach: 140.0,
            dock: (f32::NEG_INFINITY, f32::INFINITY),
            bar: (f32::NEG_INFINITY, f32::INFINITY),
            joined: true,
            collapse: None,
        }
    }

    /// How far it has pulled away from the bar: 0 hanging, 1 as the neck
    /// breaks.
    pub fn pulled(&self, side: Side) -> f32 {
        let pulled = self.across - self.depth(side) / 2.0;
        ((pulled - LEAVE) / (self.reach - LEAVE)).clamp(0.0, 1.0)
    }

    /// How far it has left the bar: as [`Pose::pulled`], and free too where
    /// no neck could reach it from under the bar.
    pub fn detached(&self, side: Side) -> f32 {
        let lo = self.bar.0 + NECK_ROOM + OFF_BAR;
        let hi = (self.bar.1 - NECK_ROOM - OFF_BAR).max(lo);
        let off = (lo - self.along).max(self.along - hi).max(0.0);
        self.pulled(side).max(smoothstep(0.0, OFF_BAR, off))
    }

    /// Its corners once it is open, where it is now, which take the
    /// pointer.
    pub fn rect(&self, frame: &Frame) -> [Point; 2] {
        let (l, a) = self.half(frame.side);
        frame.rect(
            (self.along - l, self.across - a),
            (self.along + l, self.across + a),
        )
    }

    /// Whether `at`, on the monitor, is on its grip.
    pub fn on_grip(&self, frame: &Frame, at: (f32, f32)) -> bool {
        let (along, across) = frame.local(at);
        let near = self.across - self.depth(frame.side) / 2.0;
        (along - self.along).abs() <= GRIP_HIT.0 / 2.0
            && (near..near + GRIP_HIT.1).contains(&across)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (f32, f32) = (280.0, 180.0);

    fn frame() -> Frame {
        Frame {
            side: Side::Below,
            edge: 44.0,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            bar: (0.0, 1920.0),
        }
    }

    #[test]
    fn a_neck_reaches_a_card_only_from_under_the_bar() {
        // A bar from 100 to 800; the neck's foot reaches to 720 whole, and
        // thins away by 760.
        let mut card = Pose {
            dock: (254.0, 646.0),
            bar: (100.0, 800.0),
            ..Pose::hanging(400.0, SIZE, Side::Below)
        };
        assert_eq!(card.detached(Side::Below), 0.0);
        // Past where it docks, but under the bar.
        card.along = 720.0;
        assert_eq!(card.detached(Side::Below), 0.0);
        card.along = 740.0;
        assert!(card.detached(Side::Below) > 0.0);
        card.along = 760.0;
        assert_eq!(card.detached(Side::Below), 1.0);
    }

    #[test]
    fn a_card_of_any_size_hangs_as_deep_as_it_is() {
        let card = Pose::hanging(400.0, (200.0, 120.0), Side::Below);
        assert_eq!(card.across, 60.0);
        let [p0, p1] = card.rect(&frame());
        assert_eq!(
            (p0, p1),
            (Point::new(300.0, 44.0), Point::new(500.0, 164.0))
        );
        let across = Pose::hanging(400.0, (200.0, 120.0), Side::Right);
        assert_eq!(across.across, 100.0);
    }

    #[test]
    fn a_card_leaves_the_bar_between_14_and_140_pixels_out() {
        let mut card = Pose::hanging(400.0, SIZE, Side::Below);
        assert_eq!(card.detached(Side::Below), 0.0);
        card.across += 14.0 + 63.0;
        assert_eq!(card.detached(Side::Below), 0.5);
        card.across += 100.0;
        assert_eq!(card.detached(Side::Below), 1.0);
    }

    #[test]
    fn the_grip_is_in_the_middle_of_the_cards_edge_at_the_bar() {
        let card = Pose::hanging(400.0, SIZE, Side::Below);
        assert!(card.on_grip(&frame(), (400.0, 44.0 + 9.0)));
        assert!(card.on_grip(&frame(), (340.0, 44.0 + 2.0)));
        assert!(!card.on_grip(&frame(), (400.0, 44.0 + 40.0)));
        assert!(!card.on_grip(&frame(), (200.0, 44.0 + 9.0)));
    }
}
