//! The settings panel, which a right-click on a panel opens, hanging from
//! it.

use engine::Side;
use renderer::{Corner, Fillet, Point, RoundedRect, Scene};

use crate::theme::Theme;

/// Logical pixels.
pub const CARD: (f32, f32) = (280.0, 180.0);
/// Of the card's free corners, and of the concave corners where it meets
/// the panel.
pub const RADIUS: f32 = 14.0;

/// The card, and room beside it along the panel for the concave corners.
pub fn size(side: Side) -> (u32, u32) {
    let (w, h) = CARD;
    let (w, h) = match side {
        Side::Above | Side::Below => (w + 2.0 * RADIUS, h),
        Side::Left | Side::Right => (w, h + 2.0 * RADIUS),
    };
    (w as u32, h as u32)
}

/// The card's corners on the popup.
pub fn card(side: Side) -> [Point; 2] {
    let (w, h) = CARD;
    match side {
        Side::Above | Side::Below => [Point::new(RADIUS, 0.0), Point::new(RADIUS + w, h)],
        Side::Left | Side::Right => [Point::new(0.0, RADIUS), Point::new(w, RADIUS + h)],
    }
}

/// In the panel's colour and without an outline, so that the card reads as
/// part of the panel.
pub fn draw(scene: &mut Scene, side: Side, t: &Theme) {
    let [p0, p1] = card(side);
    // Reaching past the edge against the panel, so that only the free
    // corners show rounded.
    let (mut q0, mut q1) = (p0, p1);
    match side {
        Side::Below => q0.y -= RADIUS,
        Side::Above => q1.y += RADIUS,
        Side::Right => q0.x -= RADIUS,
        Side::Left => q1.x += RADIUS,
    }
    scene.push(RoundedRect {
        p0: q0,
        p1: q1,
        r: RADIUS,
        color: t.surface,
    });
    // Where the card's sides leave the panel, so that the panel's edge
    // curves into them.
    let fillets = match side {
        Side::Below => [
            (Point::new(p0.x, 0.0), Corner::TopRight),
            (Point::new(p1.x, 0.0), Corner::TopLeft),
        ],
        Side::Above => [
            (Point::new(p0.x, p1.y), Corner::BottomRight),
            (Point::new(p1.x, p1.y), Corner::BottomLeft),
        ],
        Side::Right => [
            (Point::new(0.0, p0.y), Corner::BottomLeft),
            (Point::new(0.0, p1.y), Corner::TopLeft),
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
            r: RADIUS,
            color: t.surface,
        });
    }
}
