//! A [`Picture`] is drawn by the CPU rasteriser, so that what the renderer
//! draws can be checked without a display or a GPU. It is stored as PNG, so
//! that a reference image can be committed and opened in any viewer.

use png;
use std::path::Path;

use crate::scene::{Rgba, Shape};

/// RGBA rather than the rasteriser's BGRA, because that is what PNG stores.
/// Premultiplied, because that is what the compositor shows. PNG means
/// straight alpha, so a viewer shows translucent pixels too dark unless the
/// picture is put [`over`](Picture::over) an opaque ground first.
pub struct Picture {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Picture {
    /// The physical size is rounded down, as the compositor does, so that a
    /// picture has the size of the buffer a surface would get.
    pub fn paint(size: (f32, f32), scale: f32, shapes: &[Box<dyn Shape>]) -> Self {
        let (w, h) = ((size.0 * scale) as u32, (size.1 * scale) as u32);
        let mut px = vec![0u8; w as usize * h as usize * 4];
        crate::cpu::draw(&mut px, w as usize, h as usize, scale, shapes);
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        Self { w, h, px }
    }

    /// Over an opaque ground every pixel comes out opaque, where
    /// premultiplied and straight alpha agree, so that the saved PNG looks
    /// right in any viewer.
    pub fn over(mut self, ground: Rgba) -> Self {
        // Products of two bytes reach 255 * 255, hence `u16`.
        let a = ground.a as u16;
        let g = [ground.r, ground.g, ground.b, 255].map(|c| ((c as u16 * a + 127) / 255) as u8);

        for b in self.px.as_chunks_mut::<4>().0 {
            let inv = 255 - b[3] as u16;
            for i in 0..4 {
                b[i] = (b[i] as u16 + (g[i] as u16 * inv + 127) / 255) as u8;
            }
        }
        self
    }

    /// Creates missing directories, so that a new reference image can be
    /// saved where it belongs.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = png::Encoder::new(file, self.w, self.h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // PNG errors become I/O errors, so callers handle a single kind.
        let mut w = encoder.write_header().map_err(std::io::Error::other)?;
        w.write_image_data(&self.px).map_err(std::io::Error::other)
    }

    /// Anything other than 8-bit RGBA is refused rather than converted: a
    /// reference image in another format has been re-saved by some other
    /// tool, and comparing against a conversion of it would hide that.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?));
        let mut reader = decoder.read_info().map_err(std::io::Error::other)?;
        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader.next_frame(&mut buf).map_err(std::io::Error::other)?;
        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return Err(std::io::Error::other(format!(
                "{}: not 8-bit RGBA",
                path.display()
            )));
        }

        Ok(Self {
            w: info.width,
            h: info.height,
            px: buf,
        })
    }

    /// Returns the largest channel difference and the number of pixels off
    /// by more than `tolerance`, rather than a verdict, so that a failing
    /// check can say whether a few pixels are badly off or many slightly. The
    /// tolerance exists because a later backend, or a change of rounding, may
    /// move a channel by a step without anything looking different.
    ///
    /// `None` if the sizes differ, since then no pixel has a counterpart.
    pub fn diff(&self, other: &Picture, tolerance: u8) -> Option<(u8, usize)> {
        if self.w != other.w || self.h != other.h {
            return None;
        }

        let mut worst = 0;
        let mut n_off = 0;

        for (a, b) in self
            .px
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.px.as_chunks::<4>().0)
        {
            // A pixel is as far off as its worst channel.
            let d = a
                .iter()
                .zip(b)
                .map(|(byte_a, byte_b)| byte_a.abs_diff(*byte_b))
                .max()
                .unwrap_or(0);
            worst = worst.max(d);
            if d > tolerance {
                n_off += 1
            };
        }

        Some((worst, n_off))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Circle, Point, Rect, Scene};

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba) -> Rect {
        Rect {
            p0: Point::new(x0, y0),
            p1: Point::new(x1, y1),
            color,
        }
    }

    /// A 1×1 picture holding one premultiplied RGBA pixel, so that tests of
    /// `over` can state their input exactly instead of drawing it.
    fn pixel(px: [u8; 4]) -> Picture {
        Picture {
            w: 1,
            h: 1,
            px: px.to_vec(),
        }
    }

    #[test]
    fn a_picture_saved_reads_back_the_same() {
        let mut s = Scene::new();
        s.push(Circle {
            center: Point::new(8.0, 8.0),
            r: 6.0,
            color: Rgba::from_hex(0xc9a96e, 1.0),
        });
        let p = Picture::paint((16.0, 16.0), 2.0, &s.shapes);
        assert_eq!((p.w, p.h), (32, 32));
        let path = std::env::temp_dir().join(format!("tonish-{}.png", std::process::id()));
        p.save(&path).unwrap();
        let back = Picture::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.diff(&p, 0), Some((0, 0)));
    }

    #[test]
    fn pictures_apart_say_how_far() {
        let mut s = Scene::new();
        let a = Picture::paint((4.0, 4.0), 1.0, &s.shapes);
        s.push(rect(0.0, 0.0, 1.0, 1.0, Rgba::from_hex(0xffffff, 1.0)));
        let b = Picture::paint((4.0, 4.0), 1.0, &s.shapes);
        assert_eq!(a.diff(&b, 2), Some((255, 1)), "one pixel");
        let small = Picture::paint((2.0, 2.0), 1.0, &s.shapes);
        assert_eq!(a.diff(&small, 2), None, "not the same size");
    }

    #[test]
    fn paint_sizes_the_picture_in_physical_pixels() {
        let p = Picture::paint((2.0, 3.0), 2.0, &[]);
        assert_eq!((p.w, p.h), (4, 6));
        assert_eq!(p.px.len(), 4 * 6 * 4);
    }

    #[test]
    fn paint_stores_rgba_not_the_rasterisers_bgra() {
        // PNG wants red first; the rasteriser writes blue first.
        let mut s = Scene::new();
        s.push(rect(0.0, 0.0, 2.0, 2.0, Rgba::from_hex(0xff8000, 1.0)));
        let p = Picture::paint((2.0, 2.0), 1.0, &s.shapes);
        assert!(
            p.px.as_chunks::<4>()
                .0
                .iter()
                .all(|&c| c == [255, 128, 0, 255])
        );
    }

    #[test]
    fn over_fills_empty_pixels_with_the_ground() {
        let p = pixel([0, 0, 0, 0]).over(Rgba::from_hex(0x0000ff, 1.0));
        assert_eq!(p.px, [0, 0, 255, 255]);
    }

    #[test]
    fn over_leaves_opaque_pixels_alone() {
        let p = pixel([10, 20, 30, 255]).over(Rgba::from_hex(0x0000ff, 1.0));
        assert_eq!(p.px, [10, 20, 30, 255]);
    }

    #[test]
    fn over_shows_the_ground_through_translucent_pixels() {
        let p = pixel([128, 0, 0, 128]).over(Rgba::from_hex(0x0000ff, 1.0));
        assert_eq!(p.px, [128, 0, 127, 255]);
    }

    #[test]
    fn over_a_translucent_ground_stays_translucent_and_premultiplied() {
        // Straight alpha would store 255 for blue and look too bright.
        let p = pixel([0, 0, 0, 0]).over(Rgba::from_hex(0x0000ff, 0.5));
        assert_eq!(p.px, [0, 0, 128, 128]);
    }

    #[test]
    fn over_a_transparent_ground_changes_nothing() {
        let p = pixel([128, 0, 0, 128]).over(Rgba::from_hex(0x0000ff, 0.0));
        assert_eq!(p.px, [128, 0, 0, 128]);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(Picture::load(Path::new("/nonexistent/tonish.png")).is_err());
    }
}
