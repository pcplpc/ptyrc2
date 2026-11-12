# SPDX-License-Identifier: GPL-3.0-or-later
"""Confetti: the smallest app. One call starts a native effect.

The driver animates the burst by itself, at its own frame rate; nothing
travels on the socket while it plays.

    ptyrc-app confetti
    ptyrc-app confetti --count 200 --bursts 3 --sparkle
"""

from __future__ import annotations

import argparse
import random
import time
from collections.abc import Sequence

from ptyrc import Pilot, Sprite

from . import _cli

__all__ = ["burst", "main"]


def burst(
    pilot: Pilot,
    *,
    frame: int | str = "raw",
    count: int = 64,
    duration: float = 3.0,
    origin: tuple[float, float] | None = None,
    seed: int | None = None,
) -> Sprite:
    """Throw one burst on `frame`. `origin` is a cell of the frame's view;
    by default the burst starts a third down the middle."""
    params: dict[str, object] = {
        "count": count,
        "duration_ms": int(duration * 1000),
        "seed": random.getrandbits(32) if seed is None else seed,
    }
    if origin is not None:
        params["origin"] = list(origin)
    # An effect has no content of its own: it draws on the whole view of
    # its frame. `z` keeps it above other sprites.
    return pilot.effect("confetti", params, frame=frame, z=100)


def app(pilot: Pilot, args: argparse.Namespace) -> None:
    if args.sparkle:
        # `behind`: twinkles show only where the screen is free.
        pilot.effect("sparkle", {"count": 10}, frame=args.frame, blend="behind")
    for i in range(args.bursts):
        burst(
            pilot,
            frame=args.frame,
            count=args.count,
            duration=args.duration,
            seed=None if args.seed is None else args.seed + i,
        )
        # Bursts overlap: the next one starts while this one falls.
        time.sleep(args.duration / 3 if i + 1 < args.bursts else args.duration)
    # Returning closes the connection, and the driver removes our sprites.


def main(argv: Sequence[str] | None = None) -> int:
    out = _cli.parser("confetti", "A burst of confetti.")
    out.add_argument("--count", type=int, default=64, help="pieces per burst")
    out.add_argument("--duration", type=float, default=3.0, help="seconds per burst")
    out.add_argument("--bursts", type=int, default=1, help="number of bursts")
    out.add_argument("--sparkle", action="store_true", help="twinkles in the back")
    out.add_argument("--seed", type=int, help="same seed, same burst")
    return _cli.run(app, _cli.parse(out, argv))


if __name__ == "__main__":
    raise SystemExit(main())
