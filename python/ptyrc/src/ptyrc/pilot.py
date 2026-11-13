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

from . import clock
from ._wire import Connection, PtyrcError, live_sessions, runtime_dir, socket_path
from .tools import Locator, Tools, frame_locator
from .widgets import Widgets

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
    """Screen cursor. `frame`, `pos`: the innermost frame showing it and
    its position there, in frame cells. `trail`: `(ms, x, y)` samples,
    oldest first."""

    x: int
    y: int
    visible: bool
    frame: int | None
    trail: tuple[tuple[int, int, int], ...] = ()
    pos: tuple[int, int] | None = None

    @classmethod
    def from_wire(cls, raw: dict[str, Any]) -> Cursor:
        """From `screen.cursor`, or from the data of a `cursor` event."""
        pos = raw.get("pos")
        return cls(
            x=raw["x"],
            y=raw["y"],
            visible=raw.get("visible", True),
            frame=raw.get("frame"),
            trail=tuple(tuple(s) for s in raw.get("trail") or ()),
            pos=tuple(pos) if pos else None,
        )


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
    slide: bool = True,
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
    alpha: float | None = None,
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
      `ignore_case`), following it; it slides along the match to stay
      whole in the view unless `slide=False`;
    - `auto="top-right"` (or True): a quiet spot chosen by the driver;
    - `cursor=(dx, dy)`: follows the cursor.

    Content, one of: `rows` (a space is transparent unless `opaque`),
    `cells` (rows of runs `{"t": "ab", "fg": ...}`), `frames` (an
    animation: each a list of rows, or a dict with `rows` / `cells` and
    `ms`; `loop` True, False or a count), `effect` (a native generator,
    drawing on `size` cells or on the whole view).

    Style keywords (`fg`, `bg`, `bold`...) extend `style`. `layer`: `"back"`
    (the default: behind the app's text, seen where the app shows none),
    `"front"`, `"wallpaper"` (behind `back`), `"hud"` (in front of
    `front`), or an integer; the app is 0, negative layers are behind
    it, `z` orders inside a layer. `blend="tint"` keeps the glyph below
    and applies the style; it is in front unless `layer` says otherwise.
    `alpha`: opacity, 0 to 1; the colours are mixed with those below
    (the app's, or the host's defaults: `Pilot.host_colors`) and written
    as RGB.
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
        if not slide:
            anchor["slide"] = False
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
    if alpha is not None:
        spec["alpha"] = alpha
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
        self._effect: str | None = None

    def update(self, **patch: Any) -> None:
        """Patch wire fields: `anchor`, `content`, `frames`, `layer`, `z`..."""
        self._pilot.call("draw.update", id=self.id, **patch)
        if "effect" in patch:
            self._effect = patch["effect"]["name"]

    def tune(self, **params: Any) -> None:
        """New parameters for its running effect, which keeps its state:
        `draw.update {id, effect}` under the effect's own name. The
        effect says which it takes (`bad-params`). The name is asked
        once (`draw.list`) when this handle did not start the effect."""
        if self._effect is None:
            kind = (self.info() or {}).get("kind", "")
            if not kind.startswith("effect:"):
                raise PtyrcError("bad-params", f"sprite {self.id} runs no effect")
            self._effect = kind.removeprefix("effect:")
        self.update(effect={"name": self._effect, "params": params})

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
        """Its entry in `draw.list`. `screen`: its top-left cell, None
        while hidden; `visible`: some of it is in the view; `covered`:
        share of a back sprite under the app's text; `style`: the one
        it was given. Inside `batch()`: a dict, filled when the batch
        is sent, left empty when the sprite is gone."""
        out: dict[str, Any] = {}

        def fill(res: Any) -> None:
            out.update(next((s for s in res["sprites"] if s["id"] == self.id), {}))

        res = self._pilot._request("draw.list", {}, fill)
        return out if res is None else (out or None)

    @property
    def visible(self) -> bool:
        """Some of it is in the view of a usable frame. Asks the driver."""
        return bool((self.info() or {}).get("visible"))

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
        self.info = self._pilot.call("frame.get", frame=self.id)["frame"]
        return self

    def locator(self, pattern: str, **kw: Any) -> Locator:
        """A place on this frame, by the text there: see `Locator`."""
        return frame_locator(self, pattern, **kw)

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
        """Safety of each cell of the view, digits: 9 free and still, 0
        busy."""
        return self._pilot.call("frame.heat", frame=self.id)["rows"]

    def heat_classes(self) -> list[str]:
        """Class of each cell of the view: `h` hot (changed within 10 s),
        `t` static text, `p` panel (background of another colour), `.`
        free."""
        return self._pilot.call("frame.heat", frame=self.id)["classes"]

    def idle(self) -> dict[str, Any]:
        """`score`: share of the view static for 10 s; `idle`: score >=
        0.8; `free`: share static and without text."""
        return self._pilot.call("frame.idle", frame=self.id)

    def check_idle(self) -> float:
        """Share of the view that is static, 0 to 1. Asked now."""
        return float(self.idle()["score"])

    @property
    def is_idle(self) -> bool:
        """At least 80 % of the view is static. Asked now."""
        return bool(self.idle()["idle"])

    def wait_idle(self, timeout: float = 30.0) -> Frame:
        """Wait until the frame is idle, without polling; refreshed.
        Raises `TimeoutError`."""
        self._pilot.wait_idle(self.id, timeout=timeout)
        return self.refresh()

    def draw(self, rows: Any = None, **kw: Any) -> Sprite:
        return self._pilot.draw(rows, frame=self.id, **kw)

    def __repr__(self) -> str:
        return f"Frame({self.id}, {self.kind}, {self.state}, rect={self.rect})"


class Pilot(Tools, Widgets):
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
        host_fg: str | None = None,
        host_bg: str | None = None,
        record: str | Path | None = None,
        binary: str | None = None,
        timeout: float = 10.0,
    ) -> Pilot:
        """Start `ptyrc run --headless` on `argv` and connect to it. The
        driver is killed by `close()`. `host_fg`, `host_bg`: the default
        colours of the host that is not there, `"#rrggbb"`. `record`: write
        the session to this tape, typed bytes included (`Pilot.replay`)."""
        name = name or f"py-{os.getpid()}-{time.monotonic_ns() % 10**9}"
        if name in live_sessions():
            raise PtyrcError(
                "bad-request",
                f"session {name} is already running",
                "choose another name, or connect to it: Pilot(name)",
            )
        cmd = [binary or find_binary(), "run", "--headless", "--name", name]
        cmd += ["--size", f"{size[0]}x{size[1]}"]
        if host_log is not None:
            cmd += ["--host-log", str(host_log)]
        if host_fg is not None:
            cmd += ["--host-fg", host_fg]
        if host_bg is not None:
            cmd += ["--host-bg", host_bg]
        if record is not None:
            cmd += ["--record", str(record), "--record-input"]
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
                # The driver's own words: `ptyrc: <message>`, then a hint.
                message, _, hint = err.removeprefix("ptyrc: ").strip().partition("\n")
                raise PtyrcError(
                    "gone",
                    f"driver exited {proc.returncode}: {message}",
                    hint.strip() or None,
                )
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
        `batch()` it is queued and returns None; the methods that read
        (`text`, `find`, `sprites`, `Sprite.info`) return a list or a
        dict that is filled when the batch is sent."""
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
    def host_colors(self) -> dict[str, Any]:
        """Default colours of the host: `fg`, `bg` as `"#rrggbb"`, and
        `from`: for each, `"host"` (it answered), `"option"` or
        `"default"`. Asks the driver."""
        return self.info()["host_colors"]

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
        out: list[str] = []
        params = _opt(frame=frame, rect=rect)
        self._request("screen.text", params, lambda res: out.extend(res["lines"]))
        return out

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
        out: list[Match] = []
        self._request(
            "screen.find",
            {"pattern": pattern, **params},
            lambda res: out.extend(Match.from_wire(m) for m in res["matches"]),
        )
        return out

    def cursor(self, trail: bool = False) -> Cursor:
        if self._batch is not None:
            raise RuntimeError("cursor inside a batch")
        return Cursor.from_wire(self.call("screen.cursor", trail=trail))

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
        res = self._conn.call("screen.wait", params, timeout=clock.real(timeout) + 5.0)
        if not res.get("matched", False):
            raise TimeoutError(f"wait_for({pattern!r})")
        match = res.get("match")
        return Match.from_wire(match) if match else None

    def wait_idle(self, frame: int | str = "raw", *, timeout: float = 30.0) -> Frame:
        """Wait until `frame` is idle (`Frame.is_idle`), without polling.
        A name (`idle`, `quietest`, ...) is resolved by the driver at
        each look: `wait_idle("idle")` returns the first pane that
        qualifies. Raises `TimeoutError`."""
        if self._batch is not None:
            raise RuntimeError("wait_idle inside a batch")
        params = {"idle": True, "frame": frame, "timeout_ms": int(timeout * 1000)}
        res = self._conn.call("screen.wait", params, timeout=clock.real(timeout) + 5.0)
        if not res.get("matched", False):
            raise TimeoutError(f"wait_idle({frame!r})")
        return self.frame(res["frame"])

    # -- typing -----------------------------------------------------------

    def send(self, text: str) -> None:
        """Type text."""
        self.call("input.send", text=text)

    def keys(self, *keys: str) -> None:
        """Press keys: `enter`, `esc`, `ctrl+c`, `up`... Other strings are
        typed as text; a near miss of a key name (`Enter`, `return`,
        `ctrl+xx`) is refused, with the name that was meant."""
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
        cursor), `largest`, `quietest`, `idle` (the most static pane)."""
        if isinstance(sel, str) and sel.isdigit():
            sel = int(sel)
        return Frame(self, self.call("frame.get", frame=sel)["frame"])

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
        sprite = self._sprite("draw.put", spec)
        sprite._effect = (spec.get("effect") or {}).get("name")
        return sprite

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
        """Every sprite of the session, as `draw.list` tells them."""
        out: list[dict[str, Any]] = []
        self._request("draw.list", {}, lambda res: out.extend(res["sprites"]))
        return out

    def clear(self, all: bool = False) -> int:
        """Remove this connection's sprites, or every sprite with `all`."""
        res = self.call("draw.clear", all=all)
        return res["removed"] if res else 0

    # -- events and debug -------------------------------------------------

    def subscribe(self, *topics: str, snapshot: bool = False) -> None:
        """Ask for events: `screen` (screen rows written), `text` (per
        frame: the frame rows whose text changed, the origin after a
        scroll), `cursor`, `frames`, `idle` (a frame became idle, or
        busy), `draw` (sprites removed with the rows or the frame they
        were on), `widgets` (a widget placed, moved, resized, hidden),
        `input`, `size`, `apps` (the info of an app when it changes),
        `move` (`{"frame": id}`: this app is asked to go to that frame;
        subscribing says it can), `exit`.

        `snapshot`: the state now comes first, as events: `idle` of each
        frame on screen, `cursor`, `frames`, `size`. No poll at start."""
        params: dict[str, Any] = {"topics": list(topics)}
        if snapshot:
            params["snapshot"] = True
        self.call("subscribe", **params)

    def event(self, timeout: float | None = None) -> tuple[str, Any] | None:
        """The next event, `(topic, data)`; None after `timeout` seconds
        (session time, `clock`) of silence. Raises `PtyrcError` (`gone`)
        when the connection is closed and nothing is left to read."""
        wait = None if timeout is None else clock.real(timeout)
        try:
            msg = self._conn.events.get(timeout=wait)
        except queue.Empty:
            return None
        if msg is None:
            self._conn.events.put(None)  # for the next reader
            raise PtyrcError("gone", "connection closed")
        return msg["event"], msg.get("data")

    def events(
        self, *topics: str, timeout: float | None = None, snapshot: bool = False
    ) -> Iterator[tuple[str, Any]]:
        """Subscribe to `topics` and yield `(topic, data)` as they come.
        Silence ends it: `timeout` seconds without an event. A closed
        connection does not: it raises `PtyrcError` (`gone`), as any
        call would.

            for topic, data in pilot.events("cursor"):
                print(data["x"], data["y"])
        """
        if topics:
            self.subscribe(*topics, snapshot=snapshot)
        while (event := self.event(timeout)) is not None:
            yield event

    def debug_overlay(self, what: str = "frames") -> None:
        """Show `frames` or `heat` on the host; `off` hides it."""
        self.call("debug.overlay", what=what)

    def dump(self) -> dict[str, Any]:
        return self.call("debug.dump")

    # -- terminal ---------------------------------------------------------

    def show(self) -> None:
        """Mirror the remote screen in this terminal; ^C leaves."""
        try:
            self._cli("show")
        except KeyboardInterrupt:
            pass

    def attach(
        self, *, detach: str = "ctrl+t", resize: bool = False, sprites: bool = False
    ) -> None:
        """The remote screen in this terminal and its keyboard there;
        `detach` leaves. `resize`: the session takes this terminal's size
        (headless sessions). `sprites`: show them too."""
        args = ["attach", "--detach", detach]
        args += ["--resize"] * resize + ["--sprites"] * sprites
        self._cli(*args)

    def _cli(self, *args: str) -> None:
        subprocess.run([find_binary(), "--session", self.session, *args], check=False)


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
                self.results.append(PtyrcError.from_wire(entry["error"]))
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
