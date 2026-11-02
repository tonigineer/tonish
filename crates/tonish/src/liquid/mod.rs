//! Cards of liquid that hang from an edge: one grows out of the bar like a
//! drop, can be pulled off it on a neck shaped as a soap film, left anywhere
//! and docked again, to the bar or to any other straight edge it is given,
//! such as the frame's, and folds back into the edge it hangs from, as
//! pleama's cards do. The settings panel is one; any panel that opens from
//! the bar can be another, with a size of its own.
//!
//! [`Edge`] is where an edge runs, and a [`Dock`] where along it a card may
//! hang. [`Card`] is a card's life on springs and under the pointer, [`Pose`]
//! is where it is at one moment, and [`draw()`] turns a pose into shapes,
//! with the [`Look`] the settings give it.

mod card;
mod draw;
mod edge;
mod pose;

pub use card::Card;
pub use draw::{Look, draw};
pub use edge::{Dock, Edge};
pub use pose::Pose;
