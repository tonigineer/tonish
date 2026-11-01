use std::fmt;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use calloop::channel::{self, Event};
use calloop::signals::{Signal, Signals};
use calloop::timer::{TimeoutAction, Timer};
use inotify::{Inotify, WatchDescriptor, WatchMask};
use renderer::Scene;
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::{Interest, Mode, PostAction, RegistrationToken};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::calloop::{EventLoop, LoopHandle},
    reexports::calloop_wayland_source::WaylandSource,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{SeatState, pointer::cursor_shape::CursorShapeManager},
    shell::xdg::XdgShell,
    shell::{
        WaylandSurface,
        wlr_layer::{
            KeyboardInteractivity, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use tracing::{debug, error, info, warn};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface},
};

use crate::app::{
    App, Drawn, Layer, On, Output, OutputId, Paint, PopupSpec, Redraw, SurfaceId, SurfaceSpec,
};
use crate::gles;
use crate::socket::{self, Socket};

mod input;
mod popup;

/// Whether surfaces are drawn with OpenGL ES once it has started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Gpu {
    /// The GPU starts in the background after the first frame, which the
    /// CPU draws, and every surface moves to it once it is ready, so that
    /// the panels appear at once and nothing waits for the GPU.
    #[default]
    Enabled,
    /// Every surface on the CPU, for the memory: measured with NVIDIA's
    /// driver, the GPU adds about 100 MB to tonish even while idle.
    Disabled,
}

/// How the engine runs, as the command line sets it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub gpu: Gpu,
    /// Every surface is drawn again as soon as it has been drawn, without
    /// waiting for the compositor's next frame, unless [`App::max_fps`] caps
    /// it: to see how many frames a second tonish reaches.
    pub draw_always: bool,
}

/// Returns `Ok` after a quit message, SIGINT (Ctrl+C) or SIGTERM, once the
/// loop has finished what it was doing. The errors are strings, because the
/// only thing a caller does with them is print them.
pub fn run(app: Box<dyn App>, options: Options) -> Result<(), String> {
    let Options { gpu, draw_always } = options;
    // Before connecting to Wayland, so that a second instance stops before
    // it has made a single surface.
    let listening = app.socket().map(|path| socket::listen(&path)).transpose()?;
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland display: {e}"))?;
    let (globals, queue) = registry_queue_init(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<Engine> = EventLoop::try_new().map_err(|e| e.to_string())?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| e.to_string())?;

    // A signalfd on the loop instead of a handler, so that a signal is
    // handled between events like any other input and never interrupts the
    // engine halfway through a frame.
    let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM]).map_err(|e| e.to_string())?;
    event_loop
        .handle()
        .insert_source(signals, |event, _, engine| {
            info!(signal = ?event.signal(), "shutting down");
            engine.quit = true;
        })
        .map_err(|e| e.error.to_string())?;

    // Declared after the event loop, so that it is dropped first: the file is
    // gone before the clients' connections close with the loop, and a client
    // waiting for tonish to end can start a new one at once.
    let _socket: Option<Socket> = match listening {
        Some((listener, socket)) => {
            let clients = event_loop.handle();
            event_loop
                .handle()
                .insert_source(
                    Generic::new(listener, Interest::READ, Mode::Level),
                    move |_, listener, _| {
                        while let Ok((stream, _)) = listener.accept() {
                            let _ = stream.set_nonblocking(true);
                            let mut line = Vec::new();
                            let _ = clients.insert_source(
                                Generic::new(stream, Interest::READ, Mode::Level),
                                move |_, stream, engine| Ok(engine.client(stream, &mut line)),
                            );
                        }
                        Ok(PostAction::Continue)
                    },
                )
                .map_err(|e| e.error.to_string())?;
            Some(socket)
        }
        None => None,
    };

    let shm = Shm::bind(&globals, &qh).map_err(|_| "no wl_shm")?;
    // Watching is a convenience, so failing to set it up only costs the
    // reloading, not the shell.
    let watch = match Watch::new(app.watched()) {
        Ok(watch) => watch,
        Err(e) => {
            warn!("cannot watch for changes, so edits apply after a restart: {e}");
            None
        }
    };

    if let Some(watch) = &watch {
        // A second descriptor for polling, so that the `Inotify` itself stays
        // in the engine, where reading it needs no unsafe access through the
        // event source.
        let fd = watch
            .inotify
            .as_fd()
            .try_clone_to_owned()
            .map_err(|e| e.to_string())?;
        event_loop
            .handle()
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                |_, _, engine| {
                    engine.files_changed();
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| e.error.to_string())?;
    }

    let mut engine = Engine {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        compositor: CompositorState::bind(&globals, &qh).map_err(|_| "no wl_compositor")?,
        layer_shell: LayerShell::bind(&globals, &qh)
            .map_err(|_| "no layer shell (wlr-layer-shell)")?,
        pool: SlotPool::new(1 << 16, &shm).map_err(|e| e.to_string())?,
        shm,
        qh: qh.clone(),
        conn: conn.clone(),
        handle: event_loop.handle(),
        seat_state: SeatState::new(&globals, &qh),
        xdg_shell: XdgShell::bind(&globals, &qh).ok(),
        cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        pointers: Vec::new(),
        keyboards: Vec::new(),
        pressed: None,
        grab: None,
        popup: None,
        wanted: None,
        outs: Vec::new(),
        app,
        scene: Scene::new(),
        gpu,
        draw_always,
        gles: GlesState::NotStarted,
        watch,
        quit: false,
    };

    while !engine.quit {
        event_loop
            .dispatch(None, &mut engine)
            .map_err(|e| e.to_string())?;
        if engine.app.to_reshape() {
            engine.reshape(false);
        }
        engine.update_popup();
        engine.redraw();
    }
    // Destroys the surfaces and sends that, so the panels are gone before a
    // client waiting for the end hears about it.
    drop(engine);
    let _ = conn.flush();
    Ok(())
}

struct Surface {
    /// Before `role`, so that it is dropped first: EGL draws into the
    /// wl_surface until its window is gone.
    window: Option<gles::Window>,
    role: Role,
    /// Logical pixels: the buffer is this times the output's scale.
    size: (u32, u32),
    configured: bool,
    /// A frame callback is on its way, so that an animation asks for one
    /// per frame rather than one per draw.
    frame_pending: bool,
    /// When its last frame was drawn, which [`App::max_fps`] counts from.
    drawn_at: Option<Instant>,
    /// The timer that draws its next frame once [`App::max_fps`] allows it.
    /// Any draw cancels it, so that a surface drawn for another reason, such
    /// as the GPU taking over, goes on with one chain of frames and not two.
    timer: Option<RegistrationToken>,
}

enum Role {
    Layer {
        layer: LayerSurface,
        spec: SurfaceSpec,
    },
    Popup {
        layer: LayerSurface,
        spec: PopupSpec,
    },
}

impl Surface {
    /// Whether the surface can take `spec` in place: a layer surface's
    /// namespace is fixed, and so is its layer at the protocol version the
    /// engine uses.
    fn keeps(&self, spec: &SurfaceSpec) -> bool {
        match &self.role {
            Role::Layer { spec: old, .. } => old.name == spec.name && old.layer == spec.layer,
            Role::Popup { .. } => false,
        }
    }
}

impl Role {
    fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Role::Layer { layer, .. } => layer.wl_surface(),
            Role::Popup { layer, .. } => layer.wl_surface(),
        }
    }
}

/// A surface by its indices into the outputs and their surfaces, or the
/// popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    Layer(usize, usize),
    Popup,
}

/// For the log, where the output is a field of its own.
impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Target::Layer(_, si) => write!(f, "{si}"),
            Target::Popup => f.write_str("popup"),
        }
    }
}

/// A free function rather than a method, so that it borrows only these two
/// fields and leaves the rest of the engine free.
fn surface<'a>(
    outs: &'a mut [Out],
    popup: &'a mut Option<Surface>,
    t: Target,
) -> Option<&'a mut Surface> {
    match t {
        Target::Layer(oi, si) => outs.get_mut(oi)?.surfaces.get_mut(si),
        Target::Popup => popup.as_mut(),
    }
}

struct Out {
    info: Output,
    wl: wl_output::WlOutput,
    surfaces: Vec<Surface>,
}

struct Watch {
    inotify: Inotify,
    files: Vec<(WatchDescriptor, PathBuf)>,
}

impl Watch {
    fn new(paths: Vec<PathBuf>) -> std::io::Result<Option<Self>> {
        if paths.is_empty() {
            return Ok(None);
        }
        let inotify = Inotify::init()?;
        let mut files = Vec::new();
        for path in paths {
            let dir = match path.parent() {
                Some(dir) if !dir.as_os_str().is_empty() => dir,
                _ => Path::new("."),
            };
            // CLOSE_WRITE rather than MODIFY, so that a file is read once it
            // is complete and not halfway through a write; MOVED_TO for
            // editors that rename a new file over the old one.
            let wd = inotify
                .watches()
                .add(dir, WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO)?;
            files.push((wd, path));
        }
        Ok(Some(Self { inotify, files }))
    }
}

struct Engine {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    qh: QueueHandle<Engine>,
    /// For EGL, which works on the connection's wl_display.
    conn: Connection,
    /// For the source that hears from the GPU's thread, added only when the
    /// thread starts.
    handle: LoopHandle<'static, Engine>,
    seat_state: SeatState,
    /// `None` on a compositor without xdg-shell, where no popup opens.
    xdg_shell: Option<XdgShell>,
    /// `None` on a compositor without cursor-shape-v1, which then shows
    /// whatever cursor it likes over the surfaces.
    cursor_shapes: Option<CursorShapeManager>,
    /// `None` on a compositor without wp_viewporter, where the grab's
    /// transparent surface takes a buffer as large as itself.
    viewporter: Option<popup::WpViewporter>,
    pointers: Vec<input::Pointer>,
    keyboards: Vec<(wl_seat::WlSeat, wl_keyboard::WlKeyboard)>,
    /// The last button press, which a popup's grab has to name.
    pressed: Option<(wl_seat::WlSeat, u32)>,
    /// The popup's invisible xdg_popup that holds the grab. Before `outs`, so
    /// that it is dropped first: an xdg_popup has to go before the surface it
    /// hangs from.
    grab: Option<popup::Grab>,
    popup: Option<Surface>,
    /// [`App::popup`]'s answer when last asked, so that a popup opens only
    /// when the answer changes.
    wanted: Option<PopupSpec>,
    outs: Vec<Out>,
    app: Box<dyn App>,
    scene: Scene,
    gpu: Gpu,
    draw_always: bool,
    gles: GlesState,
    watch: Option<Watch>,
    quit: bool,
}

enum GlesState {
    NotStarted,
    /// While the CPU goes on drawing every surface.
    Starting(gles::Thread),
    /// Draws every surface.
    Running(gles::Thread),
    /// It failed to start or to draw, and is not tried again: a driver
    /// that failed once is likely to fail again.
    Failed,
}

impl Engine {
    /// The client asking to quit keeps its connection, which closes when the
    /// process ends: its read then returns once the shell is really gone.
    fn client(&mut self, stream: &UnixStream, line: &mut Vec<u8>) -> PostAction {
        let Some(text) = socket::read_line(stream, line) else {
            return PostAction::Continue;
        };
        let reply = self.app.message(&text);
        debug!(msg = %text, reply = %reply.text, "message");
        socket::reply(stream, &reply.text);
        if reply.quit {
            info!(msg = %text, "shutting down");
            self.quit = true;
            PostAction::Disable
        } else {
            PostAction::Remove
        }
    }

    fn add_output(&mut self, wl: wl_output::WlOutput) {
        let Some(info) = self.output_state.info(&wl) else {
            return;
        };
        let out = Output {
            id: OutputId(info.id),
            name: info.name.clone(),
            size: info
                .logical_size
                .map_or((1920.0, 1080.0), |(w, h)| (w as f32, h as f32)),
            // sctk starts at 1, but a compositor sending 0 would make every
            // buffer zero-sized, which is a protocol error.
            scale: info.scale_factor.max(1),
        };
        let specs = self.app.surfaces(&out);
        info!(
            monitor = %name(&out),
            size = ?out.size,
            scale = out.scale,
            surfaces = specs.len(),
            "monitor added"
        );
        let surfaces = specs
            .into_iter()
            .map(|spec| self.layer(&wl, spec))
            .collect();
        self.outs.push(Out {
            info: out,
            wl,
            surfaces,
        });
    }

    /// Committed without a buffer, because wlr-layer-shell sends the first
    /// `configure`, which carries the size to draw at, only after such a
    /// commit.
    fn layer(&self, wl: &wl_output::WlOutput, spec: SurfaceSpec) -> Surface {
        use smithay_client_toolkit::shell::wlr_layer::Layer as L;
        let level = match spec.layer {
            Layer::Background => L::Background,
            Layer::Bottom => L::Bottom,
            Layer::Top => L::Top,
            Layer::Overlay => L::Overlay,
        };
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            level,
            Some(format!("tonish-{}", spec.name)),
            Some(wl),
        );
        layer.set_anchor(spec.anchor);
        layer.set_size(spec.size.0, spec.size.1);
        layer.set_exclusive_zone(spec.exclusive);
        let m = spec.margin;
        layer.set_margin(m.0, m.1, m.2, m.3);
        // Panels take no keys yet, and any other mode lets them take the
        // keyboard focus away from windows.
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        self.input_region(layer.wl_surface(), spec.input);
        layer.commit();
        debug!(name = %spec.name, "surface made");
        Surface {
            window: None,
            role: Role::Layer { layer, spec },
            size: (0, 0),
            configured: false,
            frame_pending: false,
            drawn_at: None,
            timer: None,
        }
    }

    /// Collects every event before telling the app, so that one save that
    /// arrives as several events reaches it once.
    fn files_changed(&mut self) {
        let Some(watch) = &mut self.watch else {
            return;
        };
        let mut changed: Vec<PathBuf> = Vec::new();
        let mut buffer = [0; 4096];
        while let Ok(events) = watch.inotify.read_events(&mut buffer) {
            let mut any = false;
            for event in events {
                any = true;
                let hit = watch.files.iter().find(|(wd, path)| {
                    *wd == event.wd && event.name.is_some() && event.name == path.file_name()
                });
                if let Some((_, path)) = hit
                    && !changed.contains(path)
                {
                    debug!(file = %path.display(), "changed");
                    changed.push(path.clone());
                }
            }
            if !any {
                break;
            }
        }
        let mut reshape = false;
        for path in &changed {
            reshape |= self.app.changed(path);
        }
        if reshape {
            self.reshape(true);
        }
    }

    /// The whole surface, as by default, or nothing, which lets every click
    /// through.
    fn input_region(&self, ws: &wl_surface::WlSurface, input: bool) {
        if input {
            ws.set_input_region(None);
        } else if let Ok(empty) = smithay_client_toolkit::compositor::Region::new(&self.compositor)
        {
            ws.set_input_region(Some(empty.wl_region()));
        }
    }

    /// Matches the app's new surfaces to the old ones by their place in the
    /// list, as [`SurfaceId`] does. Surfaces that are gone are destroyed as
    /// they are dropped, and new ones are drawn once the compositor
    /// configures them, as on a first start. A popup goes along when
    /// `settings` say the settings changed, since its place comes from the
    /// old shape of what it opened from, and otherwise only when the
    /// surface its grab hangs from is replaced.
    fn reshape(&mut self, settings: bool) {
        if settings {
            self.lose_popup();
        }
        let parent = self.wanted.as_ref().map(|p| p.parent);
        for oi in 0..self.outs.len() {
            let wl = self.outs[oi].wl.clone();
            let id = self.outs[oi].info.id;
            let specs = self.app.surfaces(&self.outs[oi].info);
            let kept = specs.len();
            let mut old = std::mem::take(&mut self.outs[oi].surfaces).into_iter();
            let mut surfaces = Vec::with_capacity(specs.len());
            for (si, spec) in specs.into_iter().enumerate() {
                let surf = match old.next() {
                    Some(surf) if surf.keeps(&spec) => self.reshape_one(surf, spec),
                    _ => {
                        if parent == Some((id, SurfaceId(si))) {
                            self.lose_popup();
                        }
                        self.layer(&wl, spec)
                    }
                };
                surfaces.push(surf);
            }
            if parent.is_some_and(|(o, SurfaceId(si))| o == id && si >= kept) {
                self.lose_popup();
            }
            self.outs[oi].surfaces = surfaces;
        }
    }

    /// The new state applies with a commit, which the compositor answers with
    /// a configure, and the configure draws the surface at its new size.
    fn reshape_one(&self, mut surf: Surface, spec: SurfaceSpec) -> Surface {
        let Role::Layer { layer, spec: old } = &mut surf.role else {
            return surf;
        };
        if *old == spec {
            return surf;
        }
        layer.set_anchor(spec.anchor);
        layer.set_size(spec.size.0, spec.size.1);
        layer.set_exclusive_zone(spec.exclusive);
        let m = spec.margin;
        layer.set_margin(m.0, m.1, m.2, m.3);
        self.input_region(layer.wl_surface(), spec.input);
        debug!(name = %spec.name, "surface reshaped");
        // A surface that changes size waits for the configure with its new
        // size: the app's redraw, which comes first, would draw the new shape
        // cut to the old size, for a frame and for nothing. One that keeps its
        // size is drawn with the redraw, since compositors such as sway send
        // no configure for a size that stays.
        if resized(surf.size, spec.size) {
            surf.configured = false;
        }
        *old = spec;
        match &surf.window {
            Some(window) => window.commit(layer.wl_surface().clone()),
            None => layer.commit(),
        }
        surf
    }

    fn redraw(&mut self) {
        let redraw = self.app.to_redraw();
        if redraw == Redraw::Nothing {
            return;
        }
        for t in self.targets(redraw) {
            self.draw(t);
        }
    }

    fn targets(&self, redraw: Redraw) -> Vec<Target> {
        let outs: Vec<_> = self
            .outs
            .iter()
            .map(|o| (o.info.id, o.surfaces.len()))
            .collect();
        targets(&outs, redraw)
    }

    fn is_popup_layer(&self, layer: &LayerSurface) -> bool {
        self.popup
            .as_ref()
            .is_some_and(|p| matches!(&p.role, Role::Popup { layer: l, .. } if l == layer))
    }

    fn on_of(&self, t: Target) -> On {
        match t {
            Target::Layer(oi, si) => On::Surface(self.outs[oi].info.id, SurfaceId(si)),
            Target::Popup => On::Popup,
        }
    }

    fn target_of(&self, ws: &wl_surface::WlSurface) -> Option<Target> {
        if self
            .popup
            .as_ref()
            .is_some_and(|p| p.role.wl_surface() == ws)
        {
            return Some(Target::Popup);
        }
        self.outs.iter().enumerate().find_map(|(oi, o)| {
            let si = o.surfaces.iter().position(|s| s.role.wl_surface() == ws)?;
            Some(Target::Layer(oi, si))
        })
    }

    fn find_layer(&self, layer: &LayerSurface) -> Option<(usize, usize)> {
        self.outs.iter().enumerate().find_map(|(oi, o)| {
            let si = o
                .surfaces
                .iter()
                .position(|s| matches!(&s.role, Role::Layer { layer: l, .. } if l == layer))?;
            Some((oi, si))
        })
    }

    /// The output the surface is on, and its size, if it can be drawn.
    fn drawable(&self, t: Target) -> Option<(usize, (u32, u32))> {
        let (oi, surf) = match t {
            Target::Layer(oi, si) => (oi, self.outs.get(oi)?.surfaces.get(si)?),
            Target::Popup => {
                let surf = self.popup.as_ref()?;
                let Role::Popup { spec, .. } = &surf.role else {
                    return None;
                };
                let parent = spec.parent.0;
                (self.outs.iter().position(|o| o.info.id == parent)?, surf)
            }
        };
        let size = surf.size;
        (surf.configured && size.0 > 0 && size.1 > 0).then_some((oi, size))
    }

    fn draw(&mut self, t: Target) {
        let Some((oi, size)) = self.drawable(t) else {
            return;
        };
        let (id, scale) = (self.outs[oi].info.id, self.outs[oi].info.scale);
        let started = Instant::now();
        self.scene.clear();
        let mut paint = Paint {
            scene: &mut self.scene,
            size: (size.0 as f32, size.1 as f32),
            scale: scale as f32,
            time: started,
        };
        let on = match t {
            Target::Layer(_, si) => On::Surface(id, SurfaceId(si)),
            Target::Popup => On::Popup,
        };
        match on {
            On::Surface(_, si) => self.app.draw(id, si, &mut paint),
            On::Popup => self.app.draw_popup(&mut paint),
        }
        let animating = self.draw_always || self.app.animating(on);
        let unpaced = self.draw_always && self.app.max_fps(on).is_none();
        let px = (size.0 as i32 * scale, size.1 as i32 * scale);
        let monitor = name(&self.outs[oi].info).to_owned();
        let Some(surf) = surface(&mut self.outs, &mut self.popup, t) else {
            return;
        };
        surf.drawn_at = Some(started);
        if let Some(token) = surf.timer.take() {
            self.handle.remove(token);
        }
        // Before the commit that shows this frame, which takes the request
        // along, whichever thread makes it: requests leave in the order they
        // are made.
        if animating && !surf.frame_pending {
            let ws = surf.role.wl_surface().clone();
            if unpaced {
                // Once the loop has handled what is pending, rather than at
                // the compositor's next frame. The flush sends a frame the
                // CPU drew, since the loop may never go to sleep, which is
                // where it flushes otherwise.
                self.handle.insert_idle(move |e| {
                    if let Some(t) = e.target_of(&ws) {
                        if let Some(surf) = surface(&mut e.outs, &mut e.popup, t) {
                            surf.frame_pending = false;
                        }
                        e.draw(t);
                        let _ = e.conn.flush();
                    }
                });
            } else {
                ws.frame(&self.qh, ws.clone());
            }
            surf.frame_pending = true;
        }
        if let GlesState::Running(thread) = &mut self.gles {
            let built = started.elapsed();
            let frame = gles::Frame {
                surface: surf.role.wl_surface().clone(),
                size: px,
                scale,
                scene: std::mem::take(&mut self.scene),
                monitor,
                target: t,
                on,
                built,
                sent: Instant::now(),
            };
            // The GPU's thread logs the draw.
            surf.window
                .get_or_insert_with(|| thread.window())
                .draw(frame);
            return;
        }
        surf.role.wl_surface().set_buffer_scale(scale);
        self.draw_cpu(t, &monitor, px, scale);
        let took = started.elapsed();
        debug!(
            monitor = %monitor,
            surface = %t,
            size = ?px,
            on = %"cpu",
            took = ?took,
            "drew"
        );
        self.app.drew(
            on,
            Drawn {
                took,
                waited: Duration::ZERO,
                gpu: false,
            },
        );
        // After the first frame, so that it doesn't wait even for the
        // thread to be made.
        if self.gpu == Gpu::Enabled && matches!(self.gles, GlesState::NotStarted) {
            self.start_gpu();
        }
    }

    fn draw_cpu(&mut self, t: Target, monitor: &str, (w, h): (i32, i32), scale: i32) {
        let (buffer, px) = match self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        {
            Ok(buffer) => buffer,
            Err(e) => {
                error!(monitor = %monitor, "no buffer to draw into: {e}");
                return;
            }
        };
        renderer::cpu::draw(px, w as usize, h as usize, scale as f32, &self.scene.shapes);
        let Some(surf) = surface(&mut self.outs, &mut self.popup, t) else {
            return;
        };
        let ws = surf.role.wl_surface();
        if buffer.attach_to(ws).is_ok() {
            ws.damage_buffer(0, 0, w, h);
        }
        ws.commit();
    }

    fn start_gpu(&mut self) {
        let (report, reports) = channel::channel();
        let started = self
            .handle
            .insert_source(reports, |event, _, engine| {
                if let Event::Msg(report) = event {
                    engine.gpu_reported(report);
                }
            })
            .map_err(|e| e.error.to_string())
            .and_then(|_| gles::Thread::start(self.conn.clone(), report));
        self.gles = match started {
            Ok(thread) => GlesState::Starting(thread),
            Err(e) => {
                warn!("cannot use the GPU, so every surface is drawn on the CPU: {e}");
                GlesState::Failed
            }
        };
    }

    /// Once the GPU has started, every surface is drawn at once, which
    /// moves it, rather than at its next draw: a panel may not be drawn
    /// again for a long time.
    fn gpu_reported(&mut self, report: gles::Report) {
        match report {
            gles::Report::Started(device) => {
                self.gles = match std::mem::replace(&mut self.gles, GlesState::Failed) {
                    GlesState::Starting(thread) => GlesState::Running(thread),
                    other => other,
                };
                self.app.gpu_started(&device);
                for t in self.targets(Redraw::Everything) {
                    self.draw(t);
                }
            }
            gles::Report::Drew(on, drawn) => self.app.drew(on, drawn),
            gles::Report::Failed(e) if matches!(self.gles, GlesState::Starting(_)) => {
                warn!("cannot use the GPU, so every surface is drawn on the CPU: {e}");
                self.gles = GlesState::Failed;
            }
            gles::Report::Failed(e) => self.stop_gpu(&e),
        }
    }

    /// Replaces every surface on the GPU with a new one, which the CPU draws
    /// once the compositor configures it. The CPU cannot draw into the old
    /// ones: a compositor with explicit sync rejects a shared-memory buffer
    /// on a surface EGL has drawn into. Surfaces without a window stay as
    /// they are.
    fn stop_gpu(&mut self, e: &str) {
        warn!("the GPU failed, so every surface is drawn on the CPU: {e}");
        self.gles = GlesState::Failed;
        // Before the layer surfaces, one of which is its parent.
        self.lose_popup();
        for oi in 0..self.outs.len() {
            for si in 0..self.outs[oi].surfaces.len() {
                let surf = &self.outs[oi].surfaces[si];
                let Role::Layer { spec, .. } = &surf.role else {
                    continue;
                };
                if surf.window.is_none() {
                    continue;
                }
                let (wl, spec) = (self.outs[oi].wl.clone(), spec.clone());
                self.outs[oi].surfaces[si] = self.layer(&wl, spec);
            }
        }
    }

    /// Closes the popup without the app asking, as when its parent goes
    /// away, and tells the app.
    fn lose_popup(&mut self) {
        self.grab = None;
        if self.popup.take().is_some() {
            debug!("popup closed");
            self.app.popup_closed();
        }
    }

    /// Whether the popup hangs from a surface on this output.
    fn popup_on(&self, output: OutputId) -> bool {
        self.popup
            .as_ref()
            .is_some_and(|p| matches!(&p.role, Role::Popup { spec, .. } if spec.parent.0 == output))
    }
}

/// Indices into the outputs and their surfaces, each once, so that a surface
/// asked for twice in one batch is drawn once. Outputs and surfaces that are
/// gone are left out, because the event that removed them may have come in
/// the same batch, after the app asked.
fn targets(outs: &[(OutputId, usize)], redraw: Redraw) -> Vec<Target> {
    let mut targets: Vec<_> = match redraw {
        Redraw::Nothing => Vec::new(),
        Redraw::Everything => outs
            .iter()
            .enumerate()
            .flat_map(|(oi, &(_, n))| (0..n).map(move |si| Target::Layer(oi, si)))
            .chain([Target::Popup])
            .collect(),
        Redraw::Surfaces(surfaces) => surfaces
            .into_iter()
            .filter_map(|on| match on {
                On::Surface(id, SurfaceId(si)) => {
                    let oi = outs.iter().position(|&(o, _)| o == id)?;
                    (si < outs[oi].1).then_some(Target::Layer(oi, si))
                }
                On::Popup => Some(Target::Popup),
            })
            .collect(),
    };
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn name(out: &Output) -> &str {
    out.name.as_deref().unwrap_or("unnamed")
}

/// Whether a surface of `size` changes size with the spec's size. Along an
/// axis the spec leaves to the compositor, it counts as staying: the
/// compositor may still change it, as for new margins, and the surface is
/// then drawn twice, but a surface that waited for a configure that never
/// comes would not be drawn at all.
fn resized(size: (u32, u32), spec: (u32, u32)) -> bool {
    (spec.0 > 0 && spec.0 != size.0) || (spec.1 > 0 && spec.1 != size.1)
}

// ── Wayland handlers ─────────────────────────────────────────────────────────

impl CompositorHandler for Engine {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    /// The compositor's time stamp is left out: animations step to the time
    /// the engine starts each frame, the same clock for every surface.
    ///
    /// A frame that comes sooner than [`App::max_fps`] allows is drawn when
    /// it is due, on a timer: the compositor offers frames only at the
    /// monitor's refresh, so skipping one would wait a whole refresh more.
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, ws: &wl_surface::WlSurface, _: u32) {
        let Some(t) = self.target_of(ws) else {
            return;
        };
        let on = self.on_of(t);
        let Some(surf) = surface(&mut self.outs, &mut self.popup, t) else {
            return;
        };
        surf.frame_pending = false;
        let due = self
            .app
            .max_fps(on)
            .filter(|fps| *fps > 0.0)
            .zip(surf.drawn_at)
            .map(|(fps, at)| at + Duration::from_secs_f32(1.0 / fps));
        match due {
            Some(due) if due > Instant::now() + Duration::from_millis(1) => {
                let ws = ws.clone();
                let timer = self
                    .handle
                    .insert_source(Timer::from_deadline(due), move |_, _, e| {
                        if let Some(t) = e.target_of(&ws) {
                            // Before the draw, which would otherwise remove
                            // this very timer while it runs.
                            if let Some(surf) = surface(&mut e.outs, &mut e.popup, t) {
                                surf.timer = None;
                            }
                            e.draw(t);
                        }
                        TimeoutAction::Drop
                    });
                match timer {
                    Ok(token) => {
                        if let Some(surf) = surface(&mut self.outs, &mut self.popup, t) {
                            surf.timer = Some(token);
                        }
                    }
                    Err(e) => {
                        warn!("cannot wait for the next frame, so it is drawn at once: {e}");
                        self.draw(t);
                    }
                }
            }
            _ => self.draw(t),
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Engine {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_output(output);
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let Some(oi) = self.outs.iter().position(|o| o.wl == output) else {
            return;
        };
        info!(monitor = %name(&self.outs[oi].info), "monitor removed");
        if self.popup_on(self.outs[oi].info.id) {
            self.lose_popup();
        }
        self.outs.remove(oi);
    }
}

impl LayerShellHandler for Engine {
    /// The compositor closes a layer surface mainly when its output goes
    /// away, so the whole output is dropped instead of a single surface.
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.is_popup_layer(layer) {
            self.lose_popup();
            return;
        }
        if let Some((oi, _)) = self.find_layer(layer) {
            if self.popup_on(self.outs[oi].info.id) {
                self.lose_popup();
            }
            let out = self.outs.remove(oi);
            info!(monitor = %name(&out.info), "surface closed, monitor dropped");
        }
    }

    /// The compositor sends 0 along an axis it leaves to the client, so the
    /// spec's own size stands there. A configure may repeat the size the
    /// surface already has, which is not worth a redraw. sctk has
    /// acknowledged the configure already.
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        c: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.is_popup_layer(layer) {
            self.configure_popup(c.new_size);
            return;
        }
        let Some((oi, si)) = self.find_layer(layer) else {
            return;
        };
        let surf = &mut self.outs[oi].surfaces[si];
        let Role::Layer { spec, .. } = &surf.role else {
            return;
        };
        let size = (
            if c.new_size.0 > 0 {
                c.new_size.0
            } else {
                spec.size.0
            },
            if c.new_size.1 > 0 {
                c.new_size.1
            } else {
                spec.size.1
            },
        );
        if surf.configured && surf.size == size {
            return;
        }
        surf.size = size;
        surf.configured = true;
        self.draw(Target::Layer(oi, si));
    }
}

impl ShmHandler for Engine {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Engine {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Engine);
delegate_output!(Engine);
delegate_shm!(Engine);
delegate_layer!(Engine);
delegate_registry!(Engine);

#[cfg(test)]
mod tests {
    use super::*;

    struct Nothing;

    impl App for Nothing {
        fn surfaces(&mut self, _: &Output) -> Vec<SurfaceSpec> {
            Vec::new()
        }
        fn draw(&mut self, _: OutputId, _: SurfaceId, _: &mut Paint) {}
    }

    #[test]
    fn a_length_left_to_the_compositor_counts_as_staying() {
        assert!(resized((1920, 44), (0, 60)));
        assert!(resized((600, 44), (800, 44)));
        assert!(!resized((1920, 44), (0, 44)));
        assert!(!resized((600, 44), (600, 44)));
        // From down the left to along the top: the thickness changes axis.
        assert!(resized((44, 1080), (0, 44)));
    }

    #[test]
    fn everything_is_every_surface_of_every_output() {
        let outs = [(OutputId(7), 2), (OutputId(9), 1)];
        let everything = [
            Target::Layer(0, 0),
            Target::Layer(0, 1),
            Target::Layer(1, 0),
            Target::Popup,
        ];
        assert_eq!(targets(&outs, Redraw::Everything), everything);
        assert_eq!(targets(&outs, Redraw::Nothing), []);
    }

    #[test]
    fn a_surface_asked_for_twice_is_drawn_once() {
        let outs = [(OutputId(7), 2), (OutputId(9), 1)];
        let twice = Redraw::Surfaces(vec![
            On::Surface(OutputId(9), SurfaceId(0)),
            On::Popup,
            On::Surface(OutputId(7), SurfaceId(1)),
            On::Surface(OutputId(9), SurfaceId(0)),
            On::Popup,
        ]);
        let once = [Target::Layer(0, 1), Target::Layer(1, 0), Target::Popup];
        assert_eq!(targets(&outs, twice), once);
    }

    #[test]
    fn surfaces_that_are_gone_are_left_out() {
        let outs = [(OutputId(7), 1)];
        let gone = Redraw::Surfaces(vec![
            On::Surface(OutputId(8), SurfaceId(0)),
            On::Surface(OutputId(7), SurfaceId(1)),
            On::Surface(OutputId(7), SurfaceId(0)),
        ]);
        assert_eq!(targets(&outs, gone), [Target::Layer(0, 0)]);
    }

    #[test]
    fn without_a_wayland_display_it_says_so() {
        // SAFETY: nextest runs each test in a process of its own.
        unsafe {
            std::env::remove_var("WAYLAND_SOCKET");
            std::env::set_var("WAYLAND_DISPLAY", "tonish-no-such-display");
        }
        let err = run(Box::new(Nothing), Options::default()).unwrap_err();
        assert!(err.starts_with("no Wayland display"), "{err}");
    }
}
