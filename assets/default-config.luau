-- tonish settings
--
-- tonish wrote this file on its first start. Edit it, and tonish applies the
-- changes as soon as you save.
-- Delete it to get the defaults back on the next start. A setting you leave
-- out takes its default.
--
-- The file is Luau. It runs once for every monitor, and returns that
-- monitor's settings. `monitor` describes it: its name, such as "DP-1", and
-- its width, height and scale, in pixels as your compositor counts them. A
-- setting can follow it, such as
--
--     length = monitor.width - 600,
--
-- for a bar 300 pixels short of either end of every monitor.
--
-- tonish writes tonish.d.luau beside this file: load it in luau-lsp, and
-- your editor completes the settings and marks a misspelt one.

local settings: Settings = {
    -- The width of the line around the bar and every panel, in pixels. 0
    -- draws none. One width for all of them, so that the bar's line runs on
    -- round the panels that open from it.
    border = 0,

    -- The bar along an edge of every monitor.
    bar = {
        -- Orientation.Horizontal puts the bar along the top of every
        -- monitor, Orientation.Vertical down the left.
        orientation = Orientation.Horizontal,

        -- How thick the bar is, in pixels.
        thickness = 44,

        -- How long the bar is along its edge, in pixels. 0 spans the whole
        -- monitor; a shorter bar sits in the middle of the edge.
        length = 0,

        -- How far the bar keeps from its edge, in pixels.
        gap = 0,

        -- How round the bar's corners are, in pixels, up to half its
        -- thickness. One number, such as radius = 12, rounds every corner
        -- alike. A corner left out of the table is square.
        radius = {
            top_left = 0,
            top_right = 0,
            bottom_right = 0,
            bottom_left = 0,
        },

        -- Only for a bar shorter than the monitor, with a gap of 0: it curves
        -- out into its edge at its corners there, with this radius in
        -- pixels, up to its thickness less the radius of its far corners or
        -- the border, whichever is larger. 0 rounds those corners like the
        -- others instead.
        inverse_radius = 0,
    },

    -- Every panel that opens from the bar, such as the settings, which open
    -- on a right-click on the bar.
    panel = {
        -- How round a panel's corners away from the bar are, in pixels, as
        -- for the bar. The corners at the bar follow inverse_radius instead.
        radius = {
            top_left = 14,
            top_right = 14,
            bottom_right = 14,
            bottom_left = 14,
        },

        -- A panel is part of the bar it opens from, and curves out of it at
        -- its corners there, with this radius in pixels. 0 meets the bar in a
        -- sharp corner. No line runs between a panel and the bar: the bar's
        -- line turns into the panel's where they meet.
        inverse_radius = 14,
    },

    -- How panels move, such as the settings as they open, close and are
    -- pulled off the bar. Most motions are springs: stiffness sets how fast
    -- one moves, at least 1, and damping how it comes to rest: 1 without
    -- overshooting, lower overshoots and wobbles for longer, at least 0.05.
    animation = {
        -- The most frames a second an animation draws, such as 60 to spare
        -- the GPU on a fast monitor. 0 draws every frame the monitor shows.
        max_fps = 0,

        -- A panel growing out of the bar.
        popout = { stiffness = 260, damping = 0.5 },

        -- A panel folding back into the bar.
        fold = { stiffness = 420, damping = 0.85 },

        -- A panel following the pointer while its grip is held.
        drag = { stiffness = 420, damping = 0.6 },

        -- Pulling a panel off the bar by its grip.
        detaching = {
            -- How far out, in pixels, the neck between them breaks, at
            -- least 20.
            distance = 140,
            -- How much the bar holds the panel back while the neck holds:
            -- from 0, not at all, to 0.9.
            pull = 0.35,
            -- How thick the neck is: 1 as shipped, 2 twice as thick.
            thickness = 1,
            -- How hard the bar's edge springs back as the neck breaks, in
            -- pixels a second.
            kick = 260,
        },

        -- A panel let go while a neck joins it to the bar, docking there,
        -- and how hard the bar's edge springs out as it arrives.
        attaching = { stiffness = 320, damping = 0.45, kick = 200 },

        -- How much a panel squashes and stretches as it moves, 0 not at
        -- all, and how it wobbles once it stops.
        wobble = { amount = 1, stiffness = 220, damping = 0.22 },

        -- The waves on the bar's edge where a panel leaves or joins it: how
        -- high they rise in pixels and how many seconds they run. The
        -- spring is the bar's edge ringing when kicked.
        ripples = { height = 7, duration = 1.5, stiffness = 400, damping = 0.25 },
    },

    -- A frame along every edge of the monitor, which the bar is part of.
    -- Where the frame meets the bar, the two curve into each other. Both
    -- width and inverse_radius at 0 leave the frame out.
    frame = {
        -- How wide the frame is along every edge, in pixels. A bar with no
        -- gap takes the frame's place along its edge; with a gap, it sits
        -- inside the frame. 0 leaves out the frame's sides, and only its
        -- curved corners show.
        width = 0,

        -- How round the frame's inside corners are, in pixels: where two of
        -- its sides meet, and where it meets a bar that spans the monitor.
        -- Beside a bar with a gap, at most half the gap. A shorter bar
        -- curves into the frame with the bar's own inverse_radius.
        inverse_radius = 0,

        -- The frame's colour, as #rrggbb, or #rrggbbaa with its opacity.
        color = "#111318",
    },

    -- The monitor's own corners, rounded over everything on it, the frame
    -- and the bar too, as on a screen whose glass is round there.
    corners = {
        -- How round, in pixels. 0 leaves them square.
        radius = 0,

        -- What shows outside the curve, as #rrggbb, or #rrggbbaa with its
        -- opacity: black, as the screen's edge is.
        color = "#000000",
    },

    -- Figures on how tonish runs, in the top left corner below the bar:
    -- frames a second, how long a frame takes, CPU, memory and the GPU.
    hud = {
        -- false hides them. `tonish msg hud` hides or shows them until the
        -- next save that changes this.
        show = true,
    },
}

return settings
