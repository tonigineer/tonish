//! Everything Wayland lives here, so that a shell only describes its
//! surfaces and draws scenes through [`App`], and can be tested without a
//! compositor.

mod app;
mod wayland;

pub use app::{App, Layer, Output, OutputId, Paint, SurfaceId, SurfaceSpec};
pub use renderer;
pub use smithay_client_toolkit::shell::wlr_layer::Anchor;
pub use wayland::run;
