
use crate::renderer::scene::{Point, Shape};

/// Draw `shapes` into `px` (premultiplied BGRA, `w`×`h` physical pixels,
/// cleared first), `scale` physical pixels a logical one.
pub fn draw(px: &mut [u8], w: usize, h: usize, scale: f32, shapes: &[Box<dyn Shape>]) {
    px.fill(0);
    // The bytes as pixels, four each: B, G, R, A.
    let (px, _) = px.as_chunks_mut::<4>();
    // A pixel is covered as far as the edge is past its middle: half a
    // pixel either way blends.
    let cover = |d: f32| (0.5 - d * scale).clamp(0.0, 1.0);
    // The pixels whose middles are in lo..hi (and a pixel more).
    let pad = 1.0 / scale;
    let span = |lo: f32, hi: f32, n: usize| {
        (
            ((lo - pad) * scale - 0.5).ceil().max(0.0) as usize,
            ((((hi + pad) * scale - 0.5).ceil()).max(0.0) as usize).min(n),
        )
    };

    for s in shapes {
        let [p0, p1] = s.bounds();
        let (x0, x1) = span(p0.x, p1.x, w);
        let (y0, y1) = span(p0.y, p1.y, h);

        let c = s.color();
        let [red, green, blue] = [c.r, c.g, c.b].map(|v| v as f32 / 255.0);

        for y in y0..y1 {
            let py = (y as f32 + 0.5) / scale;
            let row = &mut px[y * w + x0..y * w + x1];
            for (x, bytes) in (x0..).zip(row) {
                let p = Point::new((x as f32 + 0.5) / scale, py);
                let alpha = cover(s.distance(p)) * c.a;
                if alpha <= 0.0 {
                    continue;
                }

                // Blend color with current color based on alpha
                let keep = 1.0 - alpha;
                let blend = |new: f32, curr: u8| (new * alpha * 255.0 + curr as f32 * keep + 0.5) as u8;
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

    #[test]
    fn inside_a_box_its_colour_outside_nothing() {
        let mut s = Scene::new();
        s.push(rect(2.0, 2.0, 8.0, 8.0, Rgba::from_hex(0xff8000, 1.0)));
        assert_eq!(at(&s, (10, 10), 1.0, (4, 4)), [0, 128, 255, 255]);
        assert_eq!(at(&s, (10, 10), 1.0, (9, 9)), [0, 0, 0, 0]);
    }

    #[test]
    fn an_edge_through_a_pixel_covers_half_of_it() {
        let mut s = Scene::new();
        // Its left edge through the middle of pixel 2.
        s.push(rect(2.5, 0.0, 7.5, 10.0, Rgba::from_hex(0xffffff, 1.0)));
        assert_eq!(at(&s, (10, 10), 1.0, (2, 5))[3], 128);
    }

    #[test]
    fn half_see_through_blends_with_what_is_under_it() {
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0x0000ff, 1.0)));
        s.push(rect(0.0, 0.0, 10.0, 10.0, Rgba::from_hex(0xff0000, 0.5)));
        // Premultiplied: half red over blue.
        assert_eq!(at(&s, (10, 10), 1.0, (5, 5)), [128, 0, 128, 255]);
    }

    #[test]
    fn at_twice_the_scale_it_is_twice_as_big() {
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 4.0, 4.0, Rgba::from_hex(0xffffff, 1.0)));
        assert_eq!(at(&s, (10, 10), 2.0, (7, 7))[3], 255);
        assert_eq!(at(&s, (10, 10), 2.0, (8, 8))[3], 0);
    }

    #[test]
    fn a_ring_is_empty_in_its_middle() {
        let mut s = Scene::new();
        let circle = Circle {
            center: Point::new(10.0, 10.0),
            r: 8.0,
            color: Rgba::from_hex(0xffffff, 0.0),
        };
        s.push(circle.outline(2.0, Rgba::from_hex(0xffffff, 1.0)));
        assert_eq!(at(&s, (20, 20), 1.0, (10, 10))[3], 0);
        assert_eq!(at(&s, (20, 20), 1.0, (10, 2))[3], 255);
    }
}
