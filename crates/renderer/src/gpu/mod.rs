//! OpenGL ES 3.0, on whichever context is current, so that the engine can
//! draw into a Wayland surface and the tests into a framebuffer without a
//! display. Every shape becomes one instance of a quad over its box, and the
//! fragment shader evaluates its [`Sdf`](crate::shapes::Sdf) at each pixel's
//! centre, with the same coverage as [`cpu::draw`](crate::cpu::draw), so
//! that both backends draw the same pixels to within rounding.
//!
//! A scene with a shape the shader does not know is drawn by the CPU and
//! shown with [`Gpu::draw_image`]. Handing the surface back to the CPU
//! instead would mean committing a shared-memory buffer to it, which a
//! compositor with explicit sync rejects once EGL has drawn into it.

use std::ffi::c_void;

use glow::HasContext;

use crate::shapes::Shape;

const SHAPE: [&str; 2] = [include_str!("shape.vert"), include_str!("shape.frag")];
const IMAGE: [&str; 2] = [include_str!("image.vert"), include_str!("image.frag")];

/// The layout of the attributes `new` declares, field by field.
#[repr(C)]
#[derive(Clone, Copy)]
struct Instance {
    rect: [f32; 4],
    radii: [f32; 4],
    /// The cut's centre and radius, and the outline's width.
    cut: [f32; 4],
    /// The edge's normal and offset, and the corners' radius, which is
    /// below 0 for a shape that melts into nothing.
    melt: [f32; 4],
    /// The melted shape's clip, as its corners.
    clip: [f32; 4],
    /// The waves on its edge: centre, amplitude, group and phase, with an
    /// amplitude of 0 for none.
    wave: [f32; 4],
    /// The neck's `from` and `to`, each as x and y.
    neck: [f32; 4],
    /// The neck's half width at its ends and in its middle, and its blend,
    /// with a fourth value of 0 for no neck, and for a neck 1 more than its
    /// cut, which is never negative.
    more: [f32; 4],
    /// The bulge's centre and height, 0 for none, and the edge's extent.
    bulge: [f32; 4],
    color: [u8; 4],
}

/// A shape without [`Shape::sdf`] is in the scene. Nothing was drawn, so
/// the caller can draw the frame on the CPU and show it with
/// [`Gpu::draw_image`].
#[derive(Debug, PartialEq)]
pub struct Unsupported;

/// Frees nothing when dropped: its GL objects go with the context.
pub struct Gpu {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    buffer: glow::Buffer,
    size: Option<glow::UniformLocation>,
    scale: Option<glow::UniformLocation>,
    /// Kept between frames so that its allocation is reused.
    instances: Vec<Instance>,
    image: glow::Program,
    /// Without attributes, because the image program takes none, while the
    /// shapes' array would feed it their instances.
    image_vao: glow::VertexArray,
    image_height: Option<glow::UniformLocation>,
    texture: glow::Texture,
    /// Reallocated only when the size changes.
    texture_size: (u32, u32),
}

/// # Safety
///
/// `gl` has to be current on this thread.
unsafe fn compile(
    gl: &glow::Context,
    [vertex, fragment]: [&str; 2],
) -> Result<glow::Program, String> {
    // SAFETY: the caller keeps the context current.
    unsafe {
        let program = gl.create_program()?;
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            // Only marked for deletion: the program keeps it.
            gl.delete_shader(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        Ok(program)
    }
}

impl Gpu {
    /// Compiles the shaders. `load` looks up a GL function by name, as
    /// `eglGetProcAddress` does.
    ///
    /// # Safety
    ///
    /// The context `load` belongs to has to be current on this thread now,
    /// and whenever the returned `Gpu` is used.
    pub unsafe fn new(load: impl FnMut(&str) -> *const c_void) -> Result<Self, String> {
        // SAFETY: the caller keeps the context current, which every call
        // below needs.
        unsafe {
            let gl = glow::Context::from_loader_function(load);
            let program = compile(&gl, SHAPE)?;
            let image = compile(&gl, IMAGE)?;
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let buffer = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            let stride = size_of::<Instance>() as i32;
            for (index, size, kind, offset) in [
                (0, 4, glow::FLOAT, 0),
                (1, 4, glow::FLOAT, 16),
                (2, 4, glow::FLOAT, 32),
                (3, 4, glow::UNSIGNED_BYTE, 144),
                (4, 4, glow::FLOAT, 48),
                (5, 4, glow::FLOAT, 64),
                (6, 4, glow::FLOAT, 80),
                (7, 4, glow::FLOAT, 96),
                (8, 4, glow::FLOAT, 112),
                (9, 4, glow::FLOAT, 128),
            ] {
                gl.enable_vertex_attrib_array(index);
                // Normalised, so that the colour bytes arrive as 0..1.
                let normalised = kind == glow::UNSIGNED_BYTE;
                gl.vertex_attrib_pointer_f32(index, size, kind, normalised, stride, offset);
                gl.vertex_attrib_divisor(index, 1);
            }
            let image_vao = gl.create_vertex_array()?;
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            // Without mipmaps a texture is only complete with a filter that
            // needs none; `texelFetch` ignores the filter itself.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            Ok(Self {
                size: gl.get_uniform_location(program, "u_size"),
                scale: gl.get_uniform_location(program, "u_scale"),
                image_height: gl.get_uniform_location(image, "u_height"),
                gl,
                program,
                vao,
                buffer,
                instances: Vec::new(),
                image,
                image_vao,
                texture,
                texture_size: (0, 0),
            })
        }
    }

    /// The GPU and its driver, as `GL_RENDERER` names them, for the log.
    pub fn device(&self) -> String {
        // SAFETY: `new`'s caller keeps the context current.
        unsafe { self.gl.get_parameter_string(glow::RENDERER) }
    }

    /// Clears the framebuffer bound now, `w` × `h` physical pixels, and
    /// draws the shapes into it, as [`cpu::draw`](crate::cpu::draw) does
    /// into its buffer.
    pub fn draw(
        &mut self,
        w: u32,
        h: u32,
        scale: f32,
        shapes: &[Box<dyn Shape>],
    ) -> Result<(), Unsupported> {
        self.instances.clear();
        for shape in shapes {
            let sdf = shape.sdf().ok_or(Unsupported)?;
            let c = shape.color();
            let (melt, clip, wave, neck, more, bulge) = match sdf.melt {
                Some(m) => (
                    [m.normal.x, m.normal.y, m.offset, m.radius],
                    [m.clip[0].x, m.clip[0].y, m.clip[1].x, m.clip[1].y],
                    m.wave
                        .map_or([0.0; 4], |w| [w.center, w.amplitude, w.group, w.phase]),
                    m.neck
                        .map_or([0.0; 4], |n| [n.from.x, n.from.y, n.to.x, n.to.y]),
                    m.neck.map_or([0.0; 4], |n| {
                        [n.end, n.waist, n.blend, 1.0 + n.cut.max(0.0)]
                    }),
                    {
                        let [lo, hi] = m.extent;
                        m.bulge
                            .map_or([0.0, 0.0, lo, hi], |b| [b.center, b.height, lo, hi])
                    },
                ),
                None => (
                    [0.0, 0.0, 0.0, -1.0],
                    [0.0; 4],
                    [0.0; 4],
                    [0.0; 4],
                    [0.0; 4],
                    [0.0; 4],
                ),
            };
            self.instances.push(Instance {
                rect: [sdf.center.x, sdf.center.y, sdf.half.x, sdf.half.y],
                radii: sdf.radii,
                cut: [sdf.cut.x, sdf.cut.y, sdf.cut_radius, sdf.outline],
                melt,
                clip,
                wave,
                neck,
                more,
                bulge,
                color: [c.r, c.g, c.b, c.a],
            });
        }
        // SAFETY: `Instance` is `repr(C)` from 4-byte fields, so it has no
        // padding, and every byte of it is initialised.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                self.instances.as_ptr().cast::<u8>(),
                size_of_val(self.instances.as_slice()),
            )
        };
        let gl = &self.gl;
        // SAFETY: `new`'s caller keeps the context current.
        unsafe {
            gl.viewport(0, 0, w as i32, h as i32);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            if self.instances.is_empty() {
                return Ok(());
            }
            gl.enable(glow::BLEND);
            // Premultiplied source-over, as the CPU rasteriser blends.
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.uniform_2_f32(self.size.as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(self.scale.as_ref(), scale);
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.buffer));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            // One draw call for the whole scene: OpenGL blends the instances
            // in order, which keeps the scene's back-to-front order.
            gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, self.instances.len() as i32);
        }
        Ok(())
    }

    /// Copies `px`, a frame as [`cpu::draw`](crate::cpu::draw) leaves it,
    /// into the framebuffer bound now, unchanged.
    ///
    /// # Panics
    ///
    /// If `px` is shorter than `w * h * 4` bytes.
    pub fn draw_image(&mut self, w: u32, h: u32, px: &[u8]) {
        let px = &px[..(w * h * 4) as usize];
        let gl = &self.gl;
        // SAFETY: `new`'s caller keeps the context current, and `px` holds
        // the w × h pixels the upload reads.
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            let data = glow::PixelUnpackData::Slice(Some(px));
            if self.texture_size == (w, h) {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    w as i32,
                    h as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    data,
                );
            } else {
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w as i32,
                    h as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    data,
                );
                self.texture_size = (w, h);
            }
            gl.viewport(0, 0, w as i32, h as i32);
            // A copy, not a blend: the frame is complete as it is.
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.image));
            gl.uniform_1_f32(self.image_height.as_ref(), h as f32);
            gl.bind_vertex_array(Some(self.image_vao));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }
}

#[cfg(test)]
mod headless;

#[cfg(test)]
mod tests {
    use super::headless::Headless;
    use super::*;
    use crate::cpu;
    use crate::scene::{Point, Scene};
    use crate::shapes::fixtures::{Dice, circle, random_shape, white};
    use crate::shapes::{Rect, Shape};

    /// A GPU that draws into a framebuffer, with no display.
    struct Target {
        headless: Headless,
        gpu: Gpu,
        size: (u32, u32),
    }

    /// `None` without an EGL that offers Mesa's surfaceless platform. The
    /// tests then pass without checking anything, and the desktop tests,
    /// which CI runs with Mesa, check the GPU instead. Shaders that don't
    /// compile are a failure, not a skip, or a broken shader would pass.
    fn target(size: (u32, u32)) -> Option<Target> {
        let headless = Headless::new()?;
        headless.framebuffer(size).ok()?;
        // SAFETY: `Headless::new` made its context current, and the tests
        // that use it run on this thread only, each in a process of its own.
        let gpu = unsafe { Gpu::new(|name| headless.load(name)) }
            .unwrap_or_else(|e| panic!("the shaders do not compile: {e}"));
        Some(Target {
            headless,
            gpu,
            size,
        })
    }

    impl Target {
        fn draw(&mut self, scale: f32, shapes: &[Box<dyn Shape>]) -> Result<Vec<u8>, Unsupported> {
            let (w, h) = self.size;
            self.gpu.draw(w, h, scale, shapes)?;
            Ok(self.read())
        }

        fn show(&mut self, px: &[u8]) -> Vec<u8> {
            let (w, h) = self.size;
            self.gpu.draw_image(w, h, px);
            self.read()
        }

        /// In `cpu::draw`'s layout, premultiplied BGRA from the top row
        /// down, so that the two compare byte by byte.
        fn read(&self) -> Vec<u8> {
            let (w, h) = self.size;
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            // SAFETY: the context is current, and the buffer holds w × h
            // RGBA pixels.
            unsafe {
                self.headless.gl.read_pixels(
                    0,
                    0,
                    w as i32,
                    h as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut rgba)),
                );
            }
            // OpenGL's rows run bottom to top.
            let row = (w * 4) as usize;
            rgba.chunks_exact(row)
                .rev()
                .flat_map(|r| {
                    r.as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|p| [p[2], p[1], p[0], p[3]])
                })
                .collect()
        }
    }

    #[test]
    fn the_gpu_draws_what_the_cpu_draws() {
        let Some(mut gpu) = target((64, 48)) else {
            eprintln!("skipped: no EGL with Mesa's surfaceless platform");
            return;
        };
        let (w, h) = (64, 48);
        let mut dice = Dice(0x51ed_270b);
        let mut worst = 0;
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
            for _ in 0..40 {
                let mut scene = Scene::new();
                let logical = (w as f32 / scale, h as f32 / scale);
                for _ in 0..1 + (dice.roll() * 6.0) as usize {
                    scene.shapes.push(random_shape(&mut dice, logical));
                }
                let mut cpu = vec![0u8; w * h * 4];
                cpu::draw(&mut cpu, w, h, scale, &scene.shapes);
                let gpu = gpu.draw(scale, &scene.shapes).unwrap();
                for (i, (a, b)) in cpu.iter().zip(&gpu).enumerate() {
                    let off = a.abs_diff(*b);
                    worst = worst.max(off);
                    assert!(
                        off <= 2,
                        "scale {scale}, pixel ({}, {}), channel {}: {a} on the CPU, {b} on the GPU",
                        i / 4 % w,
                        i / 4 / w,
                        i % 4
                    );
                }
            }
        }
        eprintln!("largest difference: {worst}, on {}", gpu.gpu.device());
    }

    #[test]
    fn an_image_from_the_cpu_shows_unchanged() {
        let Some(mut gpu) = target((64, 48)) else {
            eprintln!("skipped: no EGL with Mesa's surfaceless platform");
            return;
        };
        let mut dice = Dice(0x2545_f491);
        let mut scene = Scene::new();
        for _ in 0..8 {
            scene.shapes.push(random_shape(&mut dice, (32.0, 24.0)));
        }
        let mut px = vec![0u8; 64 * 48 * 4];
        cpu::draw(&mut px, 64, 48, 2.0, &scene.shapes);
        assert_eq!(gpu.show(&px), px);
    }

    #[test]
    fn a_shape_without_an_sdf_leaves_the_frame_to_the_cpu() {
        let Some(mut gpu) = target((16, 16)) else {
            eprintln!("skipped: no EGL with Mesa's surfaceless platform");
            return;
        };
        let nested: Box<dyn Shape> = Box::new(circle().outline(2.0, white()).outline(1.0, white()));
        let plain: Box<dyn Shape> = Box::new(Rect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(8.0, 8.0),
            color: white(),
        });
        assert_eq!(gpu.draw(1.0, &[plain, nested]), Err(Unsupported));
    }
}
