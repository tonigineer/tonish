# Testing

## Unit tests

`just test` runs them with cargo-nextest, in release mode, then the doctests. They need no display. The renderer, the settings, the command line, the socket and the messages are all tested in plain processes.

Three kinds of tests deserve a mention:

- **Property tests on shapes**, in `shapes/mod.rs`. Every shape belongs in their lists. They check that `bounds` contains the shape, and that `inside`, `hole` and `sdf` agree with `distance`.
- **The reference test** in `cpu.rs`. It draws 600 random scenes both with `draw` and with the plain per-pixel algorithm, and compares every byte. The scenes come from a fixed xorshift, so a failure repeats on every run.
- **The GPU tests** in `gpu/mod.rs`. `the_gpu_draws_what_the_cpu_draws` draws 200 random scenes with OpenGL ES into a framebuffer, without a display, and allows them to differ from the CPU rasteriser's by 2 per channel. The others check that a frame the CPU drew shows unchanged, and that a shape without `sdf` leaves the frame to the CPU. Their context comes from `gpu/headless.rs`, which the GPU benchmarks share, and needs an EGL that offers Mesa's surfaceless platform, as Mesa and NVIDIA's driver do. Where there is none, the tests pass without checking anything, and the desktop tests check the GPU instead. Shaders that don't compile fail them, rather than counting as no GPU.

## Desktop tests

`just desktop` runs tonish on a headless sway and checks what it does there, through `swaymsg` and screenshots taken with grim. The tests are written for [pytest](https://docs.pytest.org) and live in `tests/desktop/`, one file per topic:

| File | What it tests |
|---|---|
| `test_startup.py` | Room on every monitor, the first start, the log, one tonish per session |
| `test_settings.py` | Editing the settings, broken edits, and the room a panel keeps with a gap or a shorter length |
| `test_shutdown.py` | `tonish msg quit`, and Ctrl+C |
| `test_drawing.py` | `tonish msg redraw`, `--draw-always`, and how the bar looks: its logo, its rounded corners, and the concave corners of a shorter bar |
| `test_gpu.py` | The panels appear before the GPU has started, the GPU draws what the CPU draws, new panels after a settings change go to the GPU, tonish quits cleanly with the GPU running, and `--disable-gpu` never starts it |
| `test_settings_panel.py` | A right-click on the bar hangs the settings panel below it at the pointer; it animates as it opens and then stops drawing, `max_fps` caps its frames, its grip pulls it off the bar and it stays where it is let go and stops drawing once it rests, dragged back it hangs again and takes the grab back at a press on its grip, and a left click on the bar, a click outside or Escape folds it back |
| `test_hud.py` | The figures in the corner show text without taking room from windows, `tonish msg hud` hides and shows them, `hud.show = false` hides them until `tonish msg hud`, and they draw a few times a second |
| `test_timing.py` | How long drawing takes on this machine's GPU: warm and cold redraws, and resizes, each within a budget |

A test asks for the `desktop` fixture from `conftest.py` and gets a sway of its own, which is gone again after the test:
- a fresh `HOME`, settings folder and runtime folder, so tests can't affect each other or your desktop;
- two 3840×2160 monitors at scale 2, so problems with several monitors or with scaling show up.

The fixture is a `Desktop` from `harness.py`, with what tests need to look at tonish:
- `workspaces()` for the room left to windows;
- `at(x, y)` for a pixel;
- `logged()` for tonish's log, which has the engine at debug level, and `draws()` for the draws in it, of every surface or of one, such as `draws("0")` for the bar;
- `hide_hud()` to hide the figures in the corner, which change with every run, for a test of how panels look;
- `run_tonish(...)` for a second tonish, such as `tonish msg quit`;
- `wait_for_panels()` to wait until tonish is ready for messages and edits;
- `wait_for_gpu()` to wait until the GPU has taken over both monitors' panels;
- `click(x, y, button)` to click in the layout's logical pixels, with a button as Linux names it, such as `BTN_RIGHT`;
- `drag(points, seconds)` to press the left button at the first point, move through the others and let go at the last, with the virtual pointer's own motion: sway passes no motion on for its `seat - cursor` commands while a button is held;
- `key(name)` to type a key, such as `Escape`, with wtype.

The headless sway has no input devices, so it offers clients no pointer. The first `click` gives it one with a `VirtualPointer`, a Wayland client in the harness that speaks the wire protocol itself, and then clicks with sway's `seat - cursor` commands. Tests that type keys are skipped where wtype isn't installed.

The module also has `wait_for`, which polls a condition for up to three seconds, and `near`, which compares colours with the tolerance that scaling needs.

A test that compares runs, such as the CPU against the GPU, asks for `desktops` instead: a function that starts one desktop per call, with arguments for tonish, such as `desktops("--disable-gpu")`.

The headless sway draws without a GPU, so Mesa's software renderer stands in for one, even on a machine with NVIDIA's driver: tonish's log names it as `llvmpipe`. That is enough to check the shader and the engine's EGL side, but not how fast either is. For that, `Desktop(hardware=True)` has sway composite on this machine's GPU, and tonish then draws on it too, because EGL takes the GPU only from a compositor that shares buffers on it. `timing.py` measures draws there, for the timing tests and [`just bench-desktop`](drawing.md#on-the-gpu-in-a-compositor).

The tests run `target/release/tonish`, which `just desktop` builds first. To test another binary, set `TONISH` to its path.

Tests come in three kinds:

- **Plain tests** are general: what tonish does, such as keeping room for its panels, reloading its settings, quitting on a message. CI runs these.
- **`@pytest.mark.local`** tests are exact: how panels look, down to the pixel. CI leaves them out, so that changing a panel's look needs no round trip through CI.
- **`@pytest.mark.hardware`** tests time drawing on this machine's GPU, which CI has none of. They take half a minute, most of it idling for the cold draws, so they run only when you ask for them with `-m hardware`, and are skipped where sway finds no GPU to composite on. Their budgets leave room for a GPU busy with other work, such as a game.

Arguments to `just desktop` go to pytest:

```sh
just desktop                                # every test but the timing tests
just desktop -k redraw                      # those whose name contains "redraw"
just desktop --lf                           # those that failed last time
just desktop -m hardware                    # the timing tests
just desktop -m "not local and not hardware"  # what CI runs
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
