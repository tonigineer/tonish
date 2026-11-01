//! The settings panel, which a right-click on a panel opens next to the
//! pointer.

use engine::Side;
use renderer::{Point, Rect, Scene, Shape};

use crate::theme::Theme;

/// Logical pixels.
pub const SIZE: (u32, u32) = (280, 180);

/// `side` is where the panel sits from the panel it hangs from.
pub fn draw(scene: &mut Scene, size: (f32, f32), side: Side, t: &Theme) {
    let card = Rect {
        p0: Point::new(0.0, 0.0),
        p1: Point::new(size.0, size.1),
        color: t.surface,
    };
    scene.push(card);
    // Inset by half its width, because an outline is centred on the edge
    // and the half outside the surface would be cut off. On the side
    // against the panel it goes outside altogether, so that no line
    // separates the card from the panel it hangs from.
    let edge = 1.0;
    let (mut p0, mut p1) = (
        Point::new(edge / 2.0, edge / 2.0),
        Point::new(size.0 - edge / 2.0, size.1 - edge / 2.0),
    );
    match side {
        Side::Below => p0.y = -edge,
        Side::Above => p1.y = size.1 + edge,
        Side::Right => p0.x = -edge,
        Side::Left => p1.x = size.0 + edge,
    }
    scene.push(Rect { p0, p1, ..card }.outline(edge, t.outline));
}
