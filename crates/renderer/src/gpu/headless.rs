//! An OpenGL ES 3 context without a display, for the GPU tests and for
//! `benches/draw.rs`, which includes this file by its path. A bench is a
//! crate of its own that sees only the renderer's public API, where this
//! has no place, so the file names nothing from the renderer.

use std::ffi::c_void;

use glow::HasContext;
use khronos_egl as egl;

/// Mesa's platform for rendering without any window system.
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

type Lib = egl::DynamicInstance<egl::EGL1_5>;

/// Current on the thread that made it, until the process ends.
pub struct Headless {
    /// Leaked, because the display and the context are never destroyed,
    /// and the library they live in must not be unloaded under them.
    lib: &'static Lib,
    pub gl: glow::Context,
}

impl Headless {
    /// `None` without an EGL that offers Mesa's surfaceless platform, as
    /// Mesa and NVIDIA's driver do.
    pub fn new() -> Option<Self> {
        // SAFETY: libEGL is the system's EGL, which implements the API.
        let lib: &'static Lib = Box::leak(Box::new(unsafe { Lib::load_required() }.ok()?));
        // SAFETY: the surfaceless platform takes no native display.
        let display = unsafe {
            lib.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        }
        .ok()?;
        lib.initialize(display).ok()?;
        lib.bind_api(egl::OPENGL_ES_API).ok()?;
        let config = lib
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    egl::PBUFFER_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES3_BIT,
                    egl::NONE,
                ],
            )
            .ok()??;
        let context = lib
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_MAJOR_VERSION, 3, egl::NONE],
            )
            .ok()?;
        lib.make_current(display, None, None, Some(context)).ok()?;
        // SAFETY: the context was just made current.
        let gl = unsafe { glow::Context::from_loader_function(|name| load(lib, name)) };
        Some(Self { lib, gl })
    }

    /// Looks up a GL function by name, for `Gpu::new`.
    pub fn load(&self, name: &str) -> *const c_void {
        load(self.lib, name)
    }

    /// Binds a new framebuffer, `w` × `h` pixels, which every draw goes into
    /// from then on.
    pub fn framebuffer(&self, (w, h): (u32, u32)) -> Result<(), String> {
        let gl = &self.gl;
        // SAFETY: the context is current.
        unsafe {
            let rb = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w as i32, h as i32);
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(rb),
            );
        }
        Ok(())
    }
}

fn load(lib: &Lib, name: &str) -> *const c_void {
    lib.get_proc_address(name)
        .map_or(std::ptr::null(), |f| f as *const c_void)
}
