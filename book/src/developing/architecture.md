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
| `draw` | What's on this surface? The app adds shapes to a `Scene`, and the engine draws it on the CPU or the GPU, see [Drawing](drawing.md#the-gpu). Asked when the compositor gives the surface a new size, when `to_redraw` names it, at every frame while it animates, and once more for every surface when the GPU takes over. `Paint::time` is when the engine started the frame, which animations step their springs to. |
| `max_fps` | At most how many frames a second for this surface while it animates? A frame the compositor offers sooner is drawn when it is due, on a timer: skipping it would wait a whole refresh more. |
| `animating` | Does this surface move? Asked after every draw. While it says yes, the engine asks the compositor for the surface's next frame with the commit that shows this one, and draws it again when the frame comes: the compositor sets the pace, and a surface it doesn't show gets no frames, so its animation waits. |
| `to_redraw` | What should I draw again? Asked after every batch of events, so that anything the app handles can lead to a redraw. A surface named several times in one batch is drawn once. |
| `watched` | Which files should I watch? Asked once at start. |
| `changed` | This file changed: does that change your surfaces? `true` makes the engine ask `surfaces` again. A surface that keeps its name and layer changes in place, without a blink, and is drawn again at its new size; any other is replaced. A change that leaves every surface the same, such as the radius alone, still needs `to_redraw`. |
| `socket` | Where should I listen for messages? Asked first, because the socket also keeps a second instance from starting. |
| `message` | A client sent this line: what's the answer, and should I quit? |
| `pressed` | A pointer button was pressed on this surface or the popup, at this point in logical pixels. |
| `released`, `moved` | The button was let go, or the pointer moved. While a button is held, the surface it was pressed on hears of both even outside it, so that a drag goes on. |
| `left` | The pointer left this surface or the popup, so that what lit up under it can go dark. No motion over it follows until the pointer comes back. |
| `cursor` | Which cursor here? Asked as the pointer enters and moves, and set only when it changes. |
| `key` | A key was pressed. Only the popup's grab gets the keyboard, and only keys that are in the same place on every layout, such as Escape, are named, because the engine reads keys without a keyboard layout. |
| `popup` | Should a popup be open, where on the monitor, how large, holding the grab or not, and which part of it takes the pointer? Asked after every batch of events. A popup opens when the answer changes to one, changes with it, and closes when it is none. |
| `popup_dismissed` | A click outside every surface of tonish ended the popup's grab. The popup stays until `popup` answers none, so that it can animate away. |
| `popup_closed` | The engine closed the popup, because its monitor went away. |
| `to_reshape` | Should I ask for your surfaces again? Asked after every batch of events, so that a surface can come or go on a message, as the figures in the corner do on `tonish msg hud`. Unlike `changed`, it leaves an open popup open. |
| `drew` | A frame was drawn, in this time, by the CPU or the GPU. A GPU frame is reported once the GPU's thread has drawn it. The shell counts these for its figures. |
| `gpu_started` | The GPU took over, on this device. |
| `draw_popup` | What's on the popup? `draw` for the popup. |

A popup is an overlay layer surface of its own rather than an xdg_popup, because the compositor unmaps an xdg_popup the moment a click lands outside it, which leaves nothing to animate. It covers the whole monitor while it is open, so that it is never resized, which flickers, however far its card is pulled, and only the card takes the pointer. Opened for a button press, it also gets a transparent xdg_popup exactly over it, hanging from the bar, which holds the compositor's grab: that gives it the keyboard and tells it of a click outside. sway and Hyprland both keep that click from the window under it, and no other way tells a panel of the click while letting it through on both. While the grab holds, that xdg_popup also takes the pointer where the popup does, and the popup itself lets every click through to it: Hyprland lets only the grab's own surface and its xdg parent have the pointer during a grab, and a layer surface's popup has no xdg parent. The shell asks for input over the whole monitor while the card is dragged, since during a grab sway holds on to no surface for a held button, and the drag would end where the pointer left the surface it was pressed on. For the same reason a card pulled off the bar lets go of the grab only once it is let go: the release would reach no surface otherwise. Docked again, it takes the grab back at the next press on it, since a grab can only start with a press. Its transparent buffer is one pixel, stretched over the popup with wp_viewporter. The engine changes the popup's input region only when it changes, since each change commits both surfaces.

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

The settings are a Luau script, which `config.rs` runs with mlua for every monitor, in a fresh sandboxed state each time, with `monitor` describing it and constants such as `Orientation` for the settings that take one of a few values. A constant is a string underneath, in a table that makes a misspelt one an error rather than `nil`, which would read as a setting left out. `assets/tonish.d.luau` declares them for luau-lsp, which then completes them in the user's editor. It runs the shipped file first and merges what the user's file returns over it, section by section at every depth, so that the defaults live in one place, and then reads the table with serde. A `radius` table is merged whole, because a corner it leaves out is square. serde_path_to_error names the setting in an error, since the table no longer knows the lines its values came from. The shell keeps each monitor's settings, and `layout` builds its screen from them: today one panel holding the logo, shaped by the `bar` section, and the settings panel that opens from it, shaped by the `panel` section. When the file changes, the shell runs it for every monitor, applies it only if it runs for all of them, builds their screens again and tells the engine to reshape the surfaces. The engine changes each surface in place, and draws one that changes size only once the compositor's configure brings its new size, so that it is drawn once, at that size.

A panel that opens from the bar is a card of liquid, in `liquid`: `Frame` is where the bar's edge runs, `Card` is the card's life on springs and under the pointer, from growing out of the bar to folding back, `Pose` is where it is at one moment, and `draw` turns a pose into shapes with the `Look` the settings give it. Nothing there is about settings: `Shell::hang` hangs a card of any size from a panel at the pointer, and the settings panel is one of `settings::SIZE`. The shell keeps the open card, routes the pointer to it, and draws its pose on the popup.

Messages are an enum, `ipc::Message`. `tonish msg` takes its names from it, and the shell parses incoming lines with it, so the two can't disagree. A new message is a new variant and an arm in `Shell::message`.
