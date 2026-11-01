"""In the headless sway, Mesa's software renderer stands in for a GPU,
which is enough to check the shader and the engine's EGL side, though not
how fast either is.
"""

from harness import wait_for


def drawn(desktop):
    """Waits for two equal screenshots in a row as well, because the
    compositor shows a frame some time after tonish logs that it drew it."""
    desktop.wait_for_panels()
    assert wait_for(lambda: desktop.draws() >= 2), "the panels were never drawn"
    shots = [desktop.pixels()]

    def settled():
        shots.append(desktop.pixels())
        return shots[-1] == shots[-2]

    assert wait_for(settled), "the screen keeps changing"
    return shots[-1][1]


def test_disable_gpu_never_starts_the_gpu(desktops):
    d = desktops("--disable-gpu")
    drawn(d)
    # A second, because starting the GPU takes Mesa's software renderer a
    # quarter of that.
    started = wait_for(lambda: "EGL started" in d.logged(), seconds=1.0)
    assert not started, d.logged()
    assert "on=gpu" not in d.logged(), d.logged()


def test_the_panels_appear_before_the_gpu_has_started(desktop):
    desktop.wait_for_gpu()
    log = desktop.logged()
    first = log.index("drew monitor=")
    assert "on=cpu" in log[first : log.index("\n", first)], log
    assert first < log.index("GPU started"), log


def test_the_gpu_draws_the_panels_as_the_cpu_does(desktops):
    cpu = desktops("--disable-gpu")
    drawn(cpu)
    cpu.hide_hud()
    want = drawn(cpu)
    cpu.close()
    gpu = desktops()
    gpu.wait_for_gpu()
    gpu.hide_hud()
    got = drawn(gpu)
    assert len(got) == len(want), "the screenshots differ in size"
    # Rounding in the blending, which the renderer's own test bounds by 2
    # per channel before grim scales the screenshot.
    worst = max((abs(a - b) for a, b in zip(want, got, strict=True)), default=0)
    assert worst <= 2, f"a channel differs by {worst} between the CPU and the GPU"


def test_new_panels_go_to_the_gpu_after_a_settings_change(desktop):
    desktop.wait_for_gpu()
    before = desktop.logged().count("on=gpu")
    desktop.settings.write_text(
        "return { bar = { orientation = Orientation.Vertical, thickness = 30 } }\n"
    )
    left = [(30, 0, 1920 - 30, 1080)] * 2
    assert wait_for(lambda: desktop.workspaces() == left), desktop.workspaces()
    assert wait_for(lambda: desktop.logged().count("on=gpu") >= before + 2), (
        desktop.logged()
    )


def test_quitting_with_the_gpu_running_ends_cleanly(desktop):
    desktop.wait_for_gpu()
    sent = desktop.run_tonish("msg", "quit")
    assert sent.returncode == 0, sent.stderr
    assert wait_for(lambda: desktop.tonish.poll() is not None), "tonish still runs"
    assert desktop.tonish.returncode == 0
