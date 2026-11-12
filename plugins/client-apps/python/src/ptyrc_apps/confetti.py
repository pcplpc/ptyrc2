# SPDX-License-Identifier: GPL-3.0-or-later
"""Confetti: the smallest app. One call starts a native effect.

The driver animates a burst by itself, at its own frame rate; nothing
travels on the socket while it plays.

    ptyrc-app confetti
    ptyrc-app confetti --size 200 --bursts 3 --sparkle
    ptyrc-app confetti --front
    ptyrc-app confetti --party              # a burst every 3 s
    ptyrc-app confetti --party --bursts 4   # four every 3 s

A round is `--bursts` bursts, each started at its own random offset in
the first `PERIOD` seconds. Without `--party` the app plays one round
and exits once the last burst is sent: the bursts persist and the driver
ends them. With `--party` a round starts every `PERIOD` seconds until
interrupted.

Each burst reads the frame's heat classes once (`Frame.heat_classes`)
and starts at a random cell, cold areas first, away from the previous
bursts: it leaves alone what the program keeps changing.

By default the pieces fall behind the app's text, in the cells it leaves
free; `--front` lets them fall over it. Behind the text a burst repaints
only free cells: the text does not flicker.
"""

from __future__ import annotations

import argparse
import itertools
import random
import time
from collections.abc import Sequence

from ptyrc import Pilot, Sprite

from . import _cli

__all__ = ["MAX_BURSTS", "PERIOD", "burst", "main", "play", "sparkle", "spot"]

PERIOD = 3.0
"""Seconds of a round: its bursts start within it, the next round after."""

MAX_BURSTS = 10


def burst(
    pilot: Pilot,
    *,
    frame: int | str = "raw",
    size: int = 64,
    duration: float = 3.0,
    origin: tuple[float, float] | None = None,
    seed: int | None = None,
    layer: int | str = "back",
    persist: bool = False,
) -> Sprite:
    """Throw one burst of `size` pieces on `frame`. `origin` is a cell of
    the frame's view; by default the burst starts a third down the
    middle. A `persist` burst outlives the connection; the driver removes
    it when it ends."""
    params: dict[str, object] = {
        "count": size,
        "duration_ms": int(duration * 1000),
        "seed": random.getrandbits(32) if seed is None else seed,
    }
    if origin is not None:
        params["origin"] = list(origin)
    # An effect has no content of its own: it draws on the whole view of
    # its frame. `z` keeps it above the other sprites of its layer.
    return pilot.effect(
        "confetti", params, frame=frame, layer=layer, z=100, persist=persist
    )


def spot(
    classes: Sequence[str],
    rng: random.Random,
    avoid: Sequence[tuple[int, int]] = (),
    reach: int = 3,
) -> tuple[int, int]:
    """A random cell `(x, y)` of a view given by its heat classes, one
    string per row. A cell not hot (`h`) weighs the share of cells not
    hot within `reach` of it; a hot cell weighs nothing, unless all are
    hot. Never within `reach` of a cell of `avoid`; when nothing else is
    left, the first of `avoid` are forgotten."""
    h, w = len(classes), max(map(len, classes), default=0)
    if not h or not w:
        return 0, 0
    cold = [[c != "h" for c in row.ljust(w, ".")] for row in classes]
    # Cold cells of each window, from sums along the rows, then down the
    # columns: `below[y][x]` counts those of rows `< y` in the columns
    # within `reach` of `x`.
    lo = [max(0, x - reach) for x in range(w)]
    hi = [min(w, x + reach + 1) for x in range(w)]
    below = [[0] * w]
    for row in cold:
        run = [0, *itertools.accumulate(row)]
        along = [run[b] - run[a] for a, b in zip(lo, hi)]
        below.append([n + m for n, m in zip(below[-1], along)])
    span = [b - a for a, b in zip(lo, hi)]
    cells: list[tuple[int, int]] = []
    weights: list[float] = []
    for y in range(h):
        y0, y1 = max(0, y - reach), min(h, y + reach + 1)
        near = [n - m for n, m in zip(below[y1], below[y0])]
        gaps = [i for i, j in avoid if abs(y - j) <= reach]
        free: Sequence[int] = range(w)
        if gaps:
            free = [x for x in free if all(abs(x - i) > reach for i in gaps)]
        row, rows = cold[y], y1 - y0
        cells += [(x, y) for x in free]
        weights += [near[x] / (rows * span[x]) if row[x] else 0.0 for x in free]
    if not cells:  # a view too small for the gaps
        return spot(classes, rng, avoid[1:], reach)
    if not any(weights):  # all hot: anywhere
        return rng.choice(cells)
    return rng.choices(cells, weights)[0]


def play(pilot: Pilot, args: argparse.Namespace, rng: random.Random) -> None:
    """One round of bursts, or a round every `PERIOD` seconds with
    `args.party`. One thread: the start times of a round are drawn and
    sorted, then slept to in turn."""
    layer = "front" if args.front else "back"
    rounds = itertools.count() if args.party else range(1)
    begin = time.monotonic()
    avoid: list[tuple[int, int]] = []
    for n in rounds:
        start = begin + n * PERIOD
        starts = sorted(start + rng.random() * PERIOD for _ in range(args.bursts))
        if args.sparkle and n == 0:
            # Far in the back: behind the app's text and the confetti. One
            # round: as long as its bursts, past our exit.
            last = None if args.party else starts[-1] - begin + args.duration
            sparkle(pilot, args.frame, last)
        for at in starts:
            time.sleep(max(0.0, at - time.monotonic()))
            # Asked again each time: the pane may have been resized or gone.
            frame = _cli.pick(pilot, args.frame)
            origin = spot(frame.heat_classes(), rng, avoid)
            avoid = [*avoid, origin][-args.bursts :]
            burst(
                pilot,
                frame=frame.id,
                size=args.size,
                duration=args.duration,
                origin=origin,
                seed=rng.getrandbits(32),
                layer=layer,
                persist=not args.party,
            )


def sparkle(pilot: Pilot, frame: int | str, seconds: float | None) -> Sprite:
    """Twinkles on layer -2; for `seconds`, persistent, or until we leave."""
    params: dict[str, object] = {"count": 10}
    if seconds is not None:
        params["duration_ms"] = int(seconds * 1000)
    return pilot.effect(
        "sparkle", params, frame=frame, layer=-2, persist=seconds is not None
    )


def main(argv: Sequence[str] | None = None, rng: random.Random | None = None) -> int:
    out = _cli.parser("confetti", "A burst of confetti.")
    out.add_argument("--size", type=int, default=64, help="pieces per burst")
    out.add_argument("--duration", type=float, default=3.0, help="seconds per burst")
    out.add_argument(
        "--bursts", type=int, default=1, help=f"bursts at once, up to {MAX_BURSTS}"
    )
    out.add_argument("--party", action="store_true", help="party mode")
    out.add_argument("--sparkle", action="store_true", help="twinkles in the back")
    out.add_argument("--front", action="store_true", help="fall over the text")
    args = _cli.parse(out, argv)
    if not 1 <= args.bursts <= MAX_BURSTS:
        out.error(f"--bursts must be between 1 and {MAX_BURSTS}")
    rng = rng or random.Random()
    return _cli.run(lambda pilot, args: play(pilot, args, rng), args)


if __name__ == "__main__":
    raise SystemExit(main())
