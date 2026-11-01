# Getting started

## Tools

| Tool | For |
|---|---|
| Rust, current stable | everything; the workspace uses edition 2024 |
| [just](https://github.com/casey/just) | the recipes below |
| [cargo-nextest](https://nexte.st) | `just test` |
| sway, grim, Mesa | `just desktop`, the [tests on a headless sway](testing.md#desktop-tests). |
| wtype | The desktop tests that type keys, which are skipped without it. |
| [uv](https://docs.astral.sh/uv/) | The Python tools: pytest for `just desktop`, ruff for `just fmt` and `just lint`. uv installs them at the versions in `uv.lock`. |
| [mdBook](https://rust-lang.github.io/mdBook/) 0.5 | `just book`, this book |

## Recipes

`just` lists every recipe, by group:

| Recipe | What it does |
|---|---|
| `just run [args]` | Builds and runs tonish in your session. Arguments go to tonish, such as `--disable-gpu`. |
| `just build [args]` | Builds the whole workspace. Arguments go to cargo, such as `--release`. |
| `just fmt` | Formats all the code: Rust with rustfmt, Python with ruff. |
| `just install [dir]` | Builds tonish for this machine and installs it to `~/.local/bin`, or to the folder you give. |
| `just clean` | Removes what building and testing left behind, the book and the Python tools included. |
| `just lint` | Formatting, clippy and ruff, with warnings as errors. |
| `just test` | The [unit tests](testing.md#unit-tests). |
| `just check` | `just lint`, then `just test`: run it before calling a change done. |
| `just desktop [args]` | Runs the [desktop tests](testing.md#desktop-tests). Arguments go to pytest, such as `-k redraw` or `--lf`. |
| `just check-all` | `just check`, then the desktop tests: everything a pull request checks in code. |
| `just bench [filter]`, `just bench-save`, `just bench-compare` | [Benchmark](drawing.md#measuring) the renderer, and compare against a saved baseline. |
| `just doc` | Builds the [API documentation](api.md), private items included, with warnings as errors. |
| `just book` | Builds this book, with the API documentation inside it, into `book/build`. |
| `just book-serve` | Serves this book at `http://localhost:3000` and rebuilds it as you edit. The API documentation isn't part of it there. |
| `just version`, `just version-check`, `just bump patch` | Print, check and raise the version for a [release](releasing.md). |

## Where things are

| Path | What's there |
|---|---|
| `crates/renderer` | Shapes, scenes, the CPU rasteriser, the OpenGL ES backend with its shaders in `gpu/`, PNG snapshots, and the benchmarks in `benches/`. It knows nothing of Wayland. |
| `crates/engine` | The Wayland side: the `App` trait, monitors, layer surfaces, the event loop, the socket, file watching, and EGL with the GPU's thread in `gles.rs`. |
| `crates/tonish` | The shell: panels, settings, the command line, messages, logging. |
| `assets/default-config.toml` | The default settings, compiled into tonish and written on its first start. |
| `tests/desktop/` | The desktop tests, for pytest. |
| `pyproject.toml`, `uv.lock` | The Python tools for the desktop tests, their versions and their settings. |
| `book/` | This book. |
| `.github/workflows/` | CI for pull requests, releases, and the book's site. |
| `.cargo/config.toml` | Builds for the CPU you build on, which suits benchmarks. Releases override it. |
