from harness import ROOT, wait_for


def test_tonish_keeps_room_at_the_top_of_every_monitor(desktop):
    def kept(rooms):
        return len(rooms) == 2 and all(
            x == 0 and y > 0 and w == 1920 and y + h == 1080 for x, y, w, h in rooms
        )

    assert wait_for(lambda: kept(desktop.workspaces())), (
        f"room for windows: {desktop.workspaces()}"
    )
    assert desktop.tonish.poll() is None


def test_the_log_names_every_monitor(desktop):
    desktop.wait_for_panels()
    log = desktop.logged()
    for monitor in ("HEADLESS-1", "HEADLESS-2"):
        assert f"monitor added monitor={monitor} " in log


def test_tonish_writes_its_settings_on_first_start(desktop):
    written = desktop.settings
    shipped = (ROOT / "assets" / "default-config.luau").read_text()
    assert wait_for(lambda: written.exists() and written.read_text() == shipped), (
        f"{written} is missing or differs from assets/default-config.luau"
    )


def test_tonish_writes_the_settings_types_beside_them(desktop):
    written = desktop.settings.parent / "tonish.d.luau"
    shipped = (ROOT / "assets" / "tonish.d.luau").read_text()
    assert wait_for(lambda: written.exists() and written.read_text() == shipped), (
        f"{written} is missing or differs from assets/tonish.d.luau"
    )


def test_a_second_tonish_does_not_start(desktop):
    desktop.wait_for_panels()
    second = desktop.run_tonish()
    assert second.returncode == 1
    assert "already running" in second.stderr
    assert desktop.tonish.poll() is None, "the first tonish ended"
