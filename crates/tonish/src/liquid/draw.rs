//! The card as one liquid with the bar, from its pose.

use engine::Side;
use renderer::shapes::{Bulge, Neck, Wave};
use renderer::{Circle, Liquid, Point, Rgba, RoundedRect, Scene, Shape};

use super::pose::{GRIP, GRIP_IN};
use super::{Frame, Pose};
use crate::config::{Config, Ripples};
use crate::hang::Edges;
use crate::theme::Theme;

/// What the settings make of every card: its corners, the line round it,
/// and how its neck and the waves it leaves look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Before they are fitted to a card's depth.
    pub edges: Edges,
    /// A multiple of the shipped neck's width.
    pub thickness: f32,
    pub ripples: Ripples,
}

impl Look {
    pub fn new(c: &Config) -> Self {
        Self {
            edges: Edges {
                radii: c.panel.radius,
                inverse_radius: c.panel.inverse_radius,
                border: c.border,
            },
            thickness: c.animation.detaching.thickness,
            ripples: c.animation.ripples,
        }
    }

    /// Fitted to a card of `size` hanging on `side` of the bar.
    pub fn edges(&self, side: Side, size: (f32, f32)) -> Edges {
        let pose = Pose::hanging(0.0, size, side);
        self.edges.fit(pose.depth(side), side)
    }

    /// Room at each end of such a card, along the bar, for its concave
    /// corners.
    pub fn room(&self, side: Side, size: (f32, f32)) -> f32 {
        self.edges(side, size).inverse_radius
    }
}

/// The waves the card leaves on the bar's edge, `t` seconds after they
/// started, as on water after a drop falls in: they rise over a tenth of a
/// second, then run apart along the edge at 60 pixels a second and die
/// away, their crests running at twice that.
fn wave(center: f32, t: f32, r: &Ripples) -> Option<Wave> {
    if t >= r.duration {
        return None;
    }
    // Down to a 28th of their height by the end, a quarter of a pixel for
    // the shipped 7.
    Some(Wave {
        center,
        amplitude: r.height * (-t / (0.3 * r.duration)).exp() * smoothstep(0.0, 0.1, t),
        group: 60.0 * t,
        phase: 120.0 * t,
    })
}

/// The waist of a soap film between two rings of radius 1, `s` of the way
/// to the longest it can be: the larger root `a` of `a·cosh(H/a) = 1` with
/// `H = 0.662743 s`. It hardly narrows until `s` nears 1, where it is still
/// 0.5524 wide, and past which no film can hold: it collapses at once. The
/// constants are where `x·tanh x = 1`, at `x = 1.199679`, the longest film
/// `x / cosh x`; rounder ones put the root visibly off near the limit,
/// where it moves fast.
fn catenoid_waist(s: f32) -> f32 {
    let h = 0.662_743 * s.clamp(0.0, 1.0);
    if h < 1e-4 {
        return 1.0;
    }
    // Between the waist where the film is longest and the rings' radius.
    let (mut lo, mut hi) = (h / 1.199_679, 1.0f32);
    for _ in 0..30 {
        let a = (lo + hi) / 2.0;
        if a * (h / a).cosh() > 1.0 {
            hi = a;
        } else {
            lo = a;
        }
    }
    (lo + hi) / 2.0
}

pub(super) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The radii of a card off the bar: the corners at the bar, hidden in it
/// while the card hangs, round like the ones across from them.
fn free(side: Side, [tl, tr, br, bl]: [f32; 4]) -> [f32; 4] {
    match side {
        Side::Below => [bl, br, br, bl],
        Side::Above => [tl, tr, tr, tl],
        Side::Right => [tr, tr, br, br],
        Side::Left => [tl, tl, bl, bl],
    }
}

/// The card, the neck that joins it to the bar while it is pulled off, the
/// bar's edge with its bulge and waves, the bead a collapsing neck leaves and
/// the grip, in the bar's colour and melted into the bar, so that they read
/// as one liquid. Early in its opening the card is narrow along the bar and
/// reaches out like a drop leaving it, as pleama-rs's cards do; pulled off,
/// it takes its own shape early, and the neck carries the join until it
/// collapses.
pub fn draw(scene: &mut Scene, frame: &Frame, look: &Look, card: &Pose, t: &Theme) {
    let side = frame.side;
    let e = look.edges(side, card.size);
    let b = e.border;
    let g = card.grow.max(0.0);
    let det = card.detached(side);
    let into = e.radii.iter().fold(0.0f32, |m, &r| m.max(r)) + b + 1.0;
    let (l, a) = (card.length(side), card.depth(side));
    let bell = 4.0 * g.min(1.0) * (1.0 - g.min(1.0));
    let (sa, sc) = card.stretch;

    let length = l
        * (0.3 + 0.7 * smoothstep(0.0, 0.9, g))
        * (1.0 + (g - 1.0).max(0.0) * 0.5)
        * (1.0 + sa - 0.5 * sc);
    // Across: hanging, from inside the bar out to its far edge, grown out of
    // the bar by `g`; free, its own box, grown about its middle. In between,
    // its near edge slides from one to the other. Past an end of the bar it
    // is its own box at once, round at its corners there: the part of it in
    // the bar's row past the bar's end would be cut off square.
    let ends = (card.along - length / 2.0, card.along + length / 2.0);
    let over = (frame.bar.0 - ends.0).max(ends.1 - frame.bar.1);
    let own = (2.5 * det).max(smoothstep(0.0, 8.0, over)).min(1.0);
    let hang_far = -into + (card.across + a / 2.0 + into) * g;
    let near = mix(-into, card.across - a / 2.0 * g, own);
    let far = mix(hang_far, card.across + a / 2.0 * g, own);
    let mid = (near + far) / 2.0;
    let depth = (far - near) * (1.0 + sc - 0.5 * sa - 0.06 * bell);
    let card_box = Blob {
        along: ends,
        across: (mid - depth / 2.0, mid + depth / 2.0),
        radii: free(side, e.radii),
    };
    let shown = g >= 0.002;
    // A liquid bridge from the bar to the card's near edge, shaped as a soap
    // film between two rings: it hardly narrows as it stretches, until at
    // the break it collapses all at once, and the stubs it leaves snap back
    // into the bar and the card.
    let ring = l * g.min(1.0) * look.thickness * 0.2;
    let neck = if shown && card.joined && det > 0.001 && det < 0.995 {
        // Thinner as the card stretches away, thicker as it squashes back:
        // the neck pulses with its wobble.
        let pulse = (1.0 - 2.5 * sc).clamp(0.4, 1.6);
        Some(Bridge {
            from: (card.along, -2.0),
            to: (card.along, near + 2.0),
            end: ring,
            waist: ring * catenoid_waist(det / 0.95) * pulse,
            blend: 8.0 + 12.0 * (1.0 - det),
            cut: 0.0,
        })
    } else {
        // Collapsed: the film at its longest, pinched through at its waist,
        // and then cut back further and further, which leaves two horns that
        // shrink back to where they stood. Half a pixel past the waist, or
        // the pinch would show as a hairline.
        card.collapse.filter(|_| shown).map(|(at, left)| {
            let waist = ring * catenoid_waist(1.0);
            Bridge {
                from: (at, -2.0),
                to: (card.along, near + 2.0),
                end: ring,
                waist,
                blend: 8.0,
                cut: mix(ring, waist, left) + 0.5,
            }
        })
    };
    // The bar's edge swells after the card while the neck stretches, most
    // halfway, and is kicked as it breaks or joins.
    let sag = 96.0 * det * (1.0 - det) * (1.0 - 0.5 * det) * g.min(1.0) + card.bounce;
    let bulge = (shown && sag > 0.05).then_some(Bulge {
        center: card.along,
        height: sag,
    });
    let wave = card.ripple.and_then(|(at, s)| wave(at, s, &look.ripples));
    // The card's own corners at the bar, no wider than the bar's edge
    // reaches past its sides; the neck brings its own blend.
    let space = (ends.0 - frame.bar.0).min(frame.bar.1 - ends.1).max(0.0);
    let blend = (e.inverse_radius * smoothstep(0.0, 0.3, g)).min(space);
    let attached = shown && card.joined && det < 0.995;

    let layer = |inset: f32, color: Rgba, out: &mut Vec<Box<dyn Shape>>| {
        let edge = Edge {
            frame,
            inset,
            blend: blend + inset,
            wave,
            bulge,
        };
        if attached {
            // One liquid: the card, its neck, and the bar's edge.
            let reach = edge.reach(Some(&card_box), neck.as_ref());
            out.push(Box::new(edge.liquid(&card_box, neck, reach, color)));
        } else {
            // The card on its own, round on every side, and the bar's edge
            // only where it moves.
            if shown {
                out.push(Box::new(card_box.rounded(frame, inset, color)));
            }
            if wave.is_some() || bulge.is_some() || neck.is_some() {
                let sunk = Blob {
                    along: (card.along, card.along),
                    across: (-into, -into),
                    radii: [0.0; 4],
                };
                let reach = edge.reach(None, neck.as_ref());
                out.push(Box::new(edge.liquid(&sunk, neck, reach, color)));
            }
        }
        // The bead the collapse leaves in the middle falls into the card.
        if let Some((at, stray)) = card.drop {
            let r = 7.0 * stray - inset;
            if r > 0.0 {
                let f = 1.0 - stray;
                out.push(Box::new(Circle {
                    center: frame.at(mix(at, card.along, f), near * (0.5 + 0.5 * f)),
                    r,
                    color,
                }));
            }
        }
    };
    // Every line before every fill, so that where two shapes overlap, the
    // fills cover the lines between them.
    let mut shapes = Vec::new();
    if b > 0.0 {
        layer(0.0, t.outline, &mut shapes);
    }
    layer(b, t.surface, &mut shapes);
    scene.shapes.extend(shapes);

    // The grip, along the middle of the card's edge nearest the bar, once
    // it is open.
    let fade = smoothstep(0.7, 1.0, g);
    if fade > 0.0 {
        let at = near.max(0.0) + GRIP_IN;
        let [p0, p1] = frame.rect(
            (card.along - GRIP.0 / 2.0, at - GRIP.1 / 2.0),
            (card.along + GRIP.0 / 2.0, at + GRIP.1 / 2.0),
        );
        scene.push(RoundedRect {
            p0,
            p1,
            radii: [GRIP.1 / 2.0; 4],
            color: t.on_surface.alpha(fade * (0.25 + 0.35 * card.grip)),
        });
    }
}

/// The neck between the bar and a card pulled off it, along and across.
#[derive(Clone, Copy)]
struct Bridge {
    from: (f32, f32),
    to: (f32, f32),
    end: f32,
    waist: f32,
    blend: f32,
    cut: f32,
}

/// A box by its extent along the bar and across it, with its corner radii
/// in the monitor's order.
struct Blob {
    along: (f32, f32),
    across: (f32, f32),
    radii: [f32; 4],
}

impl Blob {
    /// Moved in by `inset`, as the fill inside a border.
    fn inset(&self, inset: f32) -> Blob {
        let mid = (self.along.0 + self.along.1) / 2.0;
        let half = ((self.along.1 - self.along.0) / 2.0 - inset).max(0.0);
        let across = (
            self.across.0 + inset,
            (self.across.1 - inset).max(self.across.0 + inset),
        );
        let most = half.min((across.1 - across.0) / 2.0).max(0.0);
        Blob {
            along: (mid - half, mid + half),
            across,
            radii: self.radii.map(|r| (r - inset).clamp(0.0, most)),
        }
    }

    fn rect(&self, frame: &Frame) -> [Point; 2] {
        frame.rect((self.along.0, self.across.0), (self.along.1, self.across.1))
    }

    fn rounded(&self, frame: &Frame, inset: f32, color: Rgba) -> RoundedRect {
        let blob = self.inset(inset);
        let [p0, p1] = blob.rect(frame);
        RoundedRect {
            p0,
            p1,
            radii: blob.radii,
            color,
        }
    }
}

/// The bar's edge as a [`Liquid`] melts things into it, moved in by `inset`
/// as the fill inside a border: the concave corners keep their centres and
/// grow by as much, so that the line keeps its width round them and runs on
/// into the bar's.
struct Edge<'a> {
    frame: &'a Frame,
    inset: f32,
    blend: f32,
    wave: Option<Wave>,
    bulge: Option<Bulge>,
}

impl Edge<'_> {
    /// What the liquid covers, along the bar and out from its edge: the
    /// card, the waves and the bulge, so that the CPU never works through
    /// the empty rest of the monitor.
    fn reach(&self, card: Option<&Blob>, neck: Option<&Bridge>) -> ((f32, f32), f32) {
        let (mut lo, mut hi, mut out) = (f32::INFINITY, f32::NEG_INFINITY, 2.0f32);
        if let Some(c) = card {
            lo = lo.min(c.along.0);
            hi = hi.max(c.along.1);
            out = out.max(c.across.1);
        }
        if let Some(n) = neck {
            let r = n.end.max(n.waist);
            lo = lo.min(n.from.0.min(n.to.0) - r);
            hi = hi.max(n.from.0.max(n.to.0) + r);
            out = out.max(n.to.1);
        }
        // A crest on the bulge rises as high as the two together.
        let mut rise = 0.0;
        if let Some(w) = self.wave {
            let r = w.group + 3.0 * Wave::SPREAD;
            lo = lo.min(w.center - r);
            hi = hi.max(w.center + r);
            rise += w.amplitude.max(0.0);
        }
        if let Some(b) = self.bulge {
            let r = 3.0 * Bulge::SPREAD;
            lo = lo.min(b.center - r);
            hi = hi.max(b.center + r);
            rise += b.height.max(0.0);
        }
        out = out.max(rise);
        let pad = self.blend + 2.0;
        ((lo - pad, hi + pad), out + pad)
    }

    fn liquid(
        &self,
        card: &Blob,
        neck: Option<Bridge>,
        (along, out): ((f32, f32), f32),
        color: Rgba,
    ) -> Liquid {
        let f = self.frame;
        let blob = card.inset(self.inset);
        let [p0, p1] = blob.rect(f);
        let (normal, offset) = f.edge();
        let neck = neck.map(|n| Neck {
            from: f.at(n.from.0, n.from.1),
            to: f.at(n.to.0, n.to.1),
            end: (n.end - self.inset).max(0.0),
            waist: (n.waist - self.inset).max(0.0),
            blend: n.blend,
            cut: n.cut,
        });
        let [c0, c1] = f.rect((along.0, -f.overlap), (along.1, out));
        Liquid {
            p0,
            p1,
            radii: blob.radii,
            normal,
            offset: offset - self.inset,
            blend: self.blend,
            wave: self.wave,
            bulge: self.bulge,
            neck,
            extent: [f.bar.0 + self.inset, f.bar.1 - self.inset],
            bounds: [
                Point::new(c0.x.max(0.0), c0.y.max(0.0)),
                Point::new(c1.x.min(f.screen.0), c1.y.min(f.screen.1)),
            ],
            color,
        }
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
    fn the_waves_rise_run_apart_and_die_away() {
        let r = Config::default().animation.ripples;
        let rising = wave(100.0, 0.05, &r).unwrap();
        let full = wave(100.0, 0.1, &r).unwrap();
        let late = wave(100.0, 1.0, &r).unwrap();
        assert!(rising.amplitude < full.amplitude && late.amplitude < full.amplitude / 5.0);
        assert_eq!((late.center, late.group, late.phase), (100.0, 60.0, 120.0));
        assert_eq!(wave(100.0, r.duration, &r), None);
    }

    #[test]
    fn the_neck_narrows_as_a_soap_film_does() {
        assert_eq!(catenoid_waist(0.0), 1.0);
        let half = catenoid_waist(0.5);
        let most = catenoid_waist(1.0);
        // It hardly narrows half way, and is still more than half as wide
        // at the longest a film can be.
        assert!(half > 0.9 && half < 1.0, "{half}");
        assert!((most - 0.5524).abs() < 2e-3, "{most}");
        for i in 0..20 {
            let (a, b) = (i as f32 / 20.0, (i + 1) as f32 / 20.0);
            assert!(catenoid_waist(b) < catenoid_waist(a), "at {b}");
        }
    }

    fn draw_at(card: &Pose) -> Scene {
        let mut scene = Scene::new();
        draw(
            &mut scene,
            &frame(),
            &Look::new(&Config::default()),
            card,
            &Theme::builtin(),
        );
        scene
    }

    #[test]
    fn hanging_the_card_is_one_liquid_with_the_bar() {
        let card = Pose {
            grow: 1.0,
            ..Pose::hanging(400.0, SIZE, Side::Below)
        };
        let scene = draw_at(&card);
        // The liquid, and the grip.
        assert_eq!(scene.shapes.len(), 2);
        let liquid = &scene.shapes[0];
        assert!(liquid.distance(Point::new(400.0, 44.0 + 90.0)) < 0.0);
        assert!(liquid.distance(Point::new(400.0, 44.0 + 181.0)) > 0.0);
    }

    #[test]
    fn pulled_halfway_a_neck_joins_the_card_to_the_bar() {
        let card = Pose {
            grow: 1.0,
            across: 90.0 + 14.0 + 63.0,
            ..Pose::hanging(400.0, SIZE, Side::Below)
        };
        let liquid = &draw_at(&card).shapes[0];
        // Between the bar and the card, under the card's middle, and not
        // beside it.
        assert!(liquid.distance(Point::new(400.0, 44.0 + 40.0)) < 0.0);
        assert!(liquid.distance(Point::new(290.0, 44.0 + 40.0)) > 0.0);
    }

    #[test]
    fn free_the_card_is_a_rounded_box_of_its_own() {
        let card = Pose {
            grow: 1.0,
            across: 600.0,
            ..Pose::hanging(400.0, SIZE, Side::Below)
        };
        let scene = draw_at(&card);
        // The card, and the grip: the bar's edge is still.
        assert_eq!(scene.shapes.len(), 2);
        let card = &scene.shapes[0];
        assert!(card.distance(Point::new(400.0, 44.0 + 600.0)) < 0.0);
        assert!(card.distance(Point::new(400.0, 44.0 + 300.0)) > 0.0);
    }
}
