# Testing

## Unit tests

`just test` runs them with cargo-nextest, in release mode, then the doctests. They need no display. The renderer, the settings, the command line, the socket and the messages are all tested in plain processes.

Three kinds of tests deserve a mention:

- **Property tests on shapes**, in `shapes/mod.rs`. Every shape belongs in their lists. They check that `bounds` contains the shape, and that `inside`, `hole` and `sdf` agree with `distance`.
- **The reference test** in `cpu.rs`. It draws 600 random scenes both with `draw` and with the plain per-pixel algorithm, and compares every byte. The scenes come from a fixed xorshift, so a failure repeats on every run.
- **The GPU tests** in `gpu/mod.rs`. `the_gpu_draws_what_the_cpu_draws` draws 200 random scenes with OpenGL ES into a framebuffer, without a display, and allows them to differ from the CPU rasteriser's by 2 per channel. The others check that a frame the CPU drew shows unchanged, and that a shape without `sdf` leaves the frame to the CPU. Their context comes from `gpu/headless.rs`, which the GPU benchmarks share, and needs an EGL that offers Mesa's surfaceless platform, as Mesa and NVIDIA's driver do. Where there is none, the tests pass without checking anything, and the desktop tests check the GPU instead.

## Desktop tests

`just desktop` runs tonish on a headless sway and checks what it does there, through `swaymsg` and screenshots taken with grim. The tests are written for [pytest](https://docs.pytest.org) and live in `tests/desktop/`, one file per topic:

| File | What it tests |
|---|---|
| `test_startup.py` | Room on every monitor, the first start, the log, one tonish per session |
| `test_settings.py` | Editing the settings, and broken edits |
| `test_shutdown.py` | `tonish msg quit`, and Ctrl+C |
| `test_drawing.py` | `tonish msg redraw`, and how the bar looks |
| `test_gpu.py` | The panels appear before the GPU has started, the GPU draws what the CPU draws, new panels after a settings change go to the GPU, tonish quits cleanly with the GPU running, and `--disable-gpu` never starts it |
| `test_settings_panel.py` | A right-click on the bar opens the settings panel at the pointer, and a left click on the bar, a click outside or Escape closes it |

A test asks for the `desktop` fixture from `conftest.py` and gets a sway of its own, which is gone again after the test:
- a fresh `HOME`, settings folder and runtime folder, so tests can't affect each other or your desktop;
- two 3840×2160 monitors at scale 2, so problems with several monitors or with scaling show up.

The fixture is a `Desktop` from `harness.py`, with what tests need to look at tonish:
- `workspaces()` for the room left to windows;
- `at(x, y)` for a pixel;
- `logged()` for tonish's log, which has the engine at debug level, and `draws()` for the draws in it;
- `run_tonish(...)` for a second tonish, such as `tonish msg quit`;
- `wait_for_panels()` to wait until tonish is ready for messages and edits;
- `wait_for_gpu()` to wait until the GPU has taken over both monitors' panels;
- `click(x, y, button)` to click in the layout's logical pixels, with a button as Linux names it, such as `BTN_RIGHT`;
- `key(name)` to type a key, such as `Escape`, with wtype.

The headless sway has no input devices, so it offers clients no pointer. The first `click` gives it one with a `VirtualPointer`, a Wayland client in the harness that speaks the wire protocol itself, and then clicks with sway's `seat - cursor` commands. Tests that type keys are skipped where wtype isn't installed.

The module also has `wait_for`, which polls a condition for up to three seconds, and `near`, which compares colours with the tolerance that scaling needs.

A test that compares runs, such as the CPU against the GPU, asks for `desktops` instead: a function that starts one desktop per call, with arguments for tonish, such as `desktops("--disable-gpu")`.

The headless sway draws without a GPU, so Mesa's software renderer stands in for one, even on a machine with NVIDIA's driver: tonish's log names it as `llvmpipe`. That is enough to check the shader and the engine's EGL side, but not how fast either is.

The tests run `target/release/tonish`, which `just desktop` builds first. To test another binary, set `TONISH` to its path.

Tests come in two kinds:

- **Plain tests** are general: what tonish does, such as keeping room for its panels, reloading its settings, quitting on a message. CI runs these.
- **`@pytest.mark.local`** tests are exact: how panels look, down to the pixel. CI leaves them out, so that changing a panel's look needs no round trip through CI.

Arguments to `just desktop` go to pytest:

```sh
just desktop                  # every test
just desktop -k redraw        # those whose name contains "redraw"
just desktop --lf             # those that failed last time
just desktop -m "not local"   # what CI runs
```

When a test fails, the report ends with the last 20 lines of tonish's log for every desktop that still runs, and a screenshot of each is saved as `tests/desktop/out/<test>.png`, or `<test>-<n>.png` when the test has several.

pytest and ruff come through [uv](https://docs.astral.sh/uv/), at the versions in `uv.lock`, both on your machine and in CI. `pyproject.toml` holds their settings.

## Pull requests

A pull request into `main` runs these jobs. The others wait for "Version raised" and don't run if it fails, so that a forgotten `just bump` costs seconds instead of a full run:

| Job | What it checks |
|---|---|
| Version raised | The version is newer than on `main`, except for branches named `fix/*`. |
| Lint | Formatting, clippy and ruff, and that the API documentation and this book build. |
| Build | That `Cargo.lock` is up to date, the tests, then the release build, packaged as it would be released. |
| Desktop | The general desktop tests, in an Arch Linux container with Mesa and wtype, with the packaged binary. When they fail, the screenshots are kept as the `desktop-screenshots` artifact. |
