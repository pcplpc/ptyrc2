# SPDX-License-Identifier: GPL-3.0-or-later
"""Sand: a thin stream piles on the text of an idle pane.

    ptyrc-app sand                 # the most idle pane, once it is idle
    ptyrc-app sand -f 3 --rate 300 --now
    ptyrc-app sand -f active --cursor --now

What it shows:

- `Pilot.wait_idle`: the driver knows which panes have been still for
  a while, and answers when one is. A big animation starts there, not
  where someone is typing. Nothing is polled.
- A native effect from a draw extension (`plugins/draw-ext/sand`): the
  simulation runs in the driver, 80 steps a second, with nothing on the
  socket. One request starts it; the app then only stays connected.
- An effect that sees the screen: the text under it is its walls, read
  by the driver at each render. Text written on the pile takes the
  place of its grains; a resize cuts off what no longer fits.
- `--cursor`: the stream falls on the cursor's column.
- A frame replaced: of two panes with little text, resized together,
  the driver cannot tell which is which and allocates new frames. The
  app is told by the topic `frames` and moves its sprite to the pane
  that took the place (`Sprite.update(frame=...)`): the effect, and its
  pile, go on. Nothing is polled: the app sleeps on the socket.
"""

from __future__ import annotations

import argparse
import random
import time
from collections.abc import Sequence

from ptyrc import Frame, Pilot, PtyrcError, Sprite

from . import _cli

__all__ = ["follow", "heir", "main", "pour", "still"]

POLL = 0.5  # seconds between two looks at a layout still settling


def pour(pilot: Pilot, frame: Frame, rate: float, cursor: bool = False) -> Sprite:
    """Start the sand on `frame`. `cursor`: the stream follows the
    cursor's column."""
    params = {"rate": rate, "cursor": cursor, "seed": random.getrandbits(32)}
    # Behind the text: a grain never hides a letter.
    return pilot.effect("sand", params, frame=frame.id, layer="back")


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
    pilot: Pilot, frame: Frame, sprite: Sprite, seconds: float | None = None
) -> None:
    """Stay connected, until the session ends or for `seconds`; move
    `sprite` to the pane that takes the place of `frame`. Driven by the
    topic `frames`: every change of layout comes with the frames as
    they are, so nothing is asked."""
    end = None if seconds is None else time.monotonic() + seconds
    pilot.subscribe("frames", "exit")
    while True:
        left = None if end is None else end - time.monotonic()
        if left is not None and left <= 0:
            return
        event = pilot.event(timeout=left)
        if event is None or event[0] == "exit":
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


def still(pilot: Pilot, sel: int | str, timeout: float) -> Frame | None:
    """The frame `sel` once it is idle; None after `timeout` seconds.
    One request: the driver answers when it is so. It resolves a name
    at each look: the most idle pane now may not be the one of a moment
    ago. A name that finds no pane (a plain shell) is the raw frame."""
    try:
        return pilot.wait_idle(sel, timeout=timeout)
    except TimeoutError:
        return None
    except PtyrcError as exc:
        if exc.code != "no-frame" or isinstance(sel, int):
            raise
    try:
        return pilot.wait_idle("raw", timeout=min(timeout, 1.0))
    except TimeoutError:
        return None


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    end = None if seconds is None else time.monotonic() + seconds
    while True:
        left = 30.0 if end is None else end - time.monotonic()
        if left <= 0:
            return
        if args.now:
            frame: Frame | None = _cli.pick(pilot, args.frame)
        else:
            frame = still(pilot, args.frame, left)
        if frame is not None and frame.usable:
            break
        if args.now or frame is not None:
            time.sleep(POLL)  # a layout still settling
    sand = pour(pilot, frame, args.rate, args.cursor)
    follow(pilot, frame, sand, None if end is None else end - time.monotonic())


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("sand", "Sand falling in an idle pane.", frame="idle")
    out.add_argument(
        "--rate", type=float, default=60.0, help="grains per second [default: 60]"
    )
    out.add_argument(
        "--cursor",
        action="store_true",
        help="pour over the cursor's column; from the middle while the "
        "cursor is out of the pane or on its first column",
    )
    out.add_argument(
        "--now", action="store_true", help="do not wait for the pane to be idle"
    )
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
