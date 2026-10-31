use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use calloop::signals::{Signal, Signals};
use inotify::{Inotify, WatchDescriptor, WatchMask};
use renderer::Scene;
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::{Interest, Mode, PostAction};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::calloop::EventLoop,
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
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};

use crate::app::{App, Layer, Output, OutputId, Paint, SurfaceId, SurfaceSpec};
use crate::socket::{self, Socket};

/// Returns `Ok` after a quit message, SIGINT (Ctrl+C) or SIGTERM, once the
/// loop has finished what it was doing. The errors are strings, because the
/// only thing a caller does with them is print them.
pub fn run(app: Box<dyn App>) -> Result<(), String> {
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
        .insert_source(signals, |_, _, engine| engine.quit = true)
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
            eprintln!("cannot watch for changes, so edits apply after a restart: {e}");
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
        outs: Vec::new(),
        app,
        scene: Scene::new(),
        watch,
        quit: false,
    };

    while !engine.quit {
        event_loop
            .dispatch(None, &mut engine)
            .map_err(|e| e.to_string())?;
    }
    // Destroys the surfaces and sends that, so the panels are gone before a
    // client waiting for the end hears about it.
    drop(engine);
    let _ = conn.flush();
    Ok(())
}

struct Surface {
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
    outs: Vec<Out>,
    app: Box<dyn App>,
    scene: Scene,
    watch: Option<Watch>,
    quit: bool,
}

impl Engine {
    /// The client asking to quit keeps its connection, which closes when the
    /// process ends: its read then returns once the shell is really gone.
    fn client(&mut self, stream: &UnixStream, line: &mut Vec<u8>) -> PostAction {
        let Some(text) = socket::read_line(stream, line) else {
            return PostAction::Continue;
        };
        let reply = self.app.message(&text);
        socket::reply(stream, &reply.text);
        if reply.quit {
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
            Some(format!("gineer-{}", spec.name)),
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
        let size = (surf.size.0 as f32, surf.size.1 as f32);
        self.scene.clear();
        let mut paint = Paint {
            scene: &mut self.scene,
            size,
            scale: scale as f32,
        };
        self.app.draw(out.info.id, SurfaceId(si), &mut paint);
        let (w, h) = (surf.size.0 as i32 * scale, surf.size.1 as i32 * scale);
        let Ok((buffer, px)) = self
            .pool
            .create_buffer(w, h, w * 4, wl_shm::Format::Argb8888)
        else {
            return;
        };
        renderer::cpu::draw(px, w as usize, h as usize, scale as f32, &self.scene.shapes);
        let ws = surf.layer.wl_surface();
        ws.set_buffer_scale(scale);
        if buffer.attach_to(ws).is_ok() {
            ws.damage_buffer(0, 0, w, h);
        }
        surf.layer.commit();
    }
}

// ── Wayland handlers ─────────────────────────────────────────────────────

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
        self.outs.retain(|o| o.wl != output);
    }
}

impl LayerShellHandler for Engine {
    /// The compositor closes a layer surface mainly when its output goes
    /// away, so the whole output is dropped instead of a single surface.
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some((oi, _)) = self.find_layer(layer) {
            self.outs.remove(oi);
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
    fn without_a_wayland_display_it_says_so() {
        // SAFETY: nextest runs each test in a process of its own.
        unsafe {
            std::env::remove_var("WAYLAND_SOCKET");
            std::env::set_var("WAYLAND_DISPLAY", "gineer-no-such-display");
        }
        let err = run(Box::new(Nothing)).unwrap_err();
        assert!(err.starts_with("no Wayland display"), "{err}");
    }
}
