//! The settings panel, which a right-click on the bar opens. It grows out of
//! the bar's edge like a drop and folds back into it, as pleama's cards do.

use engine::Side;
use renderer::shapes::Wave;
use renderer::{Liquid, Point, Rgba, Scene};

use crate::config::Config;
use crate::hang::Edges;
use crate::theme::Theme;

/// Logical pixels, as width and height on the monitor.
pub const CARD: (f32, f32) = (280.0, 180.0);

/// Room at each end of the popup, along the bar, for the concave corners.
pub fn room(side: Side, c: &Config) -> f32 {
    edges(side, c).inverse_radius
}

pub fn edges(side: Side, c: &Config) -> Edges {
    Edges {
        radii: c.panel.radius,
        inverse_radius: c.panel.inverse_radius,
        border: c.border,
    }
    .fit(card_across(side), side)
}

/// The card's length along the bar and its depth away from it.
fn card_along(side: Side) -> f32 {
    match side {
        Side::Above | Side::Below => CARD.0,
        Side::Left | Side::Right => CARD.1,
    }
}

fn card_across(side: Side) -> f32 {
    match side {
        Side::Above | Side::Below => CARD.1,
        Side::Left | Side::Right => CARD.0,
    }
}

/// Room past the card, away from the bar, for its bounce, which swells it by
/// about half its overshoot of 5 %.
fn bounce(side: Side) -> f32 {
    (card_across(side) * 0.08).ceil()
}

/// The card, the room for its concave corners and its bounce, and
/// `overlap`: how far the popup reaches over the bar's border, which the
/// card covers where it meets the bar, so that no line runs between them.
pub fn size(side: Side, c: &Config, overlap: f32) -> (u32, u32) {
    let along = card_along(side) + 2.0 * room(side, c);
    let across = overlap + card_across(side) + bounce(side);
    let (w, h) = match side {
        Side::Above | Side::Below => (along, across),
        Side::Left | Side::Right => (across, along),
    };
    (w.ceil() as u32, h.ceil() as u32)
}

/// Positions on the popup by how far along the bar from the popup's start
/// and how far away from the bar's edge, so that one piece of geometry
/// serves every side.
struct Frame {
    side: Side,
    size: (f32, f32),
    overlap: f32,
}

impl Frame {
    fn new(side: Side, c: &Config, overlap: f32) -> Self {
        let (w, h) = size(side, c, overlap);
        Self {
            side,
            size: (w as f32, h as f32),
            overlap,
        }
    }

    fn at(&self, along: f32, across: f32) -> Point {
        let o = self.overlap;
        match self.side {
            Side::Below => Point::new(along, o + across),
            Side::Above => Point::new(along, self.size.1 - o - across),
            Side::Right => Point::new(o + across, along),
            Side::Left => Point::new(self.size.0 - o - across, along),
        }
    }

    /// The box between two corners given along and across, in either order.
    fn rect(&self, (a0, c0): (f32, f32), (a1, c1): (f32, f32)) -> [Point; 2] {
        let (p, q) = (self.at(a0, c0), self.at(a1, c1));
        [
            Point::new(p.x.min(q.x), p.y.min(q.y)),
            Point::new(p.x.max(q.x), p.y.max(q.y)),
        ]
    }

    fn along(&self) -> f32 {
        match self.side {
            Side::Above | Side::Below => self.size.0,
            Side::Left | Side::Right => self.size.1,
        }
    }

    /// The bar's side of its edge, as a [`Liquid`] takes it: points whose
    /// position along the normal is below the offset.
    fn edge(&self) -> (Point, f32) {
        let o = self.overlap;
        match self.side {
            Side::Below => (Point::new(0.0, 1.0), o),
            Side::Above => (Point::new(0.0, -1.0), o - self.size.1),
            Side::Right => (Point::new(1.0, 0.0), o),
            Side::Left => (Point::new(-1.0, 0.0), o - self.size.0),
        }
    }
}

/// The card's corners on the popup once it is open, which take the pointer.
pub fn card(side: Side, c: &Config, overlap: f32) -> [Point; 2] {
    let room = room(side, c);
    Frame::new(side, c, overlap).rect((room, 0.0), (room + card_along(side), card_across(side)))
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Something melted into the bar's edge, along the bar and out of its edge,
/// reaching back into the bar far enough that its corners there never show.
struct Blob {
    along: (f32, f32),
    out: f32,
    into: f32,
    radii: [f32; 4],
    blend: f32,
}

impl Blob {
    /// Moved in by `inset`, as the fill inside a border: the concave
    /// corners keep their centres and grow by as much, so that the line
    /// keeps its width round them and runs on into the bar's. It is drawn
    /// out to `reach` from the edge, which spares the CPU the empty rest of
    /// the popup once only waves are left.
    fn liquid(
        &self,
        frame: &Frame,
        inset: f32,
        wave: Option<Wave>,
        reach: f32,
        color: Rgba,
    ) -> Liquid {
        let mid = (self.along.0 + self.along.1) / 2.0;
        let half = ((self.along.1 - self.along.0) / 2.0 - inset).max(0.0);
        let out = (self.out - inset).max(-self.into);
        let [p0, p1] = frame.rect((mid - half, -self.into), (mid + half, out));
        let most = half.min((out + self.into) / 2.0).max(0.0);
        let (normal, offset) = frame.edge();
        Liquid {
            p0,
            p1,
            radii: self.radii.map(|r| (r - inset).clamp(0.0, most)),
            normal,
            offset: offset - inset,
            blend: self.blend + inset,
            wave,
            bounds: frame.rect((0.0, -frame.overlap), (frame.along(), reach)),
            color,
        }
    }
}

/// How long the waves the card leaves run, by when they have flattened to
/// a quarter of a pixel.
pub const RIPPLE: f32 = 1.5;

/// The waves the card leaves as it folds back, `t` seconds after it went
/// in, as on water after a drop falls in: they rise out of the card's last
/// sliver over a tenth of a second, then run apart along the bar's edge at
/// 60 pixels a second and die away, their crests running at twice that.
pub fn wave(mid: f32, t: f32) -> Option<Wave> {
    if t >= RIPPLE {
        return None;
    }
    Some(Wave {
        center: mid,
        amplitude: 7.0 * (-t / 0.45).exp() * smoothstep(0.0, 0.1, t),
        group: 60.0 * t,
        phase: 120.0 * t,
    })
}

/// The card at `grow`, from 0, shut, to 1, open, and past 1 while it
/// bounces, in the bar's colour and melted into it, so that it reads as part
/// of the bar, with the waves it leaves on the bar's edge as it folds back,
/// `ripple` seconds after it went in. Early on the card is narrow along the
/// bar and reaches out like a drop leaving it, as pleama-rs's cards do; its
/// concave corners grow in with it.
pub fn draw(
    scene: &mut Scene,
    side: Side,
    c: &Config,
    overlap: f32,
    grow: f32,
    ripple: Option<f32>,
    t: &Theme,
) {
    let frame = Frame::new(side, c, overlap);
    let e = edges(side, c);
    let mid = e.inverse_radius + card_along(side) / 2.0;
    let wave = ripple.and_then(|t| wave(mid, t));
    let g = grow.max(0.0);
    let into = e.radii.iter().fold(0.0f32, |m, &r| m.max(r)) + e.border + 1.0;
    let card = if g >= 0.002 {
        let along = card_along(side)
            * (0.3 + 0.7 * smoothstep(0.0, 0.9, g))
            * (1.0 + (g - 1.0).max(0.0) * 0.5);
        Blob {
            along: (mid - along / 2.0, mid + along / 2.0),
            out: card_across(side) * g,
            into,
            radii: e.radii,
            blend: e.inverse_radius * smoothstep(0.0, 0.3, g),
        }
    } else if wave.is_some() {
        // Sunk into the bar, so that only the edge and its waves show.
        Blob {
            along: (mid, mid),
            out: -into,
            into,
            radii: [0.0; 4],
            blend: 0.0,
        }
    } else {
        return;
    };
    let reach = card.out.max(0.0) + card.blend + wave.map_or(0.0, |w| w.amplitude) + 2.0;
    // The line is the whole shape in its colour, and the fill the same
    // shape a border's width in.
    let b = e.border;
    if b > 0.0 {
        scene.push(card.liquid(&frame, 0.0, wave, reach, t.outline));
    }
    scene.push(card.liquid(&frame, b, wave, reach, t.surface));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waves_rise_run_apart_and_die_away() {
        let rising = wave(100.0, 0.05).unwrap();
        let full = wave(100.0, 0.1).unwrap();
        let late = wave(100.0, 1.0).unwrap();
        assert!(rising.amplitude < full.amplitude && late.amplitude < full.amplitude / 5.0);
        assert_eq!((late.center, late.group, late.phase), (100.0, 60.0, 120.0));
        assert_eq!(wave(100.0, RIPPLE), None);
    }

    #[test]
    fn after_the_card_only_the_bars_edge_and_its_waves_are_drawn() {
        let mut scene = Scene::new();
        let c = Config::default();
        draw(
            &mut scene,
            Side::Below,
            &c,
            0.0,
            0.0,
            Some(0.3),
            &Theme::builtin(),
        );
        assert_eq!(scene.shapes.len(), 1);
        let [p0, p1] = scene.shapes[0].bounds();
        assert_eq!(p0.y, 0.0);
        assert!(p1.y < 12.0, "drawn down to {}", p1.y);
        draw(
            &mut scene,
            Side::Below,
            &c,
            0.0,
            0.0,
            None,
            &Theme::builtin(),
        );
        assert_eq!(scene.shapes.len(), 1, "nothing more once they are gone");
    }
}
