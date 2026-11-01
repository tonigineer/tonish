use renderer::Scene;
use smithay_client_toolkit::shell::wlr_layer::Anchor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceSpec {
    /// Becomes the layer surface's namespace, behind a prefix the engine
    /// adds. Compositors match namespaces in rules such as sway's
    /// `layer_effects`, so renaming it breaks those rules in users' configs.
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
    /// `false` lets every click through to whatever is under the surface,
    /// as for figures shown over windows.
    pub input: bool,
}

/// One of the app's surfaces, for the events that happen on it and for
/// [`Redraw`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum On {
    Surface(OutputId, SurfaceId),
    Popup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    /// The evdev code, as wl_pointer sends it.
    Other(u32),
}

/// Keys arrive as evdev codes, without a keyboard layout, so that the engine
/// needs no xkbcommon. That covers the keys that are in the same place on
/// every layout, and only those are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
}

/// A layer surface of its own over the monitor rather than an xdg_popup,
/// which the compositor unmaps the moment a click lands outside it, so that
/// it can animate as it closes. The engine gives it an invisible xdg_popup
/// for the compositor's grab. The app places it, so it ends up exactly where
/// it was asked to be, even off the monitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PopupSpec {
    /// The surface whose button press opened it, which the grab hangs from.
    pub parent: (OutputId, SurfaceId),
    /// Logical pixels on the monitor, from its top left corner, regardless of
    /// the room other surfaces keep.
    pub at: (i32, i32),
    /// Logical pixels: room for everything it shows while it moves, so that
    /// it is never resized during an animation, which would make it flicker.
    pub size: (u32, u32),
    /// While `true`, it holds the grab: it gets the keyboard, and a click
    /// outside every surface of the app reaches [`App::popup_dismissed`]
    /// instead of the window under it. A grab can only start in the batch of
    /// a button press, so turning it on later does nothing.
    pub grab: bool,
    /// The part that takes the pointer, as x, y, width and height in logical
    /// pixels on the popup. Clicks elsewhere go to whatever is under it.
    pub input: Option<(i32, i32, i32, i32)>,
}

/// Where something is from what it opens out of: a panel below a bar along
/// the top, to the right of one down the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Above,
    Below,
    Left,
    Right,
}

/// How a frame of a surface went, for an app that shows such figures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    /// Building the scene and drawing it, up to handing it to the
    /// compositor.
    pub took: Duration,
    /// How long it waited for the GPU's thread, busy with other frames.
    pub waited: Duration,
    /// Drawn by the GPU's shader, rather than by the CPU rasteriser.
    pub gpu: bool,
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
    /// When the engine started drawing the frame, which animations step
    /// their springs to. Passed in rather than read by the app, so that a
    /// shell's animations can be tested at any time.
    pub time: Instant,
}

/// Keeps the shell free of Wayland: it only says which surfaces it wants
/// and what is on them, so all of it can be tested without a compositor.
pub trait App {
    /// Called when the output appears, and for every output again after
    /// [`App::changed`] returns `true`. Returning no surfaces leaves the
    /// monitor without any.
    fn surfaces(&mut self, output: &Output) -> Vec<SurfaceSpec>;

    /// Called when the compositor configures a surface with a new size, for
    /// every surface [`App::to_redraw`] names, and for every surface once
    /// more when the GPU takes over from the CPU. The scene starts empty
    /// each time, so `draw` paints the whole surface, not what changed.
    fn draw(&mut self, output: OutputId, surface: SurfaceId, paint: &mut Paint);

    /// Asked after every batch of events, whatever they were, so that any of
    /// the app's methods can lead to a redraw: the app keeps what changed
    /// until it is asked, and hands it over by taking it. A surface named
    /// several times in one batch is drawn once.
    fn to_redraw(&mut self) -> Redraw {
        Redraw::Nothing
    }

    /// Asked once, before the first output appears, so the set of files is
    /// fixed for the whole run. The engine watches their folders rather than
    /// the files, because editors save by writing a new file and renaming it
    /// over the old one, and a watch on the old file would go quiet.
    fn watched(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// `true` makes the engine ask [`App::surfaces`] again. A surface that
    /// keeps its name and layer is changed in place and drawn again at the
    /// size the compositor gives it, without a blink. Any other is replaced,
    /// because a layer surface's namespace can never change and its layer
    /// only with a newer protocol version than the engine uses, and a
    /// replaced surface blinks. A change that leaves every surface's spec as
    /// it was still needs [`App::to_redraw`] to show.
    fn changed(&mut self, _path: &Path) -> bool {
        false
    }

    /// Asked once, before anything else, because the socket also keeps a
    /// second instance in the same session from starting. `None` runs
    /// without one.
    fn socket(&self) -> Option<PathBuf> {
        None
    }

    /// A pointer button pressed on one of the app's surfaces, at `at` in
    /// logical pixels on it.
    fn pressed(&mut self, _on: On, _at: (f32, f32), _button: Button) {}

    /// Only a popup's grab gets the keyboard, so keys arrive only while one
    /// holds it.
    fn key(&mut self, _key: Key) {}

    /// Asked after every batch of events, like [`App::to_redraw`]. A popup
    /// opens when the answer changes to `Some`, changes with it, and closes
    /// when it is `None`. A click on another of the app's surfaces while it
    /// holds the grab arrives as [`App::pressed`].
    fn popup(&self) -> Option<PopupSpec> {
        None
    }

    /// A click outside every surface of the app ended the popup's grab. The
    /// popup stays until [`App::popup`] answers `None`, so that it can
    /// animate away.
    fn popup_dismissed(&mut self) {}

    /// The engine closed the popup, as when its monitor went away. It does
    /// not open again until [`App::popup`] answers something else.
    fn popup_closed(&mut self) {}

    /// Asked after every draw of a surface: whether it moves, so that the
    /// engine draws it again when the compositor is ready for its next
    /// frame. A surface the compositor doesn't show gets no frames, so its
    /// animation waits until it is shown again.
    fn animating(&self, _on: On) -> bool {
        false
    }

    /// The most frames a second the surface is drawn while it animates.
    /// `None` takes every frame the compositor offers, at the monitor's
    /// refresh rate.
    fn max_fps(&self, _on: On) -> Option<f32> {
        None
    }

    /// Asked after every batch of events, like [`App::to_redraw`]: `true`
    /// makes the engine ask [`App::surfaces`] again for every monitor, as
    /// after [`App::changed`], so that a surface can come or go on a
    /// message. Unlike a change of the settings, it leaves an open popup
    /// open, unless the surface it hangs from is replaced.
    fn to_reshape(&mut self) -> bool {
        false
    }

    /// A frame was drawn. A frame for the GPU is reported once the GPU's
    /// thread has drawn it, in a later batch of events.
    fn drew(&mut self, _on: On, _drawn: Drawn) {}

    /// The GPU has taken over drawing, on this device as OpenGL names it.
    fn gpu_started(&mut self, _device: &str) {}

    /// [`App::draw`] for the popup.
    fn draw_popup(&mut self, _paint: &mut Paint) {}

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

/// `Everything` is a variant of its own so that an app need not list its
/// surfaces to draw them all.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Redraw {
    #[default]
    Nothing,
    Everything,
    /// Surfaces that are gone by the time the engine asks, or that the
    /// compositor has not configured yet, are skipped; a configure draws them
    /// anyway.
    Surfaces(Vec<On>),
}

/// `quit` is part of the reply rather than a call the app makes, because the
/// app has no hold on the event loop, which is what keeps it testable.
pub struct Reply {
    /// One line, like the message. No length goes with it, because clients
    /// read until the connection closes.
    pub text: String,
    pub quit: bool,
}
