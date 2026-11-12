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
- Stillness: the effect notes where the text changes. It rains less
  on columns that changed in the last ten seconds, so the rain keeps
  away from where someone types.
- A frame replaced by a resize: the sprite is moved to the pane that
  took its place (`sand.heir`).
"""

from __future__ import annotations

import argparse
import random
import time
from collections.abc import Sequence

from ptyrc import Frame, Pilot, Sprite

from . import _cli
from .sand import heir

__all__ = ["main", "pour"]

POLL = 0.5  # seconds between two looks at a frame replaced


def pour(pilot: Pilot, frame: Frame, rate: float) -> Sprite:
    """Start the rain on `frame`."""
    params = {"rate": rate, "seed": random.getrandbits(32)}
    return pilot.effect("rain", params, frame=frame.id, layer="back")


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    frame = _cli.pick(pilot, args.frame)
    rain = pour(pilot, frame, args.rate)
    if frame.kind == "raw":  # the screen: always there
        return _cli.hold(pilot, seconds)
    end = None if seconds is None else time.monotonic() + seconds
    while end is None or time.monotonic() < end:
        time.sleep(POLL)
        if _cli.on_screen(frame) or frame.state != "inactive":
            continue  # in sight, or settling: the driver keeps the sprite
        if (new := heir(pilot, frame)) is not None:
            rain.update(frame=new.id)
            frame = new


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
