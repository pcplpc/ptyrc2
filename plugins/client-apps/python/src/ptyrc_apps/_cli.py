# SPDX-License-Identifier: GPL-3.0-or-later
"""What every app shares: the session option and a clean way out."""

from __future__ import annotations

import argparse
import contextlib
import sys
from collections.abc import Callable, Iterator, Sequence
from typing import Any

from ptyrc import Cursor, Frame, Pilot, PtyrcError, Sprite, clock

__all__ = [
    "Nozzle",
    "Sight",
    "follow",
    "frame_arg",
    "heir",
    "hold",
    "in_sight",
    "movable",
    "on_screen",
    "parse",
    "parser",
    "pick",
    "run",
    "ticks",
]


def parser(prog: str, description: str, frame: str = "raw") -> argparse.ArgumentParser:
    """An argument parser with the options common to all apps. `frame` is
    the app's default frame."""
    out = argparse.ArgumentParser(prog=f"ptyrc-app {prog}", description=description)
    out.add_argument(
        "-s",
        "--session",
        help="session to draw on [default: $PTYRC_SESSION]",
    )
    out.add_argument(
        "-f",
        "--frame",
        default=frame,
        help="frame id or name: raw, active, largest, quietest, idle "
        f"[default: {frame}]",
    )
    return out


def run(
    app: Callable[[Pilot, argparse.Namespace], None],
    args: argparse.Namespace,
) -> int:
    """Connect, run `app`, leave quietly.

    Sprites belong to the connection: when the app returns, is interrupted
    or crashes, the driver removes what it drew. Nothing to clean up.
    """
    try:
        pilot = Pilot(args.session)
    except PtyrcError as exc:
        print(f"ptyrc-app: {exc}", file=sys.stderr)
        return 1
    try:
        with pilot:
            app(pilot, args)
    except KeyboardInterrupt:
        pass
    except PtyrcError as exc:
        if exc.code != "gone":  # the session ended: a normal way to stop
            print(f"ptyrc-app: {exc}", file=sys.stderr)
            return 1
    return 0


def frame_arg(value: str) -> int | str:
    """`-f 3` is an id, `-f active` a name."""
    return int(value) if value.isdigit() else value


def parse(
    out: argparse.ArgumentParser, argv: Sequence[str] | None
) -> argparse.Namespace:
    # Options between the words: `marks ERROR -i WARN`.
    args = out.parse_intermixed_args(argv)
    args.frame = frame_arg(args.frame)
    return args


def pick(pilot: Pilot, sel: int | str) -> Frame:
    """The frame `sel`, or the raw frame when a name finds no pane (a
    plain shell has none)."""
    try:
        return pilot.frame(sel)
    except PtyrcError as exc:
        if exc.code != "no-frame" or isinstance(sel, int):
            raise
        return pilot.frame("raw")


def ticks(period: float, seconds: float | None = None) -> Iterator[float]:
    """Yield the time step, then sleep to the next tick; stop after
    `seconds`. The step is measured, so motion keeps its speed when a
    tick is late."""
    start = last = clock.monotonic()
    while seconds is None or last - start < seconds:
        clock.sleep(max(0.0, last + period - clock.monotonic()))
        now = clock.monotonic()
        yield min(now - last, 4 * period)
        last = now


def hold(pilot: Pilot, seconds: float | None = None) -> None:
    """Stay connected with nothing to send: until the session ends, or
    for `seconds`. Sprites live as long as the connection. A session
    that ends without a word raises `gone`: `run` takes it for an end."""
    for _ in pilot.events("exit", timeout=seconds):
        return


def heir(
    frame: Frame, panes: Sequence[Frame], scale: tuple[float, float] = (1.0, 1.0)
) -> Frame | None:
    """Of `panes`, the one that took the place of `frame`, now gone:
    usable, same key (another tmux window is not a place to follow
    to), over at least half of the smaller of the two; the one that
    overlaps most. `scale`: how much the screen grew since `frame` was
    seen, across and down; its rectangle is scaled as much first."""
    x, y, w, h = (v * k for v, k in zip(frame.rect, scale * 2, strict=True))

    def shared(pane: Frame) -> float:
        px, py, pw, ph = pane.rect
        across = min(x + w, px + pw) - max(x, px)
        down = min(y + h, py + ph) - max(y, py)
        return max(0, across) * max(0, down)

    heirs = [
        pane
        for pane in panes
        if pane.id != frame.id
        and pane.kind == "pane"
        and pane.usable
        and pane.key == frame.key
        and 2 * shared(pane) >= min(w * h, pane.size[0] * pane.size[1])
    ]
    return max(heirs, key=shared, default=None)


def movable(pilot: Pilot) -> bool:
    """Subscribe to the topic `move`: the app says it changes frame
    while it runs (`ptyrc manage move`); the event `{"frame": id}` then
    comes to it alone. False on a driver that has no such topic: the
    app runs as before, and is restarted to be moved."""
    try:
        pilot.subscribe("move")
    except PtyrcError as exc:
        if exc.code != "bad-params":
            raise
        return False
    return True


class Nozzle:
    """The host pointer over a frame, told to the effect of `sprite`:
    `Sprite.tune(hover=[x, y])`, a cell of the view, or `hover=None`
    when the pointer is elsewhere (`hover`); a press on the frame, when
    it is released: `Sprite.tune(bomb=[x, y, ms])`, the cell pressed and
    how long; `bomb=[x, y, ms, dx, dy]` when released on another cell,
    that many cells away: its aim (`bomb`).

    A hover, a click, are told on a sprite, and not on an effect: a
    sprite of blanks, the size of the view, lies under the effect (`z`
    -1). The pointer on a sprite over it is not on the frame. With
    `bomb` the driver takes the clicks on the frame: the program in it
    gets none. On a driver without the topics `hover`, `mouse` nothing
    is told."""

    def __init__(
        self,
        pilot: Pilot,
        frame: Frame,
        sprite: Sprite,
        hover: bool = True,
        bomb: bool = False,
    ) -> None:
        self.sprite = sprite
        self.at: list[int] | None = None  # as last told
        # A press: its cell, when, its screen cell, the last one told.
        self.down: tuple[list[int], float, list[int], list[int]] | None = None
        self.pad: Sprite | None = None
        topics = [t for t, on in (("hover", hover), ("mouse", bomb)) if on]
        try:
            pilot.subscribe(*topics)
        except PtyrcError as exc:
            if exc.code != "bad-params":
                raise
            return
        self._on = (frame.id, frame.size)
        self.pad = pilot.draw(
            self._blank(frame), frame=frame.id, at=(0, 0), space="view", z=-1
        )

    @staticmethod
    def _blank(frame: Frame) -> list[str]:
        w, h = frame.size
        return [" " * w] * h

    def place(self, frame: Frame) -> None:
        """The frame followed is `frame` now, or has another size."""
        if self.pad is None or self._on == (frame.id, frame.size):
            return
        with contextlib.suppress(PtyrcError):  # gone, or settling: the next one
            if self._on[0] != frame.id:
                self.pad.update(frame=frame.id)
            self.pad.set(self._blank(frame))
            self._on = (frame.id, frame.size)

    def hover(self, data: dict[str, Any]) -> None:
        """An event of the topic `hover`."""
        if self.pad is None:
            return
        at = data.get("at") if data.get("sprite") == self.pad.id else None
        if at != self.at:
            self.at = at
            self.sprite.tune(hover=at)

    def mouse(self, data: dict[str, Any]) -> None:
        """An event of the topic `mouse`: the release of a press on the
        frame is told, with the cell pressed, the time it was held and,
        released elsewhere, the cells from the one to the other."""
        if self.pad is None or data.get("sprite") != self.pad.id:
            return
        action, at = data.get("action"), data.get("at")
        # The screen cell: told off the sprite too, where `at` is not.
        here = [data.get("x"), data.get("y")]
        if action == "press" and at is not None:
            self.down = (at, clock.monotonic(), here, here)
        elif action in ("drag", "release") and self.down is not None:
            at, since, start, last = self.down
            if None not in here:
                last = here
            self.down = (at, since, start, last)
            if action == "drag":
                return
            self.down = None
            bomb = [*at, int(1000 * (clock.monotonic() - since))]
            if None not in start and last != start:
                bomb += [last[0] - start[0], last[1] - start[1]]
            self.sprite.tune(bomb=bomb)


def follow(
    pilot: Pilot,
    frame: Frame,
    sprite: Sprite,
    seconds: float | None = None,
    beats: Iterator[float] | None = None,
    hover: bool = False,
    bomb: bool = False,
) -> None:
    """Stay connected, until the session ends or for `seconds`; move
    `sprite` to the pane that takes the place of `frame`. Driven by the
    topic `frames`: every change of layout comes with the frames as
    they are, so nothing is asked. The size of the screen is noted
    while the frame is settled: after a resize the place is looked for
    in proportion. It takes the topic `move` too: the sprite goes to
    the frame named, and is followed there.

    `beats`: the app's own work between two events. Each `next()` gives
    the seconds to wait before the one after: a generator does its work
    after each `yield`.

    `hover`: the effect of `sprite` is told the pointer; `bomb`: the
    clicks on the frame (`Nozzle`)."""
    now = clock.monotonic()
    end = None if seconds is None else now + seconds
    beat = None if beats is None else now + next(beats)
    screen: tuple[int, int] | None = None  # when `frame` was last settled
    pilot.subscribe("frames", "exit", snapshot=True)
    movable(pilot)
    nozzle = Nozzle(pilot, frame, sprite, hover, bomb) if hover or bomb else None
    while True:
        now = clock.monotonic()
        if end is not None and now >= end:
            return
        if beats is not None and beat is not None and now >= beat:
            beat = now + next(beats)
        waits = [t - now for t in (end, beat) if t is not None]
        event = pilot.event(timeout=min(waits) if waits else None)
        if event is None:
            continue
        if event[0] == "exit":
            return
        if event[0] == "hover":
            if nozzle is not None:
                nozzle.hover(event[1])
            continue
        if event[0] == "mouse":
            if nozzle is not None:
                nozzle.mouse(event[1])
            continue
        if nozzle is not None:
            nozzle.place(frame)  # as the last event left it
        if event[0] == "move":
            try:
                new = pilot.frame(event[1]["frame"])
                sprite.update(frame=new.id)
            except PtyrcError as exc:
                if exc.code not in ("no-frame", "not-usable"):
                    raise
                continue  # gone since it was asked: it stays
            frame, screen = new, None
            continue
        if event[0] != "frames" or frame.kind == "raw":
            continue
        frames = [Frame(pilot, info) for info in event[1]["frames"]]
        raw = next((f for f in frames if f.kind == "raw"), None)
        mine = next((f for f in frames if f.id == frame.id), None)
        if mine is not None and mine.state != "inactive":
            # In sight, or settling: the driver keeps the sprite.
            if raw is not None and raw.state == mine.state == "stable":
                frame, screen = mine, raw.size
            continue
        scale = (1.0, 1.0)
        if raw is not None and screen is not None:
            scale = (raw.size[0] / screen[0], raw.size[1] / screen[1])
        if (new := heir(frame, frames, scale)) is not None:
            sprite.update(frame=new.id)
            frame, screen = new, raw.size if raw is not None else None


def on_screen(frame: Frame) -> bool:
    """Refresh `frame`; False while it cannot be drawn on: out of sight
    (another tmux window), or a new layout still settling. Its sprites
    are kept by the driver and return with it."""
    try:
        return frame.refresh().usable
    except PtyrcError as exc:
        if exc.code != "no-frame":
            raise
        return False


@contextlib.contextmanager
def in_sight() -> Iterator[None]:
    """Drop what is left of a step when its frame goes out of sight in
    the middle of it; `on_screen` then waits for it."""
    try:
        yield
    except PtyrcError as exc:
        if exc.code != "not-usable":
            raise


class Sight:
    """What an app that plays on a frame knows of it, told by the
    driver: nothing is asked on a period.

    Topics, subscribed with a snapshot: `frames` (the layout: whether
    the frame is on screen, and where), `text` (rows of its view
    changed, or the view moved), `cursor` when wanted. A tick starts
    with `look()`, which takes the events that came since the last one;
    the text is read again only when told. The events of the topics
    the app subscribed to itself are in `others`, for that look.

        sight = Sight(pilot, frame)
        for _ in ticks(period):
            if not sight.look():
                continue            # out of sight, or settling
            if sight.changed:
                walls = walls_of(sight.rows, *frame.origin)
            with sight.drawing():
                ...

    `frame` is kept up to date in place: `origin`, `rect`, `usable`.
    """

    def __init__(self, pilot: Pilot, frame: Frame, cursor: bool = False) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rows: list[str] = []  # the text of the view
        self.changed = False  # `rows` was read by the last `look()`
        self.cursor: tuple[int, int] | None = None  # in frame cells; None: hidden
        #: The events of other topics the last `look()` took, in order.
        self.others: list[tuple[str, Any]] = []
        self._at: Cursor | None = None
        self._stale = True
        topics = ("frames", "text", "cursor") if cursor else ("frames", "text")
        pilot.subscribe(*topics, snapshot=True)

    def look(self) -> bool:
        """Take the events waiting. False while the frame cannot be
        drawn on: out of sight (another tmux window), or a new layout
        still settling; its sprites are kept by the driver."""
        frame = self.frame
        self.others = []
        while (event := self.pilot.event(timeout=0)) is not None:
            topic, data = event
            if topic == "frames":
                infos = (f for f in data["frames"] if f["id"] == frame.id)
                info = next(infos, None) or {**frame.info, "usable": False}
                self._stale |= (info["rect"], info["origin"]) != (
                    frame.info["rect"],
                    frame.info["origin"],
                )
                frame.info = info
            elif topic == "text" and data["frame"] == frame.id:
                frame.info.update(origin=data["origin"], rect=data["rect"])
                self._stale = True
            elif topic == "cursor":
                self._at = Cursor.from_wire(data)
            elif topic not in ("frames", "text"):
                self.others.append(event)
        self.changed = False
        if not frame.usable:
            # Asked meanwhile: a raw frame alone settles with no event.
            if not on_screen(frame):
                return False
            self._stale = True
        if self._stale:
            with self.drawing():
                self.rows = frame.text()
                self.changed, self._stale = True, False
        at = self._at
        if at is None or not at.visible:
            self.cursor = None
        elif at.frame == frame.id and at.pos is not None:
            self.cursor = at.pos
        else:
            self.cursor = frame.to_frame(at.x, at.y)
        return not self._stale

    @contextlib.contextmanager
    def drawing(self) -> Iterator[None]:
        """A step on the frame. What is left of it is dropped when the
        frame goes out of sight in the middle; `look` then waits."""
        try:
            yield
        except PtyrcError as exc:
            if exc.code != "not-usable":
                raise
            self.frame.info["usable"] = False
