# Drawing

## Shapes are distances

A shape in tonish isn't a set of pixels or a path. It's a function that gives the signed distance from any point to the shape's edge: negative inside, positive outside. That one number gives three things:
- antialiased edges, from how far a pixel's centre is from the edge;
- outlines, by folding the distance into a band around the edge;
- independence from the monitor's scale, because shapes live in logical pixels.

The GPU's fragment shader works with the same number, so the CPU and the GPU draw the same scenes, see [The GPU](#the-gpu).

Every shape implements the `Shape` trait in `crates/renderer/src/shapes/`, one file per shape:

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

The solid and empty stretches must not change a single byte of the result. A test in `cpu.rs` keeps the plain algorithm as a reference, every pixel of the buffer through `distance`, and compares it with `draw` on 600 random scenes.

## The GPU

`gpu::Gpu` in the renderer draws a scene with OpenGL ES 3.0, in one draw call. Every shape becomes an instance of a quad over its box. The fragment shader, `gpu/shape.frag`, computes the distance at each pixel's centre and turns it into coverage exactly as the CPU rasteriser does.

A shader can't call `distance`, so every shape describes itself with `sdf`, in the one form the shader knows: a box with rounded corners, which is also a circle, folded into an outline or not. `Sdf::distance` is the shader's function written again in Rust, so that the tests can check every shape's `sdf` against its `distance` without a GPU. The two have to change together.

A scene with a shape that has no `sdf`, such as an outline of an outline, is drawn by the CPU rasteriser, and the GPU shows the result as a texture. The surface stays with EGL even then. A compositor with explicit sync, such as Hyprland with NVIDIA's driver, ends the connection when a surface that EGL has drawn into gets a shared-memory buffer.

The engine draws the first frame on the CPU and starts the GPU only after it, so that the panels never wait for the GPU, see the [Running](../using/running.md#the-gpu) page. It loads libEGL at that moment, so tonish runs where there is none.

Everything EGL and OpenGL do happens on a thread of its own, `gles::Thread`, which starts EGL, compiles the shaders and then draws every GPU frame for the rest of the run. Starting takes about 0.1 s, which on the engine's thread would stall every surface. And NVIDIA's driver makes the first frame cost about 45 ms on any thread but the one that started EGL, and about 2 ms on that one, so the thread that started EGL keeps it. The thread compiles the shaders on a context without a surface, which `EGL_KHR_surfaceless_context` allows and Mesa and NVIDIA's driver offer.

Until the thread reports that it has started, the engine draws every surface on the CPU. Then it draws every surface once more, which moves it, and from then on it builds each frame's scene and sends it to the thread, which is why shapes are `Send`. The thread draws only the latest frame of each surface, so it never falls behind, and draws a scene with a shape the shader doesn't know on the CPU itself. It makes a surface's EGL window at its first frame, and draws that frame twice ahead, because EGL allocates its buffers only as frames need them, which stalls a swap for 0.2 to 2 ms with NVIDIA's driver: so it happens when the surface moves, not in an animation. That window has to go before the surface: the engine holds a `gles::Window` for it, and dropping one destroys the window on the thread and waits for that.

If EGL or OpenGL fails, the thread drops every window and reports it. The engine logs a warning, replaces the surfaces that were on the GPU with new ones, and draws everything on the CPU from then on.

The two backends agree to within rounding. `the_gpu_draws_what_the_cpu_draws` in `gpu/mod.rs` draws 200 random scenes both ways and allows a difference of 2 per channel.

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
just bench gpu             # only the GPU, or only the CPU with cpu
just bench-save before     # on the code as it was
just bench-compare before  # after your change
```

The GPU benchmarks draw into a framebuffer without a display, and wait for each frame to be finished. They need an EGL that offers Mesa's surfaceless platform, as the GPU tests do, and are skipped without one. Neither kind includes showing the frame.

Measured on an Intel Core i9-13900K, with the renderer built for that CPU, as `.cargo/config.toml` does, and an NVIDIA RTX 4090 with the 615 driver:

| Case | CPU | GPU |
|---|---|---|
| `bar/1920x44@2` | 0.11 ms | 0.013 ms |
| `bar/1920x200@2` | 0.25 ms | 0.017 ms |
| `fullscreen/1920x1080@2` | 1.4 ms | 0.038 ms |
| `overlay/1920x1080@2` | 5.1 ms | 0.074 ms |

About 13 µs of every GPU time is a fixed cost, the same for any small surface, for handing the frame over and waiting for it.

With `TONISH_LOG=engine=debug`, tonish logs each draw in your own session, with its size, the time it took, and `on=cpu` or `on=gpu`. `tonish msg redraw` draws every panel again, so you can time a draw whenever you like, and `--disable-gpu` keeps the panels on the CPU to compare. For a GPU draw, `took` is the time spent on the frame, building its scene on the engine's thread and drawing and swapping it on the GPU's thread, and `waited` is how long it waited in between for the GPU's thread, busy with other frames. Neither is the GPU's own work, which only the benchmarks measure. Starting the GPU is no part of any draw: the `GPU started` line has a time of its own.
