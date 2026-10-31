"""Each test gets a headless sway of its own, with a fresh HOME, XDG folders
and runtime folder, so that tests cannot affect each other or the real
desktop. It has two 3840×2160 monitors at scale 2, so that bugs with several
monitors or with scaling show up.
"""

import json
import os
import shutil
import subprocess
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


def near(a, b, by=6):
    """Scaling blurs edges by a few steps, hence the tolerance."""
    return len(a) == len(b) and all(abs(x - y) <= by for x, y in zip(a, b, strict=True))


def wait_for(cond, seconds=3.0, step=0.02):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if cond():
            return True
        time.sleep(step)
    return bool(cond())


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
        display = self.dir / "display"
        if not wait_for(lambda: display.exists() and display.read_text(), 10):
            self.close()
            raise RuntimeError("sway did not start")
        self.env["WAYLAND_DISPLAY"] = display.read_text()
        socks = list(run.glob(f"sway-ipc.*.{self.sway.pid}.sock"))
        self.env["SWAYSOCK"] = str(socks[0]) if socks else ""
        # Open for as long as tonish runs, which outlives any `with` here;
        # close() closes it.
        self.log = open(self.dir / "tonish.log", "w")  # noqa: SIM115
        # The engine at debug, so that tests can count draws and a failure
        # shows them.
        self.tonish = subprocess.Popen(
            [str(TONISH)],
            env=dict(self.env, TONISH_LOG="engine=debug,info"),
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )

    @property
    def settings(self):
        return self.dir / "home" / ".config" / "tonish" / "config.toml"

    @property
    def socket(self):
        return self.dir / "run" / f"tonish-{self.env['WAYLAND_DISPLAY']}.sock"

    def run_tonish(self, *args):
        """A second tonish, such as `tonish msg quit`, beside the one running."""
        return subprocess.run(
            [str(TONISH), *args],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )

    def wait_for_panels(self):
        """Once the panels reserve room, the engine is past setting up its socket
        and its watch, so a message or an edit from here on cannot be missed."""

        def up(rooms):
            return len(rooms) == 2 and all(y > 0 or x > 0 for x, y, w, h in rooms)

        assert wait_for(lambda: up(self.workspaces())), (
            f"room for windows: {self.workspaces()}"
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
        """Captured at scale 1, so that tests address pixels in logical coordinates."""
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

    def draws(self):
        return self.logged().count("drew monitor=")

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
        for p in (getattr(self, "tonish", None), self.sway):
            if p is not None and p.poll() is None:
                p.terminate()
                try:
                    p.wait(5)
                except subprocess.TimeoutExpired:
                    p.kill()
        if hasattr(self, "log"):
            self.log.close()
        shutil.rmtree(self.dir, ignore_errors=True)
