import os

from harness import wait_for


def test_editing_the_settings_moves_the_panel(desktop):
    desktop.wait_for_panels()
    path = desktop.settings
    # Most editors write a new file and rename it over the old one.
    new = path.with_name("config.toml.new")
    new.write_text('orientation = "vertical"\nthickness = 30\n')
    os.replace(new, path)
    left = [(30, 0, 1920 - 30, 1080)] * 2
    assert wait_for(lambda: desktop.workspaces() == left), (
        f"after a rename: {desktop.workspaces()}"
    )
    # Others write into the file itself.
    path.write_text('orientation = "horizontal"\nthickness = 60\n')
    top = [(0, 60, 1920, 1080 - 60)] * 2
    assert wait_for(lambda: desktop.workspaces() == top), (
        f"after a write in place: {desktop.workspaces()}"
    )


def test_a_broken_edit_keeps_the_panel(desktop):
    desktop.wait_for_panels()
    before = desktop.workspaces()
    desktop.settings.write_text('orientation = "vertical"\nthickness = 0\n')
    assert wait_for(lambda: "keeping the settings from before" in desktop.logged()), (
        "tonish did not report the broken file"
    )
    assert desktop.tonish.poll() is None
    assert desktop.workspaces() == before
