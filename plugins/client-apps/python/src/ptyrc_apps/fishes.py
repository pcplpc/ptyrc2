# SPDX-License-Identifier: GPL-3.0-or-later
"""Fishes: an aquarium behind the text of a scrolling pane.

    ptyrc run vim long-file.txt      # or less, man
    ptyrc-app fishes
    ptyrc-app fishes --count 12 --shy 2 -f active
    ptyrc-app fishes -a              # the whole aquarium
    ptyrc-app fishes --tank -a       # in a widget, a bowl to a tank

What it shows:

- `space="frame"`: a fish holds a position of the frame, not of the
  window. Scroll the file and the fishes scroll with the text.
- Events, not polls (`_cli.Sight`): the topic `cursor` tells where the
  cursor is, in frame cells: fishes flee it, farther when it moves.
  The topic `text` tells when the text changed or scrolled: the
  hideouts are looked for again then.
- `pilot.batch()`: all the moves of a tick go out in one request and
  show in one repaint.
- The back layer: fishes swim behind the text, between the words.

Half of the school, rounded down, drawn at birth, is shy (`--shy N`:
N). A shy fish swims to the nearest spot of the view where the text
hides all of it but a cell at most (`hideouts`), out of reach of the
cursor, slows down and stays there 4 to 10 s; then it goes to another
spot, drawn at random among those at least two lengths away (`lure`).
Fear comes first.

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

`--tank`: the fishes in one widget (`Tank`), from a bowl of 9x4 with
one fish to a tank of 40x10; border `tank`. The water is composed in
the content, a `widget.set` a tick: a sprite is clipped to its frame,
not to a widget, and the content follows the widget when the driver
moves, sizes or hides it. The same `swim`, no text and no cursor; `-d`:
sand, and decorations on it; `-b`: bubbles; `-r`: random species.
Hover: the fishes come to the pointer, as to a finger on the glass.
`--print [WxH]`: no driver, the variants on stdout.
"""

from __future__ import annotations

import argparse
import math
import random
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Frame, Pilot, Sprite, clock

from . import _cli, _widgets
from ._reef import DECOR, Events, Reef, Shape, mirror, pad

__all__ = [
    "BORDERS",
    "CLASSIC",
    "SPECIES",
    "STAY",
    "TANKS",
    "WAIT",
    "Fish",
    "Species",
    "Tank",
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
STILL = 1.0  # seconds without a scroll: the view is still

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
    surveyed: Box | None = None  # the view at the last look at the reef
    settle: float | None = None  # time of a second look, the view still
    reef = Reef()
    events = Events(reef, clock.monotonic(), rng) if args.random else None
    # Told by the driver (`_cli.Sight`): the text of the view when it
    # changed or scrolled, the cursor when it moved. Nothing is asked.
    sight = _cli.Sight(pilot, frame, cursor=True)
    for dt in _cli.ticks(TICK, seconds):
        if not sight.look():
            continue
        box = view(frame)
        cursor = sight.cursor
        moving, before = cursor != before, cursor
        cells = [fish.cell[:2] for fish in school]
        now = clock.monotonic()
        due, going = tide(cells, due, box, now, rng, [f.length for f in school])
        with sight.drawing():
            # The reef is looked at when the text is read, and once
            # more `STILL` after a scroll: the view is still then.
            if sight.changed or (settle is not None and now >= settle):
                still = surveyed == box
                settle = None if still else now + STILL
                surveyed = box
                if sight.changed:
                    rows = pad(sight.rows, box[2] - box[0])
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


TANKS: tuple[tuple[int, int], ...] = ((9, 4), (14, 5), (20, 6), (28, 8), (40, 10))
WATER, SAND = (67, 74), (180, 143, 137)
BORDERS: dict[str, dict[str, Any]] = {
    # Glass: thin walls, a rim, a thick floor.
    "tank": {
        "corners": "╓╖╚╝",
        "top": "╌",
        "bottom": "═",
        "left": "║",
        "right": "║",
        "style": {"fg": 67},
        "title": {"style": {"fg": 110}},
    },
}
_NAMED = {
    "cyan": 6,
    "bright-cyan": 14,
    "bright-yellow": 11,
    "bright-magenta": 13,
    "bright-green": 10,
}


@dataclass
class _Bubble:
    x: float
    y: float
    speed: float  # rows per second
    phase: float


class Tank:
    """An aquarium of `w` by `h` cells, in its own cells: `step(dt)`,
    then `rows()`. As many fishes as the water holds, `count` at most:
    one in a bowl. A row of surface over the water when there are four
    rows; with `decorate`, sand under it and decorations on the sand,
    some in front of the fishes."""

    def __init__(
        self,
        w: int,
        h: int,
        rng: random.Random,
        count: int = 8,
        decorate: bool = False,
        bubbles: bool = False,
        species: bool = False,
    ) -> None:
        self.w, self.h, self.rng = w, h, rng
        self.t = 0.0
        self.top = 1 if h >= 4 else 0
        self.floor = h - 1  # the lowest row of water
        self.sand = decorate and h >= 4
        self.decor: list[tuple[Shape, int, dict[str, Any], bool, int]] = []
        if self.sand:
            self._plant()
        self.vents: list[int] = []
        if bubbles:
            self.vents = sorted(rng.sample(range(1, w - 1), max(1, w // 14)))
        self.air: list[_Bubble] = []
        self.finger: tuple[int, int] | None = None
        kinds = [k for k in SPECIES if k.length <= w - 3] if species else [CLASSIC]
        # A small tank is a slow one.
        self.pace = 0.3 + 0.5 * min(1.0, w / 40)
        self.school = []
        for i in range(max(1, min(count, round(w * h / 45)))):
            kind = rng.choice(kinds)
            self.school.append(
                Fish(
                    x=rng.uniform(0, max(0, w - kind.length)),
                    y=rng.randint(self.top, self._low),
                    vx=rng.choice((-1, 1)) * rng.uniform(0.5, 1.0) * CALM * self.pace,
                    color=rng.choice(kind.colors) if species else COLORS[i % 5],
                    kind=kind,
                )
            )

    @property
    def _low(self) -> int:
        """The lowest row a fish swims on: over the sand."""
        return max(self.top, self.floor - 1) if self.sand else self.floor

    def _plant(self) -> None:
        """Decorations along the sand, left to right, a gap between."""
        rng, room = self.rng, self.floor - self.top
        fit = [s for s in DECOR if s.h <= room and s.w <= max(3, self.w // 3)]
        x = rng.randint(0, 3)
        big = False
        while fit:
            shape = rng.choices(fit, [s.weight for s in fit])[0]
            if shape.h >= 4 and big:  # one castle a tank
                continue
            if x + shape.w > self.w:
                break
            big = big or shape.h >= 4
            ink = rng.choice(shape.inks)
            front = rng.random() < shape.front
            self.decor.append((shape, x, ink, front, rng.randrange(8)))
            x += shape.w + rng.randint(2, 2 + self.w // 5)
            if self.w < 12:  # a bowl: one thing in it
                break

    def on_move(self, x: int, y: int) -> None:
        """The pointer on the cell `(x, y)`: the fishes come."""
        self.finger = (x, y)

    def on_leave(self) -> None:
        self.finger = None

    def step(self, dt: float) -> None:
        self.t += dt
        rng = self.rng
        box = (0, self.top, self.w, self._low)
        for i, fish in enumerate(self.school):
            fish.home = None
            if self.finger is not None:
                # Around the pointer, each at its place, nose to it.
                side = -1 if i % 2 else 1
                x = self.finger[0] + (1 if side > 0 else -fish.length - 1)
                y = self.finger[1] + ((i + 1) // 2) * (-1 if i % 4 < 2 else 1)
                fish.home = (
                    max(0, min(self.w - fish.length, x)),
                    max(self.top, min(self._low, y)),
                )
            swim(fish, dt, box, None, False, rng)
            if fish.home is not None and abs(fish.vx) < 0.5:
                # Still: it looks at the pointer.
                middle = fish.x + fish.length / 2
                fish.vx = math.copysign(0.0, self.finger[0] - middle)  # type: ignore[index]
            elif fish.home is None and abs(fish.vx) < 1.0:
                fish.vx = math.copysign(
                    rng.uniform(0.5, 1.0) * CALM * self.pace, fish.vx
                )
        for vent in self.vents:
            if rng.random() < 0.6 * dt:
                self.air.append(
                    _Bubble(vent, self._low, rng.uniform(1.2, 2.2), rng.uniform(0, 6))
                )
        for bubble in self.air:
            bubble.y -= bubble.speed * dt
        self.air = [b for b in self.air if b.y >= self.top - 0.5]

    def rows(self) -> list[list[dict[str, Any]]]:
        """The tank as rows of runs, `w` cells each."""
        w, h, t = self.w, self.h, self.t
        grid: list[list[dict[str, Any]]] = [
            [{"t": " "} for _ in range(w)] for _ in range(h)
        ]

        def put(x: int, y: int, glyph: str, **style: Any) -> None:
            if 0 <= x < w and 0 <= y < h and glyph != " ":
                grid[y][x] = {"t": glyph, **style}

        def stand(front: bool) -> None:
            for shape, x0, ink, ahead, phase in self.decor:
                if ahead != front:
                    continue
                frames = shape.frames
                art = frames[(int(t * 1000 / shape.ms) + phase) % len(frames)]
                for dy, row in enumerate(art):
                    for dx, glyph in enumerate(row):
                        fg = next(
                            (c for k, c in ink.items() if k and glyph in k), ink[""]
                        )
                        put(x0 + dx, self.floor - shape.h + dy, glyph, fg=fg)

        if self.top:
            # The surface: ripples that drift.
            for x in range(w):
                wave = math.sin(0.9 * x - 1.7 * t) + math.sin(0.37 * x + 0.9 * t)
                if wave > -0.6:
                    put(
                        x, 0, "~" if wave < 0.9 else "≈", fg=WATER[wave > 0.3], dim=True
                    )
        if self.sand:
            for x in range(w):
                grain = (x * 2654435761 + w * 40503) >> 7
                glyph = ".,.:. .._"[grain % 9]
                put(x, self.floor, glyph, fg=SAND[grain % 3], dim=True)
        stand(False)
        for bubble in self.air:
            x = round(bubble.x + 0.9 * math.sin(bubble.phase + 2.5 * bubble.y))
            high = (self._low - bubble.y) / max(1, self._low - self.top)
            glyph = "." if high < 0.35 else "o" if high < 0.8 else "O"
            put(x, round(bubble.y), glyph, fg=110, dim=True)
        for fish in self.school:
            x, y, glyph = fish.cell
            for dx, ch in enumerate(glyph):
                put(x + dx, y, ch, fg=fish.color, bold=True)
        stand(True)
        out = []
        for row in grid:
            runs: list[dict[str, Any]] = []
            for cell in row:
                if runs and {**runs[-1], "t": ""} == {**cell, "t": ""}:
                    runs[-1]["t"] += cell["t"]
                else:
                    runs.append(dict(cell))
            out.append(runs)
        return out


def _tank(args: argparse.Namespace, rng: random.Random, w: int, h: int) -> Tank:
    return Tank(w, h, rng, args.count, args.decorate, args.bubbles, args.random)


def _ansi(run: dict[str, Any]) -> str:
    fg = run.get("fg")
    if fg is None:
        return run["t"]
    bold = "1;" if run.get("bold") else "2;" if run.get("dim") else ""
    return f"\x1b[{bold}38;5;{_NAMED.get(fg, fg)}m{run['t']}\x1b[0m"


def show(args: argparse.Namespace) -> int:
    """`--tank --print`: the variants after a while, or one size; no
    driver."""
    rng = random.Random(args.seed)
    colour = sys.stdout.isatty()
    for w, h in TANKS if args.print == (0, 0) else (args.print,):
        tank = _tank(args, rng, w, h)
        for _ in range(80):
            tank.step(TICK)
        print("┌" + "─" * w + "┐")
        for row in tank.rows():
            text = "".join(_ansi(run) if colour else run["t"] for run in row)
            print(f"│{text}│")
        print("└" + "─" * w + "┘")
    return 0


def aquarium(
    pilot: Pilot, args: argparse.Namespace, seconds: float | None = None
) -> None:
    """`--tank`: one widget, a `Tank` a variant; the one shown swims."""
    rng = random.Random(args.seed)
    tanks = {size: _tank(args, rng, *size) for size in TANKS}
    seen = dict.fromkeys(TANKS, clock.monotonic())

    def render(w: int, h: int) -> list[list[dict[str, Any]]]:
        now = clock.monotonic()
        # Not shown for a while, it was still: no leap.
        tanks[w, h].step(min(now - seen[w, h], 3 * TICK))
        seen[w, h] = now
        return tanks[w, h].rows()

    def come(x: int, y: int) -> bool:
        for tank in tanks.values():
            tank.finger = None
        if widget.size is not None:
            tanks[widget.size].on_move(x, y)
        return False  # the next tick shows it

    def go() -> bool:
        for tank in tanks.values():
            tank.on_leave()
        return False

    deck = _widgets.Deck(pilot, args)
    widget = deck.add("fishes", list(TANKS), render)
    deck.on_hover(widget, come, go)
    deck.run(period=TICK, seconds=seconds)


def _size(text: str) -> tuple[int, int]:
    try:
        w, h = (int(n) for n in text.lower().split("x"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"not WxH: {text}") from None
    if w < 8 or h < 2:
        raise argparse.ArgumentTypeError(f"too small, 8x2 at least: {text}")
    return (w, h)


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
    out.add_argument(
        "--tank", action="store_true", help="in one widget, a bowl to a tank"
    )
    tank = out.add_argument_group("with --tank")
    _widgets.options(tank, border="tank")
    tank.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(0, 0),
        metavar="WxH",
        help="no driver: print every variant, or the size given",
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.decorate = args.bubbles = args.random = True
    if args.print is not None and not args.tank:
        out.error("--print: with --tank")
    if args.tank:
        args.border = BORDERS.get(args.border, args.border)
        if args.print is not None:
            return show(args)
        return _cli.run(lambda pilot, args: aquarium(pilot, args, seconds), args)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
