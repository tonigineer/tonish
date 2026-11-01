use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Instant;

use calloop::channel::{self, Event};
use calloop::signals::{Signal, Signals};
use inotify::{Inotify, WatchDescriptor, WatchMask};
use renderer::Scene;
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::{Interest, Mode, PostAction};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::calloop::{EventLoop, LoopHandle},
    reexports::calloop_wayland_source::WaylandSource,
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
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
    protocol::{wl_output, wl_shm, wl_surface},
};

use crate::app::{App, Layer, Output, OutputId, Paint, Redraw, SurfaceId, SurfaceSpec};
use crate::gles;
use crate::socket::{self, Socket};

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

/// Returns `Ok` after a quit message, SIGINT (Ctrl+C) or SIGTERM, once the
/// loop has finished what it was doing. The errors are strings, because the
/// only thing a caller does with them is print them.
pub fn run(app: Box<dyn App>, gpu: Gpu) -> Result<(), String> {
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
        qh,
        conn: conn.clone(),
        handle: event_loop.handle(),
        outs: Vec::new(),
        app,
        scene: Scene::new(),
        gpu,
        gles: GlesState::NotStarted,
        watch,
        quit: false,
    };

    while !engine.quit {
        event_loop
            .dispatch(None, &mut engine)
            .map_err(|e| e.to_string())?;
        engine.redraw();
    }
    // Destroys the surfaces and sends that, so the panels are gone before a
    // client waiting for the end hears about it.
    drop(engine);
    let _ = conn.flush();
    Ok(())
}

struct Surface {
    /// Before `layer`, so that it is dropped first: EGL draws into the
    /// wl_surface until its window is gone.
    window: Option<gles::Window>,
    layer: LayerSurface,
    spec: SurfaceSpec,
    /// Logical pixels: the buffer is this times the output's scale.
    size: (u32, u32),
    configured: bool,
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
    /// For the source that hears when the GPU has started, added only once
    /// the GPU is needed.
    handle: LoopHandle<'static, Engine>,
    outs: Vec<Out>,
    app: Box<dyn App>,
    scene: Scene,
    gpu: Gpu,
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
        layer.commit();
        Surface {
            window: None,
            layer,
            spec,
            size: (0, 0),
            configured: false,
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
        let mut rebuild = false;
        for path in &changed {
            rebuild |= self.app.changed(path);
        }
        if rebuild {
            self.rebuild();
        }
    }

    /// The old surfaces are destroyed as they are dropped; the new ones are
    /// drawn once the compositor configures them, as on a first start.
    fn rebuild(&mut self) {
        debug!("rebuilding every surface");
        for oi in 0..self.outs.len() {
            let wl = self.outs[oi].wl.clone();
            let specs = self.app.surfaces(&self.outs[oi].info);
            let surfaces = specs
                .into_iter()
                .map(|spec| self.layer(&wl, spec))
                .collect();
            self.outs[oi].surfaces = surfaces;
        }
    }

    fn redraw(&mut self) {
        let redraw = self.app.to_redraw();
        if redraw == Redraw::Nothing {
            return;
        }
        let outs: Vec<_> = self
            .outs
            .iter()
            .map(|o| (o.info.id, o.surfaces.len()))
            .collect();
        for (oi, si) in targets(&outs, redraw) {
            self.draw(oi, si);
        }
    }

    fn find_layer(&self, layer: &LayerSurface) -> Option<(usize, usize)> {
        self.outs.iter().enumerate().find_map(|(oi, o)| {
            let si = o.surfaces.iter().position(|s| &s.layer == layer)?;
            Some((oi, si))
        })
    }

    fn draw(&mut self, oi: usize, si: usize) {
        let out = &self.outs[oi];
        let scale = out.info.scale;
        let surf = &out.surfaces[si];
        if !surf.configured || surf.size.0 == 0 || surf.size.1 == 0 {
            return;
        }
        let started = Instant::now();
        self.scene.clear();
        let mut paint = Paint {
            scene: &mut self.scene,
            size: (surf.size.0 as f32, surf.size.1 as f32),
            scale: scale as f32,
        };
        self.app.draw(out.info.id, SurfaceId(si), &mut paint);
        let size = (surf.size.0 as i32 * scale, surf.size.1 as i32 * scale);
        if let GlesState::Running(thread) = &mut self.gles {
            let built = started.elapsed();
            let monitor = name(&out.info).to_owned();
            let surf = &mut self.outs[oi].surfaces[si];
            let frame = gles::Frame {
                surface: surf.layer.wl_surface().clone(),
                size,
                scale,
                scene: std::mem::take(&mut self.scene),
                monitor,
                index: si,
                built,
                sent: Instant::now(),
            };
            // The GPU's thread logs the draw.
            surf.window
                .get_or_insert_with(|| thread.window())
                .draw(frame);
            return;
        }
        surf.layer.wl_surface().set_buffer_scale(scale);
        self.draw_cpu(oi, si, size, scale);
        debug!(
            monitor = %name(&self.outs[oi].info),
            surface = si,
            size = ?size,
            on = %"cpu",
            took = ?started.elapsed(),
            "drew"
        );
        // After the first frame, so that it doesn't wait even for the
        // thread to be made.
        if self.gpu == Gpu::Enabled && matches!(self.gles, GlesState::NotStarted) {
            self.start_gpu();
        }
    }

    fn draw_cpu(&mut self, oi: usize, si: usize, (w, h): (i32, i32), scale: i32) {
        let out = &mut self.outs[oi];
        let surf = &mut out.surfaces[si];
        let (buffer, px) = match self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        {
            Ok(buffer) => buffer,
            Err(e) => {
                error!(monitor = %name(&out.info), "no buffer to draw into: {e}");
                return;
            }
        };
        renderer::cpu::draw(px, w as usize, h as usize, scale as f32, &self.scene.shapes);
        let ws = surf.layer.wl_surface();
        if buffer.attach_to(ws).is_ok() {
            ws.damage_buffer(0, 0, w, h);
        }
        surf.layer.commit();
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
            gles::Report::Started => {
                self.gles = match std::mem::replace(&mut self.gles, GlesState::Failed) {
                    GlesState::Starting(thread) => GlesState::Running(thread),
                    other => other,
                };
                for oi in 0..self.outs.len() {
                    for si in 0..self.outs[oi].surfaces.len() {
                        self.draw(oi, si);
                    }
                }
            }
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
        for oi in 0..self.outs.len() {
            for si in 0..self.outs[oi].surfaces.len() {
                if self.outs[oi].surfaces[si].window.is_none() {
                    continue;
                }
                let wl = self.outs[oi].wl.clone();
                let spec = self.outs[oi].surfaces[si].spec.clone();
                self.outs[oi].surfaces[si] = self.layer(&wl, spec);
            }
        }
    }
}

/// Indices into the outputs and their surfaces, each once, so that a surface
/// asked for twice in one batch is drawn once. Outputs and surfaces that are
/// gone are left out, because the event that removed them may have come in
/// the same batch, after the app asked.
fn targets(outs: &[(OutputId, usize)], redraw: Redraw) -> Vec<(usize, usize)> {
    let mut targets: Vec<_> = match redraw {
        Redraw::Nothing => Vec::new(),
        Redraw::Everything => outs
            .iter()
            .enumerate()
            .flat_map(|(oi, &(_, n))| (0..n).map(move |si| (oi, si)))
            .collect(),
        Redraw::Surfaces(surfaces) => surfaces
            .into_iter()
            .filter_map(|(id, SurfaceId(si))| {
                let oi = outs.iter().position(|&(o, _)| o == id)?;
                (si < outs[oi].1).then_some((oi, si))
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
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
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
        if let Some(out) = self.outs.iter().find(|o| o.wl == output) {
            info!(monitor = %name(&out.info), "monitor removed");
        }
        self.outs.retain(|o| o.wl != output);
    }
}

impl LayerShellHandler for Engine {
    /// The compositor closes a layer surface mainly when its output goes
    /// away, so the whole output is dropped instead of a single surface.
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some((oi, _)) = self.find_layer(layer) {
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
        let Some((oi, si)) = self.find_layer(layer) else {
            return;
        };
        let surf = &mut self.outs[oi].surfaces[si];
        let size = (
            if c.new_size.0 > 0 {
                c.new_size.0
            } else {
                surf.spec.size.0
            },
            if c.new_size.1 > 0 {
                c.new_size.1
            } else {
                surf.spec.size.1
            },
        );
        if surf.configured && surf.size == size {
            return;
        }
        surf.size = size;
        surf.configured = true;
        self.draw(oi, si);
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
    registry_handlers![OutputState];
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
    fn everything_is_every_surface_of_every_output() {
        let outs = [(OutputId(7), 2), (OutputId(9), 1)];
        assert_eq!(targets(&outs, Redraw::Everything), [(0, 0), (0, 1), (1, 0)]);
        assert_eq!(targets(&outs, Redraw::Nothing), []);
    }

    #[test]
    fn a_surface_asked_for_twice_is_drawn_once() {
        let outs = [(OutputId(7), 2), (OutputId(9), 1)];
        let twice = Redraw::Surfaces(vec![
            (OutputId(9), SurfaceId(0)),
            (OutputId(7), SurfaceId(1)),
            (OutputId(9), SurfaceId(0)),
        ]);
        assert_eq!(targets(&outs, twice), [(0, 1), (1, 0)]);
    }

    #[test]
    fn surfaces_that_are_gone_are_left_out() {
        let outs = [(OutputId(7), 1)];
        let gone = Redraw::Surfaces(vec![
            (OutputId(8), SurfaceId(0)),
            (OutputId(7), SurfaceId(1)),
            (OutputId(7), SurfaceId(0)),
        ]);
        assert_eq!(targets(&outs, gone), [(0, 0)]);
    }

    #[test]
    fn without_a_wayland_display_it_says_so() {
        // SAFETY: nextest runs each test in a process of its own.
        unsafe {
            std::env::remove_var("WAYLAND_SOCKET");
            std::env::set_var("WAYLAND_DISPLAY", "tonish-no-such-display");
        }
        let err = run(Box::new(Nothing), Gpu::default()).unwrap_err();
        assert!(err.starts_with("no Wayland display"), "{err}");
    }
}
