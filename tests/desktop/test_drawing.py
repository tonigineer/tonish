import time

import pytest
from harness import near, wait_for


def test_msg_redraw_draws_every_panel_once_more(desktop):
    desktop.wait_for_panels()
    assert wait_for(lambda: desktop.draws() >= 2), "the panels were never drawn"
    before = desktop.draws()
    sent = desktop.run_tonish("msg", "redraw")
    assert sent.returncode == 0, sent.stderr
    assert wait_for(lambda: desktop.draws() == before + 2), (
        f"{desktop.draws() - before} draws after msg redraw, not one per monitor"
    )
    time.sleep(0.2)
    assert desktop.draws() == before + 2, "more than one draw per monitor"


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
    assert not near(desktop.at(200, 60), surface), "the bar reaches past 44 px"
