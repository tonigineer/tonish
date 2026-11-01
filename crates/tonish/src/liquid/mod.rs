//! Cards of liquid that hang from the bar's edge: one grows out of the bar
//! like a drop, can be pulled off it on a neck shaped as a soap film, left
//! anywhere and docked again, and folds back into the bar, as pleama's cards
//! do. The settings panel is one; any panel that opens from the bar can be
//! another, with a size of its own.
//!
//! [`Frame`] is where the bar's edge runs, [`Card`] is a card's life on
//! springs and under the pointer, [`Pose`] is where it is at one moment,
//! and [`draw()`] turns a pose into shapes, with the [`Look`] the settings
//! give it.

mod card;
mod draw;
mod frame;
mod pose;

pub use card::Card;
pub use draw::{Look, draw};
pub use frame::Frame;
pub use pose::Pose;
