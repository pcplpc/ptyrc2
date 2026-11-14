# SPDX-License-Identifier: GPL-3.0-or-later
"""Flock: starlings dance before a sunset, behind the text.

    ptyrc-app flock                 # the sun goes down, a flock dances
    ptyrc-app flock -c -w -r        # clouds, a lake, reeds in a breeze
    ptyrc-app flock -j -d -k        # groups join, a second flock, a hawk
    ptyrc-app flock -n -o           # stars come out, the birds roost on the text
    ptyrc-app flock --all
    ptyrc-app flock --print 80x24   # no driver: the scene on stdout

A bird is a braille dot. The flock is a sheet of birds in space, seen
from the side: it stretches with its speed, rolls, twists and bends.
Where the sheet is seen on edge the birds crowd, and a cell is the
brighter the more birds it holds.

What it shows:

- A scene that is a function of the time: `Flock.dance`, `Dusk.picture`,
  `Land.picture` keep no state; the app asks what the scene is now and
  sends what changed.
- A dusk, `--dusk` seconds: the sun comes down from over the view,
  sets at `SET` of it; the birds then pour down to roost; a night of
  `REST` of it, and the dusk comes again, the birds lift off.
- Three sprites in view cells, behind the text: the sky (the glow of
  the sun, strokes of light the thicker the lower; clouds, stars),
  looked at once a second; the sun and the birds, 10 times a second; the
  land over them, 5 times. A bird before the sun is a dot off in its disc.
- The screen read as a map (`_cli.Sight`): nothing is drawn in the
  cell on each side of a glyph; with `-o` the tops of the lines of
  text are perches.
"""

from __future__ import annotations

import argparse
import math
import random
import sys
from collections.abc import Sequence
from typing import Any, NamedTuple

from ptyrc import Pilot, Sprite, clock

from . import _cli
from .life import DOTS
from .snake import walls_of
from .stars import AREA, Grid, Sky, breeze, clear, noise, runs

__all__ = [
    "REST",
    "SET",
    "Bird",
    "Dusk",
    "Flock",
    "Land",
    "Scene",
    "glow",
    "hawk",
    "height",
    "main",
    "perches",
    "show",
    "stage",
    "sun_at",
]

Dot = tuple[float, float]

PERIOD = 1 / 10  # seconds a tick: the birds
SKY = 10  # ticks between two looks at the sky
LAND = 2  # and at the land

SET = 0.8  # share of a dusk at which the sun is down
REST = 0.1  # the night, in dusks
START = 0.3  # share of a dusk gone at the start
ROOST = (0.86, 0.08, 0.04)  # the first bird goes down, the last, a flight
RISE = (0.0, 0.02, 0.03)  # the same, up
JOIN = (0.08, 0.62, 0.035)  # the first group comes in, the last, a flight
CORE = 0.3  # share of the birds that roost here, with `-j`
SECOND = (0.35, 0.62)  # the second flock: its birds, its size
HAWK = (28.0, 3.5)  # seconds between two stoops, and one

#: The sun, from high to the horizon.
SUN = (229, 228, 221, 220, 214, 208, 202, 196)
#: The sky, from the sun out; later in the dusk it is read further.
RAMP = (229, 221, 220, 214, 208, 209, 203, 204, 168, 133, 97, 61, 60, 60, 59)
BIRDS = (243, 247, 251, 255)  # by birds in a cell: 1, 2, 3, more
SOIL = (239, 95)  # the land; towards the sun
REEDS = (58, 94, 58, 101)
WATER = (60, 61, 24)
CLOUD = (240, 243)  # a cloud at night; by day it takes the sky's colours
PREY = 223  # the hawk
#: Its dots: wings swept back.
HAWK_DOTS = ((0, 0), (0, 1), (-1, 0), (1, 0), (-2, -1), (2, -1), (-3, -1), (3, -1), (-4, -2), (4, -2))  # fmt: skip


def _ease(x: float) -> float:
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def _wave(x: float, seed: int) -> float:
    """Smooth noise in [0, 1) along `x`, a knot at each integer."""
    i = math.floor(x)
    return noise(i, seed) + (noise(i + 1, seed) - noise(i, seed)) * _ease(x - i)


def stage(t: float, dusk: float, start: float = START) -> float:
    """Where in the dusk `t` seconds are: 0 the sun over the view,
    `SET` it is down, 1 the night, to `1 + REST`."""
    return (t / dusk + start) % (1.0 + REST)


def height(f: float) -> float:
    """The sun over the horizon at the stage `f`: 1 over the view, 0
    on the horizon, under 0 down."""
    return 1.0 - f / SET


def glow(f: float) -> float:
    """How much the sky glows, `0..1`: most when the sun is low,
    nothing when it is over the view and at night."""
    a = height(f)
    if a >= 0.5:
        return 0.55 * _ease((1.0 - a) / 0.5)
    if a >= 0.1:
        return 0.55 + 0.45 * _ease((0.5 - a) / 0.4)
    if a >= -0.06:
        return 1.0
    return 1.0 - _ease((-0.06 - a) / 0.2)


def sun_at(w: int, top: float, r: float, f: float) -> Dot:
    """The middle of the sun, in dots, at the stage `f`: from over a
    view `w` cells wide down to the right, under the dot row `top`
    of the horizon once set."""
    share = min(1.0, f / SET)
    return (2 * w * (0.3 + 0.28 * share), -r + share * (top + 2 * r))


def _disc(cx: float, cy: float, r: float, land: Land, h: int) -> dict[Any, int]:
    """The dots of the sun over the land, as bits by cell."""
    out: dict[tuple[int, int], int] = {}
    for py in range(max(0, math.floor(cy - r)), min(4 * h, math.ceil(cy + r) + 1)):
        y = py + 0.5 - cy
        if abs(y) >= r:
            continue
        half = math.sqrt(r * r - y * y)
        for px in range(max(0, math.floor(cx - half)), math.ceil(cx + half) + 1):
            if abs(px + 0.5 - cx) <= half and py < land.top(px, h):
                cell = (px >> 1, py >> 2)
                out[cell] = out.get(cell, 0) | DOTS[py & 3][px & 1]
    return out


class Dusk:
    """The sky of a sunset: strokes of light around the sun and along
    the horizon, the same for the same seed. `clouds`: bars of cloud
    that drift, lit from under."""

    def __init__(self, rng: random.Random, clouds: bool = False) -> None:
        self.seed = rng.getrandbits(24)
        self.clouds: list[tuple[float, float, float, float, int]] = []
        if clouds:
            for i in range(7):
                self.clouds.append(
                    (
                        rng.random(),  # across
                        0.12 + 0.62 * (i + rng.random()) / 7,  # down
                        rng.uniform(0.1, 0.24),  # long, in widths
                        rng.uniform(0.6, 1.4),  # thick
                        rng.getrandbits(20),
                    )
                )
        self._size = (0, 0)
        self._dots: list[list[list[tuple[float, int]]]] = []

    def _strokes(self, w: int, h: int) -> list[list[list[tuple[float, int]]]]:
        """Per cell, its dots that can light: the density of light
        from which each does, lowest first. A dot row is lit along
        strokes, not everywhere."""
        if (w, h) == self._size:
            return self._dots
        seed = self.seed
        rows = []
        for y in range(h):
            row: list[list[tuple[float, int]]] = [[] for _ in range(w)]
            for j in range(4):
                py = 4 * y + j
                gain = 1.5 * noise(py, seed + 1) ** 1.5
                if gain < 0.15:
                    continue
                slide = 97.0 * noise(py, seed)
                for px in range(2 * w):
                    # A stroke grows from its middle as the light does.
                    need = (1.0 - _wave(px / 19.0 + slide, seed + py)) / gain
                    if need < 1.0:
                        row[px >> 1].append((need, DOTS[j][px & 1]))
            for cell in row:
                cell.sort()
            rows.append(row)
        self._size, self._dots = (w, h), rows
        return rows

    def picture(self, w: int, h: int, f: float, sun: Dot, t: float = 0.0) -> Grid:
        """The sky over `w` by `h` cells, the horizon under its last
        row, at the stage `f` and `t` seconds; `sun`: its middle."""
        grid: Grid = [{} for _ in range(max(0, h))]
        g = glow(f)
        if w <= 0 or h <= 0 or g <= 0:
            return grid
        across, down = 2.0 * w, 4.0 * h
        sx, sy = sun
        a = height(f)
        late = 2.5 * _ease(1.0 - a) + 4.5 * _ease(-a / 0.3)
        reach = 0.62 * down
        strokes = self._strokes(w, h)
        last = len(RAMP) - 1
        # The light: round the sun, and along the horizon under it.
        offs = [(2 * x + 1.0 - sx) / reach for x in range(w)]
        round_ = [0.42 * g * math.exp(-3.0 * dx * dx / 3.2) for dx in offs]
        along = [g * math.exp(-5.0 * (dx * reach / across) ** 2) for dx in offs]
        for y in range(h):
            cy = 4 * y + 2.0
            dy = (cy - min(sy, down)) / reach
            near = math.exp(-3.0 * dy * dy)
            band = 0.34 * math.exp(-(down - cy) / (0.2 * down))
            out = grid[y]
            for x, dots in enumerate(strokes[y]):
                if not dots:
                    continue
                dens = round_[x] * near + along[x] * band
                if dots[0][0] >= dens:
                    continue
                bits = 0
                for need, bit in dots:
                    if need >= dens:
                        break
                    bits |= bit
                far = min(1.0, math.sqrt(offs[x] ** 2 / 3.2 + dy * dy) * 0.95)
                colour = RAMP[min(last, int(late + 9.5 * far + noise(x, y) * 0.9))]
                out[x] = (chr(0x2800 + bits), colour)
        for cloud in self.clouds:
            self._cloud(grid, w, h, cloud, (sx, sy, g, late), t)
        return grid

    def _cloud(
        self,
        grid: Grid,
        w: int,
        h: int,
        cloud: tuple[float, float, float, float, int],
        light: tuple[float, float, float, float],
        t: float,
    ) -> None:
        u, v, long, thick, key = cloud
        sx, sy, g, late = light
        across, down = 2 * w, 4 * h
        half = long * across / 2
        deep = max(2, round(thick * (1.5 + down / 50)))
        cx = (u * across + (0.25 + 0.4 * noise(key)) * t) % (across + 2 * half) - half
        cy = int(v * down)
        cells: dict[tuple[int, int], int] = {}
        for j in range(-deep, deep + 1):
            # A lens of strokes: flat under, frayed at its ends.
            lean = j / deep
            span = half * math.sqrt(max(0.0, 1.0 - lean * lean))
            span *= 0.55 + 0.9 * noise(j, key)
            mid = cx + half * 0.5 * (noise(j, key + 1) - 0.5)
            py = cy + j
            if not 0 <= py < down or (j and noise(j, key + 2) < 0.3):
                continue
            for px in range(max(0, int(mid - span)), min(across, int(mid + span))):
                edge = 1.0 - abs(px - mid) / max(1.0, span)
                if _wave(px / 6.0, key + j) < 0.2 + 0.9 * edge:
                    cell = (px >> 1, py >> 2)
                    cells[cell] = cells.get(cell, 0) | DOTS[py & 3][px & 1]
        last = len(RAMP) - 1
        for (x, y), bits in cells.items():
            # Lit from under, and from the side of the sun.
            under = (4 * y + 2 - cy) / deep
            near = math.hypot((2 * x - sx) / across, (4 * y - sy) / down)
            if g > 0.12:
                step = late + 1.5 + 7.0 * near - 2.0 * under + 3.0 * (1.0 - g)
                colour = RAMP[max(0, min(last, int(step)))]
            else:
                colour = CLOUD[under > 0]
            grid[y][x] = (chr(0x2800 + bits), colour)


class Bird(NamedTuple):
    """A bird of a flock: its place in the sheet, `rho` from the middle
    at the angle `phi`, turning by `spin` a second; `q` across the
    sheet. `beat`: of its own flutter. `order`: 0 the first to go to
    roost, 1 the last. `group`: the one it comes in with, 0 the core."""

    rho: float
    phi: float
    spin: float
    q: float
    beat: float
    order: float
    group: int


class Flock:
    """Birds that fly as one. `birds`: over 80 by 24 cells; `size`:
    of the flock, 1 a third of the view long at rest."""

    def __init__(self, rng: random.Random, birds: float = 260.0, size: float = 1.0):
        self.rng = random.Random(rng.getrandbits(32))
        self.density = birds
        self.size = size
        self.phases = [rng.uniform(0.0, math.tau) for _ in range(12)]
        self.way = rng.choice((-1.0, 1.0))
        self.birds: list[Bird] = []

    def count(self, w: int, h: int) -> int:
        return int(self.density * w * h / AREA)

    def _bird(self) -> Bird:
        rng = self.rng
        return Bird(
            rng.random() ** 0.62,
            rng.uniform(0.0, math.tau),
            self.way * rng.uniform(0.25, 0.7),
            rng.gauss(0.0, 0.5),
            rng.uniform(0.0, math.tau),
            rng.random(),
            rng.randrange(1, 9) if rng.random() >= CORE else 0,
        )

    def middle(self, across: float, down: float, t: float) -> Dot:
        """The middle of the flock at `t` seconds, in dots."""
        p = self.phases
        return (
            across * (0.5 + 0.3 * math.sin(0.21 * t + p[0]) + 0.07 * math.sin(0.53 * t + p[1])),
            down * (0.4 + 0.17 * math.sin(0.17 * t + p[2]) + 0.06 * math.sin(0.61 * t + p[3])),
        )  # fmt: skip

    def reach(self, across: float, down: float) -> float:
        """Half the length of the flock at rest, in dots."""
        return self.size * min(0.18 * across, 0.36 * down)

    def dance(self, w: int, h: int, t: float) -> list[Dot]:
        """Where each bird is at `t` seconds over a sky of `w` by `h`
        cells, in dots: `count(w, h)` of them, the first of one list
        at every size."""
        n = self.count(w, h)
        while len(self.birds) < n:
            self.birds.append(self._bird())
        across, down = 2.0 * w, 4.0 * h
        p = self.phases
        cx, cy = self.middle(across, down, t)
        ax, ay = self.middle(across, down, t + 0.5)
        vx, vy = (ax - cx) / 0.5, (ay - cy) / 0.5
        # Along its way; level, and the other way round, in a turn.
        slow = 0.02 * across
        way = math.tanh(vx / slow)
        turn = math.atan2(vy * way, abs(vx) + slow)
        # Long and thin when fast, a ball in a turn.
        fast = min(1.0, math.hypot(vx, vy) / (0.06 * across))
        pull = 0.6 + 0.5 * fast + 0.12 * math.sin(0.37 * t + p[4])
        reach = self.reach(across, down)
        long, wide, thick = reach * pull, 0.58 * reach / math.sqrt(pull), 0.07 * reach
        roll = 0.33 * t + 1.2 * math.sin(0.13 * t + p[5])
        twist = 2.3 * math.sin(0.19 * t + p[6])
        bend = 0.3 * reach * math.sin(0.23 * t + p[7])
        sway = 0.7 * t + p[8]
        ca, sa = math.cos(turn), math.sin(turn)
        sin, cos = math.sin, math.cos
        out = []
        for rho, phi, spin, q, beat, _, _ in self.birds[:n]:
            a = phi + spin * t
            s = rho * cos(a)
            b = rho * sin(a)
            # A wave of crowding runs down the flock.
            s += 0.09 * sin(5.0 * s - 2.3 * t)
            angle = roll + twist * s
            x = long * s
            y = (
                wide * (0.75 + 0.25 * way * s) * b * cos(angle)
                - thick * q * sin(angle)
                + bend * sin(2.0 * s + sway)
            )
            out.append(
                (
                    cx + x * ca - y * sa + 0.6 * sin(3.1 * t + beat),
                    cy + x * sa + y * ca + 0.6 * cos(2.3 * t + beat),
                )
            )
        return out


def hawk(flock: Flock, across: float, down: float, t: float) -> list[Dot]:
    """The hawk's dots at `t` seconds, the last where it is, the
    others where it was, 0.2 s apart; none between two stoops. It
    stoops through the middle of `flock`, every `HAWK[0]` seconds."""
    every, long = HAWK
    slot = math.floor(t / every)
    key = slot + 7 * int(flock.phases[9] * 1000)
    begin = slot * every + (every - long) * (0.2 + 0.6 * noise(key))
    if not begin <= t < begin + long + 1.0:
        return []
    mx, my = flock.middle(across, down, begin + long / 2)
    lean = (noise(key, 1) - 0.5) * 1.8
    dx, dy = math.sin(lean), math.cos(lean)
    span = 2.6 * flock.reach(across, down)
    out = []
    for back in (1.0, 0.8, 0.6, 0.4, 0.2, 0.0):
        f = (t - back - begin) / long
        if 0.0 <= f <= 1.0:
            out.append((mx + dx * span * (f - 0.5), my + dy * span * (f - 0.5)))
    return out


def perches(rows: Sequence[str], w: int, h: int) -> list[tuple[int, int]]:
    """Dots to perch on: two in five of the lowest of the free cells
    over the glyphs of `rows`, in a view of `w` by `h`."""
    walls = walls_of(rows)
    shown = clear(rows, w, h)
    out = []
    for x, y in sorted(walls):
        if 0 < y < h and x < w and shown[y - 1] >> x & 1:
            out += [(2 * x + i, 4 * y - 1) for i in (0, 1) if noise(2 * x + i, y) < 0.4]
    return out


class Land:
    """The land under the sky: a line of low hills. `water`: a lake
    before a far shore, the sun's path of light on it. `reeds`: reeds
    in the breeze, in front."""

    def __init__(self, rng: random.Random, water: bool = False, reeds: bool = False):
        self.seed = rng.getrandbits(24)
        self.water, self.reeds = water, reeds
        self.rng = random.Random(rng.getrandbits(32))
        #: Per dot column: height of the reed, phase of its flutter.
        self.blades: list[tuple[int, float]] = []

    def rows(self, h: int) -> int:
        """Rows of cells the land takes in a view `h` high: a row of
        hills, two in a high view; 3 with reeds; with water the far
        shore and a seventh of the view, 3 rows at least."""
        if self.water:
            return min(h, 1 + max(3, h // 7))
        if self.reeds:
            return min(h, 3)
        return min(h, 1 if h < 40 else 2)

    def horizon(self, h: int) -> int:
        """The dot row the sun sets on."""
        return 4 * (h - self.rows(h) + 1) - 2 if self.water else 4 * h - 2

    def top(self, px: int, h: int) -> int:
        """The dot row of the skyline at the dot column `px`, in a
        view `h` cells high."""
        hill = _wave(px / 23.0, self.seed) + 0.5 * _wave(px / 7.0, self.seed + 1)
        if self.water:
            return 4 * (h - self.rows(h) + 1) - 1 - int(1.6 * hill)
        return 4 * h - 1 - int(2.6 * hill * (2 if h >= 40 else 1))

    def _blade(self, px: int) -> tuple[int, float]:
        rng = self.rng
        # In clumps.
        clump = _wave(px / 17.0, self.seed + 2)
        if rng.random() > 1.5 * clump - 0.5:
            return (0, 0.0)
        return (2 + int(rng.random() ** 1.5 * 10), rng.uniform(0.0, math.tau))

    def picture(
        self,
        w: int,
        h: int,
        t: float = 0.0,
        wind: float = 1.0,
        sun: tuple[float, float, int] | None = None,
        f: float = START,
    ) -> Grid:
        """The land of a view of `w` by `h` cells at `t` seconds,
        `rows(h)` rows. `sun`: its dot column, its radius, its colour."""
        rows = self.rows(h)
        grid: Grid = [{} for _ in range(rows)]
        if w <= 0:
            return grid
        g = glow(f)
        off = 4 * (h - rows)
        base = 4 if self.water else 4 * rows
        soil: dict[tuple[int, int], int] = {}
        for px in range(2 * w):
            for py in range(max(0, self.top(px, h) - off), base):
                cell = (px >> 1, py >> 2)
                soil[cell] = soil.get(cell, 0) | DOTS[py & 3][px & 1]
        for (x, y), bits in soil.items():
            warm = sun is not None and g > 0.3 and abs(2 * x - sun[0]) < 5 * sun[1]
            grid[y][x] = (chr(0x2800 + bits), SOIL[warm])
        if self.water:
            self._lake(grid, w, t, sun, f)
        if self.reeds:
            self._bank(grid, w, t, wind)
        return grid

    def _lake(
        self,
        grid: Grid,
        w: int,
        t: float,
        sun: tuple[float, float, int] | None,
        f: float,
    ) -> None:
        a, g = height(f), glow(f)
        shine = 1.0 - _ease((-0.04 - a) / 0.14)
        deep = 4 * (len(grid) - 1)
        lit: dict[tuple[int, int], int] = {}
        dim: dict[tuple[int, int], int] = {}
        for j in range(deep):
            py = 4 + j
            far = j / deep
            if sun is not None and shine > 0:
                # The path of light: wider and broken towards the bank.
                sx, r, _ = sun
                half = r * (0.6 + 1.5 * far)
                beat = int(3.0 * t + 8 * noise(j, self.seed))
                for px in range(max(0, int(sx - half)), min(2 * w, int(sx + half) + 1)):
                    near = 1.0 - abs(px + 0.5 - sx) / half
                    odds = shine * (0.25 + 0.75 * near) * (1.0 - 0.55 * far)
                    if noise(px // 2 + 977 * j, beat) < odds:
                        cell = (px >> 1, py >> 2)
                        lit[cell] = lit.get(cell, 0) | DOTS[py & 3][px & 1]
            # Ripples: dashes that slide, slower far away.
            slide = int(t * (0.5 + 1.5 * far))
            for px in range(2 * w):
                if noise((px - slide) // 3 + 613 * j, self.seed) < 0.03 + 0.04 * g:
                    cell = (px >> 1, py >> 2)
                    dim[cell] = dim.get(cell, 0) | DOTS[py & 3][px & 1]
        for (x, y), bits in dim.items():
            grid[y][x] = (chr(0x2800 + bits), WATER[int(3 * noise(x, y + 40))])
        for (x, y), bits in lit.items():
            assert sun is not None
            grid[y][x] = (chr(0x2800 + bits), sun[2])

    def _bank(self, grid: Grid, w: int, t: float, wind: float) -> None:
        rows = len(grid)
        while len(self.blades) < 2 * w:
            self.blades.append(self._blade(len(self.blades)))
        cells: dict[tuple[int, int], int] = {}
        for i, (tall, phase) in enumerate(self.blades[: 2 * w]):
            if not tall:
                continue
            lean = breeze(i, t, wind) * tall / 5
            if wind:
                lean += 0.3 * math.sin(1.7 * t + phase)
            for j in range(tall):
                px = i + round(lean * (j / tall) ** 1.6)
                py = 4 * rows - 1 - j
                if 0 <= px < 2 * w and py >= 0:
                    cell = (px >> 1, py >> 2)
                    cells[cell] = cells.get(cell, 0) | DOTS[py & 3][px & 1]
        for (x, y), bits in cells.items():
            # In front of the water; one with the land.
            old = grid[y].get(x)
            if old is not None and old[1] in SOIL:
                bits |= ord(old[0]) - 0x2800
            grid[y][x] = (chr(0x2800 + bits), REEDS[int(4 * noise(x, y + 90))])


class Scene:
    """The scene and its options: what to draw at a time."""

    def __init__(self, args: argparse.Namespace, rng: random.Random) -> None:
        self.args = args
        self.dusk = Dusk(rng, args.clouds)
        self.land = Land(rng, args.water, args.reeds)
        self.flocks = [Flock(rng, args.birds)]
        if args.double:
            self.flocks.append(Flock(rng, args.birds * SECOND[0], SECOND[1]))
        self.sky = Sky(rng, 0.0) if args.night else None
        self.home = rng.uniform(0.62, 0.9)  # where they roost, across

    def stage(self, t: float) -> float:
        return stage(t, self.args.dusk, self.args.at)

    def rows(self, h: int) -> int:
        """Rows of the land in a view `h` high."""
        return self.land.rows(h)

    def radius(self, w: int, h: int) -> float:
        """The sun's, in dots: it grows with the view."""
        return self.args.sun * min(3.0, max(0.7, math.sqrt(w * h / AREA)))

    def sun(self, w: int, h: int, t: float) -> tuple[float, float, int]:
        """The sun: its middle in dots, its colour."""
        f = self.stage(t)
        cx, cy = sun_at(w, self.land.horizon(h), self.radius(w, h), f)
        a = max(0.0, height(f))
        return cx, cy, SUN[min(len(SUN) - 1, int((1.0 - a) ** 1.5 * len(SUN)))]

    def above(self, w: int, h: int, t: float) -> Grid:
        """The sky: stars, the glow, clouds; `h` rows."""
        f = self.stage(t)
        over = max(0, h - self.rows(h))
        if self.sky is not None:
            a = height(f)
            dark = _ease((0.3 - a) / 0.5) * (1.0 - _ease((f - 1.0) / REST))
            self.sky.density = 60.0 * dark
            grid = self.sky.picture(w, over, t, twinkle=True)
        else:
            grid = [{} for _ in range(over)]
        low = self.dusk.picture(w, over, f, self.sun(w, h, t)[:2], t)
        for row, light in zip(grid, low, strict=True):
            row.update(light)
        return grid + [{} for _ in range(h - over)]

    def birds(
        self, w: int, h: int, t: float, roosts: Sequence[tuple[int, int]] = ()
    ) -> tuple[list[Dot], list[Dot]]:
        """The birds in the air, then the hawk, in dots. `roosts`:
        dots to perch on at night; a bird without one goes into the
        land, and is not seen there."""
        args = self.args
        f = self.stage(t)
        over = max(1, h - self.rows(h))
        across, down = 2.0 * w, 4.0 * over
        out: list[Dot] = []
        stoop = hawk(self.flocks[0], across, down, t) if args.hawk else []
        if stoop and not JOIN[0] < f < ROOST[0]:
            stoop = []
        fear = 0.55 * self.flocks[0].reach(across, down)
        at = 0
        for flock in self.flocks:
            for bird, (x, y) in zip(flock.birds, flock.dance(w, over, t), strict=False):
                i, at = at, at + 1
                # Down: to roost; then up again.
                if f >= ROOST[0]:
                    sunk = (f - ROOST[0] - ROOST[1] * bird.order) / ROOST[2]
                elif args.join and bird.group:
                    sunk = 0.0
                else:
                    sunk = 1.0 - (f - RISE[0] - RISE[1] * bird.order) / RISE[2]
                if sunk > 0:
                    if i < len(roosts):
                        px, py = roosts[i]
                    elif sunk >= 1:
                        continue
                    else:
                        spread = (noise(i, 5) - 0.5) * 0.12 * across
                        px = int(self.home * across + spread) % max(1, 2 * w)
                        py = self.land.top(px, h)
                    u = min(1.0, sunk)
                    # A funnel: over the roost first, then down.
                    x += (px - x) * _ease(2.0 * u)
                    y += (py - y) * _ease(u) ** 1.6
                elif args.join and bird.group:
                    # In with its group, from a side.
                    came = JOIN[0] + (JOIN[1] - JOIN[0]) * noise(bird.group, 3)
                    u = (f - came) / JOIN[2] + 1.0 - 0.25 * bird.order
                    if u <= 0:
                        continue
                    if u < 1:
                        side = -0.1 if bird.group % 2 else 1.1
                        ex = side * across + 12 * math.cos(bird.phi) * bird.rho
                        ey = down * (0.15 + 0.4 * noise(bird.group, 4))
                        ey += 6 * math.sin(bird.phi) * bird.rho
                        x = ex + (x - ex) * _ease(u)
                        y = ey + (y - ey) * _ease(u)
                for k, (hx, hy) in enumerate(stoop):
                    # Away from the hawk, and from where it just was.
                    dx, dy = x - hx, y - hy
                    d = math.hypot(dx, dy) or 1.0
                    reach = fear * (0.4 + 0.12 * k) if k < len(stoop) - 1 else fear
                    if d < reach:
                        # Some flee further than others.
                        push = reach * (1.0 - d / reach) ** 1.5
                        push *= 0.45 + 0.9 * noise(i, 9)
                        x += dx / d * push
                        y += dy / d * push
                out.append((x, y))
        return out, stoop[-1:]

    def fore(
        self, w: int, h: int, t: float, roosts: Sequence[tuple[int, int]] = ()
    ) -> Grid:
        """The sun and the birds before it, `h` rows."""
        grid: Grid = [{} for _ in range(h)]
        if w <= 0 or h <= 0:
            return grid
        f = self.stage(t)
        cx, cy, colour = self.sun(w, h, t)
        r = self.radius(w, h)
        disc = _disc(cx, cy, r, self.land, h) if r > 0 else {}
        flown, stoop = self.birds(w, h, t, roosts)
        cells: dict[tuple[int, int], list[int]] = {}
        for x, y in flown:
            px, py = math.floor(x), math.floor(y)
            if not (0 <= px < 2 * w and 0 <= py < 4 * h):
                continue
            cell = (px >> 1, py >> 2)
            if cell in disc:
                # Before the sun: a shadow, two dots wide.
                disc[cell] &= ~(DOTS[py & 3][0] | DOTS[py & 3][1])
                continue
            one = cells.setdefault(cell, [0, 0])
            one[0] |= DOTS[py & 3][px & 1]
            one[1] += 1
        dim = round(1.2 * _ease((-0.05 - height(f)) / 0.3))
        for (x, y), (bits, n) in cells.items():
            ink = BIRDS[max(0, min(3, n - 1) - dim)]
            grid[y][x] = (chr(0x2800 + bits), ink)
        for (x, y), bits in disc.items():
            grid[y][x] = (chr(0x2800 + bits) if bits else " ", colour)
        for x, y in stoop:
            # Wings swept back.
            for ox, oy in HAWK_DOTS:
                px, py = math.floor(x) + ox, math.floor(y) + oy
                if 0 <= px < 2 * w and 0 <= py < 4 * h:
                    glyph, ink = grid[py >> 2].get(px >> 1, ("⠀", PREY))
                    bits = ord(glyph) - 0x2800 if ink == PREY else 0
                    bits |= DOTS[py & 3][px & 1]
                    grid[py >> 2][px >> 1] = (chr(0x2800 + bits), PREY)
        return grid

    def below(self, w: int, h: int, t: float) -> Grid:
        """The land, `rows(h)` rows."""
        cx, _, colour = self.sun(w, h, t)
        sun = (cx, self.radius(w, h), colour) if self.args.sun > 0 else None
        return self.land.picture(w, h, t, self.args.wind, sun, self.stage(t))


def show(args: argparse.Namespace) -> int:
    """`--print`: the scene once on stdout, no driver."""
    w, h = args.print
    scene = Scene(args, random.Random(args.seed))
    grid = scene.above(w, h, 0.0)
    for row, fore in zip(grid, scene.fore(w, h, 0.0), strict=True):
        row.update(fore)
    low = scene.below(w, h, 0.0)
    for row, land in zip(grid[max(0, h - len(low)) :], low[-h:], strict=False):
        row.update(land)
    for line in runs(grid, w):
        text = "".join(
            f"\x1b[38;5;{run['fg']}m{run['t']}\x1b[0m" if "fg" in run else run["t"]
            for run in line
        )
        sys.stdout.write(text + "\n")
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    scene = Scene(args, random.Random(args.seed))
    frame = _cli.pick(pilot, args.frame)
    sight = _cli.Sight(pilot, frame)
    size = (0, 0)
    shown: list[int] = []
    roosts: list[tuple[int, int]] = []
    sprites: list[Sprite | None] = [None, None, None]  # sky, birds, land
    sent: list[Any] = [None, None, None]
    start = clock.monotonic()

    def put(i: int, cells: Any, y: int, moved: bool) -> None:
        sprite = sprites[i]
        if sprite is None:
            sprites[i] = pilot.draw(
                cells=cells, frame=frame.id, at=(0, y), space="view", z=i
            )
        elif moved:
            sprite.update(
                anchor={"at": [0, y], "space": "view"}, content={"cells": cells}
            )
        elif cells != sent[i]:
            sprite.update(content={"cells": cells})
        sent[i] = cells

    for tick, _ in enumerate(_cli.ticks(PERIOD, seconds)):
        if not sight.look():
            continue
        w, h = frame.size
        t = clock.monotonic() - start
        rows = scene.rows(h)
        sized = (w, h) != size
        if sized or sight.changed:
            size = (w, h)
            shown = clear(sight.rows, w, h)
            if args.roost:
                roosts = perches(sight.rows, w, h)
                random.Random(args.seed).shuffle(roosts)
        with sight.drawing(), pilot.batch():
            if sized or sight.changed or tick % SKY == 0:
                put(0, runs(scene.above(w, h, t), w, shown), 0, False)
            put(1, runs(scene.fore(w, h, t, roosts), w, shown), 0, False)
            if sized or sight.changed or tick % LAND == 0:
                low = scene.below(w, h, t)[-rows:]
                put(2, runs(low, w, shown[h - rows :]), h - rows, sized)


def _size(text: str) -> tuple[int, int]:
    try:
        w, h = (int(n) for n in text.lower().split("x"))
    except ValueError:
        raise argparse.ArgumentTypeError(f"not a size: {text}") from None
    if w < 1 or h < 1:
        raise argparse.ArgumentTypeError(f"not a size: {text}")
    return w, h


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "flock",
        "Starlings dance before a sunset, behind the text.",
        frame="largest",
    )
    out.add_argument(
        "--birds",
        type=float,
        default=260.0,
        metavar="N",
        help="birds over 80 by 24 cells [default: 260]",
    )
    out.add_argument(
        "--dusk",
        type=float,
        default=300.0,
        metavar="SECONDS",
        help="the time the sun takes to go down [default: 300]",
    )
    out.add_argument(
        "--at",
        type=float,
        default=START,
        metavar="F",
        help="share of the dusk gone at the start: 0 the sun over the "
        f"view, {SET} it sets, 1 the night [default: {START}]",
    )
    out.add_argument(
        "--sun",
        type=float,
        default=7.0,
        metavar="DOTS",
        help="radius of the sun at 80 by 24 cells, 2 dots a column; 0: no sun [default: 7]",
    )
    out.add_argument(
        "--wind",
        type=float,
        default=1.0,
        help="strength of the breeze in the reeds, < 0 to the left [default: 1]",
    )
    out.add_argument("--seed", type=int, help="same seed, same dusk")
    out.add_argument(
        "-c", "--clouds", action="store_true", help="bars of cloud, lit from under"
    )
    out.add_argument(
        "-w", "--water", action="store_true", help="a lake, the sun's path on it"
    )
    out.add_argument(
        "-r", "--reeds", action="store_true", help="reeds in the breeze, in front"
    )
    out.add_argument(
        "-j", "--join", action="store_true", help="groups come in and join the flock"
    )
    out.add_argument(
        "-d", "--double", action="store_true", help="a second, smaller flock"
    )
    out.add_argument(
        "-k", "--hawk", action="store_true", help="a hawk stoops, the flock parts"
    )
    out.add_argument(
        "-n", "--night", action="store_true", help="the stars come out as it sets"
    )
    out.add_argument(
        "-o", "--roost", action="store_true", help="the birds roost on the text"
    )
    out.add_argument("-a", "--all", action="store_true", help="-c -w -r -j -d -k -n -o")
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(80, 24),
        default=None,
        metavar="WxH",
        help="no driver: print the scene once, 80x24 or the size given",
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.clouds = args.water = args.reeds = args.join = True
        args.double = args.hawk = args.night = args.roost = True
    if args.dusk <= 0:
        out.error("--dusk must be positive")
    if args.birds < 0 or args.sun < 0:
        out.error("--birds and --sun must not be negative")
    args.at %= 1.0 + REST
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
