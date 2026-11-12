# SPDX-License-Identifier: GPL-3.0-or-later
"""Snow: it settles on the lines of text and melts where they change.

    ptyrc-app snow
    ptyrc-app snow --rate 40 --wind 2 -f active

What it shows:

- A native effect from a draw extension (`plugins/draw-ext/snow`): one
  request starts it, the driver plays it; the app only stays connected.
- An effect that sees the cells under it: snow lies over a line of
  text, a cell away from the words of its own row, and on the floor;
  flakes pass behind the text.
- An effect that sees the heat: the snow melts where the screen
  changed lately, and around the cursor. Type, and it goes.
- Frame rows: the snow lies on the rows of the frame, so it scrolls
  with the text; snow that lost its line falls again.
- An effect tuned in place: every few seconds the app sends a new
  wind (`Sprite.update(effect=...)`); the flakes in the air and the
  snow on the lines stay.
"""

from __future__ import annotations

import argparse
import random
import time
from collections.abc import Sequence

from ptyrc import Frame, Pilot, Sprite

from . import _cli
from .sand import heir

__all__ = ["gust", "main", "snow"]

POLL = 0.5  # seconds between two looks at a frame replaced
GUST = (6.0, 15.0)  # seconds between two winds


def snow(pilot: Pilot, frame: Frame, rate: float, wind: float) -> Sprite:
    """Start the snow on `frame`."""
    params = {"rate": rate, "wind": wind, "seed": random.getrandbits(32)}
    return pilot.effect("snow", params, frame=frame.id, layer="back")


def gust(wind: float, rng: random.Random) -> float:
    """Another wind: the same way most of the time, up to twice as strong."""
    way = 1.0 if rng.random() < 0.8 else -0.5
    return round(way * wind * rng.uniform(0.2, 2.0), 2)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random()
    frame = _cli.pick(pilot, args.frame)
    sprite = snow(pilot, frame, args.rate, args.wind)
    end = None if seconds is None else time.monotonic() + seconds
    blow = time.monotonic() + rng.uniform(*GUST)
    while pilot.connected and (end is None or time.monotonic() < end):
        time.sleep(POLL)
        if args.wind and time.monotonic() >= blow:
            blow = time.monotonic() + rng.uniform(*GUST)
            params = {"wind": gust(args.wind, rng)}
            sprite.update(effect={"name": "snow", "params": params})
        if frame.kind == "raw" or _cli.on_screen(frame) or frame.state != "inactive":
            continue  # in sight, or settling: the driver keeps the sprite
        if (new := heir(pilot, frame)) is not None:
            sprite.update(frame=new.id)
            frame = new


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("snow", "Snow on the lines of text; it melts where they change.")
    out.add_argument(
        "--rate",
        type=float,
        default=14.0,
        help="flakes per second over 80 columns [default: 14]",
    )
    out.add_argument(
        "--wind",
        type=float,
        default=0.6,
        help="drift to the right, columns per second; it gusts; 0: none [default: 0.6]",
    )
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
