import time

import pytest
from harness import near, wait_for


def test_msg_redraw_draws_every_panel_once_more(desktop):
    desktop.wait_for_panels()
    # The GPU's draws when it takes over must not count as the redraw's.
    desktop.wait_for_gpu()
    # The bars only: the figures in the corner draw on their own.
    before = desktop.draws("0")
    sent = desktop.run_tonish("msg", "redraw")
    assert sent.returncode == 0, sent.stderr
    assert wait_for(lambda: desktop.draws("0") == before + 2), (
        f"{desktop.draws('0') - before} draws after msg redraw, not one per monitor"
    )
    time.sleep(0.2)
    assert desktop.draws("0") == before + 2, "more than one draw per monitor"


@pytest.mark.local
def test_the_bar_is_44_px_high(desktop):
    want = [(0, 44, 1920, 1080 - 44)] * 2
    assert wait_for(lambda: desktop.workspaces() == want), (
        f"room for windows: {desktop.workspaces()}"
    )


@pytest.mark.local
def test_the_bar_is_drawn_with_its_logo(desktop):
    surface, primary = (0x11, 0x13, 0x18), (0xC9, 0xA9, 0x6E)
    assert wait_for(lambda: near(desktop.at(200, 22), surface)), (
        f"the bar at (200, 22): {desktop.at(200, 22)}"
    )
    assert near(desktop.at(24, 22), primary), f"the logo's dot: {desktop.at(24, 22)}"
    # Right of the figures in the corner, which start below the bar.
    assert not near(desktop.at(600, 60), surface), "the bar reaches past 44 px"


def shaped(desktop, text):
    desktop.wait_for_panels()
    desktop.settings.write_text(f"return {{ bar = {{ thickness = 44, {text} }} }}\n")
    assert wait_for(lambda: "settings changed" in desktop.logged()), desktop.logged()


@pytest.mark.local
def test_a_shorter_panel_hangs_from_the_top_through_concave_corners(desktop):
    surface = (0x11, 0x13, 0x18)
    shaped(desktop, "length = 600, radius = 12, inverse_radius = 12")
    # 600 px in the middle of 1920: from 660 to 1260.
    assert wait_for(lambda: not near(desktop.at(100, 22), surface)), desktop.at(100, 22)
    assert near(desktop.at(960, 22), surface), desktop.at(960, 22)
    # Left of it, the concave corner: filled at the monitor's edge, empty below.
    assert near(desktop.at(659, 0), surface), desktop.at(659, 0)
    assert not near(desktop.at(650, 10), surface), desktop.at(650, 10)
    # Its bottom-left corner, rounded.
    assert not near(desktop.at(661, 43), surface), desktop.at(661, 43)
    assert near(desktop.at(672, 40), surface), desktop.at(672, 40)


@pytest.mark.local
def test_a_panel_off_its_edge_rounds_every_corner(desktop):
    surface = (0x11, 0x13, 0x18)
    shaped(desktop, "length = 600, radius = 12, gap = 10")
    # From 10 down: the gap above it shows what is behind.
    assert wait_for(lambda: not near(desktop.at(960, 5), surface)), desktop.at(960, 5)
    assert near(desktop.at(960, 30), surface), desktop.at(960, 30)
    # Its top-left corner, rounded, with no concave corner beside it.
    assert not near(desktop.at(661, 11), surface), desktop.at(661, 11)
    assert near(desktop.at(672, 22), surface), desktop.at(672, 22)
    assert not near(desktop.at(650, 12), surface), desktop.at(650, 12)


def test_draw_always_draws_far_more_often_than_the_monitor_refreshes(desktops):
    d = desktops("--draw-always")
    d.wait_for_panels()
    assert wait_for(lambda: d.draws("0") > 10), d.logged()
    bars, figures = d.draws("0"), d.draws("1")
    time.sleep(1)
    bars, figures = d.draws("0") - bars, d.draws("1") - figures
    # The headless monitors refresh 60 times a second, two of them.
    assert bars > 4 * 60, f"{bars} draws of the bars in a second"
    # The figures keep their own few a second, which they are capped to.
    assert figures <= 8, f"{figures} draws of the figures in a second"
