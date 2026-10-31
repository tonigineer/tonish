# Testing

## Unit tests

`just test` runs them with cargo-nextest, in release mode, then the doctests. They need no display. The renderer, the settings, the command line, the socket and the messages are all tested in plain processes.

Two kinds of tests deserve a mention:

- **Property tests on shapes**, in `shapes/mod.rs`. Every shape belongs in their lists. They check that `bounds` contains the shape, and that `inside` and `hole` agree with `distance`.
- **The reference test** in `cpu.rs`. It draws 600 random scenes both with `draw` and with the plain per-pixel algorithm, and compares every byte. The scenes come from a fixed xorshift, so a failure repeats on every run.

## Desktop scenarios

`just desktop` runs tonish on a headless sway and checks what it does there, through `swaymsg` and screenshots taken with grim. Each scenario gets a sway of its own:
- a fresh `HOME`, settings folder and runtime folder, so scenarios can't affect each other or your desktop;
- two 3840×2160 monitors at scale 2, so problems with several monitors or with scaling show up.

Scenarios are Python functions in `tests/desktop/run.py`, marked with one of two decorators:

- **`@scenario`** is general: what tonish does, such as keeping room for its panels, reloading its settings, quitting on a message, refusing a second start. CI runs these.
- **`@local`** is exact: how panels look, down to the pixel. Only `just desktop` on your machine runs these, so that changing a panel's look needs no round trip through CI.

To run only what CI runs, set `CI`: `CI=1 just desktop`.

When a scenario fails, it prints the last lines of tonish's log and saves a screenshot in `tests/desktop/out/`.

## Pull requests

A pull request into `main` runs these jobs:

| Job | What it checks |
|---|---|
| Version raised | The version is newer than on `main`, except for branches named `fix/*`. |
| Lint | Formatting and clippy, and that the API documentation and this book build. |
| Build | The tests, then the release build, packaged as it would be released. |
| Desktop | The general scenarios, in an Arch Linux container, with the packaged binary. |
