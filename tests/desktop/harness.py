"""Each test gets a headless sway of its own, with a fresh HOME, XDG folders
and runtime folder, so that tests cannot affect each other or the real
desktop. It has two 3840×2160 monitors at scale 2, so that bugs with several
monitors or with scaling show up.
"""

import array
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
from types import MappingProxyType

ROOT = Path(__file__).resolve().parents[2]
# Linux's input-event-codes.h.
BTN_LEFT = 0x110
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


class Client:
    """A Wayland client that speaks the wire protocol itself, for the input
    devices the headless seat lacks, which no packaged tool makes on their
    own."""

    def __init__(self, env):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(str(Path(env["XDG_RUNTIME_DIR"]) / env["WAYLAND_DISPLAY"]))
        self.ids = itertools.count(2)
        self.data = b""
        self.registry = next(self.ids)
        self.request(1, 1, u32(self.registry))  # wl_display.get_registry
        self.globals = {}
        for sender, opcode, body in self.roundtrip():
            if sender == self.registry and opcode == 0:  # wl_registry.global
                name, length = struct.unpack_from("<2I", body)
                self.globals[body[8 : 8 + length - 1].decode()] = name

    def bind(self, interface, version):
        """wl_registry.bind: a new_id without an interface of its own carries
        the interface's name and version before the id."""
        new = next(self.ids)
        body = u32(self.globals[interface]) + string(interface) + u32(version, new)
        self.request(self.registry, 0, body)
        return new

    def request(self, sender, opcode, body, fds=()):
        message = u32(sender, (8 + len(body)) << 16 | opcode) + body
        ancillary = [(socket.SOL_SOCKET, socket.SCM_RIGHTS, array.array("i", fds))]
        self.sock.sendmsg([message], ancillary if fds else [])

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


class VirtualPointer(Client):
    """A pointer for the headless seat, which has no input devices and so
    offers clients none: with it, tonish binds a pointer, and sway's
    `seat - cursor` commands reach tonish."""

    def __init__(self, env):
        super().__init__(env)
        manager = self.bind("zwlr_virtual_pointer_manager_v1", 1)
        self.device = next(self.ids)
        self.request(manager, 0, u32(0, self.device))  # for the default seat
        list(self.roundtrip())

    # The layout of the two monitors side by side, which absolute motion is
    # given in.
    LAYOUT = (3840, 1080)

    def move_to(self, x, y):
        """Motion of the device itself, which sway passes on while a button
        is held, as it does not for `seat - cursor set`, a warp."""
        w, h = self.LAYOUT
        self.request(
            self.device, 1, u32(0, round(x), round(y), w, h)
        )  # motion_absolute
        self.request(self.device, 4, b"")  # frame
        list(self.roundtrip())

    def button(self, code, pressed):
        """`code` as Linux numbers buttons, such as 0x110 for the left one."""
        self.request(self.device, 2, u32(0, code, 1 if pressed else 0))
        self.request(self.device, 4, b"")
        list(self.roundtrip())


class VirtualKeyboard(Client):
    """A keyboard for the headless seat, with the standard evdev keymap, so
    that a key's code is Linux's own, which tonish reads without a layout.
    wtype uploads a keymap of its own instead, which numbers keys in the
    order it types them, so that tonish would read another key."""

    KEYMAP = (
        b"xkb_keymap {"
        b' xkb_keycodes { include "evdev" };'
        b' xkb_types { include "complete" };'
        b' xkb_compat { include "complete" };'
        b' xkb_symbols { include "pc+us" };'
        b" };\0"
    )
    # Linux's input-event-codes.h.
    KEYS = MappingProxyType({"Escape": 1})

    def __init__(self, env):
        super().__init__(env)
        seat = self.bind("wl_seat", 1)
        manager = self.bind("zwp_virtual_keyboard_manager_v1", 1)
        self.device = next(self.ids)
        self.request(manager, 0, u32(seat, self.device))  # create_virtual_keyboard
        # A file without a name rather than a memfd, which uv's Python
        # builds lack.
        with tempfile.TemporaryFile() as keymap:
            keymap.write(self.KEYMAP)
            keymap.flush()
            # keymap, as XKB_V1, with its file passed alongside.
            body = u32(1, len(self.KEYMAP))
            self.request(self.device, 0, body, fds=[keymap.fileno()])
            list(self.roundtrip())

    def key(self, name):
        code = self.KEYS[name]
        for state in (1, 0):
            self.request(self.device, 1, u32(0, code, state))
        list(self.roundtrip())


class Desktop:
    def __init__(self, *args, hardware=False):
        """`args` go to tonish, such as `--disable-gpu`. With `hardware`,
        sway composites on this machine's GPU, which makes tonish draw on it
        too: EGL takes the GPU only from a compositor that shares buffers on
        it, and Mesa's software renderer otherwise."""
        self.dir = Path(tempfile.mkdtemp(prefix="tonish-test-"))
        self.pointer = None
        self.keyboard = None
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
        # Before tonish, as a real keyboard would be: sway gives the
        # keyboard to a popup's grab when the grab starts, and a keyboard
        # that appears later never learns of it, so its keys go elsewhere.
        self.keyboard = VirtualKeyboard(self.env)
        # Open for as long as tonish runs, which outlives any `with` here;
        # close() closes it.
        self.log = open(self.dir / "tonish.log", "w")  # noqa: SIM115
        # The engine at debug, so that tests can count draws and a failure
        # shows them, unless TONISH_LOG asks for another level, such as
        # engine=trace for every pointer motion.
        self.tonish = subprocess.Popen(
            [str(TONISH), *args],
            env=dict(
                self.env, TONISH_LOG=os.environ.get("TONISH_LOG", "engine=debug,info")
            ),
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

    def drag(self, points, seconds=0.5):
        """Presses the left button at the first point, moves through the
        others in even steps over `seconds`, and lets go at the last."""
        if self.pointer is None:
            self.pointer = VirtualPointer(self.env)
            assert wait_for(lambda: "pointer added" in self.logged()), self.logged()
        self.pointer.move_to(*points[0])
        self.pointer.button(BTN_LEFT, True)
        for x, y in points[1:]:
            time.sleep(seconds / max(len(points) - 1, 1))
            self.pointer.move_to(x, y)
        self.pointer.button(BTN_LEFT, False)

    def key(self, name):
        """A key by its xkb name, such as Escape, pressed and let go."""
        self.keyboard.key(name)

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
        for device in (self.pointer, self.keyboard):
            if device is not None:
                device.close()
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
