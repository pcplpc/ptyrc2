# SPDX-License-Identifier: GPL-3.0-or-later
"""Fishes: an aquarium behind the text of a scrolling pane.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app fishes
    ptyrc-app fishes --count 12 -f active

What it shows:

- `space="frame"`: a fish holds a position of the frame, not of the
  window. Scroll the file and the fishes scroll with the text; scroll
  back and they are where they would have swum to.
- `pilot.cursor()`: fishes flee the cursor, farther when it moves.
- `pilot.batch()`: all the moves of a tick go out in one request and
  show in one repaint.
- The back layer: fishes swim behind the text, between the words.
"""

from __future__ import annotations

import argparse
import math
import random
from collections.abc import Sequence
from dataclasses import dataclass

from ptyrc import Frame, Pilot, Sprite

from . import _cli

__all__ = ["Fish", "main", "swim"]

LEFT, RIGHT = "<*//><", "><\\\\*>"
LENGTH = len(LEFT)
COLORS = ("bright-cyan", "bright-yellow", "bright-magenta", "bright-green", "cyan")
TICK = 0.1  # seconds
CALM, FEAR = 6.0, 30.0  # cells per second
NEAR, NEAR_MOVING = 8.0, 16.0  # fear radius, in columns; rows count double


@dataclass
class Fish:
    """Position in frame coordinates, speed in cells per second."""

    x: float
    y: float
    vx: float
    color: str
    vy: float = 0.0
    scared: float = 0.0  # seconds of fear left
    sprite: Sprite | None = None
    shown: tuple[int, int, str] | None = None

    @property
    def cell(self) -> tuple[int, int, str]:
        return (round(self.x), round(self.y), RIGHT if self.vx > 0 else LEFT)


def swim(
    fish: Fish,
    dt: float,
    box: tuple[int, int, int, int],
    cursor: tuple[int, int] | None,
    moving: bool,
    rng: random.Random,
) -> None:
    """One step of one fish inside `box` (x0, y0, x1, y1, frame cells)."""
    x0, y0, x1, y1 = box
    if cursor is not None:
        dx = fish.x + LENGTH / 2 - cursor[0]
        dy = (fish.y - cursor[1]) * 2  # a row is about two columns tall
        if math.hypot(dx, dy) < (NEAR_MOVING if moving else NEAR):
            # Away from the cursor, fast, for a moment.
            fish.scared = 0.8
            fish.vx = math.copysign(FEAR, dx if dx else fish.vx)
            fish.vy = math.copysign(4.0, dy) if abs(dy) < 4 and dy else 0.0
    if fish.scared > 0:
        fish.scared -= dt
        if fish.scared <= 0:
            fish.vx = math.copysign(rng.uniform(0.5, 1.0) * CALM, fish.vx)
            fish.vy = 0.0
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


def tank(frame: Frame) -> tuple[int, int, int, int]:
    """The water: as wide as the view, one view above and below it, so
    fishes are left behind by a scroll and found again."""
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy - h, ox + w, oy + 2 * h - 1)


def app(pilot: Pilot, args: argparse.Namespace) -> None:
    rng = random.Random(args.seed)
    frame = _cli.pick(pilot, args.frame)
    x0, _, x1, _ = tank(frame)
    (_, oy), (_, h) = frame.origin, frame.size
    school = [
        Fish(
            x=rng.uniform(x0, max(x0, x1 - LENGTH)),
            y=rng.randrange(oy, oy + h),
            vx=rng.choice((-1, 1)) * rng.uniform(0.5, 1.0) * CALM,
            color=COLORS[i % len(COLORS)],
        )
        for i in range(args.count)
    ]
    before = None
    for dt in _cli.ticks(TICK, args.seconds):
        # Refreshed each tick: the origin moves when the content scrolls.
        if not _cli.on_screen(frame):
            continue
        box = tank(frame)
        at = pilot.cursor()
        cursor = frame.to_frame(at.x, at.y) if at.visible else None
        moving, before = cursor != before, cursor
        with _cli.in_sight(), pilot.batch():
            for fish in school:
                swim(fish, dt, box, cursor, moving, rng)
                x, y, glyph = fish.cell
                if fish.sprite is None:
                    fish.sprite = pilot.draw(
                        [glyph],
                        frame=frame.id,
                        at=(x, y),
                        space="frame",
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


def main(argv: Sequence[str] | None = None) -> int:
    out = _cli.parser(
        "fishes", "Fishes behind the text, afraid of the cursor.", frame="largest"
    )
    out.add_argument("--count", type=int, default=6, help="fishes [default: 6]")
    out.add_argument("--seed", type=int, help="same seed, same school")
    out.add_argument("--seconds", type=float, help="stop after this long")
    return _cli.run(app, _cli.parse(out, argv))


if __name__ == "__main__":
    raise SystemExit(main())
