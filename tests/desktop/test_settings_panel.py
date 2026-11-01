import shutil

import pytest
from harness import near, wait_for

CARD = (0x11, 0x13, 0x18)
# Inside the card that a right-click at (200, 20) opens, below the bar.
INSIDE = (340, 120)


def shown(desktop):
    return near(desktop.at(*INSIDE), CARD)


def opened(desktop):
    desktop.wait_for_panels()
    desktop.click(200, 20, "BTN_RIGHT")
    assert wait_for(lambda: shown(desktop)), f"at {INSIDE}: {desktop.at(*INSIDE)}"
    assert "popup opened" in desktop.logged(), desktop.logged()


def test_a_right_click_on_the_bar_hangs_the_settings_below_it(desktop):
    opened(desktop)
    # From the bar's bottom edge, 44 px down, as tall as the card, and with
    # its left edge where the click was.
    assert near(desktop.at(340, 222), CARD), desktop.at(340, 222)
    assert not near(desktop.at(340, 228), CARD), desktop.at(340, 228)
    assert not near(desktop.at(196, 120), CARD), desktop.at(196, 120)


@pytest.mark.local
def test_the_settings_curve_out_of_the_bar_and_round_their_free_corners(desktop):
    opened(desktop)
    # The concave corner left of the card, below the bar: filled where the
    # bar's edge meets the card's side, empty further out.
    assert near(desktop.at(198, 45), CARD), desktop.at(198, 45)
    assert not near(desktop.at(188, 56), CARD), desktop.at(188, 56)
    # The card's bottom-left corner, rounded with a radius of 14.
    assert not near(desktop.at(201, 223), CARD), desktop.at(201, 223)
    assert near(desktop.at(214, 220), CARD), desktop.at(214, 220)


def test_a_left_click_on_the_bar_closes_the_settings(desktop):
    opened(desktop)
    desktop.click(600, 20, "BTN_LEFT")
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)


def test_a_click_outside_closes_the_settings(desktop):
    opened(desktop)
    desktop.click(1000, 600, "BTN_LEFT")
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)
    assert "popup closed" in desktop.logged(), desktop.logged()


@pytest.mark.skipif(shutil.which("wtype") is None, reason="needs wtype")
def test_escape_closes_the_settings(desktop):
    opened(desktop)
    desktop.key("Escape")
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)
