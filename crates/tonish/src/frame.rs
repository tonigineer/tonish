//! A frame along every edge of the monitor, the bar's body with it, and the
//! monitor's own corners.
//!
//! The frame and the bar are one [`Rim`]: the space between the monitor's
//! edge and a rectangle inside it, with the bar joined into it, so that
//! where they meet they curve into each other and a border runs on round
//! them. Surfaces of their own draw it in tiles, each only its own part, so
//! that no pixel is drawn twice, which a translucent colour would show, and
//! no surface is larger than its part of the frame. The bar's own surface
//! draws its part. They keep no room: thin surfaces of their own keep it for
//! windows along every edge, because a surface that keeps room is moved by
//! the room the others keep, in an order compositors differ on, while one
//! that keeps none stays where it is put. The corners are surfaces of their
//! own, on the overlay layer, the one compositors show over fullscreen
//! windows too, and just as large as a corner. All of these are drawn once,
//! and stay on the CPU.

use engine::Side;
use renderer::shapes::Joined;
use renderer::{Fillet, Point, Rgba, Rim, Scene};

use crate::config;
use crate::liquid;
use crate::shell::{Orientation, Panel};
use crate::theme::Theme;

/// The frame as the settings shape it, with the border every surface
/// shares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub width: f32,
    pub inverse_radius: f32,
    pub color: Rgba,
    pub border: f32,
}

impl Frame {
    /// `None` where the settings leave it out.
    pub fn new(c: &config::Config) -> Option<Self> {
        let f = c.frame;
        f.shown().then_some(Self {
            width: f.width,
            inverse_radius: f.inverse_radius,
            color: f.color,
            border: c.border,
        })
    }
}

/// A part of the frame one surface draws, on the monitor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub rect: [Point; 2],
    /// In the bar's colour rather than the frame's.
    pub bar: bool,
}

/// The frame and the bar as one shape, and the tiles it is drawn in.
pub struct Layout {
    pub rim: Rim,
    /// Each on a surface of its own.
    pub tiles: Vec<Tile>,
    /// On the bar's own surface.
    pub bar: Vec<Tile>,
}

impl Layout {
    /// The bar is the first of `panels` along an edge; there is no other
    /// yet.
    pub fn new(screen: (f32, f32), f: &Frame, panels: &[Panel]) -> Self {
        let bar = panels.iter().find(|p| p.along_edge());
        // Along the bar's edge and across it, away from the edge, so that
        // one piece of geometry serves every side.
        let side = bar.map_or(Side::Below, |p| p.side());
        let (sw, sh) = screen;
        let (length, depth) = match side {
            Side::Above | Side::Below => (sw, sh),
            Side::Left | Side::Right => (sh, sw),
        };
        let at = |along: f32, across: f32| match side {
            Side::Below => Point::new(along, across),
            Side::Above => Point::new(along, depth - across),
            Side::Right => Point::new(across, along),
            Side::Left => Point::new(depth - across, along),
        };
        let rect = |(a0, c0): (f32, f32), (a1, c1): (f32, f32)| {
            let (u, v) = (at(a0, c0), at(a1, c1));
            [
                Point::new(u.x.min(v.x), u.y.min(v.y)),
                Point::new(u.x.max(v.x), u.y.max(v.y)),
            ]
        };
        // Along and across, of a point on the monitor.
        let local = |p: Point| match side {
            Side::Below => (p.x, p.y),
            Side::Above => (p.x, depth - p.y),
            Side::Right => (p.y, p.x),
            Side::Left => (p.y, depth - p.x),
        };
        let (w, ir) = (f.width, f.inverse_radius);
        let mut tiles = Vec::new();
        let mut parts = Vec::new();
        let mut inner_near = w;
        // Every corner inside the frame curves with its inverse radius:
        // where two of its sides meet, and where it meets a bar that spans
        // it. On the bar's side, a gap may take less.
        let mut near_radius = ir;
        let mut joined = None;
        let mut side_start = w;
        // The frame along the bar's edge and its corners there, unless a
        // bar that spans it takes their place.
        let mut near_side = true;
        // A shorter bar's surface, which draws its own part.
        let mut hole = None;
        if let Some(p) = bar {
            let (g, t) = (p.gap, p.thickness);
            let origin = p.origin(screen);
            let size = p.size(screen);
            let surface = [
                Point::new(origin.0, origin.1),
                Point::new(origin.0 + size.0, origin.1 + size.1),
            ];
            if p.spans() && g == 0.0 {
                // The bar is the frame along its edge, and the frame's sides
                // curve out of its far side.
                inner_near = t;
                side_start = t;
                near_side = false;
                for (a0, a1) in [(w, w + ir), (length - w - ir, length - w)] {
                    tiles.push(rect((a0, t), (a1, t + ir)));
                }
                parts.push(Tile {
                    rect: surface,
                    bar: true,
                });
            } else if p.spans() {
                // Inside the frame, the gap away from it, curving into its
                // sides at all four corners, with one radius for all of
                // them: no more than half the gap, which the near side
                // shares between the frame's corner and the bar's.
                let (near, far) = (w + g, w + g + t);
                let r = ir.min(g / 2.0);
                near_radius = r;
                near_side = false;
                tiles.push(rect((0.0, 0.0), (length, w)));
                let [p0, p1] = rect((w, near), (length - w, far));
                joined = Some(Joined {
                    p0,
                    p1,
                    radii: [0.0; 4],
                    blend: r,
                });
                // The ends of the gap, with the frame's corner above and
                // the bar's below, and the curves below the bar.
                for (a0, a1) in [(w, w + r), (length - w - r, length - w)] {
                    tiles.push(rect((a0, far), (a1, far + r)));
                    tiles.push(rect((a0, w), (a1, near)));
                }
                parts.push(Tile {
                    rect: surface,
                    bar: true,
                });
            } else {
                // Shorter than its edge: its body joined into the frame,
                // curving into it with its own inverse radius where it
                // hangs, beside it in the frame's colour.
                let room = p.room();
                let body = body(p, screen);
                joined = Some(Joined {
                    p0: body[0],
                    p1: body[1],
                    radii: p.edges.radii,
                    blend: if p.hangs() {
                        p.edges.inverse_radius
                    } else {
                        0.0
                    },
                });
                let ((u0, v0), (u1, v1)) = (local(body[0]), local(body[1]));
                let (a0, a1) = (u0.min(u1), u0.max(u1));
                let (s0, s1) = (local(surface[0]).1, local(surface[1]).1);
                let (c0, c1) = (s0.min(s1).min(v0.min(v1)), s0.max(s1).max(v0.max(v1)));
                hole = Some(rect((a0 - room, c0), (a1 + room, c1)));
                for (from, to, bar) in [
                    (a0 - room, a0, false),
                    (a0, a1, true),
                    (a1, a1 + room, false),
                ] {
                    if to > from {
                        parts.push(Tile {
                            rect: rect((from, c0), (to, c1)),
                            bar,
                        });
                    }
                }
            }
        }
        let ends = [(w, w + ir), (length - w - ir, length - w)];
        if near_side {
            tiles.push(rect((0.0, 0.0), (length, w)));
            for (a0, a1) in ends {
                tiles.push(rect((a0, w), (a1, w + ir)));
            }
        }
        for (a0, a1) in ends {
            tiles.push(rect((a0, depth - w - ir), (a1, depth - w)));
        }
        tiles.push(rect((0.0, depth - w), (length, depth)));
        tiles.push(rect((0.0, side_start), (w, depth - w)));
        tiles.push(rect((length - w, side_start), (length, depth - w)));
        let tiles = tiles
            .into_iter()
            .flat_map(|r| hole.map_or_else(|| vec![r], |hole| minus(r, hole)))
            .map(whole)
            .filter(|[p0, p1]| p1.x > p0.x && p1.y > p0.y)
            .map(|rect| Tile { rect, bar: false })
            .collect();
        let bar = parts
            .into_iter()
            .map(|t| Tile {
                rect: whole(t.rect),
                ..t
            })
            .collect();
        // Past the monitor's edge by more than a border, so that the line
        // round the shape runs only where the frame meets what is inside it.
        let past = f.border + 2.0;
        let (n, r) = (near_radius, ir);
        Self {
            rim: Rim {
                outer: [Point::new(-past, -past), Point::new(sw + past, sh + past)],
                inner: rect((w, inner_near), (length - w, depth - w)),
                radii: match side {
                    Side::Below => [n, n, r, r],
                    Side::Above => [r, r, n, n],
                    Side::Right => [n, r, r, n],
                    Side::Left => [r, n, n, r],
                },
                joined: joined.map(|j| reach_past(j, screen, past)),
                inset: 0.0,
                stroke: 0.0,
                clip: [Point::new(0.0, 0.0), Point::new(sw, sh)],
                color: f.color,
            },
            tiles,
            bar,
        }
    }
}

/// The straight edges a card may dock to besides the bar's: the frame's
/// inside edges, or without a frame, the monitor's own, each between the
/// corners at its ends, and on the bar's side, beside the bar. The bar is
/// the first of `panels` along an edge, as for [`Layout::new`].
pub fn edges(
    screen: (f32, f32),
    f: Option<&Frame>,
    corners: f32,
    panels: &[Panel],
) -> Vec<liquid::Edge> {
    let bar = panels.iter().find(|p| p.along_edge());
    // Along the bar's edge and across it, as in `Layout::new`.
    let side = bar.map_or(Side::Below, |p| p.side());
    let (sw, sh) = screen;
    let (length, depth) = match side {
        Side::Above | Side::Below => (sw, sh),
        Side::Left | Side::Right => (sh, sw),
    };
    let at = |along: f32, across: f32| match side {
        Side::Below => Point::new(along, across),
        Side::Above => Point::new(along, depth - across),
        Side::Right => Point::new(across, along),
        Side::Left => Point::new(depth - across, along),
    };
    let local = |p: Point| match side {
        Side::Below => (p.x, p.y),
        Side::Above => (p.x, depth - p.y),
        Side::Right => (p.y, p.x),
        Side::Left => (p.y, depth - p.x),
    };
    let (w, ir, overlap) = f.map_or((0.0, 0.0, 0.0), |f| {
        (f.width, f.inverse_radius, f.border.ceil())
    });
    let cut = cut(f, corners);
    // Where the sides across the bar's edge start, at either end of it,
    // and the stretches along the bar's side, beside the bar.
    let mut start = (cut, cut);
    let mut near = vec![(cut, length - cut)];
    if let Some(p) = bar {
        let (g, t) = (p.gap, p.thickness);
        if p.spans() {
            near.clear();
            let below = match f {
                Some(_) if g > 0.0 => w + g + t + ir.min(g / 2.0),
                Some(_) => t + ir,
                None => g + t,
            };
            start = (below.max(corners), below.max(corners));
        } else {
            let (x, y) = p.origin(screen);
            let (dx, dy) = p.size(screen);
            let (u, v) = (local(Point::new(x, y)), local(Point::new(x + dx, y + dy)));
            let (a0, a1, reach) = (u.0.min(v.0), u.0.max(v.0), u.1.max(v.1));
            // A bar its edge runs on into is part of that one dock.
            near = if p.merges() {
                Vec::new()
            } else {
                vec![(cut, a0 - g), (a1 + g, length - cut)]
            };
            // A bar that reaches into a corner: the side starts below it.
            if a0 < cut {
                start.0 = start.0.max(reach + ir);
            }
            if a1 > length - cut {
                start.1 = start.1.max(reach + ir);
            }
        }
    }
    let edge = |from: (f32, f32), to: (f32, f32), inward: (f32, f32)| {
        let (p, q) = (at(from.0, from.1), at(to.0, to.1));
        let o = at(from.0 + inward.0, from.1 + inward.1);
        let (side, edge, straight) = if p.y == q.y {
            let side = if o.y > p.y { Side::Below } else { Side::Above };
            (side, p.y, (p.x.min(q.x), p.x.max(q.x)))
        } else {
            let side = if o.x > p.x { Side::Right } else { Side::Left };
            (side, p.x, (p.y.min(q.y), p.y.max(q.y)))
        };
        liquid::Edge {
            side,
            edge,
            overlap,
            screen,
            straight,
        }
    };
    let mut edges: Vec<_> = (near.into_iter())
        .map(|(a0, a1)| edge((a0, w), (a1, w), (0.0, 1.0)))
        .collect();
    edges.push(edge(
        (cut, depth - w),
        (length - cut, depth - w),
        (0.0, -1.0),
    ));
    edges.push(edge((w, start.0), (w, depth - cut), (1.0, 0.0)));
    edges.push(edge(
        (length - w, start.1),
        (length - w, depth - cut),
        (-1.0, 0.0),
    ));
    edges.retain(|e| e.straight.1 > e.straight.0);
    edges
}

/// How far in from the monitor's corners an edge along it runs straight:
/// past the frame's curved corners inside, or without a frame, past the
/// monitor's own round corners.
pub fn cut(f: Option<&Frame>, corners: f32) -> f32 {
    f.map_or(0.0, |f| f.width + f.inverse_radius).max(corners)
}

/// On whole logical pixels, as surfaces are placed, the same way for every
/// edge two tiles share, so that they meet without a gap or an overlap.
fn whole([p0, p1]: [Point; 2]) -> [Point; 2] {
    [
        Point::new(p0.x.round(), p0.y.round()),
        Point::new(p1.x.round(), p1.y.round()),
    ]
}

/// What of `r` lies outside `hole`, in up to four rectangles, some of them
/// empty.
fn minus(r: [Point; 2], hole: [Point; 2]) -> Vec<[Point; 2]> {
    let ([p0, p1], [h0, h1]) = (r, hole);
    if h0.x >= p1.x || h1.x <= p0.x || h0.y >= p1.y || h1.y <= p0.y {
        return vec![r];
    }
    let (x0, x1) = (h0.x.max(p0.x), h1.x.min(p1.x));
    vec![
        [p0, Point::new(x0, p1.y)],
        [Point::new(x1, p0.y), p1],
        [Point::new(x0, p0.y), Point::new(x1, h0.y)],
        [Point::new(x0, h1.y), Point::new(x1, p1.y)],
    ]
}

/// A joined box that touches the monitor's edge reaches past it as the rim
/// does, so that no line runs along it there.
fn reach_past(mut j: Joined, (sw, sh): (f32, f32), past: f32) -> Joined {
    if j.p0.x <= 0.0 {
        j.p0.x = -past;
    }
    if j.p0.y <= 0.0 {
        j.p0.y = -past;
    }
    if j.p1.x >= sw {
        j.p1.x = sw + past;
    }
    if j.p1.y >= sh {
        j.p1.y = sh + past;
    }
    j
}

/// The panel's body on the monitor, without the room at its ends for its
/// concave corners.
fn body(p: &Panel, screen: (f32, f32)) -> [Point; 2] {
    let origin = p.origin(screen);
    let size = p.size(screen);
    let room = p.room();
    match p.orientation {
        Orientation::Horizontal => [
            Point::new(origin.0 + room, origin.1),
            Point::new(origin.0 + size.0 - room, origin.1 + size.1),
        ],
        Orientation::Vertical => [
            Point::new(origin.0, origin.1 + room),
            Point::new(origin.0 + size.0, origin.1 + size.1 - room),
        ],
    }
}

/// The parts of `rim` within `tiles`, on a surface whose top-left corner
/// is `origin` on the monitor. With a border, a line inside the shape's edge
/// and the fill inside that, which never overlap.
pub fn draw(scene: &mut Scene, rim: &Rim, tiles: &[Tile], origin: Point, f: &Frame, t: &Theme) {
    let shift = |p: Point| Point::new(p.x - origin.x, p.y - origin.y);
    let joined = rim.joined.map(|j| Joined {
        p0: shift(j.p0),
        p1: shift(j.p1),
        ..j
    });
    let base = Rim {
        outer: rim.outer.map(shift),
        inner: rim.inner.map(shift),
        joined,
        ..*rim
    };
    let b = f.border;
    for tile in tiles {
        let clip = tile.rect.map(shift);
        let color = if tile.bar { t.surface } else { f.color };
        if b > 0.0 {
            scene.push(Rim {
                inset: b / 2.0,
                stroke: b,
                clip,
                color: t.outline,
                ..base
            });
        }
        scene.push(Rim {
            inset: b,
            stroke: 0.0,
            clip,
            color,
            ..base
        });
    }
}

/// One of the monitor's corners, on a surface `radius` square: what lies
/// outside the curve, in `color`. `toward` points from the corner into the
/// monitor.
pub fn corner(scene: &mut Scene, toward: (f32, f32), radius: f32, color: Rgba) {
    let p = |x: f32, y: f32| Point::new(x, y);
    let (cx, cy) = (
        if toward.0 > 0.0 { 0.0 } else { radius },
        if toward.1 > 0.0 { 0.0 } else { radius },
    );
    let far = p(cx + toward.0 * radius, cy + toward.1 * radius);
    scene.push(Fillet {
        p0: p(cx.min(far.x), cy.min(far.y)),
        p1: p(cx.max(far.x), cy.max(far.y)),
        center: far,
        r: radius,
        color,
    });
}
