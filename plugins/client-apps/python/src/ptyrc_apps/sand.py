# SPDX-License-Identifier: GPL-3.0-or-later
"""Sand: fills an idle pane, piling on its text.

    ptyrc-app sand                 # the most idle pane, once it is idle
    ptyrc-app sand -f 3 --rate 80 --now

What it shows:

- `Frame.is_idle`: the driver knows which panes have been still for a
  while. A big animation belongs there, not where someone is typing.
- A native effect from a draw extension (`plugins/draw-ext/sand`): the
  simulation runs in the driver, 20 steps a second, with nothing on the
  socket. The client sends the pane's text once, as walls.
- Giving way: when the pane's text changes the sand is swept away, and
  falls again once the pane is idle.
"""

from __future__ import annotations

import argparse
import random
import time
from collections.abc import Sequence

from ptyrc import Frame, Pilot, PtyrcError, Sprite

from . import _cli

__all__ = ["main", "pour"]

POLL = 0.5  # seconds between two looks at the pane


def pour(pilot: Pilot, frame: Frame, walls: Sequence[str], rate: float) -> Sprite:
    """Start the sand on `frame`. `walls`: the rows of its view; the
    grains rest on every cell that is not a space."""
    params = {"walls": list(walls), "rate": rate, "seed": random.getrandbits(32)}
    # Behind the text: a grain never hides a letter.
    return pilot.effect("sand", params, frame=frame.id, layer="back")


def app(pilot: Pilot, args: argparse.Namespace) -> None:
    end = None if args.seconds is None else time.monotonic() + args.seconds
    sand: Sprite | None = None
    walls: list[str] = []
    shape: tuple[int, ...] = ()
    frame = _cli.pick(pilot, args.frame)
    patient = not args.now
    while end is None or time.monotonic() < end:
        try:
            rect = frame.refresh().rect
            text = frame.text()
        except PtyrcError as exc:
            if exc.code != "no-frame":
                raise
            # The pane is gone (a layout change): choose again.
            frame, sand = _cli.pick(pilot, args.frame), None
            continue
        if sand is not None and (text != walls or rect != shape):
            # Someone uses the pane: give it back at once.
            sand.remove()
            sand, patient = None, True
        if sand is None and frame.usable and (not patient or frame.is_idle):
            walls, shape = text, rect
            sand = pour(pilot, frame, walls, args.rate)
        time.sleep(POLL)


def main(argv: Sequence[str] | None = None) -> int:
    out = _cli.parser("sand", "Sand falling in an idle pane.", frame="idle")
    out.add_argument(
        "--rate", type=float, default=40.0, help="grains per second [default: 40]"
    )
    out.add_argument(
        "--now", action="store_true", help="do not wait for the pane to be idle"
    )
    out.add_argument("--seconds", type=float, help="stop after this long")
    return _cli.run(app, _cli.parse(out, argv))


if __name__ == "__main__":
    raise SystemExit(main())
