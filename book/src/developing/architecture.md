# How it fits together

Three crates, each depending only on the ones before it:

```text
renderer   shapes, scenes, drawing on the CPU and the GPU    no Wayland
   ↑
engine     Wayland, EGL, the event loop, the socket, files   no idea what a panel is
   ↑
tonish     the shell: panels, settings, messages, CLI        no Wayland
```

Only the engine speaks Wayland. That keeps the renderer and the shell testable without a compositor: almost all the tests run in plain processes, and only the [desktop tests](testing.md#desktop-tests) need sway.

## The `App` trait

The engine runs an `App`, and the shell is one. The engine asks it questions, and the app answers without ever touching Wayland:

| Method | The engine asks |
|---|---|
| `surfaces` | Which surfaces does this monitor get? Asked when a monitor appears, and for every monitor again after `changed` returns `true`. |
| `draw` | What's on this surface? The app adds shapes to a `Scene`, and the engine draws it on the CPU or the GPU, see [Drawing](drawing.md#the-gpu). Asked when the compositor gives the surface a new size, when `to_redraw` names it, and once more for every surface when the GPU takes over. |
| `to_redraw` | What should I draw again? Asked after every batch of events, so that anything the app handles can lead to a redraw. A surface named several times in one batch is drawn once. |
| `watched` | Which files should I watch? Asked once at start. |
| `changed` | This file changed: does that change your surfaces? `true` makes the engine ask `surfaces` again. A surface that keeps its name and layer changes in place, without a blink, and is drawn again at its new size; any other is replaced. A change that leaves every surface the same, such as the radius alone, still needs `to_redraw`. |
| `socket` | Where should I listen for messages? Asked first, because the socket also keeps a second instance from starting. |
| `message` | A client sent this line: what's the answer, and should I quit? |
| `pressed` | A pointer button was pressed on this surface or the popup, at this point in logical pixels. |
| `key` | A key was pressed. Only the popup gets the keyboard, and only keys that are in the same place on every layout, such as Escape, are named, because the engine reads keys without a keyboard layout. |
| `popup` | Should a popup be open, and against which rectangle of which surface, on which side? Asked after every batch of events. A popup opens when the answer changes to one and closes when it is none. Opened for a button press, it gets the compositor's grab, which gives it the keyboard and closes it on a click outside every surface of tonish. |
| `popup_closed` | The compositor or the engine closed the popup, such as for a click outside it, or because the surface it hangs from went away. |
| `draw_popup` | What's on the popup? `draw` for the popup. |

## The event loop

`engine::run` sets up one calloop event loop and waits on it until something asks to quit. Everything arrives through that loop, one event at a time:

- **Wayland:** monitors appearing and disappearing, and the compositor configuring surfaces.
- **SIGINT and SIGTERM**, through a signalfd. A signal is handled between events like any other input, so it never interrupts a frame halfway.
- **The socket:** a client connects, sends a line, and gets the app's answer.
- **inotify** on the folders of the watched files. Watching the folder rather than the file catches editors that save by renaming a new file over the old one.
- **The GPU's thread**, through a channel: that it has started, or that it has failed. See [The GPU's thread](drawing.md#the-gpus-thread).

Ending is the reverse. The loop stops after the event that asked for it, the surfaces are destroyed, the GPU's thread ends, the socket file is removed, and only then do the clients' connections close. A client that asked to quit keeps its connection until that point, so `tonish msg quit` returns only once tonish is gone.

## The shell

The shell's model is a list of screens, each with its panels, each with its components:

```text
Shell → Monitor (its settings and its screen) → Screen → Panel → Component
```

The settings are a Luau script, which `config.rs` runs with mlua for every monitor, in a fresh sandboxed state each time, with `monitor` describing it and constants such as `Orientation` for the settings that take one of a few values. A constant is a string underneath, in a table that makes a misspelt one an error rather than `nil`, which would read as a setting left out. `assets/tonish.d.luau` declares them for luau-lsp, which then completes them in the user's editor. It runs the shipped file first and merges what the user's file returns over it section by section, so that the defaults live in one place, and then reads the table with serde. serde_path_to_error names the setting in an error, since the table no longer knows the lines its values came from. The shell keeps each monitor's settings, and `layout` builds its screen from them: today one panel holding the logo, shaped by the `bar` section, and the settings panel that opens from it, shaped by the `panel` section. When the file changes, the shell runs it for every monitor, applies it only if it runs for all of them, builds their screens again and tells the engine to reshape the surfaces. The engine changes each surface in place, and draws one that changes size only once the compositor's configure brings its new size, so that it is drawn once, at that size.

Messages are an enum, `ipc::Message`. `tonish msg` takes its names from it, and the shell parses incoming lines with it, so the two can't disagree. A new message is a new variant and an arm in `Shell::message`.
