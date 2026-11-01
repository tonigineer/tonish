//! The settings panel, which a right-click on a panel opens next to the
//! pointer.

use renderer::{Point, Rect, Scene, Shape};

use crate::theme::Theme;

/// Logical pixels.
pub const SIZE: (u32, u32) = (280, 180);

pub fn draw(scene: &mut Scene, size: (f32, f32), t: &Theme) {
    let card = Rect {
        p0: Point::new(0.0, 0.0),
        p1: Point::new(size.0, size.1),
        color: t.surface,
    };
    scene.push(card);
    // Inset by half its width, because an outline is centred on the edge
    // and the half outside the surface would be cut off.
    let edge = 1.0;
    let inner = Rect {
        p0: Point::new(edge / 2.0, edge / 2.0),
        p1: Point::new(size.0 - edge / 2.0, size.1 - edge / 2.0),
        ..card
    };
    scene.push(inner.outline(edge, t.outline));
}
