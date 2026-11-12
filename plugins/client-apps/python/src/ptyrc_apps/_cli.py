# SPDX-License-Identifier: GPL-3.0-or-later
"""What every app shares: the session option and a clean way out."""

from __future__ import annotations

import argparse
import contextlib
import sys
import time
from collections.abc import Callable, Iterator, Sequence

from ptyrc import Frame, Pilot, PtyrcError, Sprite

__all__ = [
    "follow",
    "frame_arg",
    "heir",
    "hold",
    "in_sight",
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
        help="session to draw on [default: $PTYRC_SESSION, else the only one]",
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
    start = last = time.monotonic()
    while seconds is None or last - start < seconds:
        time.sleep(max(0.0, last + period - time.monotonic()))
        now = time.monotonic()
        yield min(now - last, 4 * period)
        last = now


def hold(pilot: Pilot, seconds: float | None = None) -> None:
    """Stay connected with nothing to send: until the session ends, or
    for `seconds`. Sprites live as long as the connection. A session
    that ends without a word raises `gone`: `run` takes it for an end."""
    for _ in pilot.events("exit", timeout=seconds):
        return


def heir(frame: Frame, panes: Sequence[Frame]) -> Frame | None:
    """Of `panes`, the one that took the place of `frame`, now
    inactive: usable, same key (another tmux window is not a place to
    follow to), over at least half of the smaller of the two; the one
    that overlaps most."""
    x, y, w, h = frame.rect

    def shared(pane: Frame) -> int:
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


def follow(
    pilot: Pilot,
    frame: Frame,
    sprite: Sprite,
    seconds: float | None = None,
    beats: Iterator[float] | None = None,
) -> None:
    """Stay connected, until the session ends or for `seconds`; move
    `sprite` to the pane that takes the place of `frame`. Driven by the
    topic `frames`: every change of layout comes with the frames as
    they are, so nothing is asked.

    `beats`: the app's own work between two events. Each `next()` gives
    the seconds to wait before the one after: a generator does its work
    after each `yield`."""
    now = time.monotonic()
    end = None if seconds is None else now + seconds
    beat = None if beats is None else now + next(beats)
    pilot.subscribe("frames", "exit")
    while True:
        now = time.monotonic()
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
        if event[0] != "frames" or frame.kind == "raw":
            continue
        frames = [Frame(pilot, info) for info in event[1]["frames"]]
        mine = next((f for f in frames if f.id == frame.id), None)
        if mine is not None and mine.state != "inactive":
            continue  # in sight, or settling: the driver keeps the sprite
        if (new := heir(frame, frames)) is not None:
            sprite.update(frame=new.id)
            frame = new


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
