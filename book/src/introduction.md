# tonish

tonish is a desktop shell for Wayland. It puts panels on every monitor and draws them itself, pixel by pixel, with a renderer of its own.

It is early. Today there is one panel with a logo on it, and the settings file chooses its orientation and thickness. The groundwork beneath that is in place:
- settings that apply as soon as you save them;
- a clean shutdown;
- messages from scripts;
- logging;
- a renderer that draws a whole 4K screen in a few milliseconds, and moves to the GPU when that is too slow.

tonish needs a compositor that supports the wlr-layer-shell protocol, such as sway, Hyprland, niri, river or KDE's KWin. GNOME's Mutter does not support it.

This book has two parts:

- **[Using tonish](using/installing.md)**: installing it, running it, its settings and its log.
- **[Working on tonish](developing/getting-started.md)**: building and testing it, how its parts fit together, how it draws, and how a change becomes a release.
