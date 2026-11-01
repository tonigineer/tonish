//! The settings panel, which a right-click on the bar opens, hanging from
//! it.

use engine::Side;
use renderer::{Point, Scene};

use crate::config::Config;
use crate::hang::{self, Edges};
use crate::theme::Theme;

/// Logical pixels.
pub const CARD: (f32, f32) = (280.0, 180.0);

/// Room at each end of the popup, along the bar, for the concave corners.
pub fn room(side: Side, c: &Config) -> f32 {
    edges(side, c).inverse_radius
}

pub fn edges(side: Side, c: &Config) -> Edges {
    let across = match side {
        Side::Above | Side::Below => CARD.1,
        Side::Left | Side::Right => CARD.0,
    };
    Edges {
        radii: c.panel.radius,
        inverse_radius: c.panel.inverse_radius,
        border: c.border,
    }
    .fit(across, side)
}

/// The card, the room for its concave corners, and `overlap`: how far the
/// popup reaches over the bar's border, which the card covers.
pub fn size(side: Side, c: &Config, overlap: f32) -> (u32, u32) {
    let (w, h) = CARD;
    let room = 2.0 * room(side, c);
    let (w, h) = match side {
        Side::Above | Side::Below => (w + room, h + overlap),
        Side::Left | Side::Right => (w + overlap, h + room),
    };
    (w.ceil() as u32, h.ceil() as u32)
}

/// The card's corners on the popup. It meets the bar at the bar's outer
/// edge, `overlap` in, and its fill covers the bar's line between its sides,
/// so that no line runs between the two.
pub fn card(side: Side, c: &Config, overlap: f32) -> [Point; 2] {
    let (w, h) = CARD;
    let room = room(side, c);
    match side {
        Side::Below => [Point::new(room, overlap), Point::new(room + w, overlap + h)],
        Side::Above => [Point::new(room, 0.0), Point::new(room + w, h)],
        Side::Right => [Point::new(overlap, room), Point::new(overlap + w, room + h)],
        Side::Left => [Point::new(0.0, room), Point::new(w, room + h)],
    }
}

/// In the bar's colour, and always hanging from it, with sharp concave
/// corners at an inverse radius of 0, so that the card reads as part of the
/// bar.
pub fn draw(scene: &mut Scene, side: Side, c: &Config, overlap: f32, t: &Theme) {
    hang::draw(
        scene,
        card(side, c, overlap),
        side,
        true,
        edges(side, c),
        overlap,
        t.surface,
        t.outline,
    );
}
