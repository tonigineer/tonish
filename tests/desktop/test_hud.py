import shutil
import subprocess
import time

import pytest
from harness import near, wait_for


def monospace():
    """The font tonish sets the figures in, as it finds it: fontconfig alone,
    without a font installed, names none."""
    if shutil.which("fc-match") is None:
        return None
    # Not checked: no font is no error, only no file named.
    found = subprocess.run(
        ["fc-match", "--format=%{file}", "monospace"],
        capture_output=True,
        text=True,
        check=False,
    )
    return found.stdout.strip() or None


# The figures sit below the bar, 8 px in from the monitor's corner: from
# (8, 52) to (308, 144).
INSIDE = (150, 130)
BESIDE = (400, 130)
PRIMARY = (0xC9, 0xA9, 0x6E)


def covered(desktop):
    return desktop.at(*INSIDE) != desktop.at(*BESIDE)


def test_the_figures_show_in_the_top_left_corner(desktop):
    desktop.wait_for_panels()
    assert wait_for(lambda: desktop.logged().count("surface made name=hud") == 2)
    assert wait_for(lambda: covered(desktop)), (
        desktop.at(*INSIDE),
        desktop.at(*BESIDE),
    )
    # Windows keep the room the bar keeps, and no more.
    assert desktop.workspaces() == [(0, 44, 1920, 1080 - 44)] * 2


@pytest.mark.skipif(monospace() is None, reason="needs fontconfig and a font")
def test_the_figures_are_text(desktop):
    desktop.wait_for_panels()

    # The first line, "tonish" and its version, in the theme's primary colour.
    def text():
        w, px = desktop.pixels()
        return any(
            near(tuple(px[(y * w + x) * 3 : (y * w + x) * 3 + 3]), PRIMARY, by=20)
            for y in range(56, 74)
            for x in range(14, 120)
        )

    assert wait_for(text), "no text in the first line of the figures"


def test_msg_hud_hides_and_shows_the_figures(desktop):
    desktop.wait_for_panels()
    assert wait_for(lambda: covered(desktop))
    sent = desktop.run_tonish("msg", "hud")
    assert sent.stdout.strip() == "hidden", sent.stdout + sent.stderr
    assert wait_for(lambda: not covered(desktop)), desktop.at(*INSIDE)
    sent = desktop.run_tonish("msg", "hud")
    assert sent.stdout.strip() == "shown", sent.stdout + sent.stderr
    assert wait_for(lambda: covered(desktop)), desktop.at(*INSIDE)


def test_the_figures_draw_a_few_times_a_second(desktop):
    desktop.wait_for_panels()
    assert wait_for(lambda: desktop.draws("1") >= 2)
    before = desktop.draws("1")
    time.sleep(2)
    # Two a second on each of the two monitors.
    drawn = desktop.draws("1") - before
    assert 4 <= drawn <= 12, f"{drawn} draws of the figures in 2 s"


def test_hud_show_false_hides_the_figures_until_msg_hud(desktop):
    desktop.wait_for_panels()
    assert wait_for(lambda: covered(desktop))
    desktop.settings.write_text("return { hud = { show = false } }\n")
    assert wait_for(lambda: not covered(desktop)), desktop.at(*INSIDE)
    sent = desktop.run_tonish("msg", "hud")
    assert sent.stdout.strip() == "shown", sent.stdout + sent.stderr
    assert wait_for(lambda: covered(desktop)), desktop.at(*INSIDE)
