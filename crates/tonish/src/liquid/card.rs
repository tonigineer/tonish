//! A card's life, from opening to folding back: where it is and how it
//! moves, on springs, as pleama's cards do, and what the pointer does to
//! it. The shell routes events here and draws what [`Card::pose`] says.

use std::time::Instant;

use engine::{Cursor, PopupSpec};

use super::{Dock, Edge, Pose};
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

/// The pin grows in, and shrinks away, in about a quarter of a second, a
/// little past its size, and grows by a fifth under the pointer.
const PIN: (f32, f32) = (260.0, 0.55);
const PIN_LIT: f32 = 1.2;

/// Dragged along its edge past where it may dock, the share of the hand's
/// way the card follows along it, and how far it sags off the edge for each
/// pixel of that way.
const SLIDE: f32 = 0.5;
const SAG: f32 = 1.2;

/// A card that hangs from an edge, from growing out of the bar to folding
/// back into the edge it hangs from then. Its surface is the popup, which
/// covers the monitor while the card is open, so that it never has to be
/// resized, however far the card goes.
pub struct Card {
    pub spec: PopupSpec,
    /// The edge it hangs from, or last hung from, or is nearest while it is
    /// free, which its position is along and across.
    pub edge: Edge,
    /// Every edge it may dock to, its own among them, the bar's first.
    docks: Vec<Dock>,
    /// Along the first edge, where the bar itself runs straight, which the
    /// edge may run on past, into the frame's.
    bar: (f32, f32),
    /// Its width and height on the monitor, in logical pixels.
    pub size: (f32, f32),
    /// The settings' animation section, which a save changes in the middle
    /// of a motion.
    pub anim: Animation,
    /// Along its edge, where its middle may hang.
    dock: (f32, f32),
    /// As [`Dock::corners`], of its edge.
    corners: [Option<Edge>; 2],
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
    /// From 0 to 1 as the pin shows.
    pin: Spring,
    /// Seconds since the neck last broke.
    since: f32,
    /// While the grip is held: where on the card it was taken, from the
    /// card's middle, along and across.
    held: Option<(f32, f32)>,
    /// Off its edge since its neck broke, until it docks again: a neck that
    /// reaches out to it again doesn't bring it back by itself.
    out: bool,
    hover: bool,
    /// The pointer is over the pin, and was pressed on it, which a release
    /// on it too makes a click.
    on_pin: bool,
    pin_pressed: bool,
    /// Where waves started on the edge, and seconds since: a clock
    /// rather than a spring, since waves run at a steady speed.
    ripple: Option<(f32, f32)>,
    /// Waves have run since it began to fold back, so it leaves only one
    /// set.
    folded: bool,
    drop_at: f32,
    /// How far the card had left the edge at the last frame, to see it cross
    /// the points where the neck breaks and joins.
    was: f32,
    /// When its last frame was drawn, which the next one steps from. `None`
    /// while it rests, so that a motion starting after a pause starts with
    /// one frame's step rather than the whole pause.
    pub drawn: Option<Instant>,
}

impl Card {
    /// A card of `size` that hangs at `home` along the edge of the first of
    /// `docks`, which must not be empty and is the bar's, and may dock to any
    /// of them. `bar` is where along that edge the bar itself runs.
    pub fn new(
        spec: PopupSpec,
        docks: Vec<Dock>,
        bar: (f32, f32),
        size: (f32, f32),
        home: f32,
        anim: Animation,
    ) -> Self {
        let Dock {
            edge,
            range: dock,
            corners,
        } = docks[0];
        let card = Pose::hanging(home, size, edge.side);
        let mut grow = Spring::new(0.0);
        aim(&mut grow, 1.0, anim.popout);
        let mut stray = Spring::new(0.0);
        stray.aim(0.0, STRAY.0, STRAY.1);
        Self {
            spec,
            edge,
            docks,
            bar,
            size,
            anim,
            dock,
            corners,
            grow,
            along: Spring::new(card.along),
            across: Spring::new(card.across),
            bounce: Spring::new(0.0),
            stray,
            squash: (Spring::new(0.0), Spring::new(0.0)),
            recoil: Spring::new(1.0),
            pin: Spring::new(0.0),
            since: f32::INFINITY,
            held: None,
            out: false,
            hover: false,
            on_pin: false,
            pin_pressed: false,
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
            &self.pin,
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

    /// Docked anywhere but under the bar it opened from, where it shows a
    /// pin: it stays there, as a free card stays where it is let go, and
    /// takes neither the keyboard nor clicks outside, which would fold it
    /// away at the next click elsewhere.
    pub fn pinned(&self) -> bool {
        let on_bar = self.edge == self.docks[0].edge
            && (self.bar.0..=self.bar.1).contains(&self.along.target);
        !self.out && self.held.is_none() && !self.closing() && !on_bar
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
            pin: self.pin.value,
            dock: self.dock,
            straight: self.edge.straight,
            corners: self.corners,
            joined: true,
            collapse: None,
        };
        // Broken, it reaches for the card again only once the card is well
        // back inside where it would break, and the stubs have gone.
        let det = card.detached(self.edge.side);
        card.joined = (!self.out && det < 0.995) || (self.out && det < 0.9 && self.since > 0.45);
        let left = (1.0 - self.recoil.value).clamp(0.0, 1.0);
        card.collapse = (!card.joined && left > 0.001).then_some((self.drop_at, left));
        card
    }

    /// Advances everything by `dt` seconds, and sees the neck break or
    /// join.
    pub fn step(&mut self, dt: f32) {
        let a = self.anim;
        // Aimed again at every step, so that a save takes effect at once.
        let grow = if self.closing() { a.fold } else { a.popout };
        let target = self.grow.target;
        aim(&mut self.grow, target, grow);
        aim(&mut self.bounce, 0.0, a.ripples.motion());
        let pinned = self.pinned();
        let pin = Motion {
            stiffness: PIN.0,
            damping: PIN.1,
        };
        let size = match (pinned, self.on_pin) {
            (false, _) => 0.0,
            (true, false) => 1.0,
            (true, true) => PIN_LIT,
        };
        aim(&mut self.pin, size, pin);
        if pinned {
            self.spec.grab = false;
        }
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
            &mut self.pin,
        ] {
            s.step(dt);
        }
        self.since += dt;
        if self.out && !self.closing() {
            self.nearest((self.along.value, self.across.value));
        }
        if let Some((_, t)) = &mut self.ripple {
            // At most 0.1 s, as the springs, so that the waves resume rather
            // than jump after a pause.
            *t += dt.min(0.1);
        }
        let mut det = self.pose().detached(self.edge.side);
        if self.held.is_some() && !self.out && det > 0.95 {
            // In a corner, the edge across still holds it: it hangs from
            // that one instead, and the neck to this one thins away as one
            // across a corner does.
            if let Some(dock) = self.held_across(det) {
                self.switch(dock);
                det = self.pose().detached(self.edge.side);
            }
        }
        if self.held.is_some() && !self.out && det > 0.95 {
            // The neck breaks: the card is free, the edge is kicked back
            // into what it bounds, which shows only as it springs out again,
            // the card being drawn over that and unable to take from it,
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
            // rest: the edge pulls it in.
            self.dock();
        }
        if !self.closing() && self.was >= 0.05 && det < 0.05 {
            self.bounce.velocity += a.attaching.kick;
            self.ripple = Some((self.along.value, 0.0));
        }
        self.was = det;
        if self.closing() && !self.folded && det < 0.5 && self.grow.value < 0.06 {
            // All but back in the edge: it leaves waves where it went in.
            self.folded = true;
            self.ripple = Some((self.along.value, 0.0));
        }
        self.spec.input = if self.closing() {
            None
        } else if self.held.is_some() {
            // The whole monitor while the grip is held, so that the pointer
            // can't leave the popup during the drag: during the grab sway
            // holds on to no surface for a held button.
            let (w, h) = self.edge.screen;
            Some((0, 0, w as i32, h as i32))
        } else {
            let [p0, p1] = self.pose().rect(&self.edge);
            let (x0, y0) = (p0.x.floor() as i32, p0.y.floor() as i32);
            Some((x0, y0, p1.x.ceil() as i32 - x0, p1.y.ceil() as i32 - y0))
        };
    }

    /// Hangs it from where the edges are now, after the settings moved
    /// them. Along and across stay those of the nearest edge on the same
    /// side, so that a card on its neck or docked moves with its edge, and
    /// docked, slides back within where it may dock. With no edge left on
    /// that side, it keeps its place on the monitor, and docked, hangs from
    /// the first edge again.
    pub fn reframe(&mut self, docks: Vec<Dock>, bar: (f32, f32)) {
        self.bar = bar;
        let (edge, along) = (self.edge, self.along.value);
        let far = |d: &Dock| {
            let past = (d.range.0 - along).max(along - d.range.1).max(0.0);
            (d.edge.edge - edge.edge).abs() + past
        };
        let same = (docks.iter())
            .filter(|d| d.edge.side == edge.side)
            .min_by(|a, b| far(a).total_cmp(&far(b)))
            .copied();
        let first = docks.first().copied();
        self.docks = docks;
        match (same, first) {
            (Some(d), _) => {
                self.edge = d.edge;
                self.dock = d.range;
                self.corners = d.corners;
            }
            (None, Some(d)) => self.switch(d),
            (None, None) => {}
        }
        if !self.out && self.held.is_none() && !self.closing() {
            self.dock();
        }
        self.step(0.0);
    }

    /// How far it has left `dock`'s edge with its middle at `at`, along
    /// and across its own edge, as [`Pose::detached`] counts it.
    fn detached_from(&self, dock: &Dock, at: (f32, f32)) -> f32 {
        let p = self.edge.at(at.0, at.1);
        let (along, across) = dock.edge.local((p.x, p.y));
        Pose {
            along,
            across,
            dock: dock.range,
            straight: dock.edge.straight,
            corners: dock.corners,
            ..self.pose()
        }
        .detached(dock.edge.side)
    }

    /// Free, it belongs to the edge it comes nearest, so that a neck
    /// reaches out to it from there, and it docks there. Only once a neck
    /// could reach it, so that the waves it left stay on the edge it left
    /// while it is far from every edge, and only for an edge clearly
    /// nearer, so that it doesn't flick between two at a corner. `at` is
    /// where its middle is, or is headed, along and across its edge.
    fn nearest(&mut self, at: (f32, f32)) {
        let here = self.detached_from(
            &Dock {
                edge: self.edge,
                range: self.dock,
                corners: self.corners,
            },
            at,
        );
        let best = (self.docks.iter())
            .map(|d| (*d, self.detached_from(d, at)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((dock, det)) = best
            && dock.edge != self.edge
            && det < 1.0
            && det < here - 0.02
        {
            self.switch(dock);
        }
    }

    /// The dock of an edge across one of its corners that holds it more
    /// than its own edge does, `det` out of it.
    fn held_across(&self, det: f32) -> Option<Dock> {
        let at = (self.along.value, self.across.value);
        (self.corners.iter().flatten())
            .filter_map(|c| self.docks.iter().find(|d| d.edge == *c))
            .map(|d| (*d, self.detached_from(d, at)))
            .filter(|(_, across)| *across < det)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(d, _)| d)
    }

    /// Moves it onto `dock`'s edge, keeping where it is on the monitor and
    /// how it moves there. The waves, the kick and the bead belong to the
    /// edge it leaves, and go.
    fn switch(&mut self, dock: Dock) {
        let (from, to) = (self.edge, dock.edge);
        let place = |along: f32, across: f32| {
            let p = from.at(along, across);
            to.local((p.x, p.y))
        };
        let (a, c) = (self.along, self.across);
        let value = place(a.value, c.value);
        let target = place(a.target, c.target);
        let velocity = to.local_vector(from.vector((a.velocity, c.velocity)));
        // Turned, each spring keeps its stiffness on the axis it moves on.
        if from.turned(&to) {
            std::mem::swap(&mut self.along, &mut self.across);
            let (s0, s1) = self.squash;
            self.squash = (s1, s0);
        }
        for (s, v, t, w) in [
            (&mut self.along, value.0, target.0, velocity.0),
            (&mut self.across, value.1, target.1, velocity.1),
        ] {
            s.value = v;
            s.target = t;
            s.velocity = w;
        }
        if let Some(h) = &mut self.held {
            *h = to.local_vector(from.vector(*h));
        }
        self.edge = to;
        self.dock = dock.range;
        self.corners = dock.corners;
        self.ripple = None;
        for s in [&mut self.bounce, &mut self.stray, &mut self.recoil] {
            s.value = s.target;
            s.velocity = 0.0;
        }
        self.was = self.pose().detached(to.side);
    }

    /// The pointer left the card, so that the grip goes dark and the pin
    /// shrinks back. Returns whether that needs a frame.
    pub fn leave(&mut self) -> bool {
        std::mem::take(&mut self.hover) | std::mem::take(&mut self.on_pin)
    }

    /// Folds the card back into its edge, or, off it, shrinks it where
    /// it is. It lets go of clicks and the keyboard at once.
    pub fn close(&mut self) {
        aim(&mut self.grow, 0.0, self.anim.fold);
        self.spec.grab = false;
        self.spec.input = None;
        self.held = None;
    }

    pub fn on_grip(&self, at: (f32, f32)) -> bool {
        !self.closing() && self.pose().on_grip(&self.edge, at)
    }

    /// Over the pin, once it shows.
    pub fn on_pin(&self, at: (f32, f32)) -> bool {
        self.pinned() && self.pose().on_pin(&self.edge, at)
    }

    /// A press on the pin, which the release turns into a click if it is
    /// on the pin too, as a button takes a click.
    pub fn press_pin(&mut self, at: (f32, f32)) -> bool {
        self.pin_pressed = self.on_pin(at);
        self.pin_pressed
    }

    /// Whether this release ends a click on the pin.
    pub fn release_pin(&mut self, at: (f32, f32)) -> bool {
        std::mem::take(&mut self.pin_pressed) && self.on_pin(at)
    }

    pub fn on_card(&self, at: (f32, f32)) -> bool {
        let [p0, p1] = self.pose().rect(&self.edge);
        (p0.x..p1.x).contains(&at.0) && (p0.y..p1.y).contains(&at.1)
    }

    pub fn hold(&mut self, at: (f32, f32)) {
        let (along, across) = self.edge.local(at);
        self.held = Some((along - self.along.value, across - self.across.value));
        self.step(0.0);
    }

    /// While the grip is held, the card follows the pointer, a little
    /// behind. Returns whether that needs a frame: while it is held, or as
    /// the pointer comes onto the grip or leaves it, which lights it.
    pub fn drag(&mut self, at: (f32, f32)) -> bool {
        let (hover, on_pin) = (self.on_grip(at), self.on_pin(at));
        let lit = hover != self.hover || on_pin != self.on_pin;
        (self.hover, self.on_pin) = (hover, on_pin);
        let Some((da, dc)) = self.held else {
            return lit;
        };
        let (along, across) = self.edge.local(at);
        let (mut along, mut across) = (along - da, across - dc);
        if !self.out {
            // Past where it may dock, it slides on at half the hand's pace
            // and sags off the edge, as liquid off the end of a ledge, until
            // the neck breaks, rather than stopping as against a wall. Into
            // a corner, it goes as far as the edge across, which stops it.
            let side = self.edge.side;
            let [f0, f1] = self.pose().flush(side);
            let (lo, hi) = (self.dock.0, self.dock.1.max(self.dock.0));
            let (lo, hi) = (f0.unwrap_or(lo), f1.unwrap_or(hi).max(lo));
            let docked = along.clamp(lo, hi);
            if (along < lo && f0.is_none()) || (along > hi && f1.is_none()) {
                across += (along - docked).abs() * SAG;
                along = docked + (along - docked) * SLIDE;
            } else {
                along = docked;
            }
            // Near a corner, the edge across pulls it in as its own edge
            // does.
            let snap = self.snap();
            for f in [f0, f1].into_iter().flatten() {
                let gap = (along - f).abs();
                if gap < snap {
                    along -= (along - f) * self.anim.detaching.pull * (1.0 - gap / snap);
                }
            }
        }
        let half = self.pose().half(self.edge.side);
        let (along, mut across) = self.edge.keep(half, (along, across));
        if !self.out {
            // Pulled by the edge while the neck holds, less as it stretches.
            let hang = Pose::hanging(along, self.size, self.edge.side).across;
            let stretched = Pose {
                across,
                ..self.pose()
            }
            .detached(self.edge.side);
            across -= (across - hang) * self.anim.detaching.pull * (1.0 - stretched);
        }
        self.follow((along, across));
        true
    }

    /// Let go while a neck joins it to an edge, the edge pulls it in, and it
    /// docks where it is along the edge, as far as the edge runs straight.
    /// Free, it stays where it was let go.
    pub fn release(&mut self) {
        if self.held.take().is_none() {
            return;
        }
        // Where it is drawn trails the hand, so the edge it is headed for
        // is the one it docks to.
        if self.out {
            self.nearest((self.along.target, self.across.target));
        }
        let side = self.edge.side;
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

    /// How near the edge across a corner pulls the card into it: as far as
    /// a neck from it reaches, so that no card comes to rest with a neck
    /// stretched to it.
    fn snap(&self) -> f32 {
        self.anim.detaching.distance
    }

    /// Docks where it is headed along its edge, within where it may, or
    /// near a corner, in the corner, against the edge across too.
    fn dock(&mut self) {
        self.out = false;
        let along = self.along.target;
        let snap = self.snap();
        let along = match self.pose().flush(self.edge.side) {
            [Some(f), _] if along - f <= snap => f,
            [_, Some(f)] if f - along <= snap => f,
            _ => along.clamp(self.dock.0, self.dock.1.max(self.dock.0)),
        };
        let hang = Pose::hanging(along, self.size, self.edge.side);
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
        } else if self.on_pin(at) {
            Cursor::Pointer
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
        let edge = Edge {
            side: Side::Below,
            edge: 44.0,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            straight: (0.0, 1920.0),
        };
        let bar = Dock {
            edge,
            range: (154.0, 1766.0),
            corners: [None, None],
        };
        Card::new(
            spec,
            vec![bar],
            (0.0, 1920.0),
            (280.0, 180.0),
            400.0,
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
    fn a_free_card_docks_to_another_edge_it_is_let_go_near_without_a_jump() {
        let mut p = opened();
        // A frame 8 wide down the left side, below the bar.
        let left = Edge {
            side: Side::Right,
            edge: 8.0,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            straight: (60.0, 1072.0),
        };
        p.docks.push(Dock {
            edge: left,
            range: (150.0, 982.0),
            corners: [None, None],
        });
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        // The grip to where the card's left side is 40 from the frame's,
        // its middle 81 below the grip, at 681.
        let middle = |p: &Card| p.edge.at(p.along.value, p.across.value);
        let from = (400.0, p.pose().across + 44.0 - 90.0 + 9.0);
        let to = (8.0 + 40.0 + 140.0, 600.0);
        p.hold(from);
        for i in 1..=60 {
            let t = i as f32 / 60.0;
            let before = middle(&p);
            p.drag((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
            p.step(1.0 / 60.0);
            let after = middle(&p);
            let moved = (after.x - before.x).hypot(after.y - before.y);
            assert!(moved < 40.0, "a jump of {moved} at {i}: {:?}", p.edge);
        }
        run(&mut p, 0.5);
        assert_eq!(p.edge, left);
        p.release();
        run(&mut p, 1.5);
        assert!(!p.out());
        let card = p.pose();
        assert!((card.across - 140.0).abs() < 0.5, "{card:?}");
        assert!((card.along - 681.0).abs() < 1.0, "{card:?}");
    }

    /// Pulled off the bar, and let go free with its middle at `to`, beside
    /// the left side of a frame 8 wide, which meets the frame's bottom at
    /// 1072 in a corner. `fast` lets go as the hand arrives, while the card
    /// still trails it.
    fn let_go_beside_the_left_side(to: (f32, f32), fast: bool) -> Card {
        let mut p = opened();
        let bottom = Edge {
            side: Side::Above,
            edge: 1072.0,
            straight: (24.0, 1896.0),
            ..p.edge
        };
        let left = Edge {
            side: Side::Right,
            edge: 8.0,
            straight: (60.0, 1056.0),
            ..p.edge
        };
        p.docks.push(Dock {
            edge: left,
            range: (164.0, 952.0),
            corners: [None, Some(bottom)],
        });
        p.docks.push(Dock {
            edge: bottom,
            range: (178.0, 1742.0),
            corners: [Some(left), None],
        });
        pull(&mut p, 600.0);
        p.release();
        run(&mut p, 0.5);
        // Held by the grip on its top, 81 above its middle.
        let from = (400.0, p.pose().across + 44.0 - 90.0 + 9.0);
        let to = (to.0, to.1 - 81.0);
        if fast {
            p.hold(from);
            for i in 1..=6 {
                let t = i as f32 / 6.0;
                p.drag((from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
                p.step(1.0 / 60.0);
            }
        } else {
            drag(&mut p, from, to);
        }
        p.release();
        run(&mut p, 1.5);
        assert_eq!(p.edge, left);
        assert!(!p.out());
        p
    }

    #[test]
    fn let_go_near_a_corner_it_docks_in_the_corner_against_both_edges() {
        // 52 from where it sits against the bottom, its middle 90 above it.
        let p = let_go_beside_the_left_side((8.0 + 140.0 + 40.0, 930.0), false);
        let card = p.pose();
        assert!((card.along - 982.0).abs() < 0.5, "{card:?}");
        assert!((card.across - 140.0).abs() < 0.5, "{card:?}");
        assert_eq!(card.detached(Side::Right), 0.0);
    }

    #[test]
    fn pulled_off_its_edge_in_a_corner_it_hangs_from_the_edge_across() {
        let mut p = let_go_beside_the_left_side((8.0 + 140.0 + 40.0, 930.0), false);
        // Along the bottom, away from the left side, by the grip on top.
        drag(
            &mut p,
            (148.0, 982.0 - 90.0 + 9.0),
            (548.0, 982.0 - 90.0 + 9.0),
        );
        assert!(!p.out(), "{:?}", p.pose());
        assert_eq!(p.edge.side, Side::Above);
        p.release();
        run(&mut p, 1.5);
        let card = p.pose();
        assert!(!p.out());
        assert!((card.across - 90.0).abs() < 0.5, "{card:?}");
        assert!(card.along > 400.0, "{card:?}");
    }

    #[test]
    fn docked_away_from_the_bar_it_is_pinned_and_takes_no_grab() {
        let p = let_go_beside_the_left_side((8.0 + 140.0 + 40.0, 700.0), false);
        assert!(p.pinned() && !p.spec.grab);
        assert!((p.pose().pin - 1.0).abs() < 0.01, "{:?}", p.pose());
        // On the bar, wherever along it, it is not.
        let mut p = opened();
        assert!(!p.pinned() && p.pose().pin == 0.0);
        drag(&mut p, (400.0, 53.0), (900.0, 140.0));
        p.release();
        run(&mut p, 1.0);
        assert!(!p.pinned());
    }

    #[test]
    fn its_pin_takes_a_click_that_starts_and_ends_on_it() {
        let mut p = let_go_beside_the_left_side((8.0 + 140.0 + 40.0, 700.0), false);
        // From 8 to 288, and down from 610: the pin's middle at 272, 626.
        let (pin, card) = ((272.0, 626.0), (150.0, 700.0));
        assert_eq!(p.cursor(pin), Cursor::Pointer);
        assert!(p.press_pin(pin) && p.release_pin(pin));
        assert!(p.press_pin(pin) && !p.release_pin(card));
        assert!(!p.press_pin(card) && !p.release_pin(pin));
        // Under the pointer, it grows.
        p.drag(pin);
        run(&mut p, 0.5);
        assert!(p.pose().pin > 1.1, "{:?}", p.pose());
    }

    #[test]
    fn beside_the_bar_on_an_edge_it_runs_on_into_it_is_pinned() {
        let mut p = opened();
        // The bar from 500 to 1400 along an edge that runs on past it.
        p.bar = (500.0, 1400.0);
        drag(&mut p, (400.0, 53.0), (300.0, 53.0));
        p.release();
        run(&mut p, 1.0);
        assert!(p.pinned(), "{:?}", p.pose());
    }

    #[test]
    fn let_go_further_from_a_corner_it_docks_where_it_is() {
        let p = let_go_beside_the_left_side((8.0 + 140.0 + 40.0, 700.0), false);
        assert!((p.pose().along - 700.0).abs() < 1.0, "{:?}", p.pose());
    }

    #[test]
    fn let_go_on_its_way_it_docks_where_it_was_headed() {
        let p = let_go_beside_the_left_side((8.0 + 140.0 + 40.0, 930.0), true);
        assert!((p.pose().along - 982.0).abs() < 0.5, "{:?}", p.pose());
    }

    /// The bar ending at 1140, and the card's middle docking at most at
    /// 1000.
    fn shorten(p: &mut Card) {
        p.edge.straight = (0.0, 1140.0);
        p.dock = (154.0, 1000.0);
        p.docks = vec![Dock {
            edge: p.edge,
            range: p.dock,
            corners: [None, None],
        }];
    }

    #[test]
    fn near_the_bar_but_past_its_end_a_free_card_stays_free() {
        let mut p = opened();
        shorten(&mut p);
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
        shorten(&mut p);
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
