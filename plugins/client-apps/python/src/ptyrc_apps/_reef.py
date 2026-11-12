# SPDX-License-Identifier: GPL-3.0-or-later
"""The reef: what `fishes` adds to the water with `-d`, `-b` and `-r`.

- Decorations (`DECOR`) stand on text: on the free cells right above a
  line. `stand` finds the places, `Reef.survey` plants.
- Bubbles rise from vents on the bottom row of the view. A bubble is
  one request: an animation with a `motion`, played by the driver.
- Events (`Events`): a creature crossing (`CROSSERS`, played by the
  driver), an object sinking to the text below (`SUNK`), a fishing
  hook, a fish leaving.

`Reef` is the memory. A row of text is populated once (`seen`); what
stands there is kept (`decor`), and so are the vents of each area of
`AREA` rows. Sprites exist only near the view: scrolled back, an area
is drawn again as it was.

Layers, from the back: bubbles, crossers, decorations, then the fishes
with the other decorations in front of them. All behind the text.
"""

from __future__ import annotations

import math
import random
from collections.abc import Iterable, Sequence
from dataclasses import dataclass, field
from typing import Any

from ptyrc import Frame, Pilot, Sprite

__all__ = [
    "AREA",
    "CROSSERS",
    "DECOR",
    "DENSITY",
    "EVERY",
    "SUNK",
    "TALL",
    "Crosser",
    "Decor",
    "Events",
    "Reef",
    "Shape",
    "Sunk",
    "Vent",
    "bubble",
    "depths",
    "mirror",
    "pad",
    "paint",
    "stand",
]

AREA = 12  # rows of an area: one set of vents
DENSITY = 0.3  # share of the lines of text that get a decoration
EVERY = (8.0, 20.0)  # seconds between two events
FAR, BEHIND, FRONT = -3, -2, -1  # layers; the fishes are in FRONT, z 0
BUBBLES = -4

Rows = tuple[str, ...]
Ink = dict[str, int | str]  # glyphs to a colour; "": the others
Box = tuple[int, int, int, int]

_SWAP = str.maketrans("/\\()<>[]{}`'", "\\/)(><][}{'`")


def mirror(rows: Iterable[str]) -> Rows:
    """`rows` seen from the other side: what faced right faces left."""
    return tuple(row[::-1].translate(_SWAP) for row in rows)


def pad(rows: Sequence[str], width: int) -> list[str]:
    """Rows of exactly `width` cells."""
    return [row.ljust(width)[:width] for row in rows]


def paint(rows: Sequence[str], ink: Ink, **style: Any) -> list[list[dict[str, Any]]]:
    """Rows of text to rows of styled runs. A space is transparent."""

    def fg(ch: str) -> int | str:
        return next((c for k, c in ink.items() if k and ch in k), ink[""])

    out = []
    for row in rows:
        runs: list[dict[str, Any]] = []
        for ch in row:
            run = {"t": ch} if ch == " " else {"t": ch, "fg": fg(ch), **style}
            if runs and {**runs[-1], "t": ""} == {**run, "t": ""}:
                runs[-1]["t"] += ch
            else:
                runs.append(run)
        out.append(runs)
    return out


def _box(frames: Iterable[Sequence[str]]) -> tuple[Rows, ...]:
    """Pad to a common box: rows to the right, frames from the top."""
    frames = [list(rows) for rows in frames]
    w = max(len(row) for rows in frames for row in rows)
    h = max(len(rows) for rows in frames)
    return tuple(
        tuple(row.ljust(w) for row in [""] * (h - len(rows)) + rows) for rows in frames
    )


@dataclass(frozen=True)
class Shape:
    """Art in a box: `frames` of the same size, played `ms` apart, in
    one of `inks`. `weight`: how often it is planted; `front`: the share
    planted in front of the fishes."""

    name: str
    frames: tuple[Rows, ...]
    inks: tuple[Ink, ...]
    ms: int = 700
    weight: int = 1
    front: float = 0.5

    @property
    def w(self) -> int:
        return len(self.frames[0][0])

    @property
    def h(self) -> int:
        return len(self.frames[0])


def _kelp(h: int, twin: bool = False) -> tuple[Rows, ...]:
    """A stalk of `h` rows, swaying; `twin`: a shorter one next to it."""

    def stalk(n: int, phase: int) -> list[str]:
        return [" )" if (i + phase) % 2 else "( " for i in range(n)]

    def rows(phase: int) -> list[str]:
        if not twin:
            return stalk(h, phase)
        short = ["  ", *stalk(h - 1, phase + 1)]
        return [a + b for a, b in zip(stalk(h, phase), short, strict=True)]

    return _box([rows(0), rows(1)])


def _ink(*colors: int, **more: int) -> tuple[Ink, ...]:
    return tuple({"": c, **more} for c in colors)


GREENS = _ink(28, 29, 34, 35, 70, 71)
STONES = _ink(244, 246, 248)

DECOR = (
    Shape("kelp-2", _kelp(2), GREENS, weight=4),
    Shape("kelp-3", _kelp(3), GREENS, weight=4),
    Shape("kelp-4", _kelp(4), GREENS, weight=3),
    Shape("kelp-5", _kelp(5), GREENS, weight=2),
    Shape("weed-3", _kelp(3, twin=True), GREENS, weight=3),
    Shape("weed-4", _kelp(4, twin=True), GREENS, weight=2),
    Shape("grass", _box([["\\|/"]]), GREENS, weight=3),
    Shape("tuft", _box([["\\\\|//"]]), GREENS, weight=2),
    Shape(
        "shell",
        _box([["(@)"]]),
        ({"": 223, "@": 216}, {"": 224, "@": 211}, {"": 189, "@": 147}),
        weight=3,
    ),
    Shape("pebbles", _box([[".oOo."]]), STONES),
    Shape("pebble", _box([["o."]]), STONES),
    Shape("stones", _box([[".o"]]), STONES),
    Shape(
        "crab",
        _box([["V(°°)V"], ["V(°°)V"], ["v(°°)V"], ["v(°°)v"], ["V(°°)v"]]),
        ({"": 203, "°": 231}, {"": 209, "°": 231}),
        ms=600,
        weight=2,
    ),
    Shape(
        "snail",
        _box([['_@/"'], ['_@/"'], ["_@/'"]]),
        ({"": 180, "@": 173}, {"": 187, "@": 137}),
        ms=1200,
    ),
    Shape("starfish", _box([["_\\/_", " /\\ "]]), _ink(208, 203, 221), weight=2),
    Shape("coral", _box([["\\Y/", " | "]]), _ink(203, 211, 209, 177), weight=2),
    Shape(
        "coral-tree",
        _box([["y Y y", " \\|/ ", "  Y  "]]),
        _ink(203, 211, 209, 177),
        weight=2,
    ),
    Shape(
        "anemone",
        _box([["((|))", " \\_/ "], ["))|((", " \\_/ "]]),
        ({"": 213, "\\_/": 132}, {"": 219, "\\_/": 97}, {"": 216, "\\_/": 131}),
        ms=900,
        weight=2,
    ),
    Shape(
        "castle",
        _box(
            [
                [
                    "     |" + flag,
                    " _   _   _ ",
                    "| |_| |_| |",
                    "|    _    |",
                    "|___|_|___|",
                ]
                for flag in ">>~"
            ]
        ),
        tuple({**ink, ">~": 196} for ink in STONES),
        ms=450,
        weight=4,
        front=0.0,
    ),
    Shape(
        "tower",
        _box([[" |" + flag, "/^\\", "| |", "|n|"] for flag in ">>~"]),
        tuple({**ink, ">~": 208} for ink in STONES),
        ms=450,
        weight=3,
        front=0.0,
    ),
    Shape("column", _box([[" === ", " | | ", "_|_|_"]]), _ink(250, 252), front=0.0),
    Shape(
        "chest",
        _box([[" ___ ", f"/_{lock}_\\", "|___|"] for lock in "ooooo*"]),
        ({"": 130, "o": 178, "*": 229}, {"": 94, "o": 178, "*": 229}),
        ms=400,
        weight=2,
        front=0.0,
    ),
    Shape("anchor", _box([["  o  ", " -|- ", "\\_|_/"]]), STONES),
)
TALL = max(shape.h for shape in DECOR)


@dataclass(frozen=True)
class Sunk:
    """An object that falls in the water: `fall` on its way down, at
    `speed` rows per second, `rest` once on the text."""

    fall: Shape
    rest: Shape
    speed: float


def _sunk(shape: Shape, speed: float, rest: Shape | None = None) -> Sunk:
    return Sunk(shape, rest or shape, speed)


SUNK = (
    _sunk(
        Shape("coin", _box([["O"], ["0"], ["|"], ["0"]]), _ink(220), ms=140),
        4.0,
        Shape("coin", _box([["o"]]), _ink(220)),
    ),
    _sunk(Shape("key", _box([["o-w"]]), _ink(178, 250)), 5.0),
    _sunk(
        Shape("gem", _box([["<>"], ["<>"], ["<>"], ["()"]]), _ink(51, 201, 46), ms=300),
        4.5,
    ),
    _sunk(DECOR[-1], 9.0),
)


@dataclass(frozen=True)
class Crosser:
    """A creature that crosses the view, alone: an animation with a
    motion. `frames` face right; `speed` in cells per second. `rises`:
    from the bottom to the top instead. `scary`: fishes flee its mouth."""

    name: str
    frames: tuple[Rows, ...]
    inks: tuple[Ink, ...]
    speed: tuple[float, float]
    ms: int = 450
    rises: bool = False
    scary: bool = False

    @property
    def w(self) -> int:
        return len(self.frames[0][0])

    @property
    def h(self) -> int:
        return len(self.frames[0])


CROSSERS = (
    Crosser(
        "shoal",
        _box([["   ><>   ><>", "><>   ><>   ><>", "   ><>   ><>"]]),
        _ink(81, 117, 229, 216),
        (5.0, 8.0),
    ),
    Crosser(
        "turtle",
        _box(
            [
                ['    .-"""-.  _  ', "~--/_|_|_|_\\( o>", f"    `{f}'   `{f}'"]
                for f in "U-"
            ]
        ),
        ({"": 71, "o": 231}, {"": 107, "o": 231}, {"": 65, "o": 231}),
        (2.0, 3.5),
        ms=600,
    ),
    Crosser(
        "octopus",
        _box(
            [
                ["  .---.  ", " ( o o ) ", " /(/|\\)\\ "],
                ["  .---.  ", " ( o o ) ", " \\)\\|/(/ "],
            ]
        ),
        ({"": 169, "o": 231}, {"": 205, "o": 231}, {"": 173, "o": 231}),
        (2.5, 4.0),
        ms=500,
    ),
    Crosser(
        "submarine",
        _box(
            [
                [
                    "        _|_",
                    "   ____/___\\_____",
                    f"{screw}=(  o   o   o   )",
                    "  `--------------'",
                ]
                for screw in "*+x"
            ]
        ),
        ({"": 220, "o": 117, "*+x": 250},),
        (4.0, 6.0),
        ms=120,
    ),
    Crosser(
        "grouper",
        _box([["     ,--,_", "|\\  /  o  \\", "| ><   ))  >", "|/  \\_____/"]]),
        ({"": 208, "o": 231}, {"": 173, "o": 231}),
        (5.0, 7.0),
        scary=True,
    ),
    Crosser(
        "jellyfish",
        _box(
            [
                ["  ,--,  ", " (____) ", "  |)(|  ", "  ( )(  "],
                [" ,----, ", "(______)", "  )||(  ", "  /)(\\  "],
            ]
        ),
        _ink(213, 177, 219),
        (0.7, 1.1),
        ms=700,
        rises=True,
    ),
)


def depths(rows: Sequence[str], width: int) -> list[int]:
    """For each column, the first row with ink; `len(rows)` if none."""
    out = [len(rows)] * width
    for y in range(len(rows) - 1, -1, -1):
        for x, ch in enumerate(rows[y][:width]):
            if not ch.isspace():
                out[x] = y
    return out


def stand(rows: Sequence[str], g: int, width: int, w: int, h: int) -> list[int]:
    """Columns where a box of `w` x `h` cells stands on row `g` of
    `rows`, the view's text: ink under two cells of its base out of
    three, the box free, and a free column on each side."""
    if not h <= g < len(rows):
        return []

    def free(x: int) -> bool:
        return all(x >= len(rows[y]) or rows[y][x].isspace() for y in range(g - h, g))

    clear = [free(x) for x in range(width)]
    ground = [x < len(rows[g]) and not rows[g][x].isspace() for x in range(width)]
    return [
        x
        for x in range(width - w + 1)
        if 3 * sum(ground[x : x + w]) >= 2 * w and all(clear[max(0, x - 1) : x + w + 1])
    ]


def bubble(height: int, rng: random.Random) -> dict[str, Any]:
    """A bubble that rises `height` rows, as arguments of `Pilot.draw`
    less the place: a dot that wobbles in a box of 3 x 1 cells."""
    speed = rng.uniform(1.2, 2.2)  # rows per second
    ms = 300
    n = max(1, math.ceil(height / speed * 1000 / ms))
    phase, pace = rng.uniform(0, math.tau), rng.uniform(0.3, 0.6)
    ink: Ink = {"": rng.choice((67, 110, 109, 245))}
    frames = []
    for i in range(n):
        col = round(1 + 0.9 * math.sin(phase + i * pace))
        row = " " * col + "." + " " * (2 - col)
        frames.append({"cells": paint([row], ink, dim=True), "ms": ms})
    return {
        "frames": frames,
        "loop": False,
        "motion": {"vx": 0.0, "vy": -speed, "ax": 0.0, "ay": 0.0, "bounce": False},
        "ttl": n * ms / 1000 + 1,
        "layer": BUBBLES,
    }


@dataclass
class Decor:
    """A shape planted at `at`, the top left of its box, in frame cells."""

    shape: Shape
    at: tuple[int, int]
    ink: Ink
    front: bool = False
    phase: int = 0
    ms: int = 700
    sprite: Sprite | None = None

    @property
    def ground(self) -> int:
        """The row it stands on."""
        return self.at[1] + self.shape.h

    @property
    def cells(self) -> set[tuple[int, int]]:
        """Its box and two columns on each side."""
        (x, y), shape = self.at, self.shape
        return {
            (x + dx, y + dy) for dx in range(-2, shape.w + 2) for dy in range(shape.h)
        }

    def spec(self, frame: Frame) -> dict[str, Any]:
        """Arguments of `Pilot.draw`."""
        frames = self.shape.frames
        k = self.phase % len(frames)
        out: dict[str, Any] = {
            "frame": frame.id,
            "at": self.at,
            "space": "frame",
            "layer": FRONT if self.front else BEHIND,
            "z": 1,
        }
        if len(frames) == 1:
            out["cells"] = paint(frames[0], self.ink)
        else:
            out["frames"] = [
                {"cells": paint(rows, self.ink), "ms": self.ms}
                for rows in frames[k:] + frames[:k]
            ]
        return out


@dataclass
class Vent:
    """A column of bubbles, in frame cells: `left` more in this burst,
    the next at `due`."""

    x: int
    due: float
    left: int = 0


def _plant(shape: Shape, at: tuple[int, int], rng: random.Random) -> Decor:
    return Decor(
        shape=shape,
        at=at,
        ink=rng.choice(shape.inks),
        front=rng.random() < shape.front,
        phase=rng.randrange(len(shape.frames)),
        ms=round(shape.ms * rng.uniform(0.8, 1.25)),
    )


@dataclass
class Reef:
    """What was populated, in frame cells. `seen`: the rows of text
    already surveyed; `decor`: what stands on them; `vents`: the
    columns of bubbles of each area."""

    seen: set[int] = field(default_factory=set)
    decor: list[Decor] = field(default_factory=list)
    vents: dict[int, list[Vent]] = field(default_factory=dict)

    def survey(
        self, rows: Sequence[str], box: Box, still: bool, rng: random.Random
    ) -> list[Decor]:
        """Populate the rows of the view not `seen` yet; returns what
        was planted. A row within `TALL` of the top waits until the view
        is `still`: the rows above it may come. The last row of the view
        waits too. A share `DENSITY` of the lines of text gets a
        decoration: drawn by weight among those that fit, the tall ones
        favoured, at a random place off the others."""
        x0, y0, x1, y1 = box
        taken = {
            c for d in self.decor if y0 - TALL <= d.ground <= y1 + TALL for c in d.cells
        }
        planted = []
        # Not the last row: the status line of an editor or a pager.
        for g in range(y0 + 1, min(y1, y0 + len(rows) - 1)):
            if g in self.seen or (g - y0 < TALL and not still):
                continue
            self.seen.add(g)
            if rows[g - y0].isspace() or not rows[g - y0] or rng.random() >= DENSITY:
                continue
            # Weighted order, the tall ones first: the first that fits.
            order = sorted(
                DECOR, key=lambda s: -(rng.random() ** (1 / (s.weight * s.h * s.h)))
            )
            for shape in order:
                xs = stand(rows, g - y0, x1 - x0, shape.w, shape.h)
                rng.shuffle(xs)
                new = None
                for x in xs:
                    new = _plant(shape, (x0 + x, g - shape.h), rng)
                    if not new.cells & taken:
                        break
                    new = None
                if new is not None:
                    taken |= new.cells
                    planted.append(new)
                    break
        self.decor += planted
        return planted

    def prune(self, rows: Sequence[str], box: Box) -> list[Decor]:
        """Forget what stands on nothing: the text under it is gone.
        Its row is surveyed again. Returns what was removed."""
        x0, y0, _, y1 = box
        gone = []
        for d in self.decor:
            g = d.ground - y0
            if not 0 <= d.ground <= y1 or not 0 <= g < len(rows):
                continue
            under = rows[g][max(0, d.at[0] - x0) : max(0, d.at[0] - x0 + d.shape.w)]
            if under.isspace() or not under:
                gone.append(d)
        for d in gone:
            self.decor.remove(d)
            self.seen.discard(d.ground)
            if d.sprite is not None:
                d.sprite.remove()
        return gone

    def show(self, pilot: Pilot, frame: Frame, box: Box) -> None:
        """Sprites for what is within a view's height of the view; none
        for the rest."""
        _, y0, _, y1 = box
        reach = y1 - y0 + 1
        for d in self.decor:
            near = y0 - reach <= d.ground <= y1 + reach + d.shape.h
            if near and d.sprite is None:
                d.sprite = pilot.draw(**d.spec(frame))
            elif not near and d.sprite is not None:
                d.sprite.remove()
                d.sprite = None

    def area(self, box: Box, now: float, rng: random.Random) -> list[Vent]:
        """The vents under the view: those of the area of its last row,
        one every 20 columns, drawn once."""
        x0, _, x1, y1 = box
        key = y1 // AREA
        if key not in self.vents:
            xs = range(x0 + 1, max(x0 + 2, x1 - 1))
            n = min(len(xs), max(1, (x1 - x0) // 20))
            self.vents[key] = [
                Vent(x, now + rng.uniform(0.0, 4.0)) for x in sorted(rng.sample(xs, n))
            ]
        return self.vents[key]

    def bubbles(
        self, pilot: Pilot, frame: Frame, box: Box, now: float, rng: random.Random
    ) -> None:
        """Let the vents that are due blow one bubble: a burst of 2 to
        5, then 2 to 7 s of rest."""
        _, y0, _, y1 = box
        for vent in self.area(box, now, rng):
            if now < vent.due:
                continue
            pilot.draw(
                frame=frame.id,
                at=(vent.x - 1, y1),
                space="frame",
                **bubble(y1 - y0 + 1, rng),
            )
            if vent.left > 0:
                vent.left -= 1
                vent.due = now + rng.uniform(0.5, 1.2)
            else:
                vent.left = rng.randint(1, 4)
                vent.due = now + rng.uniform(2.0, 7.0)


@dataclass
class _Cross:
    """A crosser on its way: where it started, how fast."""

    kind: Crosser
    at: tuple[int, int]
    vel: tuple[float, float]
    born: float
    ends: float

    def mouth(self, now: float) -> tuple[float, float]:
        t = now - self.born
        front = self.kind.w if self.vel[0] > 0 else 0
        return (
            self.at[0] + self.vel[0] * t + front,
            self.at[1] + self.vel[1] * t + self.kind.h // 2,
        )


@dataclass
class _Fall:
    """A sinking object: `y` its top row, `rest` the one where it lands."""

    kind: Sunk
    ink: Ink
    x: int
    y: float
    rest: int
    sprite: Sprite
    shown: int


@dataclass
class _Hook:
    """A fishing line from row `top`: `n` cells out, `most` at the end
    of its way down; `wait`: seconds left down there."""

    x: int
    top: int
    most: int
    wait: float
    sprite: Sprite
    n: float = 1.0
    shown: int = 1

    @staticmethod
    def cells(n: int) -> list[list[dict[str, Any]]]:
        line = [{"t": "|", "fg": 244, "dim": True}]
        return [line] * (n - 1) + [[{"t": "J", "fg": 231, "bold": True}]]


class Events:
    """With `-r`: one event at a time, `EVERY` seconds after the last.

    - A crosser: one request, the driver plays it.
    - A sunk object falls from the top of the view down a free column
      and lands on the first text below; it joins the reef.
    - A hook comes down a free column, waits, goes back up.
    - A fish leaves: the tide brings another one.
    """

    def __init__(self, reef: Reef, now: float, rng: random.Random) -> None:
        self.reef = reef
        self.rng = rng
        self.due = now + rng.uniform(*EVERY) / 2
        self.cross: _Cross | None = None
        self.fall: _Fall | None = None
        self.hook: _Hook | None = None

    @property
    def busy(self) -> bool:
        return any(e is not None for e in (self.cross, self.fall, self.hook))

    def step(
        self,
        pilot: Pilot,
        frame: Frame,
        rows: Sequence[str],
        box: Box,
        now: float,
        dt: float,
    ) -> tuple[list[tuple[float, float]], bool]:
        """Move what is going on, start what is due. Returns what the
        fishes fear, as frame positions, and whether one is to leave."""
        threats: list[tuple[float, float]] = []
        leave = False
        if self.cross is not None:
            if now >= self.cross.ends:
                self.cross = None
            elif self.cross.kind.scary:
                threats.append(self.cross.mouth(now))
        if self.fall is not None:
            self._sink(frame, dt)
        if self.hook is not None:
            self._reel(dt)
            if self.hook is not None:
                threats.append((self.hook.x, self.hook.top + self.hook.shown - 1))
        if self.busy:
            self.due = now + self.rng.uniform(*EVERY)
        elif now >= self.due:
            what = self.rng.choice("ccccfffhhl")
            if what == "c":
                self._cross(pilot, frame, box, now)
            elif what == "f":
                self._drop(pilot, frame, rows, box)
            elif what == "h":
                self._cast(pilot, frame, rows, box)
            else:
                leave = True
                self.due = now + self.rng.uniform(*EVERY)
        return threats, leave

    def _cross(self, pilot: Pilot, frame: Frame, box: Box, now: float) -> None:
        rng = self.rng
        x0, y0, x1, y1 = box
        kind = rng.choice(CROSSERS)
        speed = rng.uniform(*kind.speed)
        frames = kind.frames
        if kind.rises:
            at = (rng.randint(x0, max(x0, x1 - kind.w)), y1 + 1)
            vel = (0.0, -speed)
            way = y1 - y0 + 1 + kind.h
        else:
            right = rng.random() < 0.5
            if not right:
                frames = tuple(mirror(rows) for rows in frames)
            at = (x0 - kind.w if right else x1, rng.randint(y0, max(y0, y1 - kind.h)))
            vel = (speed if right else -speed, 0.0)
            way = x1 - x0 + kind.w
        life = way / speed
        ink = rng.choice(kind.inks)
        pilot.draw(
            frames=[{"cells": paint(rows, ink), "ms": kind.ms} for rows in frames],
            frame=frame.id,
            at=at,
            space="frame",
            layer=FAR,
            motion={"vx": vel[0], "vy": vel[1], "ax": 0.0, "ay": 0.0, "bounce": False},
            ttl=life,
        )
        self.cross = _Cross(kind, at, vel, now, now + life)

    def _drop(self, pilot: Pilot, frame: Frame, rows: Sequence[str], box: Box) -> None:
        """A column where the object falls 3 rows or more, onto text
        above the last row."""
        rng = self.rng
        x0, y0, x1, _ = box
        kind = rng.choice(SUNK)
        w, h = kind.fall.w, kind.fall.h
        first = depths(rows, x1 - x0)
        # A free column on each side, all the way down.
        ways = [(x, min(first[x - 1 : x + w + 1]) - h) for x in range(1, x1 - x0 - w)]
        ways = [(x, r) for x, r in ways if 3 <= r < len(rows) - h - 1]
        if not ways:
            return
        x, rest = rng.choice(ways)
        ink = rng.choice(kind.fall.inks)
        falling = Decor(kind.fall, (x0 + x, y0), ink, front=True, ms=kind.fall.ms)
        sprite = pilot.draw(**falling.spec(frame))
        self.fall = _Fall(kind, ink, x0 + x, float(y0), y0 + rest, sprite, y0)

    def _sink(self, frame: Frame, dt: float) -> None:
        fall = self.fall
        assert fall is not None
        fall.y = min(fall.y + fall.kind.speed * dt, fall.rest)
        y = round(fall.y)
        if y != fall.shown:
            fall.sprite.move(fall.x, y, space="frame")
            fall.shown = y
        if y >= fall.rest:
            # On the text: one more thing the reef remembers.
            landed = Decor(fall.kind.rest, (fall.x, fall.rest), fall.ink, front=True)
            landed.sprite = fall.sprite
            if fall.kind.rest is not fall.kind.fall:
                fall.sprite.set(**landed.spec(frame))
            self.reef.decor.append(landed)
            self.fall = None

    def _cast(self, pilot: Pilot, frame: Frame, rows: Sequence[str], box: Box) -> None:
        """A column free for 4 rows or more from the top of the view."""
        rng = self.rng
        x0, y0, x1, _ = box
        first = depths(rows, x1 - x0)
        ways = [x for x in range(2, x1 - x0 - 2) if first[x] >= 4]
        if not ways:
            return
        x = rng.choice(ways)
        most = rng.randint(3, min(first[x], max(3, len(rows) * 2 // 3)))
        sprite = pilot.draw(
            cells=_Hook.cells(1), frame=frame.id, at=(x0 + x, y0), space="frame"
        )
        self.hook = _Hook(x0 + x, y0, most, rng.uniform(5.0, 9.0), sprite)

    def _reel(self, dt: float) -> None:
        hook = self.hook
        assert hook is not None
        if hook.wait > 0 and hook.n < hook.most:
            hook.n = min(hook.n + 5.0 * dt, hook.most)  # down
        elif hook.wait > 0:
            hook.wait -= dt
        else:
            hook.n -= 8.0 * dt  # up
        n = round(hook.n)
        if n < 1:
            hook.sprite.remove()
            self.hook = None
        elif n != hook.shown:
            hook.sprite.set(cells=_Hook.cells(n))
            hook.shown = n
