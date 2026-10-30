//! The CPU rasteriser.
//!
//! It evaluates every shape's signed distance at every pixel the shape may
//! cover, exactly as a GPU fragment shader would. That keeps its output a
//! faithful reference for a future GPU backend and a fallback where no GPU is
//! available. It is written to be obviously correct rather than fast; the
//! tests below pin its behaviour so that optimisations, such as filling the
//! solid interior of opaque shapes directly, can be checked against them.

use crate::renderer::scene::{Point, Shape};

/// Rasterises `shapes` into `px`, back to front.
///
/// `px` holds `w`×`h` physical pixels of premultiplied BGRA. That is the byte
/// layout of `wl_shm`'s ARGB8888 on little-endian machines, so the engine can
/// draw straight into a shared-memory buffer without converting it. Shapes
/// stay in logical pixels and `scale` maps them to physical ones, so layout
/// never has to know the output's scale.
///
/// The buffer is cleared first because shared-memory pools hand back buffers
/// that still hold an earlier frame.
///
/// # Panics
///
/// If `px` is shorter than `w * h * 4` bytes and a shape reaches the missing
/// part.
pub fn draw(px: &mut [u8], w: usize, h: usize, scale: f32, shapes: &[Box<dyn Shape>]) {
    px.fill(0);
    // Whole pixels keep byte offsets out of the loop below.
    let (px, _) = px.as_chunks_mut::<4>();
    // Coverage ramps across one physical pixel centred on the edge: cheap
    // antialiasing without supersampling.
    let cover = |d: f32| (0.5 - d * scale).clamp(0.0, 1.0);
    // Padded by a pixel so the antialiased fringe just outside the bounds is
    // not cut off, and clamped so that shapes partly or wholly off the
    // buffer are clipped instead of indexing past it.
    let pad = 1.0 / scale;
    let span = |lo: f32, hi: f32, n: usize| {
        let end = (((hi + pad) * scale - 0.5).ceil().max(0.0) as usize).min(n);
        let start = (((lo - pad) * scale - 0.5).ceil().max(0.0) as usize).min(end);
        (start, end)
    };

    for s in shapes {
        let [p0, p1] = s.bounds();
        let (x0, x1) = span(p0.x, p1.x, w);
        let (y0, y1) = span(p0.y, p1.y, h);

        // Once per shape rather than once per pixel.
        let c = s.color();
        let [red, green, blue] = [c.r, c.g, c.b].map(|v| v as f32 / 255.0);

        for y in y0..y1 {
            // Pixels are sampled at their centres, so edges on whole logical
            // pixels stay crisp at whole-number scales.
            let py = (y as f32 + 0.5) / scale;
            // Walking the slice spares a bounds check per pixel.
            let row = &mut px[y * w + x0..y * w + x1];
            for (x, bytes) in (x0..).zip(row) {
                let p = Point::new((x as f32 + 0.5) / scale, py);
                let alpha = cover(s.distance(p)) * c.a;
                if alpha <= 0.0 {
                    continue;
                }

                // Premultiplied source-over: a plain weighted sum with no
                // division per pixel. What lies underneath shows through as
                // far as this shape leaves the pixel uncovered.
                let keep = 1.0 - alpha;
                let blend =
                    |new: f32, curr: u8| (new * alpha * 255.0 + curr as f32 * keep + 0.5) as u8;
                bytes[0] = blend(blue, bytes[0]);
                bytes[1] = blend(green, bytes[1]);
                bytes[2] = blend(red, bytes[2]);
                bytes[3] = (alpha * 255.0 + bytes[3] as f32 * keep + 0.5) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::scene::{Circle, Rect, Rgba, Scene};

    /// Draws into a fresh buffer for every query, so that no test can depend
    /// on what another left behind.
    fn at(scene: &Scene, (w, h): (usize, usize), scale: f32, (x, y): (usize, usize)) -> [u8; 4] {
        let mut px = vec![0u8; w * h * 4];
        draw(&mut px, w, h, scale, &scene.shapes);
        px.as_chunks::<4>().0[y * w + x]
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba) -> Rect {
        Rect {
            p0: Point::new(x0, y0),
            p1: Point::new(x1, y1),
            color,
        }
    }

    fn white() -> Rgba {
        Rgba::from_hex(0xffffff, 1.0)
    }

    #[test]
    fn a_shape_covers_its_inside_and_nothing_outside() {
        let mut s = Scene::new();
        s.push(rect(2.0, 2.0, 8.0, 8.0, Rgba::from_hex(0xff8000, 1.0)));
        assert_eq!(at(&s, (10, 10), 1.0, (4, 4)), [0, 128, 255, 255]);
        assert_eq!(at(&s, (10, 10), 1.0, (9, 9)), [0, 0, 0, 0]);
    }

    #[test]
    fn an_edge_through_a_pixel_covers_half_of_it() {
        // A rasteriser without antialiasing would give 0 or 255.
        let mut s = Scene::new();
        s.push(rect(2.5, 0.0, 7.5, 10.0, white()));
        assert_eq!(at(&s, (10, 10), 1.0, (2, 5))[3], 128);
    }

    #[test]
    fn edges_on_whole_pixels_stay_crisp() {
        let mut s = Scene::new();
        s.push(rect(2.0, 0.0, 8.0, 10.0, white()));
        assert_eq!(at(&s, (10, 10), 1.0, (1, 5))[3], 0);
        assert_eq!(at(&s, (10, 10), 1.0, (2, 5))[3], 255);
        assert_eq!(at(&s, (10, 10), 1.0, (7, 5))[3], 255);
        assert_eq!(at(&s, (10, 10), 1.0, (8, 5))[3], 0);
    }

    #[test]
    fn translucent_pixels_are_stored_premultiplied() {
        // The compositor reads ARGB8888 as premultiplied; straight alpha
        // would store 255 for red here and show up too bright.
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0xff0000, 0.5)));
        assert_eq!(at(&s, (10, 10), 1.0, (5, 5)), [0, 0, 128, 128]);
    }

    #[test]
    fn translucent_shapes_blend_with_what_is_underneath() {
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0x0000ff, 1.0)));
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0xff0000, 0.5)));
        assert_eq!(at(&s, (10, 10), 1.0, (5, 5)), [128, 0, 128, 255]);
    }

    #[test]
    fn an_opaque_shape_replaces_what_is_underneath() {
        // A fast path for solid interiors would write the colour directly;
        // this is the result it has to match.
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0x0000ff, 1.0)));
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0xff0000, 1.0)));
        assert_eq!(at(&s, (10, 10), 1.0, (5, 5)), [0, 0, 255, 255]);
    }

    #[test]
    fn scale_changes_the_resolution_not_the_layout() {
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 4.0, 4.0, white()));
        assert_eq!(at(&s, (10, 10), 2.0, (7, 7))[3], 255);
        assert_eq!(at(&s, (10, 10), 2.0, (8, 8))[3], 0);
    }

    #[test]
    fn fractional_scales_put_edges_between_pixels() {
        // Two logical pixels at 1.25 end halfway through physical pixel 2.
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 2.0, 8.0, white()));
        assert_eq!(at(&s, (10, 10), 1.25, (2, 5))[3], 128);
    }

    #[test]
    fn an_outline_leaves_its_middle_empty() {
        let mut s = Scene::new();
        let circle = Circle {
            center: Point::new(10.0, 10.0),
            r: 8.0,
            color: Rgba::from_hex(0xffffff, 0.0),
        };
        s.push(circle.outline(2.0, white()));
        assert_eq!(at(&s, (20, 20), 1.0, (10, 10))[3], 0);
        assert_eq!(at(&s, (20, 20), 1.0, (10, 2))[3], 255);
    }

    #[test]
    fn shapes_off_the_buffer_are_clipped() {
        // Animations slide shapes across and past the edges of a surface.
        let mut s = Scene::new();
        s.push(rect(-5.0, -5.0, 3.0, 3.0, white()));
        s.push(rect(8.0, 8.0, 20.0, 20.0, white()));
        s.push(rect(30.0, 0.0, 40.0, 10.0, white()));
        s.push(rect(0.0, -20.0, 10.0, -12.0, white()));
        assert_eq!(at(&s, (10, 10), 1.0, (0, 0))[3], 255);
        assert_eq!(at(&s, (10, 10), 1.0, (9, 9))[3], 255);
        assert_eq!(at(&s, (10, 10), 1.0, (5, 5))[3], 0);
    }

    #[test]
    fn drawing_starts_from_a_clear_buffer() {
        let mut px = vec![0xff; 4 * 4 * 4];
        draw(&mut px, 4, 4, 1.0, &[]);
        assert!(px.iter().all(|&b| b == 0));
    }
}
