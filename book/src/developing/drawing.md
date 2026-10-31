# Drawing

## Shapes are distances

A shape in tonish isn't a set of pixels or a path. It's a function that gives the signed distance from any point to the shape's edge: negative inside, positive outside. That one number gives three things:
- antialiased edges, from how far a pixel's centre is from the edge;
- outlines, by folding the distance into a band around the edge;
- independence from the monitor's scale, because shapes live in logical pixels.

A GPU fragment shader works the same way, so a GPU backend can draw the same scenes later.

Every shape implements the `Shape` trait in `crates/renderer/src/shapes/`, one file per shape:

| Method | What it gives |
|---|---|
| `bounds` | A box around the shape. The rasteriser looks at nothing outside it. |
| `color` | The shape's colour, with its opacity. |
| `distance` | The signed distance from a point to the edge. It has to be the true distance near the edge, or edges and outlines come out uneven. |
| `inside` | On one row, the stretch that is at least a given depth inside the shape. |
| `hole` | On one row, the stretch inside the bounds that is at least a given depth outside the shape, such as the middle of an outline. |

## The CPU rasteriser

`cpu::draw` paints a scene into a buffer, row by row. On each row, every shape that crosses it splits the row into three kinds of pixels:

- **Solid**, from `inside`: covered completely. An opaque colour is written straight in, and a translucent one is blended at its own opacity.
- **Empty**, from `hole`: skipped.
- **Edge**, everything else: the rasteriser computes `distance` at the pixel's centre and turns it into coverage across one physical pixel.

A panel is almost all solid background, so almost no pixel needs the distance. Drawing row by row also keeps each row in the cache while every shape is drawn on it.

`inside` and `hole` may give a shorter stretch than the true one, which only costs time, but never a longer one, which would paint over an antialiased edge. Both return `None` by default, so a new shape draws correctly before it has them, only more slowly.

The solid and empty stretches must not change a single byte of the result. A test in `cpu.rs` keeps the plain algorithm as a reference, every pixel of the buffer through `distance`, and compares it with `draw` on 600 random scenes.

## Adding a shape

1. Add a file in `crates/renderer/src/shapes/` with the struct and its `Shape` implementation, and export it from `shapes/mod.rs`.
2. Implement `inside`, and `hole` if the shape is hollow, unless it is always small. Without them, every pixel it covers goes through `distance`.
3. Add it to both lists in `shapes/mod.rs`:
   - `bounds_enclose_every_shape` checks that `bounds` really contains the shape;
   - `stretches_hold_for_every_shape` checks `inside` and `hole` against `distance`.
4. Let `random_shape` in `cpu.rs` produce it, so that the reference test covers it.

## Measuring

At 160 Hz a frame has 6.25 ms for everything, drawing included. `crates/renderer/benches/draw.rs` measures the cases that matter:
- bars at scale 1 and 2, and 200 pixels thick;
- 64 small translucent shapes;
- an opaque full screen;
- the overlay: a translucent layer over the whole screen with a card on it, the heaviest frame a launcher would draw.

```sh
just bench-save before     # on the code as it was
just bench-compare before  # after your change
```

Measured on an Intel Core i9-13900K, with the renderer built for that CPU, as `.cargo/config.toml` does:

| Case | Time |
|---|---|
| `bar/1920x44@2` | 0.11 ms |
| `bar/1920x200@2` | 0.25 ms |
| `fullscreen/1920x1080@2` | 1.4 ms |
| `overlay/1920x1080@2` | 5.1 ms |

With `TONISH_LOG=engine=debug`, tonish logs each draw with its size and the time it took, in your own session.
