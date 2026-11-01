-- Types of tonish's settings, for luau-lsp, which then completes the
-- settings and their values in config.luau and marks a misspelt one.
--
-- tonish writes this file beside config.luau on every start, so that it
-- always matches the tonish that runs; changes to it don't last. Load it in
-- luau-lsp's "definitions" setting, or on its command line:
--
--     luau-lsp analyze --definitions @tonish=tonish.d.luau config.luau

export type Orientation = "horizontal" | "vertical"

export type Radius = number | {
    top_left: number?,
    top_right: number?,
    bottom_right: number?,
    bottom_left: number?,
}

export type Settings = {
    border: number?,
    bar: {
        orientation: Orientation?,
        thickness: number?,
        length: number?,
        gap: number?,
        radius: Radius?,
        inverse_radius: number?,
    }?,
    panel: {
        radius: Radius?,
        inverse_radius: number?,
    }?,
}

declare monitor: {
    name: string?,
    width: number,
    height: number,
    scale: number,
}

declare Orientation: {
    Horizontal: Orientation,
    Vertical: Orientation,
}
