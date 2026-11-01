# Settings

tonish reads its settings from `$XDG_CONFIG_HOME/tonish/config.toml`, which is `~/.config/tonish/config.toml` unless `XDG_CONFIG_HOME` is set to an absolute path. On its first start, tonish writes that file with the default settings and a comment on each one:

```toml
{{#include ../../../assets/default-config.toml}}
```

To get the defaults back, delete the file; tonish writes it again on its next start. If it can't write the file at all, for example because your home is read-only, or if neither `XDG_CONFIG_HOME` nor `HOME` is set, it starts with the built-in settings, which are the same, and logs a warning.

| Setting | Values | Default |
|---|---|---|
| `orientation` | `"horizontal"` puts the panel along the top of every monitor, `"vertical"` down the left. | `"horizontal"` |
| `thickness` | The panel's thickness in pixels, at least 1. | `44` |

Pixels here are logical pixels, as your compositor counts them: on a monitor at scale 2, a thickness of 44 is 88 physical pixels.

## Changes apply when you save

tonish watches the file and applies a change as soon as you save it, without a restart. Editors that save by writing a new file and renaming it over the old one work too. A save that changes nothing leaves the panels alone.

A broken file doesn't stop tonish. It keeps the settings it had, says what's wrong in its [log](logging.md), and applies the file again at the next save:

```text
   0.822375257s  WARN tonish::shell: keeping the settings from before: ~/.config/tonish/config.toml: TOML parse error at line 2, column 13
  |
2 | thickness = 0
  |             ^
thickness 0 is less than 1 pixel
```

## What counts as broken

- **A misspelt setting** is an error, not ignored without a word.
- **A missing setting** is an error. Every setting has to be in the file.
- **A value of the wrong kind** is an error, such as `orientation = "diagonal"`.
- **A thickness under 1** is an error. The compositor would end tonish's connection for a panel 0 pixels thick.

At start, a broken file is an error that ends tonish: it prints what's wrong and exits with 1.

## Another file

`tonish --config FILE` reads FILE instead, and watches it the same way. FILE has to exist. Quietly falling back to the defaults would hide a typo in its name.
