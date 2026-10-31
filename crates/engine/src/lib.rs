//! gineer-engine: the desktop side. A shell describes its surfaces and draws
//! scenes ([`App`]); the engine puts them on every monitor as layer
//! surfaces and draws them.

mod app;
mod wayland;

pub use app::{App, Layer, Output, OutputId, Paint, SurfaceId, SurfaceSpec};
pub use renderer;
pub use smithay_client_toolkit::shell::wlr_layer::Anchor;
pub use wayland::run;
