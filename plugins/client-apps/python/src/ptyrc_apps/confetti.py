# SPDX-License-Identifier: GPL-3.0-or-later
"""Confetti: the smallest app. One call starts a native effect.

The driver animates the burst by itself, at its own frame rate; nothing
travels on the socket while it plays.

    ptyrc-app confetti
    ptyrc-app confetti --count 200 --bursts 3 --sparkle
    ptyrc-app confetti --front
    ptyrc-app confetti --spawn              # party: a burst every 3 s
    ptyrc-app confetti --spawn --rate 60    # a burst every second

By default the pieces fall behind the app's text, in the cells it leaves
free; `--front` lets them fall over it. Behind the text a burst repaints
only free cells: the text does not flicker.
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
    layer: int | str = "back",
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
    # its frame. `z` keeps it above the other sprites of its layer.
    return pilot.effect("confetti", params, frame=frame, layer=layer, z=100)


def party(pilot: Pilot, args: argparse.Namespace) -> None:
    """`--spawn`: bursts at random places, `rate` per minute, until
    interrupted. Each burst is one request; the driver plays it."""
    rng = random.Random(args.seed)
    layer = "front" if args.front else "back"
    end = None if args.seconds is None else time.monotonic() + args.seconds
    while end is None or time.monotonic() < end:
        # Asked again each time: the pane may have been resized.
        w, h = _cli.pick(pilot, args.frame).size
        burst(
            pilot,
            frame=args.frame,
            count=args.count,
            duration=args.duration,
            origin=(rng.uniform(0.15, 0.85) * w, rng.uniform(0.15, 0.6) * h),
            seed=rng.getrandbits(32),
            layer=layer,
        )
        time.sleep(60.0 / args.rate)
    time.sleep(args.duration)  # let the last burst land


def app(pilot: Pilot, args: argparse.Namespace) -> None:
    if args.sparkle:
        # Far in the back: behind the app's text and behind the confetti.
        pilot.effect("sparkle", {"count": 10}, frame=args.frame, layer=-2)
    if args.spawn:
        return party(pilot, args)
    for i in range(args.bursts):
        burst(
            pilot,
            frame=args.frame,
            count=args.count,
            duration=args.duration,
            layer="front" if args.front else "back",
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
    out.add_argument("--front", action="store_true", help="fall over the text")
    out.add_argument("--seed", type=int, help="same seed, same burst")
    out.add_argument("--spawn", action="store_true", help="party: keep spawning")
    out.add_argument(
        "--rate",
        type=float,
        default=20.0,
        help="with --spawn: bursts per minute [default: 20, one every 3 s]",
    )
    out.add_argument("--seconds", type=float, help="with --spawn: stop after this")
    args = _cli.parse(out, argv)
    if args.rate <= 0:
        out.error("--rate must be positive")
    return _cli.run(app, args)


if __name__ == "__main__":
    raise SystemExit(main())
