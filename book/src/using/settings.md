# Settings

tonish reads its settings from `$XDG_CONFIG_HOME/tonish/config.luau`, which is `~/.config/tonish/config.luau` unless `XDG_CONFIG_HOME` is set to an absolute path. The file is a script in [Luau](https://luau.org), a small language derived from Lua, that returns a table of settings: each setting is a name and a value, such as `thickness = 44`, in the table of its section, such as `bar = { ... }`. On its first start, tonish writes that file with the default settings and a comment on each one:

```lua
{{#include ../../../assets/default-config.luau}}
```

To get the defaults back, delete the file; tonish writes it again on its next start. If it can't write the file at all, for example because your home is read-only, or if neither `XDG_CONFIG_HOME` nor `HOME` is set, it starts with the built-in settings, which are the same, and logs a warning.

A setting you leave out takes its default, so a file can return only what you change:

```lua
return { bar = { thickness = 30 } }
```

## Settings for each monitor

The file runs once for every monitor, and what it returns applies to that monitor. The table `monitor` describes it:

| Field | Value |
|---|---|
| `monitor.name` | The name the compositor gives the monitor, such as `"DP-1"`, or `nil` on a compositor that names none. |
| `monitor.width`, `monitor.height` | The monitor's size in logical pixels. |
| `monitor.scale` | The monitor's scale, a whole number. |

A setting can follow them, such as a bar 300 pixels short of either end of every monitor, or a thinner bar on one monitor:

```lua
return {
    bar = {
        length = monitor.width - 600,
        thickness = if monitor.name == "DP-1" then 30 else 44,
    },
}
```

At its start, before it knows any monitor, tonish runs the file once to check it, for a monitor 1920 by 1080 pixels at scale 1 with `name` set to `nil`, so the file has to run for that one too. tonish reads the monitors when they appear, and doesn't run the file again when a monitor's mode or scale changes; restart it then.

The file runs in a sandbox, with Luau's standard libraries such as `math` and `string`, but nothing that reads or writes files, and with its globals read-only. It has 500 ms and 64 MiB of memory for each run.

## Completion in your editor

tonish writes `tonish.d.luau` beside `config.luau`, with the types of the settings, `monitor` and the constants such as `Orientation`. It writes the file on every start where it differs, so that it matches the tonish that runs; changes you make to it don't last. Load it in [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp), and your editor completes the settings and their values, and marks a misspelt one, in a table annotated with the type `Settings`, as the shipped file's is:

```lua
local settings: Settings = {
    bar = { orientation = Orientation.Vertical },
}

return settings
```

In Zed, add the file to luau-lsp's definitions, with its full path, in the settings of Zed or of the folder:

```json
{
  "lsp": {
    "luau-lsp": {
      "settings": {
        "definitions": ["/home/you/.config/tonish/tonish.d.luau"]
      }
    }
  }
}
```

On the command line, luau-lsp checks the file the same way:

```sh
luau-lsp analyze --platform standard --definitions @tonish="$HOME/.config/tonish/tonish.d.luau" ~/.config/tonish/config.luau
```

tonish itself doesn't check the types. A table without the annotation works the same, and a misspelt setting or constant is an error when tonish runs the file either way.

## The settings

One setting stands above the sections and applies to the whole shell:

| Setting | Values | Default |
|---|---|---|
| `border` | The width of the line around the bar and every panel, in pixels. `0` draws none. One width for all of them, so that the bar's line always runs on round the panels that open from it: no line runs between a panel and the bar, and the bar's line turns into the panel's where they meet, round its concave corners or at a sharp one. On a bar that curves out into its edge, the line keeps its width round the curve and runs out into the edge. | `0` |

The rest come in three sections: `bar` for the bar along the edge of the monitor, `panel` for every panel that opens from the bar, such as the [settings panel](#the-settings-panel), and `animation` for how panels move.

| `bar` | Values | Default |
|---|---|---|
| `orientation` | `Orientation.Horizontal` puts the bar along the top of the monitor, `Orientation.Vertical` down the left. | `Orientation.Horizontal` |
| `thickness` | The bar's thickness in pixels, at least 1. | `44` |
| `length` | The bar's length along its edge in pixels, `0` or at least 1. `0` spans the whole monitor; a shorter bar sits in the middle of the edge. Windows keep clear of the edge's whole length either way. | `0` |
| `gap` | How far the bar keeps from its edge, in pixels. Windows keep clear of the gap too. | `0` |
| `radius` | How round the bar's corners are, in pixels: a table of the corners `top_left`, `top_right`, `bottom_right` and `bottom_left`, such as `radius = { bottom_left = 12, bottom_right = 12 }`, in which a corner left out is square, or one number for every corner, such as `radius = 12`. More than half the thickness counts as half. | `0` |
| `inverse_radius` | Only for a bar shorter than the monitor, with a `gap` of 0: its corners at the edge curve out into the edge with this radius, in pixels. It goes up to the thickness less the radius of the far corners or the border, whichever is larger, so that the curve ends before the far corner's rounding begins. `0` rounds those corners like the others. | `0` |

| `panel` | Values | Default |
|---|---|---|
| `radius` | How round a panel's corners away from the bar are, in pixels, as for the bar. The corners at the bar follow `inverse_radius` instead. | `14` |
| `inverse_radius` | A panel is part of the bar it opens from, and curves out of it at its corners there with this radius, in pixels. `0` meets the bar in a sharp corner. | `14` |

| `animation` | Values | Default |
|---|---|---|
| `max_fps` | The most frames a second an animation draws, such as the settings panel as it opens and closes, at least 0. `0` draws every frame the compositor offers, at the monitor's refresh rate. A lower number, such as `60` on a 165 Hz monitor, spares the GPU and the CPU, and the animation takes as long. | `0` |

Pixels here are logical pixels, as your compositor counts them: on a monitor at scale 2, a thickness of 44 is 88 physical pixels.

## The settings panel

Right-click the bar to open the settings panel. It grows out of the bar's inner edge like a drop, centred where you clicked, though no nearer the bar's ends than its straight edge allows, below a bar along the top, to the right of one down the left, shaped by the `panel` settings. Escape, a click outside it, or a left click on the bar folds it back into the bar, where it leaves waves on the bar's edge, as on water after a drop falls in: they run apart along the edge and die away within a second and a half. A click outside tonish only closes it and doesn't reach the window you clicked: sway and Hyprland keep it from the window while a panel is open. It shows no settings yet; the file is still the place to change them.

## Changes apply when you save

tonish watches the file and applies a change as soon as you save it, without a restart: it runs the file again for every monitor, and the bar changes in place, without disappearing in between. Editors that save by writing a new file and renaming it over the old one work too. A save that changes nothing leaves the bar alone.

A broken file doesn't stop tonish. If the file fails for any monitor, tonish keeps the settings it had on all of them, says what's wrong in its [log](logging.md), and applies the file again at the next save. An error names the setting, or the line for an error in the script itself:

```text
   0.022064184s  WARN tonish::shell: keeping the settings from before: ~/.config/tonish/config.luau: bar.thickness: thickness 0 is less than 1 pixel
   0.042260033s  WARN tonish::shell: keeping the settings from before: ~/.config/tonish/config.luau:2: attempt to perform arithmetic (sub) on nil and number
```

A monitor that appears while tonish runs, and that the file fails for, gets the default settings, with a warning.

## What counts as broken

- **A script that doesn't run**, such as one with a brace left open or one that does arithmetic on `nil`, is an error, which names the line. So is one that runs for longer than 500 ms or uses more than 64 MiB of memory.
- **A script that returns anything but a table** is an error.
- **A misspelt setting** is an error, not ignored without a word. The error names the settings the section has.
- **A misspelt constant**, such as `Orientation.Horizonal`, is an error, which names the line and the constants there are.
- **A setting outside its section**, such as `thickness` outside `bar`, is an error, which names the sections.
- **A value of the wrong kind** is an error, such as `orientation = 2`, `thickness = "wide"`, or a `radius` with a corner `top`.
- **A thickness under 1, or a length between 0 and 1,** is an error. The compositor would end tonish's connection for a bar 0 pixels thick or long.
- **A negative gap, radius, inverse radius, border or `max_fps`** is an error.

At start, a broken file is an error that ends tonish: it prints what's wrong and exits with 1.

## Another file

`tonish --config FILE` reads FILE instead, and watches it the same way. FILE has to exist. Quietly falling back to the defaults would hide a typo in its name.
