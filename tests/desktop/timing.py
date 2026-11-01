"""How long tonish takes to draw on this machine's GPU, as its log reports it,
for the timing tests and `just bench-desktop`.

A draw is warm when the GPU has just drawn, and cold after tonish has left
it idle for some seconds: then the first draw cost 1 to 2 ms instead of 0.1,
measured on an RTX 4090. Whether it does depends on what else runs on the GPU,
which a game keeps busy and awake. A resize costs about as much as a cold
draw, because EGL allocates the surface's buffers again at the new size.
"""

import re
import statistics
import time
from pathlib import Path

from harness import Desktop, wait_for

DREW = re.compile(r"drew monitor=\S+ .*? on=(\w+) took=(\S+) waited=(\S+)")
MS = {"ns": 1e-6, "µs": 1e-3, "ms": 1.0, "s": 1e3}


def ms(duration):
    """A duration as Rust's Debug prints it, such as 84.171µs, in ms."""
    number, unit = re.fullmatch(r"([\d.]+)(\D+)", duration).groups()
    return float(number) * MS[unit]


class Draw:
    def __init__(self, on, took, waited):
        self.on, self.took, self.waited = on, ms(took), ms(waited)


def gpu_desktop():
    """`None` where this machine offers sway no GPU to composite on, as in
    CI, or tonish none to draw on."""
    if not any(Path("/dev/dri").glob("renderD*")):
        return None
    try:
        d = Desktop(hardware=True)
    except RuntimeError:
        return None
    d.wait_for_panels()
    d.wait_for_gpu()
    if "llvmpipe" in device(d):
        d.close()
        return None
    return d


def device(desktop):
    found = re.search(r"GPU started device=(.*?) took=", desktop.logged())
    return found.group(1) if found else "unknown"


def drawn(desktop):
    return [Draw(*m.groups()) for m in DREW.finditer(desktop.logged())]


def after(desktop, change):
    """The draws that `change` causes: one for each of the two monitors."""
    before = len(drawn(desktop))
    change()
    assert wait_for(lambda: len(drawn(desktop)) >= before + 2), desktop.logged()
    new = drawn(desktop)[before:]
    assert all(d.on == "gpu" for d in new), desktop.logged()
    return new


def redraw(desktop):
    def send():
        sent = desktop.run_tonish("msg", "redraw")
        assert sent.returncode == 0, sent.stderr

    return after(desktop, send)


def slowest(draws):
    """A redraw is done when the slower of its two draws is, and after idling,
    only the first is cold: the second comes right after it."""
    return max(draws, key=lambda d: d.took)


def warm(desktop, times=20, gap=0.1):
    # The first few wake the GPU, if it has idled since its start.
    for _ in range(3):
        redraw(desktop)
    redraws = []
    for _ in range(times):
        time.sleep(gap)
        redraws.append(slowest(redraw(desktop)))
    return redraws


def cold(desktop, times=3, idle=10.0):
    redraws = []
    for _ in range(times):
        time.sleep(idle)
        redraws.append(slowest(redraw(desktop)))
    return redraws


def resizes(desktop, times=10, gap=0.3):
    warm(desktop, times=0)
    redraws = []
    for i in range(times):
        time.sleep(gap)
        thickness = 48 if i % 2 == 0 else 44
        text = f"return {{ bar = {{ thickness = {thickness} }} }}\n"
        draws = after(desktop, lambda text=text: desktop.settings.write_text(text))
        redraws.append(slowest(draws))
    return redraws


def median(draws):
    return statistics.median(d.took for d in draws)


def worst(draws):
    return max(d.took for d in draws)
