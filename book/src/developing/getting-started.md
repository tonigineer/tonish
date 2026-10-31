# Getting started

## Tools

| Tool | For |
|---|---|
| Rust, current stable | everything; the workspace uses edition 2024 |
| [just](https://github.com/casey/just) | the recipes below |
| [cargo-nextest](https://nexte.st) | `just test` |
| sway, grim, Python 3 | `just desktop`, the scenarios on a headless sway |
| [ruff](https://docs.astral.sh/ruff/) 0.17 | `just fmt` and `just lint` for the Python in the scenarios |
| [mdBook](https://rust-lang.github.io/mdBook/) 0.5 | `just book`, this book |

## Recipes

`just` lists every recipe, by group. These are the ones you'll use most:

| Recipe | What it does |
|---|---|
| `just run` | Builds and runs tonish in your session. |
| `just fmt` | Formats all the code: Rust with rustfmt, Python with ruff. |
| `just check` | Formatting, clippy and ruff with warnings as errors, and the tests: run it before calling a change done. |
| `just check-all` | `just check`, then the desktop scenarios: everything a pull request checks in code. |
| `just desktop [filter]` | Runs the [desktop scenarios](testing.md#desktop-scenarios), or only those whose name contains `filter`. |
| `just bench`, `just bench-save`, `just bench-compare` | [Benchmark](drawing.md#measuring) the renderer, and compare against a saved baseline. |
| `just doc` | Builds the [API documentation](api.md), private items included, with warnings as errors. |
| `just book` | Builds this book, with the API documentation inside it, into `book/build`. |
| `just book-serve` | Serves this book at `http://localhost:3000` and rebuilds it as you edit. The API documentation isn't part of it there. |
| `just install` | Installs your build to `~/.local/bin`. |
| `just bump patch` | Raises the version for a [release](releasing.md). |

## Where things are

| Path | What's there |
|---|---|
| `crates/renderer` | Shapes, scenes and the CPU rasteriser. It knows nothing of Wayland. |
| `crates/engine` | The Wayland side: monitors, layer surfaces, the event loop, the socket, file watching. |
| `crates/tonish` | The shell: panels, settings, the command line, messages, logging. |
| `assets/default-config.toml` | The default settings, compiled into tonish and written on its first start. |
| `tests/desktop/run.py` | The desktop scenarios. |
| `book/` | This book. |
| `.github/workflows/` | CI for pull requests, releases, and the book's site. |
| `.cargo/config.toml` | Builds for the CPU you build on, which suits benchmarks. Releases override it. |
