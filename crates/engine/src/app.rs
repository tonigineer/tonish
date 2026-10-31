use renderer::Scene;
use smithay_client_toolkit::shell::wlr_layer::Anchor;
use std::path::{Path, PathBuf};

/// The wl_output's global name, so that a shell can key per-monitor state on
/// it for as long as the monitor stays connected. A monitor plugged in again
/// gets a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputId(pub u32);

/// The index of the surface in the list [`App::surfaces`] returned for its
/// output. A shell finds what to draw by indexing its own list the same way,
/// so the order of that list must not change while the output exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub usize);

/// Taken when the monitor appears and not updated yet, so a later change of
/// mode or scale leaves it stale until tonish restarts.
#[derive(Clone, Debug)]
pub struct Output {
    pub id: OutputId,
    /// `None` on compositors without wl_output version 4, so a shell needs a
    /// screen that applies to any monitor.
    pub name: Option<String>,
    /// Logical pixels, from xdg-output. A compositor that does not send it
    /// gets a guessed 1920×1080, which is why shells should anchor to edges
    /// instead of positioning by this size.
    pub size: (f32, f32),
    /// An integer, because without the fractional-scale protocol the
    /// compositor rounds a scale such as 1.5 up to 2 and shrinks the buffer
    /// itself.
    pub scale: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Mirrors the wlr-layer-shell requests one to one, so that the protocol's
/// documentation applies field by field.
#[derive(Clone, Debug)]
pub struct SurfaceSpec {
    /// Becomes the layer surface's namespace, which compositors match in
    /// rules such as sway's `layer_effects`, so renaming it breaks those
    /// rules in users' configs.
    pub name: &'static str,
    pub layer: Layer,
    pub anchor: Anchor,
    /// 0 along an axis leaves that length to the compositor. The protocol
    /// only allows that when the surface is anchored to both edges of that
    /// axis; otherwise the compositor ends the connection.
    pub size: (u32, u32),
    pub exclusive: i32,
    /// In the protocol's order: top, right, bottom, left.
    pub margin: (i32, i32, i32, i32),
}

/// Lends the engine's one scene, cleared before every draw, so that its
/// allocation is reused across frames and surfaces instead of every draw
/// allocating its own.
pub struct Paint<'a> {
    pub scene: &'a mut Scene,
    /// Logical pixels, as the compositor configured the surface, so that a
    /// shell lays out the same way at every scale.
    pub size: (f32, f32),
    /// `f32`, although outputs only report whole scales so far, so that
    /// fractional scaling can come without changing every shell.
    pub scale: f32,
}

/// Keeps the shell free of Wayland: it only says which surfaces it wants
/// and what is on them, so all of it can be tested without a compositor.
pub trait App {
    /// Called once, when the output appears. Returning no surfaces leaves the
    /// monitor without any.
    fn surfaces(&mut self, output: &Output) -> Vec<SurfaceSpec>;

    /// Called only when the compositor configures a surface with a new size.
    /// There is no way yet for a shell to ask for a redraw, so anything that
    /// changes on its own, such as a clock, needs that first.
    fn draw(&mut self, output: OutputId, surface: SurfaceId, paint: &mut Paint);

    /// Asked once, before the first output appears, so the set of files is
    /// fixed for the whole run. The engine watches their folders rather than
    /// the files, because editors save by writing a new file and renaming it
    /// over the old one, and a watch on the old file would go quiet.
    fn watched(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// `true` makes the engine destroy every surface and ask
    /// [`App::surfaces`] again. Rebuilding is the one way to apply any
    /// change, since a layer surface's namespace can never change and its
    /// layer only with a newer protocol version than the engine uses. A save
    /// that changes nothing should return `false`, because a rebuild makes
    /// the panels blink.
    fn changed(&mut self, _path: &Path) -> bool {
        false
    }

    /// Asked once, before anything else, because the socket also keeps a
    /// second instance in the same session from starting. `None` runs
    /// without one.
    fn socket(&self) -> Option<PathBuf> {
        None
    }

    /// One line from a client, already trimmed. The engine owns the socket
    /// and the app the meaning of the messages, so that the protocol can grow
    /// without touching Wayland code.
    fn message(&mut self, _text: &str) -> Reply {
        Reply {
            text: "error: no messages are understood".into(),
            quit: false,
        }
    }
}

/// `quit` is part of the reply rather than a call the app makes, because the
/// app has no hold on the event loop, which is what keeps it testable.
pub struct Reply {
    /// One line. Clients read up to the end of the connection, so a reply
    /// that is cut short only happens when the shell dies.
    pub text: String,
    pub quit: bool,
}
