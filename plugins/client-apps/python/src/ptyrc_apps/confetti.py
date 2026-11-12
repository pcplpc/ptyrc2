# SPDX-License-Identifier: GPL-3.0-or-later
"""Confetti: the smallest app. One call starts a native effect.

The driver animates the burst by itself, at its own frame rate; nothing
travels on the socket while it plays.

    ptyrc-app confetti
    ptyrc-app confetti --count 200 --bursts 3 --sparkle
    ptyrc-app confetti --front
    ptyrc-app confetti --spawn              # party: a burst every 3 s
    ptyrc-app confetti --spawn --rate 60    # a burst every second
    ptyrc-app confetti --spawn --anywhere   # ignore the heatmap

In party mode each burst starts at a random cell, cold areas first: the
app reads the frame's heat classes once per burst (`Frame.heat_classes`)
and leaves alone what the program keeps changing.

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

__all__ = ["burst", "main", "spot"]


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


def spot(
    classes: Sequence[str],
    rng: random.Random,
    last: tuple[int, int] | None = None,
    reach: int = 3,
) -> tuple[int, int]:
    """A random cell `(x, y)` of a view given by its heat classes, one
    string per row. A cell not hot (`h`) weighs the share of cells not
    hot within `reach` of it; a hot cell weighs nothing, unless all are
    hot. Never within `reach` of `last`, unless nothing else is left."""
    h, w = len(classes), max(map(len, classes), default=0)
    if not h or not w:
        return 0, 0
    cold = [[c != "h" for c in row.ljust(w, ".")] for row in classes]
    cells, weights = [], []
    for y in range(h):
        for x in range(w):
            if last and abs(x - last[0]) <= reach and abs(y - last[1]) <= reach:
                continue
            near = [
                cold[j][i]
                for j in range(max(0, y - reach), min(h, y + reach + 1))
                for i in range(max(0, x - reach), min(w, x + reach + 1))
            ]
            cells.append((x, y))
            weights.append(sum(near) / len(near) if cold[y][x] else 0.0)
    if not cells:  # a view smaller than the gap: forget `last`
        return spot(classes, rng, None, reach)
    if not any(weights):  # all hot: anywhere
        return rng.choice(cells)
    return rng.choices(cells, weights)[0]


def party(pilot: Pilot, args: argparse.Namespace) -> None:
    """`--spawn`: bursts at random places, `rate` per minute, until
    interrupted. Each burst costs two requests, the heatmap and the burst;
    the driver plays it."""
    rng = random.Random(args.seed)
    layer = "front" if args.front else "back"
    end = None if args.seconds is None else time.monotonic() + args.seconds
    last = None
    while end is None or time.monotonic() < end:
        # Asked again each time: the pane may have been resized or gone.
        frame = _cli.pick(pilot, args.frame)
        if args.anywhere:
            w, h = frame.size
            classes = ["." * w] * h
        else:
            classes = frame.heat_classes()
        last = spot(classes, rng, last)
        burst(
            pilot,
            frame=frame.id,
            count=args.count,
            duration=args.duration,
            origin=last,
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
    out.add_argument(
        "--anywhere",
        action="store_true",
        help="with --spawn: uniform over the view, ignore the heatmap",
    )
    args = _cli.parse(out, argv)
    if args.rate <= 0:
        out.error("--rate must be positive")
    return _cli.run(app, args)


if __name__ == "__main__":
    raise SystemExit(main())
