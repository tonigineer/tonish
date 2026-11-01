import pytest
from harness import near, wait_for

CARD = (0x11, 0x13, 0x18)
# Inside the card that a right-click at (800, 20) opens, below the bar: 280
# wide and centred on the click, from 660 to 940. Clear of the figures in the
# top left corner, which reach to 308.
INSIDE = (800, 120)


def shown(desktop):
    return near(desktop.at(*INSIDE), CARD)


def opened(desktop):
    desktop.wait_for_panels()
    desktop.click(800, 20, "BTN_RIGHT")
    assert wait_for(lambda: shown(desktop)), f"at {INSIDE}: {desktop.at(*INSIDE)}"
    assert "popup opened" in desktop.logged(), desktop.logged()
    # The keyboard and the clicks outside, a frame after it shows.
    assert wait_for(lambda: "popup grabbed" in desktop.logged()), desktop.logged()


def test_a_right_click_on_the_bar_hangs_the_settings_below_it(desktop):
    opened(desktop)
    # Once it has stopped bouncing.
    assert wait_for(lambda: stopped(desktop)), desktop.logged()
    # From the bar's bottom edge, 44 px down, as tall as the card, and
    # centred on the click.
    assert near(desktop.at(800, 222), CARD), desktop.at(800, 222)
    assert not near(desktop.at(800, 228), CARD), desktop.at(800, 228)
    assert not near(desktop.at(656, 120), CARD), desktop.at(656, 120)
    assert near(desktop.at(664, 120), CARD), desktop.at(664, 120)
    assert near(desktop.at(936, 120), CARD), desktop.at(936, 120)
    assert not near(desktop.at(944, 120), CARD), desktop.at(944, 120)


@pytest.mark.local
def test_the_settings_curve_out_of_the_bar_and_round_their_free_corners(desktop):
    opened(desktop)
    assert wait_for(lambda: stopped(desktop)), desktop.logged()
    # The concave corner left of the card, below the bar: filled where the
    # bar's edge meets the card's side, empty further out.
    assert near(desktop.at(658, 45), CARD), desktop.at(658, 45)
    assert not near(desktop.at(648, 56), CARD), desktop.at(648, 56)
    # The card's bottom-left corner, rounded with a radius of 14.
    assert not near(desktop.at(661, 223), CARD), desktop.at(661, 223)
    assert near(desktop.at(674, 220), CARD), desktop.at(674, 220)


def test_a_left_click_on_the_bar_closes_the_settings(desktop):
    opened(desktop)
    desktop.click(600, 20, "BTN_LEFT")
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)


def test_a_click_outside_folds_the_settings_back(desktop):
    opened(desktop)
    desktop.click(1000, 600, "BTN_LEFT")
    assert wait_for(lambda: "popup dismissed" in desktop.logged()), desktop.logged()
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)
    # Its surface goes once it has folded back.
    assert wait_for(lambda: "popup closed" in desktop.logged()), desktop.logged()


def popup_draws(desktop):
    return desktop.logged().count("surface=popup")


def test_the_settings_animate_as_they_open_and_then_stop_drawing(desktop):
    opened(desktop)
    # Opening takes about half a second, a frame every 16 ms at 60 Hz.
    assert wait_for(lambda: popup_draws(desktop) >= 10), desktop.logged()
    assert wait_for(lambda: stopped(desktop)), desktop.logged()


def test_max_fps_caps_the_frames_of_an_animation(desktop):
    desktop.wait_for_panels()
    # Opening without a bounce, which takes 0.6 s, so that the count
    # depends on the cap alone.
    desktop.settings.write_text(
        "return { animation = { max_fps = 10, popout = { damping = 1 } } }\n"
    )
    assert wait_for(lambda: "settings changed" in desktop.logged()), desktop.logged()
    desktop.click(800, 20, "BTN_RIGHT")
    assert wait_for(lambda: shown(desktop)), desktop.at(*INSIDE)
    assert wait_for(lambda: stopped(desktop), seconds=5), desktop.logged()
    # Some 36 frames at 60 Hz, 7 at 10 a second.
    assert 3 <= popup_draws(desktop) <= 11, desktop.logged()


def stopped(desktop):
    """No draw for 0.3 s, longer than any pause between two frames."""
    before = popup_draws(desktop)
    wait_for(lambda: False, seconds=0.3)
    return popup_draws(desktop) == before


def test_escape_closes_the_settings(desktop):
    opened(desktop)
    desktop.key("Escape")
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)


# The grip lies along the middle of the card's edge at the bar, 9 px below it.
GRIP = (800, 53)


def test_pulled_off_by_its_grip_the_card_stays_where_it_is_let_go(desktop):
    opened(desktop)
    desktop.drag([GRIP, (800, 300), (800, 600)], seconds=0.6)
    # Taken 9 px below its top, the card now runs from 591 to 771.
    assert wait_for(lambda: near(desktop.at(800, 680), CARD)), desktop.at(800, 680)
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)
    # Free, it holds no grab, so a click elsewhere leaves it be.
    desktop.click(1500, 400, "BTN_LEFT")
    assert not wait_for(lambda: "popup dismissed" in desktop.logged(), seconds=0.5)
    assert near(desktop.at(800, 680), CARD), desktop.at(800, 680)


def test_dragged_back_to_the_bar_the_card_hangs_from_it_again(desktop):
    opened(desktop)
    desktop.drag([GRIP, (800, 300), (800, 600)], seconds=0.6)
    assert wait_for(lambda: near(desktop.at(800, 680), CARD)), desktop.at(800, 680)
    desktop.drag([(800, 600), (800, 300), (800, 60)], seconds=0.6)
    assert wait_for(lambda: shown(desktop)), desktop.at(*INSIDE)
    assert wait_for(lambda: not near(desktop.at(800, 680), CARD)), desktop.at(800, 680)


def test_pulled_off_the_card_stops_drawing_once_it_rests(desktop):
    opened(desktop)
    desktop.drag([GRIP, (800, 300), (800, 600)], seconds=0.6)
    # The waves the break leaves run for 1.5 s.
    assert wait_for(lambda: stopped(desktop), seconds=5), desktop.logged()


def test_docked_again_a_press_on_its_grip_takes_the_grab_back(desktop):
    opened(desktop)
    desktop.drag([GRIP, (800, 300), (800, 600)], seconds=0.6)
    assert wait_for(lambda: near(desktop.at(800, 680), CARD)), desktop.at(800, 680)
    desktop.drag([(800, 600), (800, 300), (800, 60)], seconds=0.6)
    assert wait_for(lambda: shown(desktop)), desktop.at(*INSIDE)
    assert wait_for(lambda: stopped(desktop)), desktop.logged()
    # Only the grab hears of a click outside every surface of tonish, and it
    # starts a frame after the press, once the card has the keyboard.
    desktop.click(*GRIP, "BTN_LEFT")
    assert wait_for(lambda: desktop.logged().count("popup grabbed") == 2)
    desktop.click(1000, 600, "BTN_LEFT")
    assert wait_for(lambda: "popup dismissed" in desktop.logged()), desktop.logged()
    assert wait_for(lambda: not shown(desktop)), desktop.at(*INSIDE)
