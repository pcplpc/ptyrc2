# SPDX-License-Identifier: GPL-3.0-or-later
"""Snow: it falls; with `--cover` it settles on the lines of text and
melts where they change.

    ptyrc-app snow
    ptyrc-app snow --cover --rate 40 --wind 2 -f active

What it shows:

- A native effect from a draw extension (`plugins/draw-ext/snow`): one
  request starts it, the driver plays it; the app only stays connected.
- An effect that sees the cells under it: flakes pass behind the
  text; with `--cover`, snow lies over a line of text, a cell away
  from the words of its own row, and on the floor, in smooth piles up
  to three rows deep.
- An effect that sees the heat: the snow melts where the screen
  changed lately, and around the cursor. Type, and it goes.
- Frame rows: the snow lies on the rows of the frame, so it scrolls
  with the text; snow that lost its line falls again.
- An effect tuned in place: every few seconds the app sends a new
  wind (`Sprite.tune(wind=...)`); the flakes in the air and the snow
  on the lines stay.
- `_cli.follow`: the app sleeps on the socket between two gusts, and
  follows its frame when a resize replaces it.
"""

from __future__ import annotations

import argparse
import random
from collections.abc import Iterator, Sequence

from ptyrc import Frame, Pilot, Sprite

from . import _cli

__all__ = ["gust", "gusts", "main", "snow"]

GUST = (6.0, 15.0)  # seconds between two winds


def snow(
    pilot: Pilot, frame: Frame, rate: float, wind: float, cover: bool = False
) -> Sprite:
    """Start the snow on `frame`; `cover`: it lies."""
    params = {
        "rate": rate,
        "wind": wind,
        "cover": cover,
        "seed": random.getrandbits(32),
    }
    return pilot.effect("snow", params, frame=frame.id, layer="back")


def gust(wind: float, rng: random.Random) -> float:
    """Another wind: the same way most of the time, up to twice as strong."""
    way = 1.0 if rng.random() < 0.8 else -0.5
    return round(way * wind * rng.uniform(0.2, 2.0), 2)


def gusts(sprite: Sprite, wind: float, rng: random.Random) -> Iterator[float]:
    """The beats of `_cli.follow`: a wait, then another wind."""
    while True:
        yield rng.uniform(*GUST)
        sprite.tune(wind=gust(wind, rng))


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    frame = _cli.pick(pilot, args.frame)
    sprite = snow(pilot, frame, args.rate, args.wind, args.cover)
    beats = gusts(sprite, args.wind, random.Random()) if args.wind else None
    _cli.follow(pilot, frame, sprite, seconds, beats)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "snow",
        "Snow; with --cover it lies on the lines of text and melts where they change.",
    )
    out.add_argument(
        "--rate",
        type=float,
        default=14.0,
        help="flakes per second over 80 columns [default: 14]",
    )
    out.add_argument(
        "--cover",
        action="store_true",
        help="the snow lies on the text and the floor; else it only falls",
    )
    out.add_argument(
        "--wind",
        type=float,
        default=-1.0,
        help="drift, columns per second, < 0 to the left; it gusts; 0: none [default: -1]",
    )
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
