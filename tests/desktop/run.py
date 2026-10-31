#!/usr/bin/env python3
"""tonish on a desktop: each scenario starts it in a headless sway of its
own (two 3840×2160 monitors at scale 2) and checks what it does there.

    tests/desktop/run.py [FILTER]     scenarios whose name holds FILTER

Nothing of the real desktop is used: a fresh HOME, XDG folders and runtime
folder, sway drawing into memory. Screenshots of failures go to
tests/desktop/out/.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TONISH = Path(os.environ.get("TONISH", ROOT / "target/release/tonish"))
OUT = Path(__file__).resolve().parent / "out"

SWAY = """\
output HEADLESS-2 mode 3840x2160@60Hz scale 2 position 0 0
output HEADLESS-1 mode 3840x2160@60Hz scale 2 position 1920 0
default_border none
xwayland disable
exec sh -c 'printf %s "$WAYLAND_DISPLAY" > {dir}/display'
"""


class Failed(Exception):
    pass


def check(ok, what):
    if not ok:
        raise Failed(what)


def near(a, b, by=6):
    """Whether two colours are the same but for scaling's blur."""
    return len(a) == len(b) and all(abs(x - y) <= by for x, y in zip(a, b))


def wait_for(cond, seconds, step=0.02):
    """When cond() first held, in s from now; None if it did not."""
    t0 = time.monotonic()
    while time.monotonic() - t0 < seconds:
        if cond():
            return time.monotonic() - t0
        time.sleep(step)
    return None


class Desktop:
    """A headless sway with tonish on it."""

    def __init__(self):
        self.dir = Path(tempfile.mkdtemp(prefix="tonish-test-"))
        home, run = self.dir / "home", self.dir / "run"
        (home / ".config").mkdir(parents=True)
        run.mkdir(mode=0o700)
        keep = ("PATH", "LANG", "USER", "LOGNAME")
        self.env = {k: v for k, v in os.environ.items() if k in keep}
        self.env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / ".config"), XDG_RUNTIME_DIR=str(run))
        (self.dir / "sway.conf").write_text(SWAY.format(dir=self.dir))
        self.sway = subprocess.Popen(["sway", "-c", str(self.dir / "sway.conf")],
                                     env=dict(self.env, WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="2",
                                              WLR_LIBINPUT_NO_DEVICES="1", WLR_RENDERER="pixman"),
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if wait_for(lambda: (self.dir / "display").exists() and (self.dir / "display").read_text(), 10) is None:
            raise SystemExit("sway did not start")
        self.env["WAYLAND_DISPLAY"] = (self.dir / "display").read_text()
        socks = list(run.glob(f"sway-ipc.*.{self.sway.pid}.sock"))
        self.env["SWAYSOCK"] = str(socks[0]) if socks else ""
        self.log = open(self.dir / "tonish.log", "w")
        self.tonish = subprocess.Popen([str(TONISH)], env=self.env, stdout=self.log, stderr=subprocess.STDOUT)

    def workspaces(self):
        """The room each monitor leaves windows: x, y, width, height on that monitor."""
        run = lambda what: json.loads(subprocess.run(["swaymsg", "-t", what, "-r"], env=self.env,
                                                     capture_output=True, text=True, check=False).stdout or "[]")
        at = {o["name"]: o["rect"] for o in run("get_outputs")}
        out = []
        for w in run("get_workspaces"):
            r, o = w["rect"], at.get(w["output"], {"x": 0, "y": 0})
            out.append((r["x"] - o["x"], r["y"] - o["y"], r["width"], r["height"]))
        return out

    def pixels(self):
        """The first monitor as it looks: its width, and its pixels' (r, g, b) by row (one per logical px)."""
        ppm = subprocess.run(["grim", "-o", "HEADLESS-2", "-s", "1", "-t", "ppm", "-"], env=self.env,
                             capture_output=True, check=False).stdout
        if not ppm.startswith(b"P6"):
            return 0, b""
        _, rest = ppm.split(b"\n", 1)
        size, rest = rest.split(b"\n", 1)
        _, rest = rest.split(b"\n", 1)  # the most a channel is: 255
        return int(size.split()[0]), rest

    def at(self, x, y):
        """The colour at (x, y) on the first monitor (logical px)."""
        w, px = self.pixels()
        i = (y * w + x) * 3
        return tuple(px[i:i + 3])

    def shot(self, name):
        OUT.mkdir(exist_ok=True)
        subprocess.run(["grim", "-o", "HEADLESS-2", "-s", "0.5", str(OUT / f"{name}.png")], env=self.env,
                       capture_output=True, check=False)

    def logged(self):
        self.log.flush()
        return (self.dir / "tonish.log").read_text()

    def close(self):
        for p in (self.tonish, self.sway):
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(5)
                except subprocess.TimeoutExpired:
                    p.kill()
        self.log.close()
        shutil.rmtree(self.dir, ignore_errors=True)


SCENARIOS = []


def scenario(f):
    SCENARIOS.append(f)
    return f


# ── Scenarios ───────────────────────────────────────────────────────────────

@scenario
def the_bar_keeps_its_room_at_the_top_of_every_monitor(d):
    """Windows start under the bar, 44 px down, on both monitors."""
    want = [(0, 44, 1920, 1080 - 44)] * 2
    check(wait_for(lambda: d.workspaces() == want, 3.0) is not None, f"room for windows: {d.workspaces()}")


@scenario
def the_bar_is_drawn_with_its_logo(d):
    """The bar's ground along the top, the logo's dot at its start, the desktop under it."""
    surface, primary = (0x11, 0x13, 0x18), (0xc9, 0xa9, 0x6e)
    check(wait_for(lambda: near(d.at(200, 22), surface), 3.0) is not None, f"the bar at (200, 22): {d.at(200, 22)}")
    check(near(d.at(24, 22), primary), f"the logo's dot at (24, 22): {d.at(24, 22)}")
    check(not near(d.at(200, 60), surface), "the bar reaches past 44 px")


def main():
    if not TONISH.exists():
        raise SystemExit(f"{TONISH}: build it first (cargo build --release)")
    filt = sys.argv[1] if len(sys.argv) > 1 else ""
    failed = 0
    for f in SCENARIOS:
        if filt and filt not in f.__name__:
            continue
        d = Desktop()
        name = f.__name__.replace("_", " ")
        try:
            f(d)
            print(f"  ✓ {name}")
        except Failed as e:
            failed += 1
            d.shot(f.__name__)
            tail = "\n".join("      | " + l for l in d.logged().splitlines()[-10:])
            print(f"  ✗ {name}\n      {e}\n{tail}")
        finally:
            d.close()
    print(f"\n{'all passed' if not failed else f'{failed} failed'}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
