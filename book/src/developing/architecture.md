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
| `changed` | This file changed: does that change your surfaces? `true` makes the engine destroy every surface and ask `surfaces` again. |
| `socket` | Where should I listen for messages? Asked first, because the socket also keeps a second instance from starting. |
| `message` | A client sent this line: what's the answer, and should I quit? |
| `pressed` | A pointer button was pressed on this surface or the popup, at this point in logical pixels. |
| `key` | A key was pressed. Only the popup gets the keyboard, and only keys that are in the same place on every layout, such as Escape, are named, because the engine reads keys without a keyboard layout. |
| `popup` | Should a popup be open, and where? Asked after every batch of events. A popup opens when the answer changes to one and closes when it is none. Opened for a button press, it gets the compositor's grab, which gives it the keyboard and closes it on a click outside every surface of tonish. |
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
Shell → Screen (a monitor by name, or every monitor) → Panel → Component
```

Today, `layout` builds that list from the settings: one screen for every monitor, with one panel holding the logo. The settings only choose the panel's orientation and thickness. When they change, the shell builds the list again and tells the engine to rebuild the surfaces.

Messages are an enum, `ipc::Message`. `tonish msg` takes its names from it, and the shell parses incoming lines with it, so the two can't disagree. A new message is a new variant and an arm in `Shell::message`.
