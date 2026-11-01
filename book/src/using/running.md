# Running

```sh
tonish                          # with the settings from ~/.config/tonish/config.luau
tonish --config ~/test.luau     # with the settings from another file
```

| Option | What it does |
|---|---|
| `-c`, `--config FILE` | Reads the settings from FILE instead, see [Settings](settings.md#another-file). |
| `--disable-gpu` | Draws every panel on the CPU and never starts the [GPU](#the-gpu), which saves memory. |
| `msg MESSAGE` | Sends a [message](#messages) to the tonish running in this session. |
| `-V`, `--version` | Prints the version. |
| `-h`, `--help`, `help` | Prints the help. `tonish msg --help` describes the messages. |

tonish puts a panel on every monitor and keeps room for it, so that windows stay clear of it. A monitor plugged in later gets its panel as soon as it appears.

## One per session

Only one tonish runs in a Wayland session. A second one stops right away, and the first keeps running:

```text
$ tonish
tonish: already running: another instance listens at /run/user/1000/tonish-wayland-1.sock
```

## Ending it

These all end tonish cleanly: it removes its panels, gives the room back to windows, and exits with 0.

- **`tonish msg quit`** returns only once tonish is gone, so `tonish msg quit; tonish &` restarts it.
- **Ctrl+C** in the terminal it runs in.
- **`kill`**, which sends SIGTERM.

`kill -9` ends tonish without a word. The compositor still removes its panels, and the next start cleans up what was left behind.

## The GPU

tonish draws its first frame on the CPU, so the panels appear at once, a few milliseconds after it starts. Meanwhile it starts the GPU in the background, and once the GPU is ready, a fraction of a second later, every panel moves to it, and tonish draws with OpenGL ES from then on. The GPU works on a thread of its own, so neither starting it nor drawing holds up the rest of tonish.

The GPU costs memory even while it has nothing to do. Measured on an NVIDIA RTX 4090 with the 615 driver, it adds about 100 MB to tonish, which needs about 5 MB without it, and takes about 0.1 s to start. The CPU draws a panel in a fraction of a millisecond, so while tonish shows only panels, `--disable-gpu` saves that memory at no noticeable cost. It is also the way out when the GPU misbehaves.

If the GPU can't be used, because there is no libEGL or the driver fails, at the start or later, tonish logs a warning and draws everything on the CPU from then on. The `GPU started` line in the [log](logging.md) names the device and how long starting took. `llvmpipe` there means Mesa's software renderer, so the CPU does the work in any case, and `--disable-gpu` is the better choice.

## Messages

`tonish msg` sends a message to the tonish running in this session:

| Message | What it does |
|---|---|
| `quit` | Closes all panels and ends tonish |
| `redraw` | Draws every panel again. With `TONISH_LOG=engine=debug`, the log then shows how long each draw took. |

With no tonish running, `tonish msg quit` says that nobody answers and exits with 1. An unknown message is a mistake on the command line: tonish lists the known ones and exits with 2.

Scripts can also talk to tonish directly. It listens on a socket at `$XDG_RUNTIME_DIR/tonish-$WAYLAND_DISPLAY.sock`. A message is one line of text, and so is the answer: `ok`, or `error: …` for a message tonish doesn't know.

```sh
echo quit | socat - "UNIX-CONNECT:$XDG_RUNTIME_DIR/tonish-$WAYLAND_DISPLAY.sock"
```

## Environment

| Variable | What tonish uses it for |
|---|---|
| `WAYLAND_DISPLAY` | The compositor to connect to, `wayland-0` if unset. Its name is also part of the socket's name, so that a nested session gets a socket of its own. |
| `XDG_RUNTIME_DIR` | The folder for the socket. Without it, tonish runs without a socket: no messages, and nothing stops a second tonish. `tonish msg` then fails. |
| `XDG_CONFIG_HOME`, `HOME` | Where the [settings](settings.md) are. |
| `TONISH_LOG` | How much tonish [logs](logging.md#more-or-less). |

## Exit codes

| Code | When |
|---|---|
| 0 | It ended through `quit`, Ctrl+C or SIGTERM, or `tonish msg`, `--help` or `--version` did what was asked. |
| 1 | Something went wrong, and tonish printed what, as `tonish: …`. Examples: no Wayland display, a compositor without layer shell, already running, a broken settings file at start. |
| 2 | The command line was wrong, and tonish printed how. |
