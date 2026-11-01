import os

import pytest
from harness import wait_for


def test_editing_the_settings_moves_the_panel(desktop):
    desktop.wait_for_panels()
    path = desktop.settings
    # Most editors write a new file and rename it over the old one.
    new = path.with_name("config.luau.new")
    new.write_text(
        "return { bar = { orientation = Orientation.Vertical, thickness = 30 } }\n"
    )
    os.replace(new, path)
    left = [(30, 0, 1920 - 30, 1080)] * 2
    assert wait_for(lambda: desktop.workspaces() == left), (
        f"after a rename: {desktop.workspaces()}"
    )
    # Others write into the file itself.
    path.write_text(
        "return { bar = { orientation = Orientation.Horizontal, thickness = 60 } }\n"
    )
    top = [(0, 60, 1920, 1080 - 60)] * 2
    assert wait_for(lambda: desktop.workspaces() == top), (
        f"after a write in place: {desktop.workspaces()}"
    )


def test_a_broken_edit_keeps_the_panel(desktop):
    desktop.wait_for_panels()
    before = desktop.workspaces()
    desktop.settings.write_text(
        "return { bar = { orientation = Orientation.Vertical, thickness = 0 } }\n"
    )
    assert wait_for(lambda: "keeping the settings from before" in desktop.logged()), (
        "tonish did not report the broken file"
    )
    assert desktop.tonish.poll() is None
    assert desktop.workspaces() == before


def edit(desktop, text):
    """The bar's settings, on top of the default thickness."""
    desktop.wait_for_panels()
    desktop.settings.write_text(f"return {{ bar = {{ thickness = 44, {text} }} }}\n")
    assert wait_for(lambda: "settings changed" in desktop.logged()), desktop.logged()


def test_a_gap_keeps_the_panel_off_its_edge_and_adds_to_its_room(desktop):
    edit(desktop, "gap = 10")
    want = [(0, 54, 1920, 1080 - 54)] * 2
    assert wait_for(lambda: desktop.workspaces() == want), desktop.workspaces()


def test_a_shorter_panel_keeps_its_room(desktop):
    edit(desktop, "length = 600, radius = 12, inverse_radius = 12")
    want = [(0, 44, 1920, 1080 - 44)] * 2
    assert wait_for(lambda: desktop.workspaces() == want), desktop.workspaces()


@pytest.mark.parametrize("args", [(), ("--disable-gpu",)], ids=["gpu", "cpu"])
def test_an_edit_reshapes_the_panels_in_place(desktops, args):
    """In place rather than destroyed and made again, which blinks."""
    d = desktops(*args)
    d.wait_for_panels()
    if not args:
        d.wait_for_gpu()
    d.settings.write_text(
        "return { bar = { orientation = Orientation.Horizontal, thickness = 60 } }\n"
    )
    top = [(0, 60, 1920, 1080 - 60)] * 2
    assert wait_for(lambda: d.workspaces() == top), d.workspaces()
    assert wait_for(lambda: d.logged().count("size=(3840, 120)") >= 2), d.logged()
    log = d.logged()
    assert log.count("surface made name=panel") == 2, log
    assert log.count("surface reshaped") == 2, log
    # Only at the new size: drawn before the compositor sends it, the bar
    # would show cut to the old size for a frame.
    after = log.split("settings changed", 1)[1]
    sizes = [
        line.split("size=")[1].split(")")[0] + ")"
        for line in after.splitlines()
        if "drew monitor=" in line and " surface=0 " in line
    ]
    assert sizes == ["(3840, 120)"] * 2, after
