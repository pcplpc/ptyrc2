# SPDX-License-Identifier: GPL-3.0-or-later
"""The pilot: a blocking client of a ptyrc driver."""

from __future__ import annotations

import base64
import os
import queue
import shutil
import subprocess
import time
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Self

from ._wire import Connection, PtyrcError, runtime_dir, socket_path

__all__ = ["Cursor", "Frame", "Match", "Pilot", "Sprite", "find_binary", "sprite_spec"]

_STYLE_KEYS = (
    "fg",
    "bg",
    "bold",
    "dim",
    "italic",
    "underline",
    "inverse",
    "strike",
    "blink",
)


@dataclass(frozen=True)
class Match:
    """A text match. `x`, `y`: frame coordinates. `screen`: screen cell of
    its first glyph, None when not shown."""

    frame: int
    x: int
    y: int
    len: int
    text: str
    screen: tuple[int, int] | None

    @classmethod
    def from_wire(cls, raw: dict[str, Any]) -> Match:
        screen = raw.get("screen")
        return cls(
            frame=raw["frame"],
            x=raw["x"],
            y=raw["y"],
            len=raw["len"],
            text=raw["text"],
            screen=tuple(screen) if screen else None,
        )


@dataclass(frozen=True)
class Cursor:
    """Screen cursor. `trail`: `(ms, x, y)` samples, oldest first."""

    x: int
    y: int
    visible: bool
    frame: int | None
    trail: tuple[tuple[int, int, int], ...] = ()


def find_binary() -> str:
    """The `ptyrc` binary: `PTYRC_BIN`, the workspace build, then `PATH`."""
    env = os.environ.get("PTYRC_BIN")
    if env:
        return env
    for parent in Path(__file__).resolve().parents:
        for profile in ("release", "debug"):
            candidate = parent / "target" / profile / "ptyrc"
            if candidate.is_file():
                return str(candidate)
    found = shutil.which("ptyrc")
    if found:
        return found
    raise PtyrcError("gone", "ptyrc binary not found; set PTYRC_BIN")


def sprite_spec(
    rows: Sequence[str] | str | None = None,
    *,
    frame: int | str | None = None,
    at: tuple[int, int] | None = None,
    space: str = "view",
    near: str | None = None,
    regex: bool = False,
    nth: int = 0,
    ignore_case: bool = False,
    side: str = "right",
    gap: int | None = None,
    auto: str | bool | None = None,
    cursor: tuple[int, int] | None = None,
    style: dict[str, Any] | None = None,
    cells: Sequence[Sequence[dict[str, Any]]] | None = None,
    opaque: bool = False,
    frames: Sequence[Any] | None = None,
    frame_ms: int = 100,
    loop: bool | int = True,
    layer: int | str | None = None,
    blend: str | None = None,
    z: int | None = None,
    ttl: float | None = None,
    persist: bool = False,
    motion: dict[str, Any] | None = None,
    effect: str | dict[str, Any] | None = None,
    size: tuple[int, int] | None = None,
    **style_kw: Any,
) -> dict[str, Any]:
    """Build the wire form of a sprite.

    Anchor, one of:

    - `at=(x, y)` with `space`: `"view"` is fixed on the pane (negative
      counts from the right and bottom), `"frame"` moves with scrolling;
    - `near="word"`: next to a text match (`side`, `gap`, `nth`, `regex`,
      `ignore_case`), following it;
    - `auto="top-right"` (or True): a quiet spot chosen by the driver;
    - `cursor=(dx, dy)`: follows the cursor.

    Content, one of: `rows` (a space is transparent unless `opaque`),
    `cells` (rows of runs `{"t": "ab", "fg": ...}`), `frames` (an
    animation: each a list of rows, or a dict with `rows` / `cells` and
    `ms`; `loop` True, False or a count), `effect` (a native generator,
    drawing on `size` cells or on the whole view).

    Style keywords (`fg`, `bg`, `bold`...) extend `style`. `layer`: `"back"`
    (the default: behind the app's text, seen where the app shows none),
    `"front"`, or an integer; the app is 0, negative layers are behind
    it, `z` orders inside a layer. `blend="tint"` keeps the glyph below
    and applies the style; it is in front unless `layer` says otherwise.
    `ttl` is in seconds. A sprite dies with its connection unless
    `persist`.
    """
    unknown = set(style_kw) - set(_STYLE_KEYS)
    if unknown:
        raise TypeError(f"unknown arguments: {', '.join(sorted(unknown))}")
    merged = {**(style or {}), **style_kw}

    def content(rows_: Any, cells_: Any = None) -> dict[str, Any]:
        out: dict[str, Any] = {}
        if cells_ is not None:
            out["cells"] = [list(r) for r in cells_]
        else:
            if isinstance(rows_, str):
                rows_ = rows_.split("\n")
            out["rows"] = list(rows_)
        if merged:
            out["style"] = merged
        if opaque:
            out["opaque"] = True
        return out

    spec: dict[str, Any] = {}
    if frame is not None:
        spec["frame"] = frame

    if near is not None:
        anchor: dict[str, Any] = {"text": near, "side": side}
        if gap is not None:
            anchor["gap"] = gap
        if ignore_case:
            anchor["ignore_case"] = True
        if nth:
            anchor["nth"] = nth
        if regex:
            anchor["regex"] = True
    elif auto:
        anchor = {"auto": {"prefer": "any" if auto is True else auto}}
    elif cursor is not None:
        anchor = {"cursor": {"dx": cursor[0], "dy": cursor[1]}}
    else:
        x, y = at or (0, 0)
        anchor = {"at": [x, y], "space": space}
    spec["anchor"] = anchor

    if effect is not None:
        spec["effect"] = {"name": effect} if isinstance(effect, str) else effect
    elif frames is not None:
        out_frames = []
        for item in frames:
            if isinstance(item, dict):
                one = content(item.get("rows"), item.get("cells"))
                one["ms"] = item.get("ms", frame_ms)
            else:
                one = content(item)
                one["ms"] = frame_ms
            out_frames.append(one)
        spec["frames"] = out_frames
        spec["loop"] = loop
    elif rows is not None or cells is not None:
        spec["content"] = content(rows, cells)
    else:
        raise TypeError("one of rows, cells, frames, effect is required")

    if layer is not None:
        spec["layer"] = layer
    if blend is not None:
        spec["blend"] = blend
    if z is not None:
        spec["z"] = z
    if ttl is not None:
        spec["ttl_ms"] = int(ttl * 1000)
    if persist:
        spec["persist"] = True
    if motion is not None:
        spec["motion"] = motion
    if size is not None:
        spec["size"] = list(size)
    return spec


class Sprite:
    """Handle of a sprite on the driver. Made inside `batch()`, its `id` is
    known when the batch is sent."""

    def __init__(self, pilot: Pilot, sprite_id: int | None) -> None:
        self._pilot = pilot
        self.id = sprite_id

    def update(self, **patch: Any) -> None:
        """Patch wire fields: `anchor`, `content`, `frames`, `layer`, `z`..."""
        self._pilot.call("draw.update", id=self.id, **patch)

    def set(self, rows: Any = None, **kw: Any) -> None:
        """New content, with the arguments of `sprite_spec`."""
        spec = sprite_spec(rows, **kw)
        keys = ("content", "frames", "loop")
        self.update(**{k: spec[k] for k in keys if k in spec})

    def ext(self, name: str, function: str, params: Any = None, **patch: Any) -> None:
        """Redraw in place with an extension: what it draws replaces the
        content; frame, anchor and layer are kept unless in `patch`."""
        request = {"name": name, "function": function, "params": params or {}}
        self._pilot.call("draw.ext", id=self.id, **request, **patch)

    def move(self, x: int, y: int, space: str = "view") -> None:
        self.update(anchor={"at": [x, y], "space": space})

    def info(self) -> dict[str, Any] | None:
        """Its entry in `draw.list`; `screen` is where it shows, or None."""
        return next((s for s in self._pilot.sprites() if s["id"] == self.id), None)

    def remove(self) -> None:
        """Remove it; fine if it is already gone."""
        try:
            self._pilot.call("draw.remove", id=self.id)
        except PtyrcError as exc:
            if exc.code not in ("no-sprite", "gone"):
                raise

    def __repr__(self) -> str:
        return f"Sprite({self.id})"


class Frame:
    """A frame of the driver: the raw screen or a pane. `info` is a
    snapshot; `refresh()` updates it."""

    def __init__(self, pilot: Pilot, info: dict[str, Any]) -> None:
        self._pilot = pilot
        self.info = info

    id = property(lambda self: self.info["id"])
    kind = property(lambda self: self.info["kind"])
    state = property(lambda self: self.info["state"])
    usable = property(lambda self: bool(self.info.get("usable")))
    rect = property(lambda self: tuple(self.info["rect"]))
    origin = property(lambda self: tuple(self.info["origin"]))
    extent = property(lambda self: tuple(self.info.get("extent") or (0, 0)))
    key = property(lambda self: self.info.get("key"))
    support = property(lambda self: self.info.get("support"))

    @property
    def size(self) -> tuple[int, int]:
        return (self.info["rect"][2], self.info["rect"][3])

    def to_screen(self, x: int, y: int) -> tuple[int, int] | None:
        """Screen cell of a frame position, None when not shown."""
        rx, ry, w, h = self.info["rect"]
        dx, dy = x - self.info["origin"][0], y - self.info["origin"][1]
        return (rx + dx, ry + dy) if 0 <= dx < w and 0 <= dy < h else None

    def to_frame(self, col: int, row: int) -> tuple[int, int] | None:
        """Frame position of a screen cell, None outside the view."""
        rx, ry, w, h = self.info["rect"]
        dx, dy = col - rx, row - ry
        if not (0 <= dx < w and 0 <= dy < h):
            return None
        return (self.info["origin"][0] + dx, self.info["origin"][1] + dy)

    def refresh(self) -> Frame:
        for info in self._pilot.call("frame.list")["frames"]:
            if info["id"] == self.id:
                self.info = info
                return self
        raise PtyrcError("no-frame", str(self.id))

    def text(self) -> list[str]:
        """What the view shows."""
        return self._pilot.text(frame=self.id)

    def known(self, y0: int | None = None, y1: int | None = None) -> list[str]:
        """Remembered lines `y0 <= y < y1` (default: the whole extent)."""
        params: dict[str, Any] = {"frame": self.id}
        if y0 is not None:
            params["y0"] = y0
        if y1 is not None:
            params["y1"] = y1
        return self._pilot.call("frame.text", **params)["lines"]

    def find(self, pattern: str, **kw: Any) -> list[Match]:
        return self._pilot.find(pattern, frame=self.id, **kw)

    def heat(self) -> list[str]:
        return self._pilot.call("frame.heat", frame=self.id)["rows"]

    def draw(self, rows: Any = None, **kw: Any) -> Sprite:
        return self._pilot.draw(rows, frame=self.id, **kw)

    def __repr__(self) -> str:
        return f"Frame({self.id}, {self.kind}, {self.state}, rect={self.rect})"


class Pilot:
    """Client of one driver session.

    `Pilot()` connects to the session named by the environment or the only
    live one; `Pilot("name")` to a named one; `Pilot.spawn(argv)` starts a
    headless driver and owns it.
    """

    def __init__(
        self,
        session: str | None = None,
        *,
        path: str | Path | None = None,
        timeout: float = 5.0,
    ) -> None:
        self._conn = Connection(path or socket_path(session), timeout=timeout)
        self._proc: subprocess.Popen[bytes] | None = None
        self._batch: _Batch | None = None
        self.hello: dict[str, Any] = self._conn.hello

    @classmethod
    def spawn(
        cls,
        argv: Sequence[str],
        *,
        size: tuple[int, int] = (80, 24),
        name: str | None = None,
        env: dict[str, str] | None = None,
        host_log: str | Path | None = None,
        binary: str | None = None,
        timeout: float = 10.0,
    ) -> Pilot:
        """Start `ptyrc run --headless` on `argv` and connect to it. The
        driver is killed by `close()`."""
        name = name or f"py-{os.getpid()}-{time.monotonic_ns() % 10**9}"
        cmd = [binary or find_binary(), "run", "--headless", "--name", name]
        cmd += ["--size", f"{size[0]}x{size[1]}"]
        if host_log is not None:
            cmd += ["--host-log", str(host_log)]
        cmd += ["--", *argv]
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            env={**os.environ, **(env or {})},
        )
        path = runtime_dir() / f"{name}.sock"
        deadline = time.monotonic() + timeout
        while True:
            if proc.poll() is not None:
                err = proc.stderr.read().decode(errors="replace") if proc.stderr else ""
                raise PtyrcError("gone", f"driver exited {proc.returncode}: {err}")
            if path.exists():
                try:
                    pilot = cls(path=path, timeout=timeout)
                    break
                except PtyrcError:
                    pass
            if time.monotonic() > deadline:
                proc.kill()
                raise PtyrcError("timeout", "driver did not start")
            time.sleep(0.01)
        pilot._proc = proc
        return pilot

    # -- plumbing ---------------------------------------------------------

    def call(self, method: str, **params: Any) -> Any:
        """Raw request: `pilot.call("screen.text", frame=1)`. Inside
        `batch()` it is queued and returns None."""
        return self._request(method, params)

    def _request(
        self,
        method: str,
        params: dict[str, Any],
        then: Callable[[Any], None] | None = None,
        timeout: float = 30.0,
    ) -> Any:
        if self._batch is not None:
            self._batch.add(method, params, then)
            return None
        result = self._conn.call(method, params, timeout=timeout)
        if then is not None:
            then(result)
        return result

    def batch(self) -> _Batch:
        """Context manager: the calls made inside go out as one request and
        show in one repaint.

            with pilot.batch() as batch:
                head.move(x, y)
                tail.set(["~"])
            batch.results      # one result, or PtyrcError, per call
        """
        return _Batch(self)

    @property
    def connected(self) -> bool:
        return not self._conn.closed

    def close(self) -> None:
        self._conn.close()
        if self._proc is not None:
            self._proc.terminate()
            try:
                self._proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self._proc.kill()
                self._proc.wait()
            if self._proc.stderr:
                self._proc.stderr.close()
            self._proc = None

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    # -- session ----------------------------------------------------------

    def info(self) -> dict[str, Any]:
        return self.call("session.info")

    @property
    def session(self) -> str:
        return self.hello["session"]

    @property
    def argv(self) -> list[str]:
        return self.hello["argv"]

    @property
    def size(self) -> tuple[int, int]:
        """`(cols, rows)` now."""
        cols, rows = self.info()["size"]
        return (cols, rows)

    def resize(self, cols: int, rows: int) -> None:
        """Resize the screen. Headless sessions only: elsewhere the host
        terminal decides."""
        self.call("session.resize", size=[cols, rows])

    # -- reading ----------------------------------------------------------

    def text(
        self, frame: int | str | None = None, rect: Sequence[int] | None = None
    ) -> list[str]:
        """Plain text of the screen or of a frame's view, one string per row."""
        return self.call("screen.text", **_opt(frame=frame, rect=rect))["lines"]

    def cells(
        self, frame: int | str | None = None, rect: Sequence[int] | None = None
    ) -> list[list[dict[str, Any]]]:
        """Rows of styled runs."""
        return self.call("screen.cells", **_opt(frame=frame, rect=rect))["rows"]

    def find(
        self,
        pattern: str,
        *,
        regex: bool = False,
        ignore_case: bool = False,
        frame: int | str | None = None,
        known: bool = False,
    ) -> list[Match]:
        """Matches in reading order. `known`: also in what the frame
        remembers outside its view."""
        params = _opt(frame=frame)
        if regex:
            params["regex"] = True
        if ignore_case:
            params["ignore_case"] = True
        if known:
            params["known"] = True
        res = self.call("screen.find", pattern=pattern, **params)
        return [Match.from_wire(m) for m in res["matches"]]

    def cursor(self, trail: bool = False) -> Cursor:
        res = self.call("screen.cursor", trail=trail)
        return Cursor(
            x=res["x"],
            y=res["y"],
            visible=res.get("visible", True),
            frame=res.get("frame"),
            trail=tuple(tuple(s) for s in res.get("trail") or ()),
        )

    def wait_for(
        self,
        pattern: str | None = None,
        *,
        regex: bool = False,
        gone: bool = False,
        stable: float | None = None,
        timeout: float = 5.0,
        frame: int | str | None = None,
    ) -> Match | None:
        """Wait until `pattern` is on screen (or `gone`), or until the
        screen has been unchanged for `stable` seconds. Raises
        `TimeoutError`."""
        params = _opt(frame=frame, pattern=pattern)
        if regex:
            params["regex"] = True
        if gone:
            params["gone"] = True
        if stable is not None:
            params["stable_ms"] = int(stable * 1000)
        params["timeout_ms"] = int(timeout * 1000)
        if self._batch is not None:
            raise RuntimeError("wait_for inside a batch")
        res = self._conn.call("screen.wait", params, timeout=timeout + 5.0)
        if not res.get("matched", False):
            raise TimeoutError(f"wait_for({pattern!r})")
        match = res.get("match")
        return Match.from_wire(match) if match else None

    # -- typing -----------------------------------------------------------

    def send(self, text: str) -> None:
        """Type text."""
        self.call("input.send", text=text)

    def keys(self, *keys: str) -> None:
        """Press keys: `enter`, `esc`, `ctrl+c`, `up`... Other strings are
        typed as text."""
        self.call("input.send", keys=list(keys))

    def paste(self, text: str) -> None:
        """Type text as a paste (bracketed when the app asked for it)."""
        self.call("input.send", text=text, paste=True)

    def write(self, data: bytes) -> None:
        """Raw bytes to the app."""
        self.call("input.send", bytes=base64.b64encode(data).decode())

    # -- frames -----------------------------------------------------------

    def frames(self, *, inactive: bool = False) -> list[Frame]:
        """Frames on screen (and remembered ones with `inactive`)."""
        infos = self.call("frame.list")["frames"]
        return [
            Frame(self, i) for i in infos if inactive or i.get("state") != "inactive"
        ]

    def panes(self, *, usable: bool = True) -> list[Frame]:
        """Panes on screen, usable ones only by default."""
        return [
            f for f in self.frames() if f.kind == "pane" and (f.usable or not usable)
        ]

    def frame(self, sel: int | str = "raw") -> Frame:
        """A frame by id or name: `raw`, `active` (the pane holding the
        cursor), `largest`, `quietest`."""
        frames = self.frames(inactive=True)
        if isinstance(sel, int) or sel.isdigit():
            fid = int(sel)
        elif sel == "raw":
            fid = 0
        else:
            # The driver resolves names; `frame.at` answers for one too.
            fid = self.call("frame.at", name=sel)["frame"]
        for f in frames:
            if f.id == fid:
                return f
        raise PtyrcError("no-frame", str(sel))

    def frame_at(self, x: int, y: int) -> tuple[int, tuple[int, int]]:
        """Frame holding screen cell `(x, y)` and the frame position there."""
        res = self.call("frame.at", x=x, y=y)
        return res["frame"], tuple(res["pos"])

    # -- drawing ----------------------------------------------------------

    def draw(self, rows: Any = None, **kw: Any) -> Sprite:
        """Add a sprite; arguments of `sprite_spec`."""
        return self.put(sprite_spec(rows, **kw))

    def put(self, spec: dict[str, Any]) -> Sprite:
        """Add a sprite from its wire form."""
        return self._sprite("draw.put", spec)

    def ext(
        self, name: str, function: str, params: Any = None, **sprite: Any
    ) -> Sprite:
        """Draw with an extension: `pilot.ext("shapes", "circle", {"r": 4})`.
        `sprite` takes wire sprite fields (`frame`, `anchor`, `layer`...)."""
        request = {"name": name, "function": function, "params": params or {}}
        return self._sprite("draw.ext", {**request, **sprite})

    def _sprite(self, method: str, params: dict[str, Any]) -> Sprite:
        sprite = Sprite(self, None)
        self._request(method, params, lambda res: setattr(sprite, "id", res["id"]))
        return sprite

    def exts(self) -> dict[str, Any]:
        return self.call("draw.exts")

    def effect(
        self, name: str, params: dict[str, Any] | None = None, **kw: Any
    ) -> Sprite:
        """Start a native effect: `pilot.effect("confetti")`."""
        return self.draw(effect={"name": name, "params": params or {}}, **kw)

    def sprites(self) -> list[dict[str, Any]]:
        return self.call("draw.list")["sprites"]

    def clear(self, all: bool = False) -> int:
        """Remove this connection's sprites, or every sprite with `all`."""
        res = self.call("draw.clear", all=all)
        return res["removed"] if res else 0

    # -- events and debug -------------------------------------------------

    def subscribe(self, *topics: str) -> None:
        """Ask for events: `screen`, `cursor`, `frames`, `input`, `size`,
        `exit`."""
        self.call("subscribe", topics=list(topics))

    def events(
        self, *topics: str, timeout: float | None = None
    ) -> Iterator[tuple[str, Any]]:
        """Subscribe to `topics` and yield `(topic, data)` as they come.
        Ends when the driver goes away, or after `timeout` seconds of
        silence.

            for topic, data in pilot.events("cursor"):
                print(data["x"], data["y"])
        """
        if topics:
            self.subscribe(*topics)
        while True:
            try:
                msg = self._conn.events.get(timeout=timeout)
            except queue.Empty:
                return
            if msg is None:
                return
            yield msg["event"], msg.get("data")

    def debug_overlay(self, what: str = "frames") -> None:
        """Show `frames` or `heat` on the host; `off` hides it."""
        self.call("debug.overlay", what=what)

    def dump(self) -> dict[str, Any]:
        return self.call("debug.dump")


class _Batch:
    """Calls collected by `Pilot.batch()`. After the block, `results` holds
    one entry per call: its result, or a `PtyrcError`."""

    def __init__(self, pilot: Pilot) -> None:
        self._pilot = pilot
        self._calls: list[dict[str, Any]] = []
        self._then: list[Callable[[Any], None] | None] = []
        self.results: list[Any] = []

    def add(
        self, method: str, params: dict[str, Any], then: Callable[[Any], None] | None
    ) -> None:
        self._calls.append({"method": method, "params": params})
        self._then.append(then)

    def __enter__(self) -> Self:
        if self._pilot._batch is not None:
            raise RuntimeError("nested batch")
        self._pilot._batch = self
        return self

    def __exit__(self, exc_type: object, *exc: object) -> None:
        self._pilot._batch = None
        if exc_type is not None or not self._calls:
            return
        reply = self._pilot._conn.call("batch", {"calls": self._calls})
        for entry, then in zip(reply["results"], self._then, strict=True):
            if "error" in entry:
                err = entry["error"] or {}
                self.results.append(
                    PtyrcError(err.get("code", "internal"), err.get("message", ""))
                )
            else:
                self.results.append(entry.get("result"))
                if then is not None:
                    then(entry.get("result"))


def _opt(**kw: Any) -> dict[str, Any]:
    return {
        k: (list(v) if isinstance(v, tuple) else v)
        for k, v in kw.items()
        if v is not None
    }
