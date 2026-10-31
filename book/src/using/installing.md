# Installing

## From a release

Every [release](https://github.com/tonigineer/tonish/releases) comes with a package for x86-64 Linux, `tonish-vX.Y.Z-x86_64-linux.tar.gz`, and a `.sha256` file to check it against:

```sh
sha256sum --check tonish-v0.0.3-x86_64-linux.tar.gz.sha256
tar xzf tonish-v0.0.3-x86_64-linux.tar.gz
install -Dm755 tonish-v0.0.3-x86_64-linux/tonish ~/.local/bin/tonish
```

The release binary is built for x86-64-v2, which practically every x86-64 CPU still in use supports.

The package also holds `config.toml`, the default settings. You don't need to copy it: tonish writes the same file on its first start (see [Settings](settings.md)).

## From source

With the tools from [Getting started](../developing/getting-started.md):

```sh
just install              # builds tonish and installs it to ~/.local/bin
just install ~/bin        # or to a folder of your choice
```

This build is tuned to the CPU it was built on, so it may not run on another machine. For that, use a release.

## Starting it with your session

Start tonish from your compositor's configuration, for example:

| Compositor | Line |
|---|---|
| sway | `exec tonish` |
| Hyprland | `exec-once = tonish` |
| niri | `spawn-at-startup "tonish"` |

The compositor has to find `tonish` on its own `PATH`, which isn't always the one your shell has. If it doesn't start, give the full path, such as `~/.local/bin/tonish`.
