//! Kept free of Wayland, so that everything deciding what a frame looks like
//! can be built and tested without a display.

pub mod cpu;
pub mod gpu;
pub mod scene;
pub mod shapes;
pub mod snapshot;

pub use scene::{Point, Rgba, Scene};
pub use shapes::{Circle, Corner, Fillet, Rect, RoundedRect, Shape};
