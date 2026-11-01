//! Positions along and across the edge a card hangs from.

use engine::Side;
use renderer::Point;

/// Positions on the monitor by how far along the bar's edge, as the
/// monitor counts along it, and how far away from the edge, so that one
/// piece of geometry serves every side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub side: Side,
    /// Where the bar's edge is across: the monitor's y for a bar along the
    /// top or bottom, its x for one down a side.
    pub edge: f32,
    /// How far the panel reaches over the bar's border, which it covers
    /// where the card meets the bar, so that no line runs between them.
    pub overlap: f32,
    /// The monitor's size, which clips everything.
    pub screen: (f32, f32),
    /// Where the bar's straight edge runs, along it, before its far corners
    /// round off.
    pub bar: (f32, f32),
}

impl Frame {
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

    /// The box between two corners given along and across, in either order.
    pub fn rect(&self, (a0, c0): (f32, f32), (a1, c1): (f32, f32)) -> [Point; 2] {
        let (p, q) = (self.at(a0, c0), self.at(a1, c1));
        [
            Point::new(p.x.min(q.x), p.y.min(q.y)),
            Point::new(p.x.max(q.x), p.y.max(q.y)),
        ]
    }

    /// The bar's side of its edge, as a [`Liquid`](renderer::Liquid) takes
    /// it.
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
    /// go: whole on the monitor, and no nearer the bar than where it hangs.
    pub fn keep(&self, (l, a): (f32, f32), (along, across): (f32, f32)) -> (f32, f32) {
        let along_max = (self.along_screen() - l).max(l);
        let across_max = (self.across_screen() - a).max(a);
        (along.clamp(l, along_max), across.clamp(a, across_max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_stays_whole_on_the_monitor() {
        let f = Frame {
            side: Side::Below,
            edge: 44.0,
            overlap: 0.0,
            screen: (1920.0, 1080.0),
            bar: (0.0, 1920.0),
        };
        assert_eq!(
            f.keep((140.0, 90.0), (0.0, 5000.0)),
            (140.0, 1080.0 - 44.0 - 90.0)
        );
        assert_eq!(f.keep((140.0, 90.0), (900.0, 0.0)), (900.0, 90.0));
    }
}
