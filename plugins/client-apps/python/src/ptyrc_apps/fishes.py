# SPDX-License-Identifier: GPL-3.0-or-later
"""Fishes: an aquarium behind the text of a scrolling pane.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app fishes
    ptyrc-app fishes --count 12 --shy 2 -f active

What it shows:

- `space="frame"`: a fish holds a position of the frame, not of the
  window. Scroll the file and the fishes scroll with the text.
- `pilot.cursor()`: fishes flee the cursor, farther when it moves.
- `pilot.batch()`: all the moves of a tick go out in one request and
  show in one repaint.
- The back layer: fishes swim behind the text, between the words.

Half of the school, rounded down, drawn at birth, is shy (`--shy N`:
N). A shy fish swims to the nearest spot of the view where the text
hides all of it but a cell at most (`hideouts`), out of reach of the
cursor, slows down and stays there 4 to 10 s; then it goes to another
spot, drawn at random among those at least two lengths away (`lure`).
Fear comes first. The text is read once a second, or when the view
moved.

A fish left out of the view by a scroll stays where it was. After a
random `WAIT` it goes, one fish a tick at most, and comes back in the
view where the text hides most of it (`entry`): it swims out from
behind the words. Back in view before its time, it carries on.
"""

from __future__ import annotations

import argparse
import math
import random
import time
from collections.abc import Sequence
from dataclasses import dataclass

from ptyrc import Frame, Pilot, Sprite

from . import _cli

__all__ = [
    "STAY",
    "WAIT",
    "Fish",
    "cover",
    "entry",
    "hideouts",
    "lure",
    "main",
    "reach",
    "swim",
    "tide",
    "view",
]

LEFT, RIGHT = "<*//><", "><\\\\*>"
LENGTH = len(LEFT)
COLORS = ("bright-cyan", "bright-yellow", "bright-magenta", "bright-green", "cyan")
TICK = 0.1  # seconds
CALM, FEAR = 6.0, 30.0  # cells per second
NEAR, NEAR_MOVING = 8.0, 16.0  # fear radius, in columns; rows count double
WAIT = (2.0, 5.0)  # seconds out of view before a fish goes
STAY = (4.0, 10.0)  # seconds a shy fish stays in a hiding place
READ = 1.0  # seconds between two reads of the text

Box = tuple[int, int, int, int]


@dataclass
class Fish:
    """Position in frame coordinates, speed in cells per second."""

    x: float
    y: float
    vx: float
    color: str
    vy: float = 0.0
    scared: float = 0.0  # seconds of fear left
    shy: bool = False
    home: tuple[int, int] | None = None  # where a shy fish hides
    stay: float = 0.0  # seconds left there
    sprite: Sprite | None = None
    shown: tuple[int, int, str] | None = None

    @property
    def cell(self) -> tuple[int, int, str]:
        right = math.copysign(1.0, self.vx) > 0  # -0.0 still faces left
        return (round(self.x), round(self.y), RIGHT if right else LEFT)


def reach(x: float, y: float, cursor: tuple[int, int]) -> float:
    """Distance from a fish at (x, y) to the cursor, in columns: a row is
    about two columns tall."""
    return math.hypot(x + LENGTH / 2 - cursor[0], (y - cursor[1]) * 2)


def approach(d: float, top: float) -> float:
    """Speed towards a point `d` cells away: slower as it nears."""
    return max(-top, min(top, 2 * d)) if abs(d) > 0.3 else 0.0


def swim(
    fish: Fish,
    dt: float,
    box: Box,
    cursor: tuple[int, int] | None,
    moving: bool,
    rng: random.Random,
) -> None:
    """One step of one fish inside `box` (x0, y0, x1, y1, frame cells)."""
    x0, y0, x1, y1 = box
    if cursor is not None:
        dx = fish.x + LENGTH / 2 - cursor[0]
        dy = (fish.y - cursor[1]) * 2
        if reach(fish.x, fish.y, cursor) < (NEAR_MOVING if moving else NEAR):
            # Away from the cursor, fast, for a moment.
            fish.scared = 0.8
            fish.vx = math.copysign(FEAR, dx if dx else fish.vx)
            fish.vy = math.copysign(4.0, dy) if abs(dy) < 4 and dy else 0.0
    if fish.scared > 0:
        fish.scared -= dt
        if fish.scared <= 0:
            fish.vx = math.copysign(rng.uniform(0.5, 1.0) * CALM, fish.vx)
            fish.vy = 0.0
    elif fish.home is not None:
        # Towards its hiding place; still once there, facing as it came.
        fish.vx = approach(fish.home[0] - fish.x, CALM) or math.copysign(0.0, fish.vx)
        fish.vy = approach(fish.home[1] - fish.y, 2.0)
    elif rng.random() < 0.02:
        fish.vy = rng.choice((-1.5, 0.0, 0.0, 1.5))  # change depth, or not
    fish.x += fish.vx * dt
    fish.y += fish.vy * dt
    # Turn at the walls.
    if fish.x < x0:
        fish.x, fish.vx = x0, abs(fish.vx)
    elif fish.x > x1 - LENGTH:
        fish.x, fish.vx = x1 - LENGTH, -abs(fish.vx)
    if not y0 <= fish.y <= y1:
        fish.y, fish.vy = min(max(fish.y, y0), y1), 0.0


def view(frame: Frame) -> Box:
    """The view in frame cells: a fish's `x` in [x0, x1 - LENGTH], its
    `y` in [y0, y1]."""
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy, ox + w, oy + h - 1)


def inside(cell: tuple[int, int], box: Box) -> bool:
    x0, y0, x1, y1 = box
    return x0 <= cell[0] <= x1 - LENGTH and y0 <= cell[1] <= y1


def tide(
    cells: Sequence[tuple[int, int]],
    due: Sequence[float | None],
    box: Box,
    now: float,
    rng: random.Random,
) -> tuple[list[float | None], int | None]:
    """Who leaves, and when. `cells`: where the fishes are; `due`: when
    each goes, None while in view. A fish out of view is given a time
    `WAIT` from `now`; one in view, none. Returns the new times and the
    fish that goes now, if any: the most overdue, one a call."""
    due = [
        None if inside(c, box) else now + rng.uniform(*WAIT) if d is None else d
        for c, d in zip(cells, due, strict=True)
    ]
    late = [i for i, d in enumerate(due) if d is not None and d <= now]
    if not late:
        return due, None
    going = min(late, key=lambda i: due[i])
    due[going] = None
    return due, going


def cover(rows: Sequence[str], box: Box) -> dict[tuple[int, int], int]:
    """For each spot of the view, how many cells of a fish there `rows`,
    the view's text, hides."""
    x0, y0, x1, y1 = box
    return {
        (x, y): sum(not c.isspace() for c in row[x - x0 : x - x0 + LENGTH])
        for y, row in zip(range(y0, y1 + 1), rows)
        for x in range(x0, max(x0, x1 - LENGTH) + 1)
    }


def hideouts(rows: Sequence[str], box: Box) -> list[tuple[int, int]]:
    """The spots where the text hides a fish but a cell at most."""
    return [spot for spot, n in cover(rows, box).items() if n >= LENGTH - 1]


def lure(
    fish: Fish,
    spots: Sequence[tuple[int, int]],
    cursor: tuple[int, int] | None,
    dt: float,
    rng: random.Random,
) -> tuple[int, int] | None:
    """Where a shy fish heads, kept in `fish.home`; None for a bold one,
    or when no spot is out of the cursor's reach. `spots`: `hideouts`.
    The nearest spot first; `STAY` there, then one drawn at random at
    least two lengths away."""
    if not fish.shy:
        return None
    safe = [s for s in spots if cursor is None or reach(*s, cursor) >= NEAR_MOVING]
    if fish.home not in safe:
        fish.home = None
    elif fish.cell[:2] == fish.home:
        fish.stay -= dt
        if fish.stay <= 0:
            hx, hy = fish.home
            away = [s for s in safe if math.hypot(s[0] - hx, s[1] - hy) > 2 * LENGTH]
            fish.home, fish.stay = rng.choice(away or safe), rng.uniform(*STAY)
    if fish.home is None and safe:
        fish.home = min(
            safe, key=lambda s: (math.hypot(s[0] - fish.x, s[1] - fish.y), rng.random())
        )
        fish.stay = rng.uniform(*STAY)
    return fish.home


def entry(rows: Sequence[str], box: Box, rng: random.Random) -> tuple[int, int]:
    """Where a fish comes back: a spot of the view where `rows`, its
    text, hides most of the fish, drawn at random among the best. With
    no text at all, a side of the view."""
    x0, y0, x1, y1 = box
    xs = range(x0, max(x0, x1 - LENGTH) + 1)
    hidden = cover(rows, box)
    best = max(hidden.values(), default=0)
    if not best:
        return rng.choice((xs[0], xs[-1])), rng.randint(y0, y1)
    return rng.choice([spot for spot, n in hidden.items() if n == best])


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    frame = _cli.pick(pilot, args.frame)
    x0, y0, x1, y1 = view(frame)
    shy = args.count // 2 if args.shy is None else args.shy
    shy = set(rng.sample(range(args.count), max(0, min(shy, args.count))))
    school = [
        Fish(
            x=rng.uniform(x0, max(x0, x1 - LENGTH)),
            y=rng.randint(y0, y1),
            vx=rng.choice((-1, 1)) * rng.uniform(0.5, 1.0) * CALM,
            color=COLORS[i % len(COLORS)],
            shy=i in shy,
        )
        for i in range(args.count)
    ]
    due: list[float | None] = [None] * len(school)
    before = None
    rows: list[str] = []
    spots: list[tuple[int, int]] = []
    read: tuple[Box, float] | None = None  # view and time of the last read
    for dt in _cli.ticks(TICK, seconds):
        # Refreshed each tick: the origin moves when the content scrolls.
        if not _cli.on_screen(frame):
            continue
        box = view(frame)
        at = pilot.cursor()
        cursor = frame.to_frame(at.x, at.y) if at.visible else None
        moving, before = cursor != before, cursor
        cells = [fish.cell[:2] for fish in school]
        now = time.monotonic()
        due, going = tide(cells, due, box, now, rng)
        with _cli.in_sight():
            if read is None or read[0] != box or now - read[1] >= READ:
                rows, read = frame.text(), (box, now)
                spots = hideouts(rows, box)
            if going is not None:
                fish = school[going]
                fish.x, fish.y = entry(rows, box, rng)
                # Calm, towards the middle of the view.
                inward = (box[0] + box[2] - LENGTH) / 2 - fish.x
                fish.vx = math.copysign(rng.uniform(0.5, 1.0) * CALM, inward)
                fish.vy = fish.scared = 0.0
                fish.home = None
            with pilot.batch():
                for fish in school:
                    if inside(fish.cell[:2], box):  # out of view, it waits
                        lure(fish, spots, cursor, dt, rng)
                        swim(fish, dt, box, cursor, moving, rng)
                    x, y, glyph = fish.cell
                    if fish.sprite is None:
                        fish.sprite = pilot.draw(
                            [glyph],
                            frame=frame.id,
                            at=(x, y),
                            space="frame",
                            layer="back",
                            fg=fish.color,
                            bold=True,
                        )
                    elif fish.cell != fish.shown:
                        # Only what changed is sent.
                        fish.sprite.update(
                            anchor={"at": [x, y], "space": "frame"},
                            content={
                                "rows": [glyph],
                                "style": {"fg": fish.color, "bold": True},
                            },
                        )
                    fish.shown = fish.cell


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "fishes", "Fishes behind the text, afraid of the cursor.", frame="largest"
    )
    out.add_argument("--count", type=int, default=8, help="fishes [default: 8]")
    out.add_argument(
        "--shy", type=int, metavar="N", help="fishes drawn to the text [default: half]"
    )
    out.add_argument("--seed", type=int, help="same seed, same school")
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
