//! Everything Wayland lives here, so that a shell only describes its
//! surfaces and draws scenes through [`App`], and can be tested without a
//! compositor.

mod app;
mod gles;
mod socket;
mod wayland;

pub use app::{
    App, Button, Key, Layer, On, Output, OutputId, Paint, PopupSpec, Redraw, Reply, Side,
    SurfaceId, SurfaceSpec,
};
// Re-exported, so that a shell builds its surfaces and scenes with exactly
// the versions the engine was built against.
pub use renderer;
pub use smithay_client_toolkit::shell::wlr_layer::Anchor;
pub use wayland::{Gpu, run};
