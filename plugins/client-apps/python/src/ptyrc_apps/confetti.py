# SPDX-License-Identifier: GPL-3.0-or-later
"""Confetti: small pops of colour, as in the first ptyrc.

A pop is one sprite: a finite animation of a few cells, played and
removed by the driver. A burst is four of them, one per colour, each at
its own cell within a second.

    ptyrc-app confetti                      # a burst every 3 s
    ptyrc-app confetti --one-shot           # one, and exit
    ptyrc-app confetti --bursts 3 --sparkle
    ptyrc-app confetti --fancy --size 200   # big explosions
    ptyrc-app confetti --random             # pops of several kinds
    ptyrc-app confetti --fireworks          # a rocket before each
    ptyrc-app confetti --all                # everything

`--fancy`: a burst is one explosion, the native effect `confetti`:
`--size` pieces thrown from one cell, falling for `--duration` seconds,
animated by the driver at its own frame rate. `--random` draws the
kind of each pop; with `--fancy`, the size, the scale, the shape, the
length and the colours of each explosion. `--fireworks`: a rocket rises
from the bottom of the view, straight, slowing down, and the pop or the
explosion starts where it stops; rockets are drawn in several ways.
`--all`: those three and `--sparkle`.

A round is `--bursts` bursts, each started at its own random offset in
the first `PERIOD` seconds. A round starts every `PERIOD` seconds until
the app is interrupted; its sprites go with its connection. With
`--one-shot` the app plays one round and exits once the last sprite is
sent: they persist and the driver ends them.

Each shot reads the frame's heat classes once (`Frame.heat_classes`)
and aims at a random cell, cold areas first, away from the previous
ones: it leaves alone what the program keeps changing.

By default everything is behind the app's text, in the cells it leaves
free; `--front` draws over it.
"""

from __future__ import annotations

import argparse
import heapq
import itertools
import random
import time
from collections.abc import Callable, Sequence
from typing import Any

from ptyrc import Pilot, Sprite

from . import _cli

__all__ = [
    "COLORS",
    "MAX_BURSTS",
    "PERIOD",
    "POP",
    "POPS",
    "ROCKETS",
    "SHAPES",
    "burst",
    "flight",
    "main",
    "play",
    "pop",
    "rocket",
    "sparkle",
    "spot",
]

PERIOD = 3.0
"""Seconds of a round: its bursts start within it, the next round after."""

MAX_BURSTS = 10

COLORS = (10, 13, 12, 11)
"""The pops of a burst: bright green, magenta, blue, yellow."""

POP = (
    ("   ", " · ", "   "),
    ("   ", " * ", "   "),
    ("· ·", " o ", "· ·"),
    (" · ", "· ·", " · "),
)
"""The pop of the first ptyrc, frame by frame."""

POPS = (
    POP,
    (("·",), ("*",), ("✦",), ("*",), ("·",)),
    (
        ("     ", "  ·  ", "     "),
        ("     ", "  +  ", "     "),
        (" \\|/ ", "- * -", " /|\\ "),
        ("· ' ·", "·   ·", "· . ·"),
    ),
    (
        ("     ", "  ·  ", "     "),
        ("  ·  ", " ·o· ", "  ·  "),
        (" · · ", "·   ·", " · · "),
        ("·   ·", "     ", "·   ·"),
    ),
    (
        ("       ", "       ", "   ·   ", "       ", "       "),
        ("       ", "       ", "   *   ", "       ", "       "),
        ("       ", "  · ·  ", "   O   ", "  · ·  ", "       "),
        ("  · ·  ", "·     ·", "   ·   ", "·     ·", "  · ·  "),
        (" ·   · ", "       ", "·     ·", "       ", " ·   · "),
    ),
)
"""The pops of `--random`: each a tuple of frames of equal size."""

SHAPES = ("fan", "ball", "ring", "star", "fountain")
"""How the native `confetti` throws its pieces."""

ROCKETS = (
    ("|", "|:."),
    ("^", "!:'"),
    ("*+", "'.,"),
    ("o", "°·"),
    ("╿", "│╎┆"),
)
"""Rockets, `(head, trail)`: the head goes through its glyphs, one a
step; the trail hangs under it, a glyph a row, first the nearest."""

STEP = 0.066
"""Seconds between two drawings of a rocket."""


def pop(
    pilot: Pilot,
    *,
    frame: int | str = "raw",
    origin: tuple[int, int] = (1, 1),
    color: int | str = 11,
    art: Sequence[Sequence[str]] = POP,
    step: float = 0.1,
    layer: int | str = "back",
    persist: bool = False,
) -> Sprite:
    """Play `art` once, centred on `origin`, a cell of the view of
    `frame`: a frame every `step` seconds, then the driver removes it."""
    w, h = len(art[0][0]), len(art[0])
    last = len(art) - 1
    frames = [
        {"cells": [[{"t": row, "fg": color, "dim": i == last}] for row in rows]}
        for i, rows in enumerate(art)
    ]
    return pilot.draw(
        frames=frames,
        frame_ms=max(1, int(step * 1000)),
        loop=False,
        frame=frame,
        at=(origin[0] - w // 2, origin[1] - h // 2),
        layer=layer,
        z=100,
        persist=persist,
    )


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
    **params: Any,
) -> Sprite:
    """Throw one explosion of `size` pieces on `frame`. `origin` is a
    cell of the frame's view; by default a third down the middle.
    `params`: more of the effect's own (`shape`, `spread`, `colors`). A
    `persist` explosion outlives the connection; the driver removes it
    when it ends."""
    params |= {
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


def flight(rise: int) -> float:
    """Seconds a rocket takes to rise `rise` rows."""
    return min(1.6, 0.45 + 0.045 * rise)


def rocket(
    pilot: Pilot,
    *,
    frame: int | str = "raw",
    origin: tuple[int, int],
    rise: int,
    seconds: float | None = None,
    color: int | str = 11,
    look: tuple[str, str] = ROCKETS[0],
    layer: int | str = "back",
    persist: bool = False,
) -> Sprite:
    """A rocket that stops at `origin` after `seconds`, from `rise` rows
    below. One sprite: the driver moves it (`motion`, a constant
    braking from twice the mean speed to none) and plays its frames: a
    trail as long as it is fast."""
    seconds = flight(rise) if seconds is None else seconds
    head, trail = look
    steps = max(1, round(seconds / STEP))
    frames = []
    for i in range(steps):
        speed = 1 - i / steps  # share of the launch speed
        rows = [[{"t": head[i % len(head)], "fg": 15, "bold": True}]]
        for k in range(round(min(len(trail), rise) * speed)):
            rows.append([{"t": trail[k], "fg": color, "dim": k > 0}])
        frames.append({"cells": rows, "ms": int(seconds * 1000 / steps)})
    return pilot.draw(
        frames=frames,
        loop=False,
        frame=frame,
        at=(origin[0], origin[1] + rise),
        motion={
            "vx": 0.0,
            "vy": -2 * rise / seconds,
            "ax": 0.0,
            "ay": 2 * rise / seconds**2,
            "bounce": False,
        },
        ttl=seconds,
        layer=layer,
        z=99,
        persist=persist,
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
    """A round of bursts every `PERIOD` seconds, or one with
    `args.one_shot`. One thread, one queue of things to do, each at its
    time: a round queues its shots and the next round, a shot with a
    rocket queues what follows the flight."""
    layer = "front" if args.front else "back"
    persist = args.one_shot
    begin = time.monotonic()
    queue: list[tuple[float, int, Callable[[], None]]] = []
    order = itertools.count()
    avoid: list[tuple[int, int]] = []
    # A plain burst is four pops in a second, as in the first ptyrc.
    colors = (None,) if args.fancy else COLORS
    kept = min(MAX_BURSTS, args.bursts * len(colors))

    def later(at: float, action: Callable[[], None]) -> None:
        heapq.heappush(queue, (at, next(order), action))

    def bang(frame: int, origin: tuple[int, int], color: int) -> None:
        if not args.fancy:
            art, step = POP, 0.1
            if args.random:
                art, step = rng.choice(POPS), rng.uniform(0.07, 0.14)
            pop(
                pilot,
                frame=frame,
                origin=origin,
                color=color,
                art=art,
                step=step,
                layer=layer,
                persist=persist,
            )
            return
        size, seconds = args.size, args.duration
        params: dict[str, Any] = {}
        if args.fireworks:
            params["shape"] = "ball"  # a fan has a floor; a rocket has none
        if args.random:
            size = max(1, round(size * rng.uniform(0.4, 1.8)))
            seconds *= rng.uniform(0.7, 1.2)
            params["shape"] = rng.choice(SHAPES)
            params["spread"] = rng.uniform(0.5, 1.3)
            if rng.random() < 0.5:  # of one colour, or two
                params["colors"] = rng.sample(range(9, 15), rng.randint(1, 2))
        burst(
            pilot,
            frame=frame,
            size=size,
            duration=seconds,
            origin=origin,
            seed=rng.getrandbits(32),
            layer=layer,
            persist=persist,
            **params,
        )

    def shot(color: int | None) -> None:
        nonlocal avoid
        # Asked again each time: the pane may have been resized or gone.
        frame = _cli.pick(pilot, args.frame)
        classes = frame.heat_classes()
        floor = len(classes) - 1
        if args.fireworks:  # room to rise
            classes = classes[: max(1, len(classes) * 2 // 3)]
        origin = spot(classes, rng, avoid)
        avoid = [*avoid, origin][-kept:]
        if color is None or args.random:
            color = rng.choice(range(9, 15))
        if not args.fireworks:
            return bang(frame.id, origin, color)
        rise = floor - origin[1]
        seconds = flight(rise) * rng.uniform(0.85, 1.15)
        rocket(
            pilot,
            frame=frame.id,
            origin=origin,
            rise=rise,
            seconds=seconds,
            color=color,
            look=rng.choice(ROCKETS),
            layer=layer,
            persist=persist,
        )
        later(time.monotonic() + seconds, lambda: bang(frame.id, origin, color))

    def round_(n: int) -> None:
        start = begin + n * PERIOD
        for at in (start + rng.random() * PERIOD for _ in range(args.bursts)):
            for color in colors:
                wait = 0.0 if color is None else 0.1 * rng.randint(0, 10)
                later(at + wait, lambda color=color: shot(color))
        if not args.one_shot:
            later(start + PERIOD, lambda: round_(n + 1))

    round_(0)
    if args.sparkle:
        # Far in the back: behind the app's text and the confetti. One
        # round: as long as it may last, past our exit.
        last = PERIOD + (flight(999) if args.fireworks else 0.0)
        last += args.duration if args.fancy else 1.5
        sparkle(pilot, args.frame, last if args.one_shot else None)
    while queue:
        at, _, action = heapq.heappop(queue)
        time.sleep(max(0.0, at - time.monotonic()))
        action()


def sparkle(pilot: Pilot, frame: int | str, seconds: float | None) -> Sprite:
    """Twinkles on layer -2; for `seconds`, persistent, or until we leave."""
    params: dict[str, object] = {"count": 10}
    if seconds is not None:
        params["duration_ms"] = int(seconds * 1000)
    return pilot.effect(
        "sparkle", params, frame=frame, layer=-2, persist=seconds is not None
    )


def main(argv: Sequence[str] | None = None, rng: random.Random | None = None) -> int:
    out = _cli.parser("confetti", "Pops of confetti.")
    out.add_argument(
        "--bursts", type=int, default=1, help=f"bursts at once, up to {MAX_BURSTS}"
    )
    out.add_argument("--one-shot", action="store_true", help="one round, then exit")
    out.add_argument("--front", action="store_true", help="draw over the text")
    out.add_argument("--sparkle", action="store_true", help="twinkles in the back")
    out.add_argument("--fancy", action="store_true", help="big animated explosions")
    out.add_argument("--random", action="store_true", help="of every size and shape")
    out.add_argument("--fireworks", action="store_true", help="a rocket before each")
    out.add_argument(
        "-a", "--all", action="store_true", help="fancy, random, fireworks, sparkle"
    )
    out.add_argument("--size", type=int, default=64, help="pieces per explosion")
    out.add_argument(
        "--duration", type=float, default=3.0, help="seconds per explosion"
    )
    args = _cli.parse(out, argv)
    if not 1 <= args.bursts <= MAX_BURSTS:
        out.error(f"--bursts must be between 1 and {MAX_BURSTS}")
    if args.all:
        args.fancy = args.random = args.fireworks = args.sparkle = True
    rng = rng or random.Random()
    return _cli.run(lambda pilot, args: play(pilot, args, rng), args)


if __name__ == "__main__":
    raise SystemExit(main())
