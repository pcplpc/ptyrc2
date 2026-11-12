# SPDX-License-Identifier: GPL-3.0-or-later
"""Rain: quiet rain behind the text.

    ptyrc-app rain
    ptyrc-app rain --rate 60 -f active

What it shows:

- A native effect from a draw extension (`plugins/draw-ext/rain`): one
  request starts it, the driver plays it, 25 frames a second, with
  nothing on the socket. The app only stays connected.
- An effect that sees the screen: drops splash on the top of the lines
  of text, on the cursor and on the bottom of the pane; they fall
  behind the text between two lines.
- The back layer: the rain shows where the app shows nothing.
- An effect that sees the heat: it rains less on columns that changed
  in the last ten seconds, so the rain keeps away from where someone
  types.
- A frame replaced by a resize: the sprite is moved to the pane that
  took its place (`_cli.follow`), on an event of the topic `frames`.
"""

from __future__ import annotations

import argparse
import random
from collections.abc import Sequence

from ptyrc import Frame, Pilot, Sprite

from . import _cli

__all__ = ["main", "pour"]


def pour(pilot: Pilot, frame: Frame, rate: float) -> Sprite:
    """Start the rain on `frame`."""
    params = {"rate": rate, "seed": random.getrandbits(32)}
    return pilot.effect("rain", params, frame=frame.id, layer="back")


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    frame = _cli.pick(pilot, args.frame)
    _cli.follow(pilot, frame, pour(pilot, frame, args.rate), seconds)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("rain", "Quiet rain behind the text.")
    out.add_argument(
        "--rate",
        type=float,
        default=20.0,
        help="drops per second over 80 columns [default: 20]",
    )
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
