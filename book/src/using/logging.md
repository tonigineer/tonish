# Logging

tonish logs to stderr and nowhere else. In a terminal you see it directly, in colour. When your compositor or systemd starts tonish, they keep its stderr in their own log, along with the rest of your session.

By default, tonish logs what happens once or rarely:
- that it is starting, and which version;
- the settings it reads and changes;
- the monitors it finds;
- the socket it listens on;
- the GPU: on which device it draws, and how long it took to start;
- why it shuts down.

A normal run fits on a screen. The time in front of each line counts seconds since tonish started:

```text
   0.000008748s  INFO tonish: starting version=0.0.6
   0.000045645s  INFO tonish::shell: settings read file=~/.config/tonish/config.luau
   0.000082751s  INFO engine::socket: listening socket=/run/user/1000/tonish-wayland-1.sock
   0.000426487s  INFO engine::wayland: monitor added monitor=HEADLESS-1 size=(1920.0, 1080.0) scale=2 surfaces=1
   0.072090913s  INFO engine::gles: GPU started device=NVIDIA GeForce RTX 4090/PCIe/SSE2 took=70.750925ms
   1.499787785s  INFO engine::wayland: shutting down signal=SIGINT
```

## More or less

`TONISH_LOG` sets how much tonish logs, as a level for everything, a level per part, or both:

```sh
TONISH_LOG=debug tonish                   # also every draw with its time, messages, file changes
TONISH_LOG=debug,sctk=info tonish         # the same, without the Wayland toolkit's own details
TONISH_LOG=engine=debug,info tonish       # debug for the engine, info for the rest
TONISH_LOG=warn tonish                    # only what goes wrong
```

The levels, from least to most: `off`, `error`, `warn`, `info`, `debug`, `trace`. The parts are the crates: `tonish` for the shell, `engine` for the Wayland side and the GPU, and `sctk` for smithay-client-toolkit, which the engine uses. Without a level for everything, a part that isn't named logs nothing at all: `TONISH_LOG=engine=debug` shows the engine's draws, but not the shell's warnings.

If tonish can't read `TONISH_LOG`, it logs a warning that names the mistake and carries on at the default level.

| Level | What it means |
|---|---|
| `error` | Something tonish needs failed, and a panel is wrong or missing until it recovers. |
| `warn` | Something is wrong, such as a broken settings file or a GPU that can't be used, and tonish works around it. |
| `info` | Starting, settings, monitors, the socket, the GPU, shutting down. |
| `debug` | Every event: each draw with its size, its time, whether the CPU or the GPU drew it, and for the GPU how long it waited for its turn; EGL starting, and each surface that moves to the GPU; button presses and keys, the pointer and keyboard as they appear, and the popup opening and closing; surfaces as they are made and reshaped; messages and their answers; file changes. [Measuring](../developing/drawing.md#measuring) explains the draws' times. |
