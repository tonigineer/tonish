#!/usr/bin/env python3
"""Each scenario gets a headless sway of its own, with a fresh HOME, XDG
folders and runtime folder, so that scenarios cannot affect each other or the
real desktop. It has two 3840×2160 monitors at scale 2, so that bugs with
several monitors or with scaling show up.
"""

import json
import os
import shutil
import signal
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
    """Scaling blurs edges by a few steps, hence the tolerance."""
    return len(a) == len(b) and all(abs(x - y) <= by for x, y in zip(a, b))


def wait_for(cond, seconds, step=0.02):
    t0 = time.monotonic()
    while time.monotonic() - t0 < seconds:
        if cond():
            return time.monotonic() - t0
        time.sleep(step)
    return None


class Desktop:
    def __init__(self):
        self.dir = Path(tempfile.mkdtemp(prefix="tonish-test-"))
        home, run = self.dir / "home", self.dir / "run"
        (home / ".config").mkdir(parents=True)
        run.mkdir(mode=0o700)
        keep = ("PATH", "LANG", "USER", "LOGNAME")
        self.env = {k: v for k, v in os.environ.items() if k in keep}
        self.env.update(
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_RUNTIME_DIR=str(run),
        )
        (self.dir / "sway.conf").write_text(SWAY.format(dir=self.dir))
        self.sway = subprocess.Popen(
            ["sway", "-c", str(self.dir / "sway.conf")],
            env=dict(
                self.env,
                WLR_BACKENDS="headless",
                WLR_HEADLESS_OUTPUTS="2",
                WLR_LIBINPUT_NO_DEVICES="1",
                WLR_RENDERER="pixman",
            ),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if (
            wait_for(
                lambda: (
                    (self.dir / "display").exists()
                    and (self.dir / "display").read_text()
                ),
                10,
            )
            is None
        ):
            raise SystemExit("sway did not start")
        self.env["WAYLAND_DISPLAY"] = (self.dir / "display").read_text()
        socks = list(run.glob(f"sway-ipc.*.{self.sway.pid}.sock"))
        self.env["SWAYSOCK"] = str(socks[0]) if socks else ""
        self.log = open(self.dir / "tonish.log", "w")
        self.tonish = subprocess.Popen(
            [str(TONISH)], env=self.env, stdout=self.log, stderr=subprocess.STDOUT
        )

    def workspaces(self):
        """Relative to each monitor, because sway reports positions in the layout of all of them."""
        run = lambda what: json.loads(
            subprocess.run(
                ["swaymsg", "-t", what, "-r"],
                env=self.env,
                capture_output=True,
                text=True,
                check=False,
            ).stdout
            or "[]"
        )
        at = {o["name"]: o["rect"] for o in run("get_outputs")}
        out = []
        for w in run("get_workspaces"):
            r, o = w["rect"], at.get(w["output"], {"x": 0, "y": 0})
            out.append((r["x"] - o["x"], r["y"] - o["y"], r["width"], r["height"]))
        return out

    def pixels(self):
        """Captured at scale 1, so that scenarios address pixels in logical coordinates."""
        ppm = subprocess.run(
            ["grim", "-o", "HEADLESS-2", "-s", "1", "-t", "ppm", "-"],
            env=self.env,
            capture_output=True,
            check=False,
        ).stdout
        if not ppm.startswith(b"P6"):
            return 0, b""
        _, rest = ppm.split(b"\n", 1)
        size, rest = rest.split(b"\n", 1)
        _, rest = rest.split(b"\n", 1)  # grim always writes 255 as the maximum value
        return int(size.split()[0]), rest

    def at(self, x, y):
        w, px = self.pixels()
        i = (y * w + x) * 3
        return tuple(px[i : i + 3])

    def shot(self, name):
        OUT.mkdir(exist_ok=True)
        subprocess.run(
            ["grim", "-o", "HEADLESS-2", "-s", "0.5", str(OUT / f"{name}.png")],
            env=self.env,
            capture_output=True,
            check=False,
        )

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

# CI runs only the general scenarios. How exactly panels look is checked
# locally, so that changing a panel does not need a round trip through CI.
ON_CI = bool(os.environ.get("CI"))


def scenario(f):
    SCENARIOS.append((f, False))
    return f


def local(f):
    SCENARIOS.append((f, True))
    return f


# ── Scenarios ───────────────────────────────────────────────────────────────


@scenario
def tonish_starts_and_keeps_room_at_the_top_of_every_monitor(d):
    def kept(rooms):
        return len(rooms) == 2 and all(
            x == 0 and y > 0 and w == 1920 and y + h == 1080 for x, y, w, h in rooms
        )

    check(
        wait_for(lambda: kept(d.workspaces()), 3.0) is not None,
        f"room for windows: {d.workspaces()}",
    )
    check(d.tonish.poll() is None, f"tonish exited with {d.tonish.returncode}")


@scenario
def tonish_writes_its_settings_on_first_start(d):
    written = d.dir / "home" / ".config" / "tonish" / "config.toml"
    shipped = (ROOT / "assets" / "default-config.toml").read_text()
    check(
        wait_for(lambda: written.exists() and written.read_text() == shipped, 3.0)
        is not None,
        f"{written} is missing or differs from assets/default-config.toml",
    )


def settings_file(d):
    return d.dir / "home" / ".config" / "tonish" / "config.toml"


def panels_up(d):
    """Once the panels reserve room, the engine is past setting up its watch,
    so an edit from here on cannot be missed."""

    def up(rooms):
        return len(rooms) == 2 and all(y > 0 or x > 0 for x, y, w, h in rooms)

    check(
        wait_for(lambda: up(d.workspaces()), 3.0) is not None,
        f"room for windows: {d.workspaces()}",
    )


@scenario
def editing_the_settings_moves_the_panel(d):
    panels_up(d)
    path = settings_file(d)
    # Most editors write a new file and rename it over the old one.
    new = path.with_name("config.toml.new")
    new.write_text('orientation = "vertical"\nthickness = 30\n')
    os.replace(new, path)
    left = [(30, 0, 1920 - 30, 1080)] * 2
    check(
        wait_for(lambda: d.workspaces() == left, 3.0) is not None,
        f"after a rename: {d.workspaces()}",
    )
    # Others write into the file itself.
    path.write_text('orientation = "horizontal"\nthickness = 60\n')
    top = [(0, 60, 1920, 1080 - 60)] * 2
    check(
        wait_for(lambda: d.workspaces() == top, 3.0) is not None,
        f"after a write in place: {d.workspaces()}",
    )


@scenario
def a_broken_edit_keeps_the_panel(d):
    panels_up(d)
    before = d.workspaces()
    settings_file(d).write_text('orientation = "vertical"\nthickness = 0\n')
    check(
        wait_for(lambda: "keeping the settings from before" in d.logged(), 3.0)
        is not None,
        "tonish did not report the broken file",
    )
    check(d.tonish.poll() is None, f"tonish exited with {d.tonish.returncode}")
    check(d.workspaces() == before, f"room for windows changed: {d.workspaces()}")


def socket_file(d):
    return d.dir / "run" / f"tonish-{d.env['WAYLAND_DISPLAY']}.sock"


def run_tonish(d, *args):
    return subprocess.run(
        [str(TONISH), *args],
        env=d.env,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )


def ended_cleanly(d):
    check(
        wait_for(lambda: d.tonish.poll() is not None, 3.0) is not None,
        "tonish is still running",
    )
    check(d.tonish.returncode == 0, f"tonish exited with {d.tonish.returncode}")
    check(not socket_file(d).exists(), f"{socket_file(d)} was left behind")
    full = [(0, 0, 1920, 1080)] * 2
    check(
        wait_for(lambda: d.workspaces() == full, 3.0) is not None,
        f"the panels' room is still kept: {d.workspaces()}",
    )


@scenario
def msg_quit_ends_tonish(d):
    panels_up(d)
    sent = run_tonish(d, "msg", "quit")
    check(sent.returncode == 0, f"tonish msg quit: {sent.stderr.strip()}")
    # msg returns only once tonish has shut down, so the socket is gone now.
    check(not socket_file(d).exists(), "msg quit returned before tonish ended")
    ended_cleanly(d)
    again = run_tonish(d, "msg", "quit")
    check(
        again.returncode == 1 and "no tonish answers" in again.stderr,
        f"msg without a tonish: {again.returncode} {again.stderr.strip()}",
    )


@scenario
def ctrl_c_ends_tonish_cleanly(d):
    panels_up(d)
    d.tonish.send_signal(signal.SIGINT)
    ended_cleanly(d)


@scenario
def a_second_tonish_does_not_start(d):
    panels_up(d)
    second = run_tonish(d)
    check(
        second.returncode == 1 and "already running" in second.stderr,
        f"second tonish: {second.returncode} {second.stderr.strip()}",
    )
    check(d.tonish.poll() is None, "the first tonish ended")


@local
def the_bar_is_44_px_high(d):
    want = [(0, 44, 1920, 1080 - 44)] * 2
    check(
        wait_for(lambda: d.workspaces() == want, 3.0) is not None,
        f"room for windows: {d.workspaces()}",
    )


@local
def the_bar_is_drawn_with_its_logo(d):
    surface, primary = (0x11, 0x13, 0x18), (0xC9, 0xA9, 0x6E)
    check(
        wait_for(lambda: near(d.at(200, 22), surface), 3.0) is not None,
        f"the bar at (200, 22): {d.at(200, 22)}",
    )
    check(near(d.at(24, 22), primary), f"the logo's dot at (24, 22): {d.at(24, 22)}")
    check(not near(d.at(200, 60), surface), "the bar reaches past 44 px")


def main():
    if not TONISH.exists():
        raise SystemExit(f"{TONISH}: build it first (cargo build --release)")
    filt = sys.argv[1] if len(sys.argv) > 1 else ""
    failed = 0
    for f, local_only in SCENARIOS:
        if filt and filt not in f.__name__:
            continue
        name = f.__name__.replace("_", " ")
        if local_only and ON_CI:
            print(f"  - {name} (local only)")
            continue
        d = Desktop()
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
