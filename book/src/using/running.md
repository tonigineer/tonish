# Running

```sh
tonish                  # with the settings from ~/.config/tonish/config.toml
tonish --config FILE    # with the settings from FILE
```

| Option | What it does |
|---|---|
| `-c`, `--config FILE` | Reads the settings from FILE instead, see [Settings](settings.md#another-file). |
| `msg MESSAGE` | Sends a [message](#messages) to the tonish running in this session. |
| `-V`, `--version` | Prints the version. |
| `-h`, `--help` | Prints the help. |

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

## Messages

`tonish msg` sends a message to the tonish running in this session:

| Message | What it does |
|---|---|
| `quit` | Closes all panels and ends tonish |

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
| 0 | It ended through `quit`, Ctrl+C or SIGTERM. |
| 1 | Something went wrong, and tonish printed what, as `tonish: …`. Examples: no Wayland display, a compositor without layer shell, already running, a broken settings file at start. |
| 2 | The command line was wrong, and tonish printed how. |
