# Drawing

## Shapes are distances

A shape in tonish isn't a set of pixels or a path. It's a function that gives the signed distance from any point to the shape's edge: negative inside, positive outside. That one number gives three things:
- antialiased edges, from how far a pixel's centre is from the edge;
- outlines, by folding the distance into a band around the edge;
- independence from the monitor's scale, because shapes live in logical pixels.

The GPU's fragment shader works with the same number, so the CPU and the GPU draw the same scenes, see [The GPU](#the-gpu).

The shapes so far are `Rect`, `RoundedRect`, with a radius of its own at each corner, and `Circle`; `Fillet`, a concave corner, which laid between two shapes of the same colour joins them in a curve; `Liquid`, a rounded box melted into an edge, straight or with waves on it, as the settings panel into the bar; `Text`, a line of text; and `Outline`, which draws any shape as a band around its edge. Every shape implements the `Shape` trait in `crates/renderer/src/shapes/`, one file per shape:

| Method | What it gives |
|---|---|
| `bounds` | A box around the shape. The rasteriser looks at nothing outside it. |
| `color` | The shape's colour, with its opacity. |
| `distance` | The signed distance from a point to the edge. It has to be the true distance near the edge, or edges and outlines come out uneven. |
| `inside` | On one row, the stretch that is at least a given depth inside the shape. |
| `hole` | On one row, the stretch inside the bounds that is at least a given depth outside the shape, such as the middle of an outline. |
| `sdf` | The shape in the one form the GPU's shader knows. `None` leaves frames with the shape to the CPU. |

## The CPU rasteriser

`cpu::draw` paints a scene into a buffer, row by row. On each row, every shape that crosses it splits the row into three kinds of pixels:

- **Solid**, from `inside`: covered completely. An opaque colour is written straight in, and a translucent one is blended at its own opacity.
- **Empty**, from `hole`: skipped.
- **Edge**, everything else: the rasteriser computes `distance` at the pixel's centre and turns it into coverage across one physical pixel.

A panel is almost all solid background, so almost no pixel needs the distance. Drawing row by row also keeps each row in the cache while every shape is drawn on it.

`inside` and `hole` may give a shorter stretch than the true one, which only costs time, but never a longer one, which would paint over an antialiased edge. Both return `None` by default, so a new shape draws correctly before it has them, only more slowly.

The solid and empty stretches must not change a single byte of the result. [The reference test](testing.md#unit-tests) in `cpu.rs`, `solid_and_empty_stretches_change_no_pixel`, holds `draw` against the plain algorithm, every pixel of the buffer through `distance`.

## The GPU

`gpu::Gpu` in the renderer draws a scene with OpenGL ES 3.0, in one draw call. Every shape becomes an instance of a quad over its box. The fragment shader, `gpu/shape.frag`, computes the distance at each pixel's centre and turns it into coverage exactly as the CPU rasteriser does.

A shader can't call `distance`, so every shape describes itself with `sdf`, in the one form the shader knows: a box with a radius at each corner, which with every radius at half its size is a circle, melted into an edge or not, which is how a `Liquid` gets its shape, with a circle cut out of it or not, which is how a `Fillet` gets its curve, and folded into an outline or not.

A `Liquid` joins its box and its edge with hg_sdf's round union, `max(r, min(a, b)) − |max((r − a, r − b), 0)|`, rather than a smooth minimum. Where the two meet at a right angle, as a panel's side meets the bar, it is the exact distance to concave corners of radius `r`, so the shape shrunk by a border's width, with `r` grown by it, is the inside of that border, and a border keeps its width round the corners. A smooth minimum would thicken it there by up to √2. The bar's side of the edge is a half-plane, so a `Liquid` is clipped to its bounds.

The edge can carry a `Wave`: groups of crests that run apart from a point, as on water after a drop falls in, with the crests running at twice the speed of their group, so that each rises at the back of its group and fades at its front. Its distance is the height over the edge divided by the length of the slope's normal, exact on a straight edge and close enough near a crest for antialiasing and a border. Crests only rise out of the edge, never into it, because the panel is drawn over the bar and can't take from it. The shader reduces the crests' phase to one wavelength before its sine and cosine, as `Wave` does, because NVIDIA's lose precision far from 0.

## Text

`Font` in `crates/renderer/src/text.rs` sets a line of text with fontdue: each character is its font's glyph, at its advance, without shaping, which is right for the figures and labels a panel shows. It rasterises each glyph once per size, on whole physical pixels, and makes a `Text` of the coverage. A `Text`'s `distance` is not a distance: it is the value that makes the rasteriser's coverage of each pixel the glyph's own, `(0.5 − coverage) / scale`, so the CPU draws text exactly as the font made it, without a second way of drawing. The shader has no glyphs, so a frame with text goes to the CPU even on the GPU. That suits a small surface such as the figures in the corner, not a large one that animates, which will need glyphs on the GPU. `Sdf::distance` is the shader's function written again in Rust, so that the tests can check every shape's `sdf` against its `distance` without a GPU. The two have to change together.

A scene with a shape that has no `sdf`, such as an outline of an outline, is drawn by the CPU rasteriser, and the GPU shows the result as a texture. The surface stays with EGL even then: a compositor with explicit sync, such as Hyprland with NVIDIA's driver, ends the connection when a surface that EGL has drawn into gets a shared-memory buffer.

The two backends agree to within 2 per channel, which `the_gpu_draws_what_the_cpu_draws` in `gpu/mod.rs` checks, see [Testing](testing.md#unit-tests).

## The GPU's thread

The engine draws the first frame on the CPU and starts the GPU only after it, so that the panels never wait for the GPU, see [Running](../using/running.md#the-gpu). libEGL is loaded only then, and not at all with `--disable-gpu`, so tonish runs where there is none.

Everything EGL and OpenGL do happens on a thread of its own, `gles::Thread`, which starts EGL, compiles the shaders and then draws every GPU frame for the rest of the run. Starting takes [about 0.1 s](../using/running.md#the-gpu), which on the engine's thread would stall every surface. And NVIDIA's driver makes the first frame cost about 45 ms on any thread but the one that started EGL, and about 2 ms on that one, so the thread that started EGL keeps it. The thread compiles the shaders on a context without a surface, which `EGL_KHR_surfaceless_context` allows and Mesa and NVIDIA's driver offer.

Until the thread reports that it has started, the engine draws every surface on the CPU. Then it draws every surface once more, which moves it to the GPU. From then on, the engine builds each frame's scene and sends it to the thread, which is why shapes are `Send`. The thread draws only the latest frame of each surface, so it never falls behind, and draws a scene with a shape the shader doesn't know on the CPU itself.

The thread makes a surface's EGL window at its first frame, and right after that frame, draws it twice more. EGL allocates a window's buffers only as frames need them, and with NVIDIA's driver each allocation stalls a swap for 0.2 to 2 ms. The extra frames get that done while the surface shows the same image, rather than in the first frames of an animation, and coming after the first frame, they don't hold it up. An EGL window has to be destroyed before its surface: the engine holds a `gles::Window` for each, and dropping one destroys the window on the thread and waits for that.

If EGL or OpenGL fails, or the thread panics, every window is gone before the thread reports it. The engine then logs a warning, replaces the surfaces that were on the GPU with new ones, and draws everything on the CPU from then on.

## Adding a shape

1. Add a file in `crates/renderer/src/shapes/` with the struct and its `Shape` implementation, and export it from `shapes/mod.rs`. The struct has to be `Send`, because scenes go to the GPU's thread.
2. Implement `inside`, and `hole` if the shape is hollow, unless it is always small. Without them, every pixel it covers goes through `distance`.
3. Implement `sdf`, so that the GPU draws the shape. If it doesn't fit the shader's form, the form has to grow: in `signed_distance` in `gpu/shape.frag` and in `Sdf::distance`, the same way.
4. Add it to the three lists in `shapes/mod.rs`:
   - `bounds_enclose_every_shape` checks that `bounds` really contains the shape;
   - `stretches_hold_for_every_shape` checks `inside` and `hole` against `distance`;
   - `sdf_matches_distance_for_every_shape` checks `sdf` against `distance`.
5. Let `random_shape`, in the test fixtures in `shapes/mod.rs`, produce it, so that the reference test and the GPU test cover it.

## Measuring

At 160 Hz a frame has 6.25 ms for everything, drawing included. `crates/renderer/benches/draw.rs` measures the cases that matter, on the CPU and on the GPU:
- bars at scale 1 and 2, and 200 pixels thick;
- 64 small translucent shapes;
- an opaque full screen;
- the overlay: a translucent layer over the whole screen with a card on it, the heaviest frame a launcher would draw.

```sh
just bench                 # every case, on the CPU and the GPU
just bench gpu             # only the GPU, or only the CPU with cpu
just bench-save before     # on the code as it was
just bench-compare before  # after your change
```

The GPU benchmarks draw into a framebuffer without a display, and wait for each frame to be finished. They need an EGL that offers Mesa's surfaceless platform, as [the GPU tests](testing.md#unit-tests) do, and are skipped without one. Neither kind includes showing the frame.

Measured on an Intel Core i9-13900K, with the renderer built for that CPU, as `.cargo/config.toml` does, and an NVIDIA RTX 4090 with the 615 driver:

| Case | CPU | GPU |
|---|---|---|
| `bar/1920x44@2` | 0.11 ms | 0.013 ms |
| `bar/1920x200@2` | 0.25 ms | 0.017 ms |
| `fullscreen/1920x1080@2` | 1.4 ms | 0.038 ms |
| `overlay/1920x1080@2` | 5.1 ms | 0.074 ms |

About 13 µs of every GPU time is a fixed cost, the same for any small surface, for handing the frame over and waiting for it.

### On the GPU, in a compositor

The benchmarks above stop before a frame is shown. `just bench-desktop` times the whole draw, as tonish's log reports it, from building the scene to swapping it: it starts tonish in a headless sway that composites on this machine's GPU, with two 3840×2160 monitors at scale 2, and prints the median and the slowest `took` of three cases:

- **warm redraws**, `tonish msg redraw` a tenth of a second apart, as in an animation;
- **resizes**, the bar's thickness changed back and forth in its settings, for which EGL allocates the surface's buffers again;
- **cold redraws**, each after ten seconds without one. With NVIDIA's driver, the first draw after that has taken 1 to 2 ms instead of 0.1, unless something else, such as a game, keeps the GPU busy.

`tonish --draw-always` draws every panel again as soon as it has drawn it, without waiting for the compositor's next frame, so that the figures in the corner show the most frames a second tonish reaches, and each draw's time, in your own session. On an NVIDIA RTX 4090, in a headless sway with two monitors at scale 2 and the engine logging at debug level, a bar 3840 by 88 pixels reached about 3,000 frames a second on each monitor on the GPU, and about 3,350 on the CPU, which then kept 82 % of a core busy. The frame itself takes 0.12 to 0.15 ms either way; the rest is handing the frame to the compositor.

A redraw counts with the slower of its two monitors' draws, and `waited` is the longest that a draw waited for the GPU's thread, busy with the other monitor. Arguments change how many of each it measures and how long it idles, such as `just bench-desktop --cold 5 --idle 20`. It needs sway and a GPU that sway can composite on, and passes sway `--unsupported-gpu` for NVIDIA's driver. The [timing tests](testing.md#desktop-tests) hold the same cases to a budget.

### In your session

With `TONISH_LOG=engine=debug`, tonish logs each draw in your own session, with its size, the time it took, and `on=cpu` or `on=gpu`. Here tonish starts, gets `tonish msg redraw`, and ends with Ctrl+C:

```text
   0.000068827s  INFO engine::socket: listening socket=/run/user/1000/tonish-wayland-1.sock
   0.000345361s  INFO engine::wayland: monitor added monitor=HEADLESS-1 size=(1920.0, 1080.0) scale=2 surfaces=1
   0.000822006s DEBUG engine::wayland: drew monitor=HEADLESS-1 surface=0 size=(3840, 88) on=cpu took=434.15µs
   0.025970083s DEBUG engine::gles: EGL started vendor=NVIDIA took=25.082495ms
   0.073747646s  INFO engine::gles: GPU started device=NVIDIA GeForce RTX 4090/PCIe/SSE2 took=72.860185ms
   0.076255002s DEBUG engine::gles: moved to the GPU monitor=HEADLESS-1 surface=0 took=2.471874ms
   0.076309759s DEBUG engine::gles: drew monitor=HEADLESS-1 surface=0 size=(3840, 88) on=gpu took=49.795µs waited=8.395µs
   1.501222744s DEBUG engine::wayland: message msg=redraw reply=ok
   1.501401184s DEBUG engine::gles: drew monitor=HEADLESS-1 surface=0 size=(3840, 88) on=gpu took=139.086µs waited=21.584µs
   1.802231057s  INFO engine::wayland: shutting down signal=SIGINT
```

`tonish msg redraw` draws every panel again, so you can time a draw whenever you like, and `--disable-gpu` keeps the panels on the CPU to compare. For a GPU draw, `took` is the time spent on the frame, building its scene on the engine's thread and drawing and swapping it on the GPU's thread, and `waited` is how long it waited in between for the GPU's thread, busy with other frames. Neither is the GPU's own work, which only the benchmarks measure. `moved to the GPU` is the time it took to make the surface's EGL window, before its first frame. Starting the GPU is no part of any draw: the `GPU started` line has a time of its own.
