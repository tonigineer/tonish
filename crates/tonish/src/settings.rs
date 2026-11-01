//! The settings panel, which a right-click on a panel opens, hanging from
//! it.

use engine::Side;
use renderer::{Point, Scene};

use crate::hang;
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
    hang::draw(scene, card(side), side, RADIUS, t.surface);
}
