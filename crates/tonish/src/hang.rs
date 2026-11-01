//! A rectangle that hangs from an edge, as the bar hangs from the monitor's
//! edge and a panel hangs from the bar: rounded at its free corners, and
//! curving out of the edge at the others, so that it reads as part of what
//! it hangs from. Shared, so that the bar and the panels end the same way.

use engine::Side;
use renderer::{Fillet, Point, Rgba, RoundedRect, Scene};

/// Logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    /// Top left, top right, bottom right, bottom left, as CSS orders them.
    pub radii: [f32; 4],
    /// Of the concave corners where the rectangle meets its edge.
    pub inverse_radius: f32,
    /// The width of the line around it.
    pub border: f32,
}

impl Edges {
    /// Cut to what a rectangle `across` deep, from its edge to its far side,
    /// has room for, `side` being where it is from its edge. Its corners
    /// must not overlap. A concave corner has to end on the straight part of
    /// its side, before the far corner's rounding begins, for the line and
    /// for the fill a border's width in; past that, the two curves meet in a
    /// spike.
    pub fn fit(self, across: f32, side: Side) -> Self {
        let radii = self.radii.map(|r| r.min(across / 2.0));
        let border = self.border.min(across / 2.0);
        let far = match side {
            Side::Below => [radii[3], radii[2]],
            Side::Above => [radii[0], radii[1]],
            Side::Right => [radii[1], radii[2]],
            Side::Left => [radii[0], radii[3]],
        };
        let room = across - far[0].max(far[1]).max(border);
        Self {
            radii,
            border,
            inverse_radius: self.inverse_radius.min(room).max(0.0),
        }
    }
}

/// `p0` and `p1` are the rectangle's top-left and bottom-right corners, and
/// `side` is where it is from its edge. If it `hangs`, two of its corners
/// lie on the edge and curve out into it, which takes `inverse_radius`
/// beside the rectangle along the edge, which the surface has to have room
/// for; at 0 they meet the edge square. If it doesn't hang, all four corners
/// are rounded. The fill reaches `cover` past
/// the edge, over the line of what the rectangle hangs from, where the
/// surface reaches there too.
pub fn draw(
    scene: &mut Scene,
    [p0, p1]: [Point; 2],
    side: Side,
    hangs: bool,
    e: Edges,
    cover: f32,
    fill: Rgba,
    line: Rgba,
) {
    let b = e.border;
    if b <= 0.0 {
        body(scene, [p0, p1], side, hangs, e.radii, cover, fill);
        if hangs && e.inverse_radius > 0.0 {
            fillets(scene, [p0, p1], side, e.inverse_radius, 0.0, fill);
        }
        return;
    }
    // The line is the whole shape in its colour, with the fill drawn over
    // it a border's width in from the free sides.
    body(scene, [p0, p1], side, hangs, e.radii, cover, line);
    let (mut q0, mut q1) = (
        Point::new(p0.x + b, p0.y + b),
        Point::new(p1.x - b, p1.y - b),
    );
    let curved = hangs && e.inverse_radius > 0.0;
    if curved {
        fillets(scene, [p0, p1], side, e.inverse_radius, 0.0, line);
    }
    if hangs {
        match side {
            Side::Below => q0.y = p0.y,
            Side::Above => q1.y = p1.y,
            Side::Right => q0.x = p0.x,
            Side::Left => q1.x = p1.x,
        }
    }
    body(
        scene,
        [q0, q1],
        side,
        hangs,
        e.radii.map(|r| (r - b).max(0.0)),
        cover,
        fill,
    );
    if curved {
        fillets(scene, [p0, p1], side, e.inverse_radius, b, fill);
    }
}

fn body(
    scene: &mut Scene,
    [p0, p1]: [Point; 2],
    side: Side,
    hangs: bool,
    radii: [f32; 4],
    cover: f32,
    color: Rgba,
) {
    // Past the edge, so that only the free corners show rounded.
    let (mut p0, mut p1) = (p0, p1);
    if hangs {
        let past = radii.iter().fold(0.0f32, |a, &r| a.max(r)) + cover;
        match side {
            Side::Below => p0.y -= past,
            Side::Above => p1.y += past,
            Side::Right => p0.x -= past,
            Side::Left => p1.x += past,
        }
    }
    scene.push(RoundedRect {
        p0,
        p1,
        radii,
        color,
    });
}

/// The concave corners where the rectangle's ends meet its edge, tangent to
/// it. For the fill over a border, `grow` is the border's width: the curve
/// keeps its centre and grows by that much, which makes it tangent to the
/// fill's edge of what the rectangle hangs from, a border's width past the
/// edge. The line between the two curves then keeps its width round them,
/// and runs on into the line of what the rectangle hangs from.
fn fillets(scene: &mut Scene, [p0, p1]: [Point; 2], side: Side, r: f32, grow: f32, color: Rgba) {
    // Along the edge and across it, away from it, so that one piece of
    // geometry serves every side.
    let (edge, start, end) = match side {
        Side::Below => (p0.y, p0.x, p1.x),
        Side::Above => (p1.y, p0.x, p1.x),
        Side::Right => (p0.x, p0.y, p1.y),
        Side::Left => (p1.x, p0.y, p1.y),
    };
    let at = |along: f32, across: f32| match side {
        Side::Below => Point::new(along, edge + across),
        Side::Above => Point::new(along, edge - across),
        Side::Right => Point::new(edge + across, along),
        Side::Left => Point::new(edge - across, along),
    };
    for (from, to, centre) in [
        (start - r, start + grow, start - r),
        (end - grow, end + r, end + r),
    ] {
        let (a, b) = (at(from, -grow), at(to, r));
        scene.push(Fillet {
            p0: Point::new(a.x.min(b.x), a.y.min(b.y)),
            p1: Point::new(a.x.max(b.x), a.y.max(b.y)),
            center: at(centre, r),
            r: r + grow,
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(radius: f32, inverse_radius: f32, border: f32) -> Edges {
        Edges {
            radii: [radius; 4],
            inverse_radius,
            border,
        }
    }

    fn fit(e: Edges) -> Edges {
        e.fit(44.0, Side::Below)
    }

    #[test]
    fn a_concave_corner_ends_where_the_free_corner_begins() {
        assert_eq!(fit(edges(12.0, 50.0, 0.0)), edges(12.0, 32.0, 0.0));
        assert_eq!(fit(edges(0.0, 50.0, 0.0)), edges(0.0, 44.0, 0.0));
    }

    #[test]
    fn with_a_border_the_fill_s_concave_corner_ends_there_too() {
        // The fill's free corner starts the border's width in, or, rounded
        // less than the border is wide, the border's width from the far side.
        assert_eq!(fit(edges(12.0, 50.0, 2.0)), edges(12.0, 32.0, 2.0));
        assert_eq!(fit(edges(1.0, 50.0, 2.0)), edges(1.0, 42.0, 2.0));
    }

    #[test]
    fn only_the_far_corners_limit_the_concave_ones() {
        // Hanging below its edge, its top corners are on the edge.
        let mut top_round = edges(0.0, 50.0, 0.0);
        top_round.radii = [20.0, 20.0, 4.0, 2.0];
        assert_eq!(fit(top_round).inverse_radius, 40.0);
    }

    #[test]
    fn corners_and_border_take_at_most_half_the_depth() {
        assert_eq!(fit(edges(100.0, 0.0, 100.0)), edges(22.0, 0.0, 22.0));
    }
}
