# SPDX-License-Identifier: GPL-3.0-or-later
"""Tools of the pilot: locators, clicks, grabbed keys, tapes, snapshots."""

from __future__ import annotations

import os
import subprocess
import threading
import time
from collections.abc import Callable, Iterable, Iterator, Sequence
from pathlib import Path
from typing import TYPE_CHECKING, Any, Self

from ._wire import PtyrcError, live_sessions, runtime_dir

if TYPE_CHECKING:
    from .pilot import Frame, Match, Pilot, Sprite

__all__ = ["Locator", "Tape", "Tools", "frame_locator"]


class Locator:
    """A place on a frame: a pattern, then steps from its match. Nothing
    is looked up until it is used, and it is looked up again each time.

        todo = pilot.frame("active").locator("TODO")
        todo.wait()
        todo.right(2).read(20)       # 20 cells, 2 after the match
        todo.nth(1).click()

    A step right counts from the last cell of the match, a step left
    from its first; steps add up.
    """

    def __init__(
        self,
        pilot: Pilot,
        pattern: str,
        *,
        frame: int | str = "raw",
        regex: bool = False,
        ignore_case: bool = False,
        nth: int = 0,
        dx: int = 0,
        dy: int = 0,
    ) -> None:
        self._pilot = pilot
        self.pattern = pattern
        self.frame = frame
        self.regex = regex
        self.ignore_case = ignore_case
        self._nth = nth
        self.dx = dx
        self.dy = dy

    def _with(self, **kw: Any) -> Locator:
        args = {
            "frame": self.frame,
            "regex": self.regex,
            "ignore_case": self.ignore_case,
            "nth": self._nth,
            "dx": self.dx,
            "dy": self.dy,
        }
        return Locator(self._pilot, self.pattern, **{**args, **kw})

    def nth(self, n: int) -> Locator:
        """The match number `n` in reading order, from 0; negative: from
        the last."""
        return self._with(nth=n)

    def right(self, n: int = 1) -> Locator:
        return self._with(dx=self.dx + n)

    def left(self, n: int = 1) -> Locator:
        return self._with(dx=self.dx - n)

    def above(self, n: int = 1) -> Locator:
        return self._with(dy=self.dy - n)

    def below(self, n: int = 1) -> Locator:
        return self._with(dy=self.dy + n)

    def all(self) -> list[Match]:
        """Every match in the view of the frame."""
        found = self._pilot.find(
            self.pattern,
            regex=self.regex,
            ignore_case=self.ignore_case,
            frame=self.frame,
        )
        return [m for m in found if m.screen is not None]

    def count(self) -> int:
        return len(self.all())

    def match(self) -> Match | None:
        found = self.all()
        try:
            return found[self._nth]
        except IndexError:
            return None

    def _offset(self, match: Match) -> tuple[int, int]:
        return ((match.len - 1 if self.dx > 0 else 0) + self.dx, self.dy)

    def pos(self) -> tuple[int, int] | None:
        """The place in frame coordinates; None without a match."""
        match = self.match()
        if match is None:
            return None
        dx, dy = self._offset(match)
        return (match.x + dx, match.y + dy)

    def screen(self) -> tuple[int, int] | None:
        """The place as a screen cell; None without a match, or off the
        screen."""
        match = self.match()
        if match is None or match.screen is None:
            return None
        dx, dy = self._offset(match)
        col, row = match.screen[0] + dx, match.screen[1] + dy
        cols, rows = self._pilot.size
        return (col, row) if 0 <= col < cols and 0 <= row < rows else None

    def _cell(self) -> tuple[int, int]:
        cell = self.screen()
        if cell is None:
            raise PtyrcError(
                "not-usable",
                f"{self!r} is not on screen",
                "wait() for it, or scroll it into the view",
            )
        return cell

    def visible(self) -> bool:
        return self.screen() is not None

    def wait(self, timeout: float = 5.0, *, gone: bool = False) -> Locator:
        """Wait until the place is on screen (`gone`: until it is not).
        Raises `TimeoutError`."""
        deadline = time.monotonic() + timeout
        while self.visible() == gone:
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError(f"{self!r}.wait({'gone' if gone else ''})")
            # Until the screen changes, or a moment.
            try:
                self._pilot.wait_for(stable=0.02, timeout=min(left, 0.2))
            except TimeoutError:
                pass
        return self

    def read(self, width: int | None = None) -> str:
        """Text of the screen from the place: `width` cells, or to the end
        of the row."""
        col, row = self._cell()
        if width is None:
            width = self._pilot.size[0] - col
        return self._pilot.text(rect=[col, row, width, 1])[0]

    def click(
        self,
        button: str = "left",
        *,
        mods: Iterable[str] = (),
        count: int = 1,
        to: Locator | tuple[int, int] | None = None,
    ) -> None:
        """Click on the place, as the app's mouse mode wants it: see
        `Pilot.click`. `to`: drag to that place or screen cell."""
        col, row = self._cell()
        if isinstance(to, Locator):
            to = to._cell()
        self._pilot.click(col, row, button, mods=mods, count=count, to=to)

    def draw(self, rows: Any = None, **kw: Any) -> Sprite:
        """A sprite at the place, fixed in the frame: it scrolls with the
        text."""
        match = self.match()
        if match is None:
            raise PtyrcError("not-usable", f"{self!r} has no match")
        dx, dy = self._offset(match)
        at = (match.x + dx, match.y + dy)
        return self._pilot.draw(rows, frame=match.frame, at=at, space="frame", **kw)

    def __repr__(self) -> str:
        text = f"locator({self.pattern!r}"
        if self.frame != "raw":
            text += f", frame={self.frame!r}"
        text += ")"
        if self._nth:
            text += f".nth({self._nth})"
        if self.dx:
            text += f".right({self.dx})" if self.dx > 0 else f".left({-self.dx})"
        if self.dy:
            text += f".below({self.dy})" if self.dy > 0 else f".above({-self.dy})"
        return text


class Tape:
    """The clock of a replay (`Pilot.replay`). Tape time is in seconds; it
    stands still while paused, and so do the driver's timers."""

    def __init__(self, pilot: Pilot) -> None:
        self._pilot = pilot

    def status(self) -> dict[str, Any]:
        """`t`, `end`, `records`, `next`, `paused`, `speed`, `done`."""
        return self._pilot.call("tape.status")

    def step(self, records: int = 1) -> dict[str, Any]:
        """Feed the next records, then pause."""
        return self._pilot.call("tape.step", records=records)

    def seek(self, t: float) -> dict[str, Any]:
        """Run to tape time `t` without waiting, then pause. Past the end
        of the tape, time passes and nothing is fed."""
        return self._pilot.call("tape.seek", t=t)

    def play(self, speed: float | None = None) -> dict[str, Any]:
        """Go on; `speed` tape seconds per second, 0: no waiting."""
        params = {} if speed is None else {"speed": speed}
        return self._pilot.call("tape.play", **params)

    def pause(self) -> dict[str, Any]:
        return self._pilot.call("tape.pause")

    @property
    def end(self) -> float:
        return self.status()["end"]


class Tools:
    """Methods of `Pilot` for the debug tools."""

    # Given by `Pilot`.
    call: Any
    _proc: subprocess.Popen[bytes] | None

    @classmethod
    def replay(
        cls,
        tape: str | Path,
        *,
        name: str | None = None,
        presets: bool = True,
        binary: str | None = None,
        timeout: float = 10.0,
    ) -> Self:
        """Start `ptyrc replay` on a tape, paused at its start, and connect
        to it; `pilot.tape` moves it. The replay is killed by `close()`."""
        from .pilot import find_binary

        name = name or f"py-{os.getpid()}-{time.monotonic_ns() % 10**9}"
        if name in live_sessions():
            raise PtyrcError("bad-request", f"session {name} is already running")
        cmd = [binary or find_binary(), "replay", str(tape), "--name", name, "--step"]
        if not presets:
            cmd.append("--no-presets")
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        path = runtime_dir() / f"{name}.sock"
        deadline = time.monotonic() + timeout
        while True:
            if proc.poll() is not None:
                err = proc.stderr.read().decode(errors="replace") if proc.stderr else ""
                message, _, hint = err.removeprefix("ptyrc: ").strip().partition("\n")
                raise PtyrcError(
                    "bad-request", f"no replay: {message}", hint.strip() or None
                )
            if path.exists():
                try:
                    pilot = cls(path=path, timeout=timeout)  # type: ignore[call-arg]
                    break
                except PtyrcError:
                    pass
            if time.monotonic() > deadline:
                proc.kill()
                raise PtyrcError("timeout", "the replay did not start")
            time.sleep(0.01)
        pilot._proc = proc
        return pilot

    @property
    def tape(self) -> Tape:
        """The clock of this session, when it is a replay."""
        return Tape(self)  # type: ignore[arg-type]

    def locator(
        self,
        pattern: str,
        *,
        frame: int | str = "raw",
        regex: bool = False,
        ignore_case: bool = False,
    ) -> Locator:
        """A place on the screen, by the text there: see `Locator`."""
        return Locator(
            self,  # type: ignore[arg-type]
            pattern,
            frame=frame,
            regex=regex,
            ignore_case=ignore_case,
        )

    def click(
        self,
        x: int,
        y: int,
        button: str = "left",
        *,
        mods: Iterable[str] = (),
        count: int = 1,
        to: tuple[int, int] | None = None,
    ) -> None:
        """Click on a screen cell: `left`, `middle`, `right`, `wheel-up`,
        `wheel-down`, `wheel-left`, `wheel-right`. `mods`: `shift`,
        `alt`, `ctrl` held. `count`: 2 is a double click. `to`: a drag,
        released on that cell. Refused when the app did not ask for the
        mouse."""
        params: dict[str, Any] = {"x": x, "y": y, "button": button}
        if mods := list(mods):
            params["mods"] = mods
        if count != 1:
            params["count"] = count
        if to is not None:
            params["to"] = list(to)
        self.call("input.click", **params)

    def clicks(
        self, *sprites: Any, timeout: float | None = None, presses: bool = True
    ) -> Iterator[dict[str, Any]]:
        """Clicks of the host mouse on sprites, as they come: `{"sprite",
        "button", "action", "mods", "at", "x", "y", "frame", "pos"}`;
        `at` is the cell in the sprite, `pos` the one in `frame`. Those
        on `sprites` (sprites or ids), or on any. `presses`: only the
        button going down; else releases, drags and the wheel too.
        Silence for `timeout` seconds ends it.

        The driver asks the host terminal for mouse reports while a
        client listens; a click that is on no sprite goes to the app.

            for click in pilot.clicks(button):
                button.update(content=["[ ok ]"])
        """
        ids = {getattr(sprite, "id", sprite) for sprite in sprites}
        self.subscribe("mouse")
        while (event := self.event(timeout)) is not None:
            topic, data = event
            if topic != "mouse" or (ids and data["sprite"] not in ids):
                continue
            if presses and data["action"] != "press":
                continue
            yield data

    def on_click(
        self, sprite: Any, handler: Callable[[dict[str, Any]], Any]
    ) -> threading.Thread:
        """Call `handler(click)` for each press on `sprite`, from a thread
        of its own, until the session ends or `handler` returns False.
        The events of this pilot are read by that thread: use another
        pilot for other topics."""

        def run() -> None:
            try:
                for click in self.clicks(sprite):
                    if handler(click) is False:
                        return
            except PtyrcError:
                pass

        thread = threading.Thread(target=run, name="ptyrc-clicks", daemon=True)
        thread.start()
        return thread

    def grab(self, *keys: str, escape: str = "ctrl+]") -> None:
        """Take keys of the host keyboard before the app: those named, or
        all. Each comes as an `input` event, `{"key", "bytes", "grabbed"}`
        (`Pilot.events()`). `escape` always releases, as `release()` and
        the end of this connection do; then `{"released": "escape"}`."""
        params: dict[str, Any] = {"escape": escape}
        if keys:
            params["keys"] = list(keys)
        self.call("input.grab", **params)

    def release(self) -> None:
        """Give the grabbed keys back to the app."""
        self.call("input.release")

    def shown(
        self,
        frame: int | str | None = None,
        rect: Sequence[int] | None = None,
        *,
        ansi: bool = False,
    ) -> list[str]:
        """Text of what the host shows: the app and the sprites over it."""
        params: dict[str, Any] = {"sprites": True, "ansi": ansi}
        if frame is not None:
            params["frame"] = frame
        if rect is not None:
            params["rect"] = list(rect)
        return self.call("screen.text", **params)["lines"]

    def snapshot(self) -> dict[str, Any]:
        """What the session shows and its frames, without ids nor times:
        `size`, `alt_screen`, `title`, `cursor`, `text`, `frames`. The
        same as `ptyrc snap`."""
        info = self.call("session.info")
        cursor = self.call("screen.cursor")
        keys = (
            "kind",
            "state",
            "usable",
            "rect",
            "origin",
            "extent",
            "key",
            "support",
        )
        return {
            "size": info["size"],
            "alt_screen": info.get("alt_screen"),
            "title": info.get("title"),
            "cursor": [cursor["x"], cursor["y"], cursor.get("visible")],
            "text": self.call("screen.text")["lines"],
            "frames": [
                {k: f.get(k) for k in keys} for f in self.call("frame.list")["frames"]
            ],
        }


def frame_locator(frame: Frame, pattern: str, **kw: Any) -> Locator:
    """`Frame.locator`."""
    return Locator(frame._pilot, pattern, frame=frame.id, **kw)
