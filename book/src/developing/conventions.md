# Conventions

## Comments

Comments are for the developers who will change the code later.

- Say why something is done the way it is, never what it does: the code already says that.
- Leave out what the names, the types or the code make obvious.
- Keep a comment for a non-obvious constraint, a trap, a rejected alternative, or a reason found outside the code, such as a spec, a compositor's behaviour or a measurement.
- Write plain prose, in full sentences.
- Section headers, `// ── Name ───…` filled to column 80, group long files. They are the one kind of "what" comment that stays.

Some comments are for users instead, and the rules above don't apply to them:
- the doc comments that clap turns into `--help`;
- the `[doc(...)]` attributes in the justfile;
- the comments in `assets/default-config.toml`.

## Logging

Log with `tracing`'s macros, never `println!` or `eprintln!`. The exceptions are the fatal error that `main` prints, and everything `tonish msg` prints: a user reads those in the terminal.

| Level | For |
|---|---|
| `error` | Something tonish needs failed, and a panel is wrong or missing until it recovers. |
| `warn` | Something the user did or the system lacks, which tonish works around. |
| `info` | What happens once or rarely. A normal run stays a screenful. |
| `debug` | Every event. |

Keep the message a fixed text, and put what varies in fields, as in `info!(monitor = %name, "monitor added")`, so that a message can be searched for. An error's own text goes at the end of the message. Use `%` for text and `?` for everything else.

## Code

- Colour channels are stored as `u8`. Use `u16` or `f32` only for the maths in between.
- A new shape gets its own file in `crates/renderer/src/shapes/`, and joins the every-shape tests. See [Adding a shape](drawing.md#adding-a-shape).
- The CPU rasteriser must not change a byte of its output for the sake of speed. See [Drawing](drawing.md).
- The GPU draws what the CPU rasteriser draws, to within 2 per channel. `signed_distance` in `gpu/shape.frag` and `Sdf::distance` are one function, written twice, and change together. See [The GPU](drawing.md#the-gpu).

## The book

A change to anything this book describes updates the book in the same change: the command line, messages, settings, logging, recipes, CI or the release flow.
