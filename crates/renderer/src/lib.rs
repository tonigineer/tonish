//! Kept free of Wayland, so that everything deciding what a frame looks like
//! can be built and tested without a display.

pub mod cpu;
pub mod scene;
pub mod snapshot;

pub use scene::{Circle, Orientation, Point, Rect, Rgba, Scene, Shape};
