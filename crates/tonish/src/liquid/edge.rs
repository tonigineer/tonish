//! Positions along and across an edge a card hangs from: the bar's, the
//! frame's, or the monitor's own.

use engine::Side;
use renderer::Point;

/// Positions on the monitor by how far along the edge, as the monitor
/// counts along it, and how far away from the edge, so that one piece of
/// geometry serves every side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    /// The side of the edge the card is on.
    pub side: Side,
    /// Where the edge is across: the monitor's y for an edge along the top
    /// or bottom, its x for one down a side.
    pub edge: f32,
    /// How far the card reaches over the edge's border, which it covers
    /// where they meet, so that no line runs between them.
    pub overlap: f32,
    /// The monitor's size, which clips everything.
    pub screen: (f32, f32),
    /// Where the edge runs straight, along it, before the corners at its
    /// ends round off.
    pub straight: (f32, f32),
}

/// An edge a card can hang from, and where along it its middle may hang:
/// far enough in from the ends of the straight edge for its concave
/// corners, which past them would curve out of nothing. Where another edge
/// meets it across an end, as two sides of the frame do, the card may sit
/// in the corner instead, against both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dock {
    pub edge: Edge,
    pub range: (f32, f32),
    /// The edges across its start and its end, as [`Edge::across_end`]
    /// finds them.
    pub corners: [Option<Edge>; 2],
}

/// How far apart two edges may end and still meet in a corner: round the
/// frame's curve inside, or the monitor's round corner, between them.
const CORNER: f32 = 96.0;

impl Edge {
    pub fn at(&self, along: f32, across: f32) -> Point {
        let e = self.edge;
        match self.side {
            Side::Below => Point::new(along, e + across),
            Side::Above => Point::new(along, e - across),
            Side::Right => Point::new(e + across, along),
            Side::Left => Point::new(e - across, along),
        }
    }

    /// Along and across, of a point on the monitor.
    pub fn local(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let e = self.edge;
        match self.side {
            Side::Below => (x, y - e),
            Side::Above => (x, e - y),
            Side::Right => (y, x - e),
            Side::Left => (y, e - x),
        }
    }

    /// A motion along and across, on the monitor.
    pub fn vector(&self, (along, across): (f32, f32)) -> (f32, f32) {
        match self.side {
            Side::Below => (along, across),
            Side::Above => (along, -across),
            Side::Right => (across, along),
            Side::Left => (-across, along),
        }
    }

    /// Along and across, of a motion on the monitor.
    pub fn local_vector(&self, (x, y): (f32, f32)) -> (f32, f32) {
        match self.side {
            Side::Below => (x, y),
            Side::Above => (x, -y),
            Side::Right => (y, x),
            Side::Left => (y, -x),
        }
    }

    /// Whether along on this edge is across on `other`.
    pub fn turned(&self, other: &Edge) -> bool {
        let flat = |s: Side| matches!(s, Side::Above | Side::Below);
        flat(self.side) != flat(other.side)
    }

    /// The one of `others` that meets this edge across the start of its
    /// straight part, at `end` 0, or across its end, at 1: square to it,
    /// facing back along it, and with its own straight part starting near
    /// this edge, as two sides of a frame meet round the curve between
    /// them.
    pub fn across_end(&self, end: usize, others: &[Edge]) -> Option<Edge> {
        let (at, back) = if end == 0 {
            (self.straight.0, 1.0)
        } else {
            (self.straight.1, -1.0)
        };
        let along = |(x, y): (f32, f32)| match self.side {
            Side::Above | Side::Below => x,
            Side::Left | Side::Right => y,
        };
        let gap = |o: &Edge| (o.edge - at) * back;
        (others.iter().copied())
            .filter(|o| self.turned(o) && along(o.vector((0.0, 1.0))) * back > 0.0)
            .filter(|o| {
                let near = (o.straight.0 - self.edge).abs();
                let near = near.min((o.straight.1 - self.edge).abs());
                (-CORNER..=0.0).contains(&gap(o)) && near <= CORNER
            })
            .min_by(|a, b| gap(b).total_cmp(&gap(a)))
    }

    /// The box between two corners given along and across, in either order.
    pub fn rect(&self, (a0, c0): (f32, f32), (a1, c1): (f32, f32)) -> [Point; 2] {
        let (p, q) = (self.at(a0, c0), self.at(a1, c1));
        [
            Point::new(p.x.min(q.x), p.y.min(q.y)),
            Point::new(p.x.max(q.x), p.y.max(q.y)),
        ]
    }

    /// The side of the edge away from the card, as a
    /// [`Liquid`](renderer::Liquid) takes it.
    pub fn edge(&self) -> (Point, f32) {
        let e = self.edge;
        match self.side {
            Side::Below => (Point::new(0.0, 1.0), e),
            Side::Above => (Point::new(0.0, -1.0), -e),
            Side::Right => (Point::new(1.0, 0.0), e),
            Side::Left => (Point::new(-1.0, 0.0), -e),
        }
    }

    fn along_screen(&self) -> f32 {
        match self.side {
            Side::Above | Side::Below => self.screen.0,
            Side::Left | Side::Right => self.screen.1,
        }
    }

    fn across_screen(&self) -> f32 {
        match self.side {
            Side::Below => self.screen.1 - self.edge,
            Side::Above => self.edge,
            Side::Right => self.screen.0 - self.edge,
            Side::Left => self.edge,
        }
    }

    /// Where the middle of a card `half` its length and depth from it may
    /// go: whole on the monitor, and no nearer the edge than where it
    /// hangs.
    pub fn keep(&self, (l, a): (f32, f32), (along, across): (f32, f32)) -> (f32, f32) {
        let along_max = (self.along_screen() - l).max(l);
        let across_max = (self.across_screen() - a).max(a);
        (along.clamp(l, along_max), across.clamp(a, across_max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(side: Side, edge: f32, straight: (f32, f32)) -> Edge {
        Edge {
            side,
            edge,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            straight,
        }
    }

    #[test]
    fn the_frame_s_sides_meet_in_its_corners() {
        // A frame 8 wide, with curves of 16 inside, and a bar 44 high.
        let bar = edge(Side::Below, 44.0, (24.0, 1896.0));
        let bottom = edge(Side::Above, 1072.0, (24.0, 1896.0));
        let left = edge(Side::Right, 8.0, (60.0, 1056.0));
        let right = edge(Side::Left, 1912.0, (60.0, 1056.0));
        let all = [bar, bottom, left, right];
        assert_eq!(left.across_end(0, &all), Some(bar));
        assert_eq!(left.across_end(1, &all), Some(bottom));
        assert_eq!(bottom.across_end(0, &all), Some(left));
        assert_eq!(bottom.across_end(1, &all), Some(right));
        // A shorter bar, far from the corners, meets no side.
        let short = edge(Side::Below, 44.0, (460.0, 1460.0));
        assert_eq!(short.across_end(0, &[short, left, right]), None);
    }

    #[test]
    fn a_motion_turns_with_the_edge() {
        for side in [Side::Below, Side::Above, Side::Right, Side::Left] {
            let e = Edge {
                side,
                edge: 30.0,
                overlap: 0.0,
                screen: (1920.0, 1080.0),
                straight: (0.0, 1080.0),
            };
            let p = e.at(100.0, 40.0);
            let q = e.at(103.0, 44.0);
            assert_eq!(e.vector((3.0, 4.0)), (q.x - p.x, q.y - p.y), "{side:?}");
            assert_eq!(e.local_vector(e.vector((3.0, 4.0))), (3.0, 4.0));
            assert_eq!(e.local((p.x, p.y)), (100.0, 40.0));
        }
    }

    #[test]
    fn the_card_stays_whole_on_the_monitor() {
        let f = Edge {
            side: Side::Below,
            edge: 44.0,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            straight: (0.0, 1920.0),
        };
        assert_eq!(
            f.keep((140.0, 90.0), (0.0, 5000.0)),
            (140.0, 1080.0 - 44.0 - 90.0)
        );
        assert_eq!(f.keep((140.0, 90.0), (900.0, 0.0)), (900.0, 90.0));
    }
}
