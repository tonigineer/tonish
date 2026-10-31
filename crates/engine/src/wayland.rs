//! The engine on Wayland: layer surfaces on every monitor (wlr-layer-shell),
//! drawn on the CPU into wl_shm, all on one calloop loop.

use renderer::Scene;
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

/// Run `app` until the compositor goes.
pub fn run(app: Box<dyn App>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland display: {e}"))?;
    let (globals, queue) = registry_queue_init(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let mut event_loop: EventLoop<Engine> = EventLoop::try_new().map_err(|e| e.to_string())?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(|e| e.to_string())?;

    let shm = Shm::bind(&globals, &qh).map_err(|_| "no wl_shm")?;
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
    };
    loop {
        event_loop
            .dispatch(None, &mut engine)
            .map_err(|e| e.to_string())?;
    }
}

struct Surface {
    layer: LayerSurface,
    spec: SurfaceSpec,
    /// Its size as the compositor gave it (logical px), once it has.
    size: (u32, u32),
    configured: bool,
}

struct Out {
    info: Output,
    wl: wl_output::WlOutput,
    surfaces: Vec<Surface>,
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
}

impl Engine {
    /// A monitor came: the shell's surfaces on it.
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

    /// A layer surface on monitor `wl` as `spec` says, committed empty: the
    /// compositor answers with its size (`configure`).
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
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.commit();
        Surface {
            layer,
            spec,
            size: (0, 0),
            configured: false,
        }
    }

    fn find_layer(&self, layer: &LayerSurface) -> Option<(usize, usize)> {
        self.outs.iter().enumerate().find_map(|(oi, o)| {
            let si = o.surfaces.iter().position(|s| &s.layer == layer)?;
            Some((oi, si))
        })
    }

    /// Draw surface `si` of monitor `oi`: the shell's scene, on the CPU,
    /// into a buffer of shared memory, attached and committed.
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
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some((oi, _)) = self.find_layer(layer) {
            self.outs.remove(oi);
        }
    }

    /// The compositor's size for a surface: 0 along an axis is the spec's
    /// own. Drawn then.
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
