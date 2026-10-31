# Testing

## Unit tests

`just test` runs them with cargo-nextest, in release mode, then the doctests. They need no display. The renderer, the settings, the command line, the socket and the messages are all tested in plain processes.

Two kinds of tests deserve a mention:

- **Property tests on shapes**, in `shapes/mod.rs`. Every shape belongs in their lists. They check that `bounds` contains the shape, and that `inside` and `hole` agree with `distance`.
- **The reference test** in `cpu.rs`. It draws 600 random scenes both with `draw` and with the plain per-pixel algorithm, and compares every byte. The scenes come from a fixed xorshift, so a failure repeats on every run.

## Desktop tests

`just desktop` runs tonish on a headless sway and checks what it does there, through `swaymsg` and screenshots taken with grim. The tests are written for [pytest](https://docs.pytest.org) and live in `tests/desktop/`, one file per topic:

| File | What it tests |
|---|---|
| `test_startup.py` | Room on every monitor, the first start, the log, one tonish per session |
| `test_settings.py` | Editing the settings, and broken edits |
| `test_shutdown.py` | `tonish msg quit`, and Ctrl+C |
| `test_drawing.py` | `tonish msg redraw`, and how the bar looks |

A test asks for the `desktop` fixture from `conftest.py` and gets a sway of its own, which is gone again after the test:
- a fresh `HOME`, settings folder and runtime folder, so tests can't affect each other or your desktop;
- two 3840×2160 monitors at scale 2, so problems with several monitors or with scaling show up.

The fixture is a `Desktop` from `harness.py`, with what tests need to look at tonish:
- `workspaces()` for the room left to windows;
- `at(x, y)` for a pixel;
- `logged()` for tonish's log, which has the engine at debug level, so that draws can be counted;
- `run_tonish(...)` for a second tonish, such as `tonish msg quit`;
- `wait_for_panels()` to wait until tonish is ready for messages and edits.

The module also has `wait_for`, which polls a condition for up to three seconds, and `near`, which compares colours with the tolerance that scaling needs.

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

When a test fails, the report ends with the last 20 lines of tonish's log, and a screenshot is saved as `tests/desktop/out/<test>.png`.

pytest and ruff come through [uv](https://docs.astral.sh/uv/), at the versions in `uv.lock`, both on your machine and in CI. `pyproject.toml` holds their settings.

## Pull requests

A pull request into `main` runs these jobs:

| Job | What it checks |
|---|---|
| Version raised | The version is newer than on `main`, except for branches named `fix/*`. |
| Lint | Formatting, clippy and ruff, and that the API documentation and this book build. |
| Build | The tests, then the release build, packaged as it would be released. |
| Desktop | The general desktop tests, in an Arch Linux container, with the packaged binary. |
