from harness import near, wait_for

SURFACE = (0x11, 0x13, 0x18)
PIN = (0xC9, 0xA9, 0x6E)


def framed(desktop, text):
    desktop.wait_for_panels()
    desktop.settings.write_text(text + "\n")
    assert wait_for(lambda: "settings changed" in desktop.logged()), desktop.logged()


def test_windows_stay_inside_the_frame_and_below_the_bar(desktop):
    framed(desktop, "return { frame = { width = 8 } }")
    # The bar in the frame's place along the top.
    inside = [(8, 44, 1920 - 16, 1080 - 44 - 8)] * 2
    assert wait_for(lambda: desktop.workspaces() == inside), desktop.workspaces()


def test_the_frame_runs_along_the_edges(desktop):
    framed(desktop, "return { frame = { width = 8 } }")

    def drawn():
        return near(desktop.at(4, 500), SURFACE) and near(
            desktop.at(1916, 500), SURFACE
        )

    assert wait_for(drawn), (desktop.at(4, 500), desktop.at(1916, 500))
    assert not near(desktop.at(12, 500), SURFACE), desktop.at(12, 500)
    assert near(desktop.at(900, 1076), SURFACE), desktop.at(900, 1076)


def test_a_shorter_bar_hangs_from_the_frame(desktop):
    text = (
        "return { bar = { length = 1000, inverse_radius = 14 }, frame = { width = 8 } }"
    )
    framed(desktop, text)
    # The bar from 460 to 1460, from the monitor's edge; beside it, the
    # frame alone.
    assert wait_for(lambda: near(desktop.at(900, 40), SURFACE)), desktop.at(900, 40)
    assert near(desktop.at(200, 4), SURFACE), desktop.at(200, 4)
    assert not near(desktop.at(200, 20), SURFACE), desktop.at(200, 20)
    # Curving out into the frame at its end.
    assert near(desktop.at(458, 10), SURFACE), desktop.at(458, 10)
    inside = [(8, 44, 1920 - 16, 1080 - 44 - 8)] * 2
    assert desktop.workspaces() == inside, desktop.workspaces()


def test_with_a_gap_the_bar_sits_inside_the_frame(desktop):
    framed(desktop, "return { bar = { gap = 6 }, frame = { width = 8 } }")
    inside = [(8, 8 + 6 + 44, 1920 - 16, 1080 - 8 - 6 - 44 - 8)] * 2
    assert wait_for(lambda: desktop.workspaces() == inside), desktop.workspaces()


def test_the_screen_corners_are_round(desktop):
    framed(desktop, "return { corners = { radius = 20 } }")
    # Black at the monitor's very corner, over the bar; the bar's colour
    # inside the curve, at the top right, where the bar holds nothing.
    assert wait_for(lambda: near(desktop.at(1918, 1), (0, 0, 0))), desktop.at(1918, 1)
    assert near(desktop.at(1904, 16), SURFACE), desktop.at(1904, 16)
    assert near(desktop.at(1918, 1078), (0, 0, 0)), desktop.at(1918, 1078)


def test_a_translucent_frame_is_drawn_once_everywhere(desktop):
    color = 'color = "#ff131899"'
    text = "return { frame = { width = 10, inverse_radius = 16, " + color + " } }"
    framed(desktop, text)
    side = (3, 500)
    assert wait_for(lambda: desktop.at(*side)[0] > 100), desktop.at(*side)
    # Where two sides meet, as along either, and in the curves inside the
    # corners, below the bar and along the bottom. The screenshot is scaled
    # down from scale 2, which rings a few pixels around an edge between
    # colours, such as the bar's, so the points keep away from those.
    for x, y in [
        (3, 1077),
        (900, 1077),
        (1916, 1077),
        (1916, 500),
        (1909, 48),
        (11, 1068),
        (1908, 1068),
    ]:
        assert desktop.at(x, y) == desktop.at(*side), (x, y, desktop.at(x, y))


def test_a_card_pulled_off_the_bar_docks_to_the_frame_s_side_pinned(desktop):
    framed(desktop, 'return { frame = { width = 8, color = "#ff1318" } }')
    desktop.click(800, 20, "BTN_RIGHT")
    assert wait_for(lambda: "popup grabbed" in desktop.logged()), desktop.logged()
    # Off the bar by its grip, 9 below its top, and over to the left, its
    # left side 30 from the frame's and its middle 81 below the grip.
    desktop.drag([(800, 53), (800, 300), (800, 600)], seconds=0.6)
    assert wait_for(lambda: near(desktop.at(800, 680), SURFACE)), desktop.at(800, 680)
    desktop.drag([(800, 600), (500, 560), (8 + 140 + 30, 500)], seconds=0.8)
    # Docked, from the frame's side at 8 to 288; let go free, it would run
    # from 38 to 318.
    assert wait_for(lambda: not near(desktop.at(300, 581), SURFACE)), desktop.at(
        300, 581
    )
    assert near(desktop.at(12, 581), SURFACE), desktop.at(12, 581)
    assert near(desktop.at(4, 581), (0xFF, 0x13, 0x18)), desktop.at(4, 581)
    # Pinned there: the pin in its top right corner, 16 in from it, its
    # shaft just above, and it stays on a click elsewhere, the bar too.
    assert wait_for(lambda: near(desktop.at(272, 491 + 16 - 3), PIN)), desktop.at(
        272, 491 + 13
    )
    desktop.click(1500, 400, "BTN_LEFT")
    desktop.click(900, 20, "BTN_LEFT")
    assert not wait_for(lambda: "popup dismissed" in desktop.logged(), seconds=0.5)
    assert near(desktop.at(12, 581), SURFACE), desktop.at(12, 581)
    # A click on the pin closes it.
    desktop.click(272, 491 + 16, "BTN_LEFT")
    assert wait_for(lambda: "popup closed" in desktop.logged()), desktop.logged()
