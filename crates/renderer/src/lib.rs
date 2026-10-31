//! The description of what to draw, kept free of Wayland.
//!
//! Nothing here needs a display, so everything that decides what a frame
//! looks like can be built and tested headless, and a GPU backend can later
//! gineer-render: drawing, and nothing of Wayland. A [`Scene`] of shapes in
//! logical pixels, drawn by the CPU ([`cpu`]) into pixels, or into a
//! [`snapshot::Picture`].

pub mod cpu;
pub mod scene;
pub mod snapshot;

pub use scene::{Circle, Point, Rect, Rgba, Scene, Shape, Side};
