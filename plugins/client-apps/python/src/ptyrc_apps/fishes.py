# SPDX-License-Identifier: GPL-3.0-or-later
"""Fishes: an aquarium behind the text of a scrolling pane.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app fishes
    ptyrc-app fishes --count 12 --shy 2 -f active
    ptyrc-app fishes -a              # the whole aquarium

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

More, each on its own flag; `-a` for all (see `_reef`):

- `-d`: decorations that stand on the text: kelp, shells, corals, a
  castle. Some behind the fishes, some in front, all behind the text.
- `-b`: dim bubbles from the bottom of the view to its top.
- `-r`: a fish that comes back is of a random `SPECIES`; now and then
  an event: a creature crosses, an object sinks to the text and stays,
  a hook comes down, a fish leaves. Fishes flee hooks and big mouths.

What was populated is remembered in frame cells (`Reef`): scroll away
and back, the same castle stands on the same line.
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
from ._reef import Events, Reef, mirror, pad

__all__ = [
    "CLASSIC",
    "SPECIES",
    "STAY",
    "WAIT",
    "Fish",
    "Species",
    "cover",
    "depart",
    "entry",
    "hideouts",
    "lure",
    "main",
    "menace",
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


@dataclass(frozen=True)
class Species:
    """A kind of fish, one row: `right`, what it looks like swimming
    right, one glyph per column it is on, in turn; `colors` to draw
    from."""

    name: str
    right: tuple[str, ...]
    colors: tuple[int | str, ...] = COLORS

    @property
    def length(self) -> int:
        return len(self.right[0])

    def glyph(self, right: bool, x: int) -> str:
        glyph = self.right[x % len(self.right)]
        return glyph if right else mirror([glyph])[0]


CLASSIC = Species("classic", (RIGHT,))
SPECIES = (
    CLASSIC,
    Species("minnow", ("><>",), (81, 117, 159, 229)),
    Species("tetra", (">(°>",), (203, 208, 214, 51)),
    Species("guppy", ("><{{°>",), (213, 141, 219, 177)),
    Species("bream", ("><(((°>",), (220, 215, 150, 111)),
    Species("sword", ("><(((°>--",), (75, 250, 153)),
    # It slithers: the body shifts with each column.
    Species("eel", ("~-~-~-~:>", "-~-~-~-:>"), (107, 143, 179)),
)


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
    kind: Species = CLASSIC
    leaving: bool = False  # the walls do not turn it
    sprite: Sprite | None = None
    shown: tuple[int, int, str] | None = None

    @property
    def length(self) -> int:
        return self.kind.length

    @property
    def cell(self) -> tuple[int, int, str]:
        right = math.copysign(1.0, self.vx) > 0  # -0.0 still faces left
        x = round(self.x)
        return (x, round(self.y), self.kind.glyph(right, x))


def reach(
    x: float, y: float, cursor: tuple[float, float], length: int = LENGTH
) -> float:
    """Distance from a fish of `length` cells at (x, y) to the cursor, in
    columns: a row is about two columns tall."""
    return math.hypot(x + length / 2 - cursor[0], (y - cursor[1]) * 2)


def approach(d: float, top: float) -> float:
    """Speed towards a point `d` cells away: slower as it nears."""
    return max(-top, min(top, 2 * d)) if abs(d) > 0.3 else 0.0


def swim(
    fish: Fish,
    dt: float,
    box: Box,
    cursor: tuple[float, float] | None,
    moving: bool,
    rng: random.Random,
) -> None:
    """One step of one fish inside `box` (x0, y0, x1, y1, frame cells)."""
    x0, y0, x1, y1 = box
    length = fish.length
    if cursor is not None:
        dx = fish.x + length / 2 - cursor[0]
        dy = (fish.y - cursor[1]) * 2
        if reach(fish.x, fish.y, cursor, length) < (NEAR_MOVING if moving else NEAR):
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
    if fish.leaving:
        pass
    elif fish.x < x0:
        fish.x, fish.vx = x0, abs(fish.vx)
    elif fish.x > x1 - length:
        fish.x, fish.vx = x1 - length, -abs(fish.vx)
    if not y0 <= fish.y <= y1:
        fish.y, fish.vy = min(max(fish.y, y0), y1), 0.0


def view(frame: Frame) -> Box:
    """The view in frame cells: a fish's `x` in [x0, x1 - LENGTH], its
    `y` in [y0, y1]."""
    (ox, oy), (w, h) = frame.origin, frame.size
    return (ox, oy, ox + w, oy + h - 1)


def inside(cell: tuple[int, int], box: Box, length: int = LENGTH) -> bool:
    x0, y0, x1, y1 = box
    return x0 <= cell[0] <= x1 - length and y0 <= cell[1] <= y1


def tide(
    cells: Sequence[tuple[int, int]],
    due: Sequence[float | None],
    box: Box,
    now: float,
    rng: random.Random,
    lengths: Sequence[int] | None = None,
) -> tuple[list[float | None], int | None]:
    """Who leaves, and when. `cells`: where the fishes are; `due`: when
    each goes, None while in view; `lengths`: theirs, `LENGTH` each by
    default. A fish out of view is given a time `WAIT` from `now`; one
    in view, none. Returns the new times and the fish that goes now, if
    any: the most overdue, one a call."""
    lengths = lengths or [LENGTH] * len(cells)
    due = [
        None if inside(c, box, n) else now + rng.uniform(*WAIT) if d is None else d
        for c, d, n in zip(cells, due, lengths, strict=True)
    ]
    late = [i for i, d in enumerate(due) if d is not None and d <= now]
    if not late:
        return due, None
    going = min(late, key=lambda i: due[i])
    due[going] = None
    return due, going


def cover(
    rows: Sequence[str], box: Box, length: int = LENGTH
) -> dict[tuple[int, int], int]:
    """For each spot of the view, how many cells of a fish of `length`
    cells there `rows`, the view's text, hides."""
    x0, y0, x1, y1 = box
    return {
        (x, y): sum(not c.isspace() for c in row[x - x0 : x - x0 + length])
        for y, row in zip(range(y0, y1 + 1), rows)
        for x in range(x0, max(x0, x1 - length) + 1)
    }


def hideouts(
    rows: Sequence[str], box: Box, length: int = LENGTH
) -> list[tuple[int, int]]:
    """The spots where the text hides a fish but a cell at most."""
    return [spot for spot, n in cover(rows, box, length).items() if n >= length - 1]


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
    if not fish.shy or fish.leaving:
        fish.home = None
        return None
    n = fish.length
    safe = [s for s in spots if cursor is None or reach(*s, cursor, n) >= NEAR_MOVING]
    if fish.home not in safe:
        fish.home = None
    elif fish.cell[:2] == fish.home:
        fish.stay -= dt
        if fish.stay <= 0:
            hx, hy = fish.home
            away = [s for s in safe if math.hypot(s[0] - hx, s[1] - hy) > 2 * n]
            fish.home, fish.stay = rng.choice(away or safe), rng.uniform(*STAY)
    if fish.home is None and safe:
        fish.home = min(
            safe, key=lambda s: (math.hypot(s[0] - fish.x, s[1] - fish.y), rng.random())
        )
        fish.stay = rng.uniform(*STAY)
    return fish.home


def entry(
    rows: Sequence[str], box: Box, rng: random.Random, length: int = LENGTH
) -> tuple[int, int]:
    """Where a fish comes back: a spot of the view where `rows`, its
    text, hides most of the fish, drawn at random among the best. With
    no text at all, a side of the view."""
    x0, y0, x1, y1 = box
    xs = range(x0, max(x0, x1 - length) + 1)
    hidden = cover(rows, box, length)
    best = max(hidden.values(), default=0)
    if not best:
        return rng.choice((xs[0], xs[-1])), rng.randint(y0, y1)
    return rng.choice([spot for spot, n in hidden.items() if n == best])


def menace(
    fish: Fish,
    cursor: tuple[int, int] | None,
    moving: bool,
    threats: Sequence[tuple[float, float]],
) -> tuple[tuple[float, float] | None, bool]:
    """What `fish` flees: a threat within `NEAR`, the nearest; else the
    cursor. As the `cursor` and `moving` of `swim`."""
    near = min(
        threats, key=lambda p: reach(fish.x, fish.y, p, fish.length), default=None
    )
    if near is not None and reach(fish.x, fish.y, near, fish.length) < NEAR:
        return near, False
    return cursor, moving


def depart(school: Sequence[Fish], box: Box, rng: random.Random) -> Fish | None:
    """A bold, calm fish of the view heads for the nearest side and
    leaves; None if there is none."""
    calm = [
        f
        for f in school
        if not f.shy and f.scared <= 0 and inside(f.cell[:2], box, f.length)
    ]
    if not calm:
        return None
    fish = rng.choice(calm)
    middle = (box[0] + box[2] - fish.length) / 2
    fish.leaving = True
    fish.vx = math.copysign(CALM, fish.x - middle)
    return fish


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
    spots: dict[int, list[tuple[int, int]]] = {}  # hideouts, by length
    read: tuple[Box, float] | None = None  # view and time of the last read
    reef = Reef()
    events = Events(reef, time.monotonic(), rng) if args.random else None
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
        due, going = tide(cells, due, box, now, rng, [f.length for f in school])
        with _cli.in_sight():
            if read is None or read[0] != box or now - read[1] >= READ:
                still = read is not None and read[0] == box
                rows, read = pad(frame.text(), box[2] - box[0]), (box, now)
                spots = {}
                with pilot.batch():
                    reef.prune(rows, box)
                    if args.decorate:
                        reef.survey(rows, box, still, rng)
                    reef.show(pilot, frame, box)
            if going is not None:
                fish = school[going]
                if args.random:
                    fish.kind = rng.choice(SPECIES)
                    fish.color = rng.choice(fish.kind.colors)
                fish.x, fish.y = entry(rows, box, rng, fish.length)
                # Calm, towards the middle of the view.
                inward = (box[0] + box[2] - fish.length) / 2 - fish.x
                fish.vx = math.copysign(rng.uniform(0.5, 1.0) * CALM, inward)
                fish.vy = fish.scared = 0.0
                fish.home, fish.leaving = None, False
            with pilot.batch():
                threats: list[tuple[float, float]] = []
                if events is not None:
                    threats, leave = events.step(pilot, frame, rows, box, now, dt)
                    if leave:
                        depart(school, box, rng)
                if args.bubbles:
                    reef.bubbles(pilot, frame, box, now, rng)
                for fish in school:
                    # Out of view, it waits; unless it is on its way out.
                    if inside(fish.cell[:2], box, fish.length) or fish.leaving:
                        if fish.shy and fish.length not in spots:
                            spots[fish.length] = hideouts(rows, box, fish.length)
                        lure(fish, spots.get(fish.length, ()), cursor, dt, rng)
                        swim(fish, dt, box, *menace(fish, cursor, moving, threats), rng)
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
    out.add_argument(
        "-d", "--decorate", action="store_true", help="decorations on the text"
    )
    out.add_argument(
        "-b", "--bubbles", action="store_true", help="bubbles from the bottom"
    )
    out.add_argument(
        "-r", "--random", action="store_true", help="random species, events"
    )
    out.add_argument("-a", "--all", action="store_true", help="-d -b -r")
    args = _cli.parse(out, argv)
    if args.all:
        args.decorate = args.bubbles = args.random = True
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
