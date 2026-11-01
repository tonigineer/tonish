//! OpenGL ES through EGL on the Wayland connection, on a thread of its own
//! that draws every GPU frame. It starts after the first frame, which the
//! CPU draws, because starting takes about 0.1 s with NVIDIA's driver. It
//! keeps EGL to itself for good, because NVIDIA's driver makes the first
//! frame cost about 45 ms on any thread but the one that started EGL, and
//! about 2 ms on that one.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use calloop::channel::Sender;
use khronos_egl as egl;
use renderer::Scene;
use renderer::gpu::{Gpu, Unsupported};
use tracing::{debug, info};

use crate::wayland::Target;
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};

/// From EGL_KHR_platform_wayland, which khronos-egl has no constant for.
const PLATFORM_WAYLAND: egl::Enum = 0x31D8;

type Lib = egl::DynamicInstance<egl::EGL1_5>;

/// Shared by [`Gles`] and every [`EglWindow`], so that it outlives all of
/// them.
struct Display {
    lib: Lib,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
}

impl Display {
    fn open(conn: &Connection) -> Result<Self, String> {
        if !wayland_egl::is_available() {
            return Err("no libwayland-egl".into());
        }
        // SAFETY: libEGL is the system's EGL, which implements the API.
        let lib = unsafe { Lib::load_required() }.map_err(|e| format!("no libEGL: {e}"))?;
        // SAFETY: the pointer is the connection's live wl_display, and the
        // display is terminated in `Display::drop`, on the GPU's thread,
        // which keeps a clone of the connection open until it has ended.
        let display = unsafe {
            lib.get_platform_display(
                PLATFORM_WAYLAND,
                conn.backend().display_ptr().cast(),
                &[egl::ATTRIB_NONE],
            )
        }
        .map_err(|e| format!("no EGL display: {e}"))?;
        lib.initialize(display)
            .map_err(|e| format!("EGL does not start: {e}"))?;
        let context = lib
            .bind_api(egl::OPENGL_ES_API)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                lib.choose_first_config(
                    display,
                    &[
                        egl::RED_SIZE,
                        8,
                        egl::GREEN_SIZE,
                        8,
                        egl::BLUE_SIZE,
                        8,
                        egl::ALPHA_SIZE,
                        8,
                        egl::SURFACE_TYPE,
                        egl::WINDOW_BIT,
                        egl::RENDERABLE_TYPE,
                        egl::OPENGL_ES3_BIT,
                        egl::NONE,
                    ],
                )
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no RGBA configuration for OpenGL ES 3".to_string())
            })
            .and_then(|config| {
                let attrs = [egl::CONTEXT_MAJOR_VERSION, 3, egl::NONE];
                let context = lib
                    .create_context(display, config, None, &attrs)
                    .map_err(|e| format!("no OpenGL ES 3 context: {e}"))?;
                Ok((config, context))
            });
        let (config, context) = match context {
            Ok(both) => both,
            Err(e) => {
                let _ = lib.terminate(display);
                return Err(e);
            }
        };
        Ok(Self {
            lib,
            display,
            config,
            context,
        })
    }

    /// The EGL implementation, such as "NVIDIA" or "Mesa Project", for the
    /// log.
    fn vendor(&self) -> String {
        self.lib
            .query_string(Some(self.display), egl::VENDOR)
            .map_or_else(|_| "unknown".into(), |s| s.to_string_lossy().into_owned())
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        let _ = self.lib.make_current(self.display, None, None, None);
        let _ = self.lib.destroy_context(self.display, self.context);
        let _ = self.lib.terminate(self.display);
    }
}

/// What the GPU's thread tells the engine.
pub(crate) enum Report {
    /// It takes frames from now on.
    Started,
    /// It failed, to start or later, and has ended. Every window was gone
    /// before it said so, so the engine may destroy the surfaces.
    Failed(String),
}

/// One surface's frame, made on the engine's thread.
pub(crate) struct Frame {
    pub(crate) surface: WlSurface,
    /// Physical pixels.
    pub(crate) size: (i32, i32),
    pub(crate) scale: i32,
    pub(crate) scene: Scene,
    /// The rest is for the log.
    pub(crate) monitor: String,
    pub(crate) target: Target,
    /// How long the engine took to build the scene, which the log counts
    /// with the drawing, as for a frame the CPU draws.
    pub(crate) built: Duration,
    /// When the engine handed the frame over, so that the log can tell the
    /// time it waited for the thread from the time spent on it.
    pub(crate) sent: Instant,
}

enum Command {
    Draw(u64, Frame),
    Commit(WlSurface),
    /// Destroys the surface's window, then answers.
    Forget(u64, mpsc::Sender<()>),
    Quit,
}

/// The engine's end of the GPU's thread. Dropping it ends the thread and
/// waits for it: when tonish quit while the thread was still inside
/// libEGL, the process crashed on its way out, as the desktop tests found.
pub(crate) struct Thread {
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
    next: u64,
}

impl Thread {
    /// `report` hears when the thread is ready for frames, and when it fails.
    pub(crate) fn start(conn: Connection, report: Sender<Report>) -> Result<Self, String> {
        let (commands, received) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("gpu".into())
            .spawn(move || {
                // Reported like any failure, or the panels on the GPU would
                // stay as they are for good. The unwinding has dropped every
                // window by the time it is caught.
                let ran = panic::catch_unwind(AssertUnwindSafe(|| run(&conn, &received, &report)));
                if ran.is_err() {
                    let _ = report.send(Report::Failed("the GPU's thread panicked".into()));
                }
            })
            .map_err(|e| format!("no thread to start it on: {e}"))?;
        Ok(Self {
            commands,
            thread: Some(thread),
            next: 0,
        })
    }

    /// The thread makes the window itself, at the surface's first frame.
    pub(crate) fn window(&mut self) -> Window {
        let key = self.next;
        self.next += 1;
        Window {
            key,
            commands: self.commands.clone(),
        }
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        // A message rather than closing the channel, which every `Window`
        // keeps open.
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A surface's window on the GPU's thread, as the engine holds it. Dropping
/// it destroys the window there, and waits for that, so that the
/// wl_surface, dropped after it, outlives the window.
pub(crate) struct Window {
    key: u64,
    commands: mpsc::Sender<Command>,
}

impl Window {
    /// The thread commits the surface, with the buffer swap. A frame of the
    /// same surface that still waits there is dropped, so that the GPU never
    /// falls behind.
    pub(crate) fn draw(&self, frame: Frame) {
        // Fails only once the thread has ended, which it reports.
        let _ = self.commands.send(Command::Draw(self.key, frame));
    }
}

impl Window {
    /// Commits the surface's pending state on the GPU's thread, between its
    /// frames. A commit on another thread could fall between a frame's
    /// attach and its commit, and a compositor with explicit sync ends the
    /// connection for a buffer committed without its sync point.
    pub(crate) fn commit(&self, surface: WlSurface) {
        // Fails only once the thread has ended, which it reports.
        let _ = self.commands.send(Command::Commit(surface));
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let (done, gone) = mpsc::channel();
        if self.commands.send(Command::Forget(self.key, done)).is_ok() {
            // Returns as well when the thread ends without answering, which
            // it does only after dropping every window.
            let _ = gone.recv();
        }
    }
}

fn run(conn: &Connection, commands: &mpsc::Receiver<Command>, report: &Sender<Report>) {
    let mut gles = match Gles::start(conn) {
        Ok(gles) => gles,
        Err(e) => {
            let _ = report.send(Report::Failed(e));
            return;
        }
    };
    let _ = report.send(Report::Started);
    let mut windows = HashMap::new();
    let mut frames: Vec<(u64, Frame)> = Vec::new();
    while let Ok(first) = commands.recv() {
        for command in std::iter::once(first).chain(commands.try_iter()) {
            match command {
                Command::Draw(key, frame) => {
                    frames.retain(|(k, _)| *k != key);
                    frames.push((key, frame));
                }
                Command::Commit(surface) => {
                    surface.commit();
                    // No frame need follow to flush it, as eglSwapBuffers
                    // would, and the main thread's flush may have gone by
                    // already. A broken connection shows on the main thread.
                    let _ = conn.flush();
                }
                Command::Forget(key, done) => {
                    frames.retain(|(k, _)| *k != key);
                    windows.remove(&key);
                    let _ = done.send(());
                }
                Command::Quit => return,
            }
        }
        for (key, frame) in frames.drain(..) {
            if let Err(e) = gles.draw(&mut windows, key, frame) {
                windows.clear();
                let _ = report.send(Report::Failed(e));
                return;
            }
        }
    }
}

/// Never leaves the GPU's thread, the only one that makes the context
/// current.
struct Gles {
    display: Rc<Display>,
    gpu: Gpu,
    /// For the frames the CPU draws, kept so that its allocation is reused.
    pixels: Vec<u8>,
}

impl Gles {
    fn start(conn: &Connection) -> Result<Self, String> {
        let started = Instant::now();
        let egl = Display::open(conn)?;
        debug!(vendor = %egl.vendor(), took = ?started.elapsed(), "EGL started");
        let d = &egl;
        // Without a surface, which EGL_KHR_surfaceless_context allows, as Mesa
        // and NVIDIA's driver offer, because the shaders are compiled before
        // any surface has a window.
        d.lib
            .make_current(d.display, None, None, Some(d.context))
            .map_err(|e| format!("cannot use a context without a surface: {e}"))?;
        let lib = &d.lib;
        // SAFETY: the context was just made current on this thread, which
        // is the only one that uses the `Gpu`, and makes it current first.
        let gpu = unsafe {
            Gpu::new(|name| {
                lib.get_proc_address(name)
                    .map_or(std::ptr::null(), |f| f as *const c_void)
            })
        }
        .map_err(|e| format!("the shaders do not compile: {e}"))?;
        // Only now, because the device is only known once a context is
        // current.
        info!(device = %gpu.device(), took = ?started.elapsed(), "GPU started");
        Ok(Self {
            display: Rc::new(egl),
            gpu,
            pixels: Vec::new(),
        })
    }

    fn draw(
        &mut self,
        windows: &mut HashMap<u64, EglWindow>,
        key: u64,
        frame: Frame,
    ) -> Result<(), String> {
        let waited = frame.sent.elapsed();
        let (window, fresh) = match windows.entry(key) {
            Entry::Occupied(window) => (window.into_mut(), false),
            Entry::Vacant(entry) => {
                let started = Instant::now();
                let window = EglWindow::new(&self.display, &frame.surface, frame.size)?;
                debug!(
                    monitor = %frame.monitor,
                    surface = %frame.target,
                    took = ?started.elapsed(),
                    "moved to the GPU"
                );
                (entry.insert(window), true)
            }
        };
        let started = Instant::now();
        let on = self.show(window, &frame)?;
        debug!(
            monitor = %frame.monitor,
            surface = %frame.target,
            size = ?frame.size,
            on = %on,
            took = ?(frame.built + started.elapsed()),
            waited = ?waited,
            "drew"
        );
        // Twice more, back to back, so that EGL allocates the three buffers a
        // fast animation needs now, while the surface shows the same image,
        // rather than at an animation's first frames: each allocation stalls
        // the swap for 0.2 to 2 ms, measured with NVIDIA's driver. After the
        // first frame rather than before, so that the surface shows without
        // waiting for them.
        if fresh {
            for _ in 0..2 {
                self.show(window, &frame)?;
            }
        }
        Ok(())
    }

    /// Draws the frame and commits the surface, which the buffer swap
    /// does. Returns which rasteriser drew it, for the log: a scene with a
    /// shape the shader does not know is drawn by the CPU, here, and the GPU
    /// shows the result.
    fn show(&mut self, window: &mut EglWindow, frame: &Frame) -> Result<&'static str, String> {
        let d = &self.display;
        let failed = |what: &str, e: egl::Error| format!("{what}: {e}");
        let size = frame.size;
        if window.size != size {
            window.window.resize(size.0, size.1, 0, 0);
            window.size = size;
        }
        d.lib
            .make_current(
                d.display,
                Some(window.surface),
                Some(window.surface),
                Some(d.context),
            )
            .map_err(|e| failed("cannot draw on the surface", e))?;
        let (w, h) = (size.0 as u32, size.1 as u32);
        let (scale, shapes) = (frame.scale as f32, &frame.scene.shapes);
        let on = match self.gpu.draw(w, h, scale, shapes) {
            Ok(()) => "gpu",
            Err(Unsupported) => {
                self.pixels.resize((w * h * 4) as usize, 0);
                renderer::cpu::draw(&mut self.pixels, w as usize, h as usize, scale, shapes);
                self.gpu.draw_image(w, h, &self.pixels);
                "cpu"
            }
        };
        // Here rather than on the engine's thread, so that it is committed
        // with the frame it belongs to: with another, a buffer that the new
        // scale does not divide is a protocol error.
        frame.surface.set_buffer_scale(frame.scale);
        d.lib
            .swap_buffers(d.display, window.surface)
            .map_err(|e| failed("cannot show the frame", e))?;
        Ok(on)
    }
}

/// A surface's EGL side.
struct EglWindow {
    display: Rc<Display>,
    surface: egl::Surface,
    /// Destroyed after `surface`, which `drop` destroys first and which
    /// draws into it.
    window: wayland_egl::WlEglSurface,
    size: (i32, i32),
}

impl EglWindow {
    /// Left current, because the swap interval can only be set on the
    /// current surface, and EGL keeps it per surface.
    fn new(display: &Rc<Display>, surface: &WlSurface, size: (i32, i32)) -> Result<Self, String> {
        let d = display;
        let window = wayland_egl::WlEglSurface::new(surface.id(), size.0, size.1)
            .map_err(|e| format!("no EGL window: {e}"))?;
        // SAFETY: the window lives in the returned `EglWindow`, which
        // destroys the EGL surface before it.
        let egl_surface = unsafe {
            d.lib.create_window_surface(
                d.display,
                d.config,
                window.ptr() as egl::NativeWindowType,
                None,
            )
        }
        .map_err(|e| format!("no EGL surface: {e}"))?;
        let window = Self {
            display: Rc::clone(display),
            surface: egl_surface,
            window,
            size,
        };
        d.lib
            .make_current(
                d.display,
                Some(window.surface),
                Some(window.surface),
                Some(d.context),
            )
            .map_err(|e| format!("cannot draw on the surface: {e}"))?;
        // 0, because at 1 the swap waits for the compositor to show the
        // previous frame, which holds up every surface's frames, and forever
        // while the surface is hidden.
        d.lib
            .swap_interval(d.display, 0)
            .map_err(|e| format!("cannot set the swap interval: {e}"))?;
        Ok(window)
    }
}

impl Drop for EglWindow {
    fn drop(&mut self) {
        let d = &self.display;
        // EGL only destroys a surface once it is no longer current, which
        // would be after its wl_egl_window is gone.
        let _ = d.lib.make_current(d.display, None, None, None);
        let _ = d.lib.destroy_surface(d.display, self.surface);
    }
}
