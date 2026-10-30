//! The description of what to draw, kept free of Wayland.
//!
//! Nothing here needs a display, so everything that decides what a frame
//! looks like can be built and tested headless, and a GPU backend can later
//! draw the same [`Scene`] without the shell changing.

pub mod scene;

pub use scene::{Circle, Point, Rect, Rgba, Scene, Shape, Side};
