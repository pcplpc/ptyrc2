# SPDX-License-Identifier: GPL-3.0-or-later
"""Sand: a thin stream piles on the text of an idle pane.

    ptyrc-app sand                 # the most idle pane, once it is idle
    ptyrc-app sand -f 3 --rate 300 --now
    ptyrc-app sand -f active --cursor --now

What it shows:

- `Frame.is_idle`: the driver knows which panes have been still for a
  while. A big animation starts there, not where someone is typing.
- A native effect from a draw extension (`plugins/draw-ext/sand`): the
  simulation runs in the driver, 80 steps a second, with nothing on the
  socket. One request starts it; the app then only stays connected.
- An effect that sees the screen: the text under it is its walls, read
  by the driver at each render. Text written on the pile takes the
  place of its grains; a resize cuts off what no longer fits.
- `--cursor`: the stream falls on the cursor's column.
- A frame replaced: a pane with little text does not keep its identity
  through a resize, the driver allocates a new frame. The app moves its
  sprite there (`Sprite.update(frame=...)`): the effect, and its pile,
  go on.
"""

from __future__ import annotations

import argparse
import random
import time
from collections.abc import Sequence

from ptyrc import Frame, Pilot, Sprite

from . import _cli

__all__ = ["heir", "main", "pour"]

POLL = 0.5  # seconds between two looks at the pane


def pour(pilot: Pilot, frame: Frame, rate: float, cursor: bool = False) -> Sprite:
    """Start the sand on `frame`. `cursor`: the stream follows the
    cursor's column."""
    params = {"rate": rate, "cursor": cursor, "seed": random.getrandbits(32)}
    # Behind the text: a grain never hides a letter.
    return pilot.effect("sand", params, frame=frame.id, layer="back")


def heir(pilot: Pilot, frame: Frame) -> Frame | None:
    """The pane that took the place of `frame`, now inactive: usable,
    same key (another tmux window is not a place to follow to), over at
    least half of the smaller of the two; the one that overlaps most."""
    x, y, w, h = frame.rect

    def shared(pane: Frame) -> int:
        px, py, pw, ph = pane.rect
        across = min(x + w, px + pw) - max(x, px)
        down = min(y + h, py + ph) - max(y, py)
        return max(0, across) * max(0, down)

    panes = [
        pane
        for pane in pilot.panes()
        if pane.id != frame.id
        and pane.key == frame.key
        and 2 * shared(pane) >= min(w * h, pane.size[0] * pane.size[1])
    ]
    return max(panes, key=shared, default=None)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    end = None if seconds is None else time.monotonic() + seconds
    while True:
        # A name is asked again until the sand falls: the most idle pane
        # now may not be the one of a moment ago.
        frame = _cli.pick(pilot, args.frame)
        if _cli.on_screen(frame) and (args.now or frame.is_idle):
            break
        if end is not None and time.monotonic() >= end:
            return
        time.sleep(POLL)
    sand = pour(pilot, frame, args.rate, args.cursor)
    while end is None or time.monotonic() < end:
        time.sleep(POLL)
        if frame.kind == "raw" or _cli.on_screen(frame) or frame.state != "inactive":
            continue  # in sight, or settling: the driver keeps the sprite
        if (new := heir(pilot, frame)) is not None:
            sand.update(frame=new.id)
            frame = new


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("sand", "Sand falling in an idle pane.", frame="idle")
    out.add_argument(
        "--rate", type=float, default=120.0, help="grains per second [default: 120]"
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
