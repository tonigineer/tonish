//! A rectangle that hangs from an edge, as a panel hangs from the monitor's
//! edge and the settings hang from a panel: rounded at its free corners,
//! and curving out of the edge at the others, so that it reads as part of
//! what it hangs from.

use engine::Side;
use renderer::{Corner, Fillet, Point, Rgba, RoundedRect, Scene};

/// `p0` and `p1` are the rectangle's top-left and bottom-right corners,
/// two of them on the edge, and `side` is where the rectangle is from the
/// edge. The concave corners take `r` beside the rectangle along the edge,
/// which the surface has to have room for.
pub fn draw(scene: &mut Scene, [p0, p1]: [Point; 2], side: Side, r: f32, color: Rgba) {
    // Past the edge, so that only the free corners show rounded.
    let (mut q0, mut q1) = (p0, p1);
    match side {
        Side::Below => q0.y -= r,
        Side::Above => q1.y += r,
        Side::Right => q0.x -= r,
        Side::Left => q1.x += r,
    }
    scene.push(RoundedRect {
        p0: q0,
        p1: q1,
        r,
        color,
    });
    let fillets = match side {
        Side::Below => [
            (Point::new(p0.x, p0.y), Corner::TopRight),
            (Point::new(p1.x, p0.y), Corner::TopLeft),
        ],
        Side::Above => [
            (Point::new(p0.x, p1.y), Corner::BottomRight),
            (Point::new(p1.x, p1.y), Corner::BottomLeft),
        ],
        Side::Right => [
            (Point::new(p0.x, p0.y), Corner::BottomLeft),
            (Point::new(p0.x, p1.y), Corner::TopLeft),
        ],
        Side::Left => [
            (Point::new(p1.x, p0.y), Corner::BottomRight),
            (Point::new(p1.x, p1.y), Corner::TopRight),
        ],
    };
    for (at, corner) in fillets {
        scene.push(Fillet {
            at,
            corner,
            r,
            color,
        });
    }
}
