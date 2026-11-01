//! Evaluates the signed distance at the pixels a shape may cover only
//! partly, exactly as the GPU's fragment shader does, so that its output is
//! the reference the GPU is tested against, and the fallback where there is
//! no GPU. Where a shape says a stretch of a row is solid, or empty, the
//! answer is known without the distance. That saves nearly all the work,
//! because a panel is almost all solid background. The tests compare every
//! pixel with the plain evaluation of every pixel, so the stretches are
//! never allowed to change a byte.

use std::ops::Range;

use crate::scene::{Point, Rgba};
use crate::shapes::Shape;

/// `px` is premultiplied BGRA, because that is the byte layout of `wl_shm`'s
/// ARGB8888 on little-endian machines: the engine draws straight into a
/// shared-memory buffer without converting it.
///
/// # Panics
///
/// If `px` is shorter than `w * h * 4` bytes.
pub fn draw(px: &mut [u8], w: usize, h: usize, scale: f32, shapes: &[Box<dyn Shape>]) {
    // Whole pixels keep byte offsets out of the loops below.
    let (px, _) = px[..w * h * 4].as_chunks_mut::<4>();
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
        start..end
    };
    // A pixel is fully covered, or not at all, once its centre is half a
    // physical pixel from the edge. The extra 1/64 of a pixel keeps rounding
    // in `inside` and `hole` from claiming a pixel that `distance` covers to
    // 0.9999; such a pixel takes the slow way instead.
    let depth = (0.5 + 1.0 / 64.0) / scale;
    let centres = |stretch: Option<[f32; 2]>, columns: &Range<usize>| match stretch {
        Some([a, b]) => {
            let start = (a * scale - 0.5).ceil().max(columns.start as f32) as usize;
            let start = start.min(columns.end);
            let end = ((b * scale - 0.5).floor() + 1.0).clamp(start as f32, columns.end as f32);
            start..end as usize
        }
        None => columns.start..columns.start,
    };
    let boxes: Vec<_> = shapes
        .iter()
        .map(|s| {
            let [p0, p1] = s.bounds();
            (span(p0.x, p1.x, w), span(p0.y, p1.y, h))
        })
        .collect();

    // Row by row, with every shape that crosses the row, so that the row
    // stays in the cache while they are drawn on it. Shape by shape over the
    // whole buffer, a frame would pass through memory once for every
    // full-screen shape in it.
    for (y, row) in px.chunks_exact_mut(w).enumerate() {
        // Shared-memory pools hand back buffers that still hold an earlier
        // frame.
        row.fill([0; 4]);
        // Pixels are sampled at their centres, so edges on whole logical
        // pixels stay crisp at whole-number scales.
        let py = (y as f32 + 0.5) / scale;
        for (s, (columns, rows)) in shapes.iter().zip(&boxes) {
            if !rows.contains(&y) {
                continue;
            }
            let c = s.color();
            let opacity = c.a as f32 / 255.0;
            let solid = centres(s.inside(py, depth), columns);
            let hole = centres(s.hole(py, depth), columns);
            for x in around(columns.clone(), [solid.clone(), hole]) {
                let p = Point::new((x as f32 + 0.5) / scale, py);
                let alpha = cover(s.distance(p)) * opacity;
                if alpha > 0.0 {
                    blend(&mut row[x], c, alpha);
                }
            }
            fill(&mut row[solid], c);
        }
    }
}

/// Premultiplied source-over, because it needs no division per pixel.
/// Channels stay in 0..=255 so blending needs no rescaling either.
fn blend(bytes: &mut [u8; 4], c: Rgba, alpha: f32) {
    let keep = 1.0 - alpha;
    let mix = |new: u8, curr: u8| (new as f32 * alpha + curr as f32 * keep + 0.5) as u8;
    *bytes = [
        mix(c.b, bytes[0]),
        mix(c.g, bytes[1]),
        mix(c.r, bytes[2]),
        mix(255, bytes[3]),
    ];
}

/// Only for pixels the shape covers fully, because it takes the colour's
/// own opacity as the alpha of every one of them.
fn fill(pixels: &mut [[u8; 4]], c: Rgba) {
    if c.a == 255 {
        // Nothing of what was underneath remains, which is what `blend`
        // computes, only slower.
        pixels.fill([c.b, c.g, c.r, 255]);
        return;
    }
    // In integers, which the compiler vectorises where it does not
    // vectorise `blend`: a full-screen translucent layer takes 3 ms instead
    // of 18. With one alpha for all pixels, the results are exactly those of
    // `blend`, which a test checks for every byte.
    let a = u16::from(c.a);
    let src = [c.b, c.g, c.r, 255].map(|v| u16::from(v) * a);
    for p in pixels {
        for k in 0..4 {
            p[k] = div255(src[k] + u16::from(p[k]) * (255 - a));
        }
    }
}

/// `x / 255`, rounded half up, without a division; exact for every `x` up
/// to 255 * 255.
fn div255(x: u16) -> u8 {
    let x = x + 128;
    ((x + (x >> 8)) >> 8) as u8
}

/// The pixels of `row` outside both cuts. Either cut may be empty, and an
/// empty one can sit anywhere, even at the start of the other; ordering
/// them by their start alone would then send pixels of the solid stretch
/// through `distance` as well and blend them twice.
fn around(row: Range<usize>, mut cuts: [Range<usize>; 2]) -> impl Iterator<Item = usize> {
    cuts.sort_by_key(|cut| cut.start);
    let [a, b] = cuts;
    let after_a = a.end.max(row.start);
    (row.start..a.start)
        .chain(after_a..b.start.max(after_a))
        .chain(b.end.max(after_a)..row.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Rgba, Scene};
    use crate::shapes::fixtures::{Dice, random_shape};
    use crate::shapes::{Circle, Rect};

    /// A fresh buffer for every query, so that no test can depend on what
    /// another left behind.
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

    /// Every pixel of the buffer through `distance`, with nothing skipped and
    /// no bounds: the definition of what `draw` produces.
    fn reference(px: &mut [u8], w: usize, scale: f32, shapes: &[Box<dyn Shape>]) {
        px.fill(0);
        let (px, _) = px.as_chunks_mut::<4>();
        for s in shapes {
            let c = s.color();
            let [red, green, blue] = [c.r, c.g, c.b].map(f32::from);
            let opacity = c.a as f32 / 255.0;
            for (i, bytes) in px.iter_mut().enumerate() {
                let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                let d = s.distance(Point::new(x / scale, y / scale));
                let alpha = (0.5 - d * scale).clamp(0.0, 1.0) * opacity;
                if alpha <= 0.0 {
                    continue;
                }
                let keep = 1.0 - alpha;
                let mix = |new: f32, curr: u8| (new * alpha + curr as f32 * keep + 0.5) as u8;
                *bytes = [
                    mix(blue, bytes[0]),
                    mix(green, bytes[1]),
                    mix(red, bytes[2]),
                    (alpha * 255.0 + bytes[3] as f32 * keep + 0.5) as u8,
                ];
            }
        }
    }

    #[test]
    fn integer_blending_rounds_exactly_as_float_blending() {
        let under: Vec<[u8; 4]> = (0..=255).map(|v| [v; 4]).collect();
        for a in 0..=255 {
            for v in 0..=255 {
                let c = Rgba {
                    r: v,
                    g: v,
                    b: v,
                    a,
                };
                let mut filled = under.clone();
                fill(&mut filled, c);
                let mut blended = under.clone();
                blended
                    .iter_mut()
                    .for_each(|p| blend(p, c, a as f32 / 255.0));
                assert_eq!(filled, blended, "colour {v}, opacity {a}");
            }
        }
    }

    #[test]
    fn around_leaves_out_both_cuts_however_they_lie() {
        let pixels = |cuts| around(0..10, cuts).collect::<Vec<_>>();
        assert_eq!(pixels([2..4, 6..8]), [0, 1, 4, 5, 8, 9]);
        assert_eq!(pixels([0..10, 0..0]), []);
        assert_eq!(pixels([0..0, 0..10]), []);
        assert_eq!(pixels([2..8, 5..5]), [0, 1, 8, 9]);
        assert_eq!(pixels([2..6, 4..8]), [0, 1, 8, 9]);
        assert_eq!(pixels([0..0, 0..0]), (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn solid_and_empty_stretches_change_no_pixel() {
        let mut dice = Dice(0x9e37_79b9);
        // The wide strip puts edges thousands of pixels from the origin,
        // where `f32` rounds the most.
        for (w, h) in [(37, 23), (64, 48), (3900, 3)] {
            for scale in [1.0, 1.25, 1.5, 2.0, 3.0] {
                for _ in 0..40 {
                    let size = (w as f32 / scale, h as f32 / scale);
                    let shapes: Vec<_> = (0..4).map(|_| random_shape(&mut dice, size)).collect();
                    let (mut fast, mut slow) = (vec![0; w * h * 4], vec![0; w * h * 4]);
                    draw(&mut fast, w, h, scale, &shapes);
                    reference(&mut slow, w, scale, &shapes);
                    let (fast, slow) = (fast.as_chunks::<4>().0, slow.as_chunks::<4>().0);
                    if let Some(i) = (0..w * h).find(|&i| fast[i] != slow[i]) {
                        panic!(
                            "{w}x{h} at {scale}, pixel ({}, {}): {:?} where the reference has {:?}",
                            i % w,
                            i / w,
                            fast[i],
                            slow[i]
                        );
                    }
                }
            }
        }
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
        // Half opacity is stored as 128/255, a hair over half, so slightly
        // less of the blue shows through.
        assert_eq!(at(&s, (10, 10), 1.0, (5, 5)), [127, 0, 128, 255]);
    }

    #[test]
    fn an_opaque_shape_replaces_what_is_underneath() {
        // The middle lies in the solid stretch, where the colour is written
        // without blending; this is the result blending has to agree with.
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
