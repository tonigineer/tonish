"""Each test gets a headless sway of its own, with a fresh HOME, XDG folders
and runtime folder, so that tests cannot affect each other or the real
desktop. It has two 3840×2160 monitors at scale 2, so that bugs with several
monitors or with scaling show up.
"""

import itertools
import json
import os
import shutil
import socket
import struct
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


def u32(*values):
    return struct.pack(f"<{len(values)}I", *values)


def string(text):
    raw = text.encode() + b"\0"
    return u32(len(raw)) + raw + b"\0" * (-len(raw) % 4)


class VirtualPointer:
    """A pointer for the headless seat, which has no input devices and so
    offers clients none: with it, tonish binds a pointer, and sway's
    `seat - cursor` commands reach tonish. It speaks the Wayland wire
    protocol itself, because no packaged tool makes just a pointer."""

    MANAGER = "zwlr_virtual_pointer_manager_v1"

    def __init__(self, env):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(Path(env["XDG_RUNTIME_DIR"]) / env["WAYLAND_DISPLAY"]))
        self.ids = itertools.count(2)
        self.data = b""
        registry = next(self.ids)
        self.request(1, 1, u32(registry))  # wl_display.get_registry
        names = {}
        for sender, opcode, body in self.roundtrip():
            if sender == registry and opcode == 0:  # wl_registry.global
                name, length = struct.unpack_from("<2I", body)
                names[body[8 : 8 + length - 1].decode()] = name
        manager = next(self.ids)
        # wl_registry.bind: a new_id without an interface of its own carries
        # the interface's name and version before the id.
        bind = u32(names[self.MANAGER]) + string(self.MANAGER) + u32(1, manager)
        self.request(registry, 0, bind)
        self.request(manager, 0, u32(0, next(self.ids)))  # for the default seat
        list(self.roundtrip())

    def request(self, sender, opcode, body):
        self.sock.sendall(u32(sender, (8 + len(body)) << 16 | opcode) + body)

    def read(self):
        while True:
            if len(self.data) >= 8:
                sender, word = struct.unpack_from("<2I", self.data)
                size = word >> 16
                if len(self.data) >= size:
                    body, self.data = self.data[8:size], self.data[size:]
                    return sender, word & 0xFFFF, body
            chunk = self.sock.recv(4096)
            if not chunk:
                raise ConnectionError("sway closed the connection")
            self.data += chunk

    def roundtrip(self):
        """The events up to the point where sway has handled every request
        sent so far."""
        done = next(self.ids)
        self.request(1, 0, u32(done))  # wl_display.sync
        while True:
            sender, opcode, body = self.read()
            if sender == 1 and opcode == 0:  # wl_display.error
                length = struct.unpack_from("<I", body, 8)[0]
                raise RuntimeError(body[12 : 12 + length - 1].decode())
            if sender == done:
                return
            yield sender, opcode, body

    def close(self):
        self.sock.close()


class Desktop:
    def __init__(self, *args, hardware=False):
        """`args` go to tonish, such as `--disable-gpu`. With `hardware`,
        sway composites on this machine's GPU, which makes tonish draw on it
        too: EGL takes the GPU only from a compositor that shares buffers on
        it, and Mesa's software renderer otherwise."""
        self.dir = Path(tempfile.mkdtemp(prefix="tonish-test-"))
        self.pointer = None
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
        # sway refuses NVIDIA's driver unless told otherwise, and ignores
        # the flag on any other.
        gpu = ["--unsupported-gpu"] if hardware else []
        self.sway = subprocess.Popen(
            ["sway", *gpu, "-c", str(self.dir / "sway.conf")],
            env=dict(
                self.env,
                WLR_BACKENDS="headless",
                WLR_HEADLESS_OUTPUTS="2",
                WLR_LIBINPUT_NO_DEVICES="1",
                WLR_RENDERER="gles2" if hardware else "pixman",
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
            [str(TONISH), *args],
            env=dict(self.env, TONISH_LOG="engine=debug,info"),
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )

    @property
    def settings(self):
        return self.dir / "home" / ".config" / "tonish" / "config.luau"

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

    def draws(self, surface=None):
        """Of every surface, or of one, such as "0" for the bar or "popup"."""
        lines = [line for line in self.logged().splitlines() if "drew monitor=" in line]
        if surface is not None:
            lines = [line for line in lines if f" surface={surface} " in line]
        return len(lines)

    def hide_hud(self):
        """The figures in the top left corner change with every run and cover
        what is under them, which tests of how panels look must not see."""
        sent = self.run_tonish("msg", "hud")
        assert sent.stdout.strip() == "hidden", sent.stdout + sent.stderr

    def swaymsg(self, *args):
        subprocess.run(
            ["swaymsg", *args], env=self.env, capture_output=True, check=True
        )

    def click(self, x, y, button):
        """At `x`, `y` in the layout's logical pixels, with a button as Linux
        names it, such as BTN_RIGHT."""
        if self.pointer is None:
            self.pointer = VirtualPointer(self.env)
            assert wait_for(lambda: "pointer added" in self.logged()), self.logged()
        self.swaymsg("seat", "-", "cursor", "set", str(x), str(y))
        self.swaymsg("seat", "-", "cursor", "press", button)
        self.swaymsg("seat", "-", "cursor", "release", button)

    def key(self, name):
        """A key by its xkb name, such as Escape, typed with wtype."""
        subprocess.run(["wtype", "-k", name], env=self.env, check=True)

    def wait_for_gpu(self):
        """Until the GPU has taken over the panels of both monitors, which
        it does with a draw of each."""
        assert wait_for(lambda: self.logged().count("on=gpu") >= 2), self.logged()

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
        if self.pointer is not None:
            self.pointer.close()
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
