//! A card's life, from opening to folding back: where it is and how it
//! moves, on springs, as pleama's cards do, and what the pointer does to
//! it. The shell routes events here and draws what [`Card::pose`] says.

use std::time::Instant;

use engine::{Cursor, PopupSpec};

use super::{Frame, Pose};
use crate::config::{Animation, Motion};
use crate::motion::Spring;

/// The bead the neck leaves falls into the card in 0.7 s.
const STRAY: (f32, f32) = (90.0, 1.0);

/// The stubs a collapsed neck leaves snap back in a tenth of a second, a
/// little past where they stood.
const RECOIL: (f32, f32) = (700.0, 0.4);

/// How much the card stretches for its speed, and at most, with a wobble
/// amount of 1.
const SQUASH: f32 = 0.0001;
const SQUASH_MAX: f32 = 0.15;

/// Dragged along the bar past where it may dock, the share of the hand's
/// way the card follows along it, and how far it sags off the bar for each
/// pixel of that way.
const SLIDE: f32 = 0.5;
const SAG: f32 = 1.2;

/// A card that hangs from the bar, from growing out of it to folding back.
/// Its surface is the popup, which covers the monitor while the card is
/// open, so that it never has to be resized, however far the card goes.
pub struct Card {
    pub spec: PopupSpec,
    pub frame: Frame,
    /// Its width and height on the monitor, in logical pixels.
    pub size: (f32, f32),
    /// The settings' animation section, which a save changes in the middle
    /// of a motion.
    pub anim: Animation,
    /// Along the bar, where the card's middle may hang: under the bar's
    /// straight edge, with room for its concave corners.
    dock: (f32, f32),
    grow: Spring,
    along: Spring,
    across: Spring,
    bounce: Spring,
    stray: Spring,
    /// How much it stretches along and across: springs that chase its
    /// speed, so that it overshoots into a squash and wobbles once it stops.
    squash: (Spring, Spring),
    /// From 0 as the neck breaks to 1 once its stubs are back in.
    recoil: Spring,
    /// Seconds since the neck last broke.
    since: f32,
    /// While the grip is held: where on the card it was taken, from the
    /// card's middle, along and across.
    held: Option<(f32, f32)>,
    /// Off the bar since its neck broke, until it docks again: a neck that
    /// reaches out to it again doesn't bring it back by itself.
    out: bool,
    hover: bool,
    /// Where waves started on the bar's edge, and seconds since: a clock
    /// rather than a spring, since waves run at a steady speed.
    ripple: Option<(f32, f32)>,
    /// Waves have run since it began to fold back, so it leaves only one
    /// set.
    folded: bool,
    drop_at: f32,
    /// How far the card had left the bar at the last frame, to see it cross
    /// the points where the neck breaks and joins.
    was: f32,
    /// When its last frame was drawn, which the next one steps from. `None`
    /// while it rests, so that a motion starting after a pause starts with
    /// one frame's step rather than the whole pause.
    pub drawn: Option<Instant>,
}

impl Card {
    /// A card of `size` that hangs at `home` along the bar, and may dock
    /// anywhere in `dock`.
    pub fn new(
        spec: PopupSpec,
        frame: Frame,
        size: (f32, f32),
        home: f32,
        dock: (f32, f32),
        anim: Animation,
    ) -> Self {
        let card = Pose::hanging(home, size, frame.side);
        let mut grow = Spring::new(0.0);
        aim(&mut grow, 1.0, anim.popout);
        let mut stray = Spring::new(0.0);
        stray.aim(0.0, STRAY.0, STRAY.1);
        Self {
            spec,
            frame,
            size,
            anim,
            dock,
            grow,
            along: Spring::new(card.along),
            across: Spring::new(card.across),
            bounce: Spring::new(0.0),
            stray,
            squash: (Spring::new(0.0), Spring::new(0.0)),
            recoil: Spring::new(1.0),
            since: f32::INFINITY,
            held: None,
            out: false,
            hover: false,
            ripple: None,
            folded: false,
            drop_at: home,
            was: 0.0,
            drawn: None,
        }
    }

    pub fn closing(&self) -> bool {
        self.grow.target <= 0.0
    }

    pub fn out(&self) -> bool {
        self.out
    }

    /// Something moves, so that it needs another frame. Holding the grip
    /// still needs none: the next drag asks for one.
    pub fn moving(&self) -> bool {
        [
            &self.grow,
            &self.along,
            &self.across,
            &self.bounce,
            &self.stray,
            &self.squash.0,
            &self.squash.1,
            &self.recoil,
        ]
        .iter()
        .any(|s| !s.settled())
            || self
                .ripple
                .is_some_and(|(_, t)| t < self.anim.ripples.duration)
    }

    pub fn gone(&self) -> bool {
        self.closing() && !self.moving()
    }

    pub fn pose(&self) -> Pose {
        let mut card = Pose {
            along: self.along.value,
            across: self.across.value,
            size: self.size,
            grow: self.grow.value,
            stretch: (self.squash.0.value, self.squash.1.value),
            bounce: self.bounce.value,
            ripple: self.ripple,
            drop: (self.stray.value > 0.01).then_some((self.drop_at, self.stray.value)),
            grip: if self.held.is_some() {
                1.0
            } else if self.hover {
                0.5
            } else {
                0.0
            },
            reach: self.anim.detaching.distance,
            dock: self.dock,
            bar: self.frame.bar,
            joined: true,
            collapse: None,
        };
        // Broken, it reaches for the card again only once the card is well
        // back inside where it would break, and the stubs have gone.
        let det = card.detached(self.frame.side);
        card.joined = (!self.out && det < 0.995) || (self.out && det < 0.9 && self.since > 0.45);
        let left = (1.0 - self.recoil.value).clamp(0.0, 1.0);
        card.collapse = (!card.joined && left > 0.001).then_some((self.drop_at, left));
        card
    }

    /// Advances everything by `dt` seconds, and sees the neck break or
    /// join.
    pub fn step(&mut self, dt: f32) {
        let side = self.frame.side;
        let a = self.anim;
        // Aimed again at every step, so that a save takes effect at once.
        let grow = if self.closing() { a.fold } else { a.popout };
        let target = self.grow.target;
        aim(&mut self.grow, target, grow);
        aim(&mut self.bounce, 0.0, a.ripples.motion());
        let squash = |v: f32| (v.abs() * SQUASH).min(SQUASH_MAX) * a.wobble.amount;
        let (along, across) = (squash(self.along.velocity), squash(self.across.velocity));
        aim(&mut self.squash.0, along, a.wobble.motion());
        aim(&mut self.squash.1, across, a.wobble.motion());
        for s in [
            &mut self.grow,
            &mut self.along,
            &mut self.across,
            &mut self.bounce,
            &mut self.stray,
            &mut self.squash.0,
            &mut self.squash.1,
            &mut self.recoil,
        ] {
            s.step(dt);
        }
        self.since += dt;
        if let Some((_, t)) = &mut self.ripple {
            // At most 0.1 s, as the springs, so that the waves resume rather
            // than jump after a pause.
            *t += dt.min(0.1);
        }
        let det = self.pose().detached(side);
        if self.held.is_some() && !self.out && det > 0.95 {
            // The neck breaks: the card is free, the bar's edge is kicked
            // back into the bar, which shows only as it springs out again,
            // the card being drawn over the bar and unable to take from it,
            // and the bead the collapse leaves falls into the card. It lets
            // go of the keyboard and of clicks outside once it is let go: the
            // grab's surface has the held button, and destroyed under it, its
            // release would reach no surface on sway.
            self.out = true;
            self.bounce.velocity -= a.detaching.kick;
            self.ripple = Some((self.along.value, 0.0));
            self.drop_at = self.along.value;
            self.stray.value = 1.0;
            self.since = 0.0;
            self.recoil = Spring::new(0.0);
            aim(
                &mut self.recoil,
                1.0,
                Motion {
                    stiffness: RECOIL.0,
                    damping: RECOIL.1,
                },
            );
        } else if self.held.is_none() && self.out && !self.closing() && det < 0.995 {
            // Let go free, it came near enough for a neck on its way to
            // rest: the bar pulls it in.
            self.dock();
        }
        if !self.closing() && self.was >= 0.05 && det < 0.05 {
            self.bounce.velocity += a.attaching.kick;
            self.ripple = Some((self.along.value, 0.0));
        }
        self.was = det;
        if self.closing() && !self.folded && det < 0.5 && self.grow.value < 0.06 {
            // All but back in the bar: it leaves waves where it went in.
            self.folded = true;
            self.ripple = Some((self.along.value, 0.0));
        }
        self.spec.input = if self.closing() {
            None
        } else if self.held.is_some() {
            // The whole monitor while the grip is held, so that the pointer
            // can't leave the popup during the drag: during the grab sway
            // holds on to no surface for a held button.
            let (w, h) = self.frame.screen;
            Some((0, 0, w as i32, h as i32))
        } else {
            let [p0, p1] = self.pose().rect(&self.frame);
            let (x0, y0) = (p0.x.floor() as i32, p0.y.floor() as i32);
            Some((x0, y0, p1.x.ceil() as i32 - x0, p1.y.ceil() as i32 - y0))
        };
    }

    /// Hangs it from where the bar is now, after the settings moved it.
    /// Along and across are the frame's, so a card on its neck or docked
    /// moves with the bar's edge; docked, it slides back within where it may
    /// dock.
    pub fn reframe(&mut self, frame: Frame, dock: (f32, f32)) {
        self.frame = frame;
        self.dock = dock;
        if !self.out && self.held.is_none() && !self.closing() {
            self.dock();
        }
        self.step(0.0);
    }

    /// The pointer left the card, so that the grip goes dark. Returns
    /// whether that needs a frame.
    pub fn leave(&mut self) -> bool {
        std::mem::take(&mut self.hover)
    }

    /// Folds the card back into the bar, or, off the bar, shrinks it where
    /// it is. It lets go of clicks and the keyboard at once.
    pub fn close(&mut self) {
        aim(&mut self.grow, 0.0, self.anim.fold);
        self.spec.grab = false;
        self.spec.input = None;
        self.held = None;
    }

    pub fn on_grip(&self, at: (f32, f32)) -> bool {
        !self.closing() && self.pose().on_grip(&self.frame, at)
    }

    pub fn on_card(&self, at: (f32, f32)) -> bool {
        let [p0, p1] = self.pose().rect(&self.frame);
        (p0.x..p1.x).contains(&at.0) && (p0.y..p1.y).contains(&at.1)
    }

    pub fn hold(&mut self, at: (f32, f32)) {
        let (along, across) = self.frame.local(at);
        self.held = Some((along - self.along.value, across - self.across.value));
        self.step(0.0);
    }

    /// While the grip is held, the card follows the pointer, a little
    /// behind. Returns whether that needs a frame: while it is held, or as
    /// the pointer comes onto the grip or leaves it, which lights it.
    pub fn drag(&mut self, at: (f32, f32)) -> bool {
        let hover = self.on_grip(at);
        let lit = hover != self.hover;
        self.hover = hover;
        let Some((da, dc)) = self.held else {
            return lit;
        };
        let (along, across) = self.frame.local(at);
        let (mut along, mut across) = (along - da, across - dc);
        if !self.out {
            // Past where it may dock, it slides on at half the hand's pace
            // and sags off the bar, as liquid off the end of an edge, until
            // the neck breaks, rather than stopping as against a wall.
            let (lo, hi) = (self.dock.0, self.dock.1.max(self.dock.0));
            let docked = along.clamp(lo, hi);
            across += (along - docked).abs() * SAG;
            along = docked + (along - docked) * SLIDE;
        }
        let half = self.pose().half(self.frame.side);
        let (along, mut across) = self.frame.keep(half, (along, across));
        if !self.out {
            // Pulled by the bar while the neck holds, less as it stretches.
            let hang = Pose::hanging(along, self.size, self.frame.side).across;
            let stretched = Pose {
                across,
                ..self.pose()
            }
            .detached(self.frame.side);
            across -= (across - hang) * self.anim.detaching.pull * (1.0 - stretched);
        }
        self.follow((along, across));
        true
    }

    /// Let go while a neck joins it to the bar, the bar pulls it in, and it
    /// docks where it is along the bar, as far as the bar's straight edge
    /// reaches. Free of the bar, it stays where it was let go.
    pub fn release(&mut self) {
        if self.held.take().is_none() {
            return;
        }
        let side = self.frame.side;
        let card = self.pose();
        let bound = Pose {
            along: self.along.target,
            across: self.across.target,
            ..card
        };
        // Where it is drawn trails the hand, so where it was headed counts
        // too: brought back fast, it is let go further out than it comes to
        // rest.
        if card.detached(side) < 0.995 || bound.detached(side) < 0.995 {
            self.dock();
        } else {
            self.spec.grab = false;
            self.follow((self.along.target, self.across.target));
        }
        self.step(0.0);
    }

    fn dock(&mut self) {
        self.out = false;
        let along = self
            .along
            .value
            .clamp(self.dock.0, self.dock.1.max(self.dock.0));
        let hang = Pose::hanging(along, self.size, self.frame.side);
        let m = self.anim.attaching.motion();
        aim(&mut self.along, hang.along, m);
        aim(&mut self.across, hang.across, m);
    }

    fn follow(&mut self, (along, across): (f32, f32)) {
        aim(&mut self.along, along, self.anim.drag);
        aim(&mut self.across, across, self.anim.drag);
    }

    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    pub fn cursor(&self, at: (f32, f32)) -> Cursor {
        if self.held.is_some() {
            Cursor::Grabbing
        } else if self.on_grip(at) {
            Cursor::Grab
        } else {
            Cursor::Default
        }
    }
}

fn aim(spring: &mut Spring, target: f32, m: Motion) {
    spring.aim(target, m.stiffness, m.damping);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use engine::{OutputId, Side, SurfaceId};

    /// Below a bar 44 high along the top of a monitor 1920 × 1080, at 400.
    fn hanging() -> Card {
        let spec = PopupSpec {
            parent: (OutputId(1), SurfaceId(0)),
            at: (0, 0),
            size: (1920, 1080),
            grab: true,
            input: None,
            parent_at: (0, 0),
        };
        let frame = Frame {
            side: Side::Below,
            edge: 44.0,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            bar: (0.0, 1920.0),
        };
        Card::new(
            spec,
            frame,
            (280.0, 180.0),
            400.0,
            (154.0, 1766.0),
            Config::default().animation,
        )
    }

    fn run(p: &mut Card, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            p.step(1.0 / 60.0);
        }
    }

    fn opened() -> Card {
        let mut p = hanging();
        run(&mut p, 1.5);
        p
    }

    /// Takes the grip and drags it to `y` on the monitor, frame by frame.
    fn pull(p: &mut Card, y: f32) {
        p.hold((400.0, 44.0 + 9.0));
        for i in 1..=30 {
            p.drag((400.0, 53.0 + (y - 53.0) * i as f32 / 30.0));
            p.step(1.0 / 60.0);
        }
        run(p, 0.5);
    }

    #[test]
    fn opened_it_rests_where_it_hangs_and_takes_the_pointer_over_the_card() {
        let p = opened();
        assert!(!p.moving());
        assert_eq!(p.spec.input, Some((260, 44, 280, 180)));
    }

    #[test]
    fn let_go_before_the_neck_breaks_it_springs_back() {
        let mut p = opened();
        pull(&mut p, 140.0);
        assert!(!p.out() && p.spec.grab);
        p.release();
        run(&mut p, 1.0);
        assert!((p.pose().across - 90.0).abs() < 0.5, "{:?}", p.pose());
    }

    #[test]
    fn pulled_far_enough_the_neck_breaks_and_the_card_is_free() {
        let mut p = opened();
        pull(&mut p, 600.0);
        assert!(p.out());
        // Free, it lets go of the keyboard and of clicks outside, once it
        // is let go.
        assert!(p.spec.grab);
        p.release();
        assert!(!p.spec.grab);
        run(&mut p, 1.0);
        assert!(p.pose().across > 400.0, "{:?}", p.pose());
        assert!(p.spec.input.is_some_and(|(_, y, _, _)| y > 400));
    }

    #[test]
    fn the_break_kicks_the_bar_and_drops_a_bead() {
        let mut p = opened();
        p.hold((400.0, 53.0));
        let mut kicked = false;
        let mut bead = false;
        for i in 1..=40 {
            p.drag((400.0, 53.0 + 600.0 * i as f32 / 40.0));
            p.step(1.0 / 60.0);
            kicked |= p.pose().bounce.abs() > 1.0;
            bead |= p.pose().drop.is_some();
        }
        assert!(kicked && bead && p.pose().ripple.is_some());
    }

    #[test]
    fn let_go_near_the_bar_a_free_card_joins_it_again() {
        let mut p = opened();
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        p.hold((400.0, p.pose().across + 44.0 - 90.0 + 9.0));
        let from = p.pose().across + 44.0 - 90.0 + 9.0;
        for i in 1..=30 {
            p.drag((400.0, from + (60.0 - from) * i as f32 / 30.0));
            p.step(1.0 / 60.0);
        }
        p.release();
        assert!(!p.out());
        run(&mut p, 1.0);
        assert!((p.pose().across - 90.0).abs() < 0.5, "{:?}", p.pose());
    }

    /// Takes the grip and drags it to `to`, frame by frame, and holds it
    /// there for half a second.
    fn drag(p: &mut Card, from: (f32, f32), to: (f32, f32)) {
        p.hold(from);
        for i in 1..=30 {
            let t = i as f32 / 30.0;
            p.drag((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
            p.step(1.0 / 60.0);
        }
        run(p, 0.5);
    }

    #[test]
    fn while_the_neck_holds_the_bar_pulls_the_card_back() {
        let mut p = opened();
        pull(&mut p, 140.0);
        // Held 9 px below its top edge, which the pointer at 140 puts 87 px
        // below where it hangs, at 177.
        let across = p.pose().across;
        assert!(across > 100.0 && across < 170.0, "{:?}", p.pose());
    }

    #[test]
    fn let_go_on_its_neck_it_docks_where_it_is_along_the_bar() {
        let mut p = opened();
        drag(&mut p, (400.0, 53.0), (900.0, 140.0));
        assert!(!p.out());
        p.release();
        run(&mut p, 1.0);
        let card = p.pose();
        assert!((card.along - 900.0).abs() < 0.5, "{card:?}");
        assert!((card.across - 90.0).abs() < 0.5, "{card:?}");
    }

    #[test]
    fn it_docks_no_further_than_the_bar_reaches() {
        let mut p = opened();
        // 64 past where it may dock, along the bar: it sags off its end.
        drag(&mut p, (400.0, 53.0), (1830.0, 53.0));
        assert!(!p.out() && p.pose().across > 120.0, "{:?}", p.pose());
        p.release();
        run(&mut p, 1.5);
        assert!((p.pose().along - 1766.0).abs() < 0.5, "{:?}", p.pose());
        assert!((p.pose().across - 90.0).abs() < 0.5, "{:?}", p.pose());
    }

    #[test]
    fn dragged_far_past_where_it_docks_it_tears_off_the_bars_end() {
        let mut p = opened();
        drag(&mut p, (400.0, 53.0), (1910.0, 53.0));
        assert!(p.out(), "{:?}", p.pose());
    }

    #[test]
    fn a_free_card_let_go_while_a_neck_reaches_it_docks() {
        let mut p = opened();
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        // Back to where the neck shows again but has not joined yet: the
        // card's centre 167 px from the bar, half way to breaking.
        let grip = p.pose().across + 44.0 - 90.0 + 9.0;
        drag(&mut p, (400.0, grip), (700.0, 130.0));
        assert!(p.out(), "{:?}", p.pose());
        p.release();
        assert!(!p.out());
        run(&mut p, 1.0);
        let card = p.pose();
        assert!((card.along - 700.0).abs() < 0.5, "{card:?}");
        assert!((card.across - 90.0).abs() < 0.5, "{card:?}");
    }

    #[test]
    fn thrown_back_at_the_bar_a_free_card_docks_where_it_comes_to_rest() {
        let mut p = opened();
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        let grip = p.pose().across + 44.0 - 90.0 + 9.0;
        p.hold((400.0, grip));
        p.drag((400.0, 140.0));
        p.step(1.0 / 60.0);
        p.release();
        run(&mut p, 1.5);
        assert!(!p.out());
        assert!((p.pose().across - 90.0).abs() < 0.5, "{:?}", p.pose());
    }

    #[test]
    fn near_the_bar_but_past_its_end_a_free_card_stays_free() {
        let mut p = opened();
        // A bar that ends at 1140, the card's middle at most at 1000.
        p.frame.bar = (0.0, 1140.0);
        p.dock = (154.0, 1000.0);
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        let grip = p.pose().across + 44.0 - 90.0 + 9.0;
        drag(&mut p, (400.0, grip), (1500.0, 130.0));
        assert!(p.pose().detached(Side::Below) >= 0.995, "{:?}", p.pose());
        p.release();
        run(&mut p, 1.5);
        assert!(p.out());
        assert!(p.pose().across > 150.0, "{:?}", p.pose());
    }

    #[test]
    fn let_go_under_the_end_of_the_bar_a_free_card_slides_in_to_dock() {
        let mut p = opened();
        p.frame.bar = (0.0, 1140.0);
        p.dock = (154.0, 1000.0);
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        // Its middle under the bar, 60 in from its end, its far side past it.
        let grip = p.pose().across + 44.0 - 90.0 + 9.0;
        drag(&mut p, (400.0, grip), (1080.0, 130.0));
        assert!(p.pose().detached(Side::Below) < 0.995, "{:?}", p.pose());
        p.release();
        run(&mut p, 1.5);
        assert!(!p.out());
        assert!((p.pose().along - 1000.0).abs() < 0.5, "{:?}", p.pose());
    }

    #[test]
    fn as_the_neck_breaks_it_collapses_and_its_stubs_snap_back() {
        let mut p = opened();
        pull(&mut p, 600.0);
        assert!(p.out());
        p.release();
        let card = p.pose();
        assert!(!card.joined, "{card:?}");
        // Just after the break, part of the stubs is still out; soon after,
        // none.
        let mut p2 = opened();
        p2.hold((400.0, 53.0));
        let mut seen = false;
        for i in 1..=40 {
            p2.drag((400.0, 53.0 + 600.0 * i as f32 / 40.0));
            p2.step(1.0 / 60.0);
            seen |= p2.pose().collapse.is_some_and(|(_, left)| left > 0.2);
        }
        assert!(seen);
        run(&mut p2, 0.5);
        assert_eq!(p2.pose().collapse, None);
    }

    #[test]
    fn closed_hanging_it_folds_back_and_leaves_waves() {
        let mut p = opened();
        p.close();
        assert!(!p.spec.grab && p.spec.input.is_none());
        run(&mut p, 0.4);
        assert!(p.pose().ripple.is_some());
        run(&mut p, 2.0);
        assert!(p.gone());
    }

    #[test]
    fn the_grip_shows_the_hand() {
        let mut p = opened();
        assert_eq!(p.cursor((400.0, 53.0)), Cursor::Grab);
        assert_eq!(p.cursor((400.0, 150.0)), Cursor::Default);
        p.hold((400.0, 53.0));
        assert_eq!(p.cursor((400.0, 150.0)), Cursor::Grabbing);
    }
}
