# SPDX-License-Identifier: GPL-3.0-or-later
"""Flock: starlings dance before a sunset, behind the text.

    ptyrc-app flock                 # the sun goes down, a flock dances
    ptyrc-app flock -c -w -r        # clouds, a lake, reeds in a breeze
    ptyrc-app flock -j -d -k        # groups join, a second flock, a hawk
    ptyrc-app flock -n -o           # stars come out, the birds roost on the text
    ptyrc-app flock --all
    ptyrc-app flock --print 80x24   # no driver: the scene on stdout

A bird is a braille dot, on its own wings (`Flock`): it flies with
its neighbours, after a lead that wanders. A few stray; now and then
a part of the flock goes its own way and comes back. A cell is the
brighter the more birds it holds.

What it shows:

- A sky and a land that are functions of the time: `Dusk.picture`,
  `Land.picture` keep no state. The birds are a state, stepped to
  the time asked; the app sends what changed.
- A dusk, `--dusk` seconds: the sun comes down from over the view,
  sets at `SET` of it; the birds then glide down, part after part, to
  roosts all along the land; a night of `REST` of it, and the dusk
  comes again, the birds take off from there.
- The host pointer over the pane (topic `hover`): the birds flee it.
- `-k`: a hawk stoops at the thick of the flock, which bursts apart
  and gathers again.
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
from collections.abc import Callable, Sequence
from typing import Any, NamedTuple

from ptyrc import Pilot, PtyrcError, Sprite, clock

from . import _cli
from .life import DOTS
from .snake import walls_of
from .stars import AREA, Grid, Sky, breeze, clear, noise, runs

__all__ = [
    "REST",
    "SET",
    "Air",
    "Bird",
    "Dusk",
    "Flock",
    "Land",
    "Scene",
    "glow",
    "height",
    "main",
    "perches",
    "show",
    "stage",
    "sun_at",
]

Dot = tuple[float, float]
Vec = tuple[float, float, float]
Scare = tuple[float, float, float]  # a dot, and how much: 0 to 1

PERIOD = 1 / 10  # seconds a tick: the birds
DT = PERIOD  # seconds a step of the birds
WARM = 10  # steps a flock just made has flown
LEAP = 50  # steps at most from a call to the next
SKY = 10  # ticks between two looks at the sky
LAND = 2  # and at the land

SET = 0.8  # share of a dusk at which the sun is down
REST = 0.1  # the night, in dusks
START = 0.3  # share of a dusk gone at the start
ROOST = (0.86, 0.08, 0.04)  # the first bird goes down, the last, a flight
RISE = (0.0, 0.045)  # the first bird goes up, the last
SITES = 9  # places along the land they roost at
JOIN = (0.08, 0.62, 0.035)  # the first group comes in, the last, a flight
CORE = 0.3  # share of the birds that roost here, with `-j`
SECOND = 0.35  # the second flock: its birds, over the first
HAWK = (90.0, 14.0)  # seconds between two hunts, and one at most
SPLIT = (50.0, 0.6)  # seconds between two splits; how often there is one
STRAY = (8.0, 0.05)  # seconds a bird strays; the share that does
LEADS = (1.0, 1.13, 0.89)  # the leads of the tribes: how fast each goes
LAGS = (6, 0.6)  # ranks in a tribe; seconds a rank is behind the one before
STILL = 0.35  # how much a pointer that rests scares, of 1
CALM = 4.0  # seconds it takes to
#: The flight. Lengths in `Flock.reach`, speeds in `PACE`.
PACE = 0.55  # reaches a second
HOOD = 0.24  # the side of a cell of neighbours
PACK = 0.08  # share of the birds in 27 cells at which they are at ease
ALIGN = 1.2  # a second: how fast a bird takes its neighbours' heading
COHERE = 1.0  # speeds a second, a cell from the middle of its neighbours
PART = 2.0  # the most it flees them, over that
LEAD = 0.9  # speeds a second, towards its lead
SLACK = 0.45  # from its lead, free
ROAM = 2.0  # the same, for a stray
WANDER = 0.35  # speeds a second, its own mind
WALL = 3.0  # speeds a second, a reach out of the view
LEVEL = 0.5  # a second: how fast it comes level
FEAR = 4.5  # speeds a second, on what scares it
DASH = 0.9  # speeds more, there
SHY = 0.9  # how far the pointer scares
WARY = 0.85  # and the hawk
STEEP = 0.5  # the slope under which a bird comes down
DOWN, UP, IN = 0, 1, 2  # a bird: on its roost, in the air, coming in

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
    """A bird: `beat`, the phase of its wander; `pace`, its speed over
    the flock's; `order`: 0 the first of its lot up, 1 the last;
    `group`: the one it comes in with, 0 the core; `clan`: the one it
    roosts with; `tribe`: the lead it follows, 1 the second flock;
    `rank`: how far behind its lead, 0 the first."""

    beat: float
    pace: float
    order: float
    group: int
    clan: int
    tribe: int
    rank: int


class Air(NamedTuple):
    """Where the birds fly: a view `w` cells wide, `over` rows of sky.
    `stage(t)`: the stage at `t` seconds; `dusk`: seconds a dusk;
    `ground(px)`: the dot row of the skyline; `roosts`: dots to perch
    on, the bird `i` on the `i`th."""

    w: int
    over: int
    stage: Callable[[float], float]
    dusk: float
    ground: Callable[[int], int]
    roosts: Sequence[tuple[int, int]] = ()


class Flock:
    """Birds, each on its own wings, stepped `DT` by `DT`.

    A bird flies at about one speed and turns: with its neighbours
    (their mean heading), towards them when few, away when many,
    after the lead of its tribe (`lead`) when far from it, away from
    what scares it. Neighbours are those of the 27 cells around its
    own, `HOOD` wide: a step is linear in the birds.

    `birds`: over 80 by 24 cells; `second`: birds of a second flock,
    over the first; `join`: groups come in from the sides; `hawk`: one
    hunts now and then.

    `fly(air, t)` steps to `t`. A time before the last one, or more
    than `LEAP` steps after: the flock starts again `WARM` steps
    before, about its leads; so a flock just made is a function of `t`.
    """

    def __init__(
        self,
        rng: random.Random,
        birds: float = 130.0,
        second: float = 0.0,
        join: bool = False,
        hawk: bool = False,
    ) -> None:
        self.rng = random.Random(rng.getrandbits(32))
        self.seed = rng.getrandbits(20)
        self.density = birds * (1.0 + second)
        self.second = second / (1.0 + second)
        self.join, self.hunted = join, hawk
        self.phases = [[rng.uniform(0.0, math.tau) for _ in range(5)] for _ in LEADS]
        self.birds: list[Bird] = []
        self.k: int | None = None  # the step it is at
        self.f = 0.0  # the stage there
        self.size = (0, 0)
        #: Per bird: where it is, its speed, in dots and dots a second.
        self.x: list[float] = []
        self.y: list[float] = []
        self.z: list[float] = []
        self.vx: list[float] = []
        self.vy: list[float] = []
        self.vz: list[float] = []
        self.state: list[int] = []  # DOWN, UP, IN
        self.tribe: list[int] = []  # the lead it follows now
        self.up: list[float] = []  # the stage it takes off at
        self.down: list[float] = []  # and heads for its roost at
        self.site: list[float] = []  # the dot column it roosts by
        self.stray: list[bool] = []
        self.rift: int | None = None  # the split there is
        self.bed = False  # the roosts are dealt
        #: The hawk: x, y, heading, its aim, 0 in a stoop, born, passes
        #: left, since when.
        self.hawk: list[float] | None = None

    def count(self, w: int, h: int) -> int:
        return int(self.density * w * h / AREA)

    def _bird(self) -> Bird:
        rng = self.rng
        return Bird(
            rng.uniform(0.0, math.tau),
            rng.uniform(0.9, 1.12),
            rng.random(),
            rng.randrange(1, 9) if rng.random() >= CORE else 0,
            rng.randrange(SITES),
            int(rng.random() < self.second),
            rng.randrange(LAGS[0]),
        )

    def reach(self, across: float, down: float) -> float:
        """The unit of the flight, in dots: a flock at ease is about
        three long."""
        return max(1.0, min(0.18 * across, 0.36 * down))

    def lead(self, tribe: int, across: float, down: float, t: float) -> Vec:
        """Where the birds of `tribe` are drawn to at `t` seconds, in
        dots; the third, into the depth."""
        p = self.phases[tribe]
        t *= LEADS[tribe]
        return (
            across * (0.5 + 0.3 * math.sin(0.27 * t + p[0]) + 0.08 * math.sin(0.61 * t + p[1])),
            down * (0.4 + 0.17 * math.sin(0.19 * t + p[2]) + 0.06 * math.sin(0.67 * t + p[3])),
            0.5 * self.reach(across, down) * math.sin(0.23 * t + p[4]),
        )  # fmt: skip

    def home(self, site: int, across: float) -> float:
        """The dot column of the roost `site`."""
        return across * (site + 0.2 + 0.6 * noise(site, self.seed)) / SITES

    def _up(self, bird: Bird) -> float:
        if self.join and bird.group:
            came = JOIN[0] + (JOIN[1] - JOIN[0]) * noise(bird.group, 3)
            return came + 0.25 * JOIN[2] * bird.order
        # A clan after the other.
        turn = (4 * bird.clan + self.seed) % SITES
        return RISE[0] + RISE[1] * (turn + 3.0 * bird.order) / (SITES + 2)

    def _fit(self, air: Air) -> None:
        """The birds of this view: the first of one list."""
        n = self.count(air.w, air.over)
        size = (air.w, air.over)
        if size != self.size and self.x:
            kx = air.w / self.size[0] if self.size[0] else 1.0
            ky = air.over / self.size[1] if self.size[1] else 1.0
            self.x = [x * kx for x in self.x]
            self.y = [y * ky for y in self.y]
            self.z = [z * kx for z in self.z]
            self.site = [x * kx for x in self.site]
        self.size = size
        while len(self.birds) < n:
            self.birds.append(self._bird())
        for row in (self.x, self.y, self.z, self.vx, self.vy, self.vz):
            row[n:] = []
            row += [0.0] * (n - len(row))
        for i in range(len(self.state), n):
            # On the ground: up at the next step, if it is its time.
            bird = self.birds[i]
            self.state.append(DOWN)
            self.tribe.append(bird.tribe)
            self.up.append(self._up(bird))
            self.down.append(ROOST[0] + ROOST[1] * bird.order)
            self.site.append(
                self.home(bird.clan, 2.0 * air.w) + (noise(i, 5) - 0.5) * 0.3 * air.w
            )
            self.stray.append(False)
        for row in (self.state, self.tribe, self.up, self.down, self.site, self.stray):
            row[n:] = []

    def _start(self, k: int, air: Air) -> None:
        """All anew at the step `k`: in the air about their leads, or
        down."""
        for row in (self.x, self.state, self.tribe, self.up, self.down, self.site):
            row.clear()
        self.stray.clear()
        self._fit(air)
        self.k, self.rift, self.bed, self.hawk = k, None, False, None
        t = k * DT
        self.f = f = air.stage(t)
        across, down = 2.0 * air.w, 4.0 * air.over
        u = self.reach(across, down)
        rng = random.Random(self.seed + 7919 * k)
        for i, bird in enumerate(self.birds[: len(self.x)]):
            if not self.up[i] <= f < self.down[i] + 0.5 * ROOST[2]:
                continue
            lx, ly, lz = self.lead(bird.tribe, across, down, t)
            ax, ay, az = self.lead(bird.tribe, across, down, t + 0.5)
            dx, dy, dz = ax - lx, ay - ly, az - lz
            d = math.sqrt(dx * dx + dy * dy + dz * dz)
            if d < 1e-6:
                dx, d = 1.0, 1.0
            v = PACE * u / d
            self.state[i] = UP
            self.x[i] = lx + rng.gauss(0.0, 0.55 * u)
            self.y[i] = ly + rng.gauss(0.0, 0.28 * u)
            self.z[i] = lz + rng.gauss(0.0, 0.35 * u)
            self.vx[i], self.vy[i], self.vz[i] = v * dx, v * dy, v * dz

    def spot(self, i: int, air: Air) -> Dot:
        """Where the bird `i` roosts: its perch, or on the land."""
        if i < len(air.roosts):
            x, y = air.roosts[i]
            return float(x), float(y)
        px = min(max(0, 2 * air.w - 1), max(0, int(self.site[i])))
        return float(px), float(air.ground(px))

    def _launch(self, i: int, air: Air, lead: Sequence[Vec], u: float) -> None:
        """Up from its roost, low and along; or in from a side."""
        bird = self.birds[i]
        across, down = 2.0 * air.w, 4.0 * air.over
        v = PACE * u
        if self.join and bird.group:
            left = bird.group % 2
            x = across * (-0.08 if left else 1.08) + (noise(i, 11) - 0.5) * 0.8 * u
            y = (
                down * (0.15 + 0.4 * noise(bird.group, 4))
                + (noise(i, 12) - 0.5) * 0.5 * u
            )
            vx, vy = (v if left else -v), 0.0
        else:  # fmt: skip
            x, y = self.spot(i, air)
            way = 1.0 if lead[bird.tribe][0] > x else -1.0
            climb = math.radians(6.0 + 22.0 * noise(i, 13))
            vx, vy = way * v * math.cos(climb), -v * math.sin(climb)
        self.state[i] = UP
        self.tribe[i] = bird.tribe
        self.x[i], self.y[i], self.z[i] = x, y, (noise(i, 14) - 0.5) * 0.8 * u
        self.vx[i], self.vy[i], self.vz[i] = vx, vy, 0.0

    def _deal(self, air: Air) -> None:
        """The roosts: the birds in the air, from left to right, go to
        the sites from left to right, a site after the other."""
        across = 2.0 * air.w
        up = sorted(
            (i for i, s in enumerate(self.state) if s == UP), key=self.x.__getitem__
        )
        for rank, i in enumerate(up):
            site = rank * SITES // len(up)
            turn = (4 * site + self.seed) % SITES
            self.down[i] = ROOST[0] + ROOST[1] * (turn + 1.2 * noise(i, 7)) / (SITES + 0.2)  # fmt: skip
            self.site[i] = self.home(site, across) + (noise(i, 5) - 0.5) * 0.14 * across  # fmt: skip
        self.bed = True

    def _split(self, t: float) -> None:
        """Now and then a side of the main flock goes its own way, for
        a while."""
        slot = math.floor(t / SPLIT[0])
        seed = self.seed
        on = noise(slot, seed + 1) < SPLIT[1] and 0.2 <= t / SPLIT[0] - slot < 0.75
        if not on:
            if self.rift is not None:
                self.rift = None
                self.tribe = [bird.tribe for bird in self.birds[: len(self.x)]]
            return
        if self.rift == slot:
            return
        self.rift = slot
        main = [i for i, s in enumerate(self.state) if s == UP and self.tribe[i] == 0]
        if len(main) < 8:
            return
        # Cut across a bearing: those the furthest along it.
        lean = math.tau * noise(slot, seed + 2)
        ca, sa = math.cos(lean), math.sin(lean)
        main.sort(key=lambda i: self.x[i] * ca + self.y[i] * sa)
        share = 0.2 + 0.3 * noise(slot, seed + 3)
        to = 1 if noise(slot, seed + 4) < 0.5 else 2
        for i in main[int(len(main) * (1.0 - share)) :]:
            self.tribe[i] = to

    def _hunt(self, t: float, air: Air, thick: Dot | None, u: float) -> None:
        """The hawk: it comes from over the view, stoops at the thick
        of the flock, pulls up and goes; at times it comes round again."""
        across, down = 2.0 * air.w, 4.0 * air.over
        every, long = HAWK
        v = PACE * u
        hawk = self.hawk
        if hawk is None:
            slot = math.floor(t / every)
            seed = self.seed
            begin = slot * every + (every - long) * (0.2 + 0.6 * noise(slot, seed + 5))
            calm = JOIN[0] + 0.02 < self.f < ROOST[0] - 0.03
            if not self.hunted or thick is None or not calm or not t - DT < begin <= t:
                return
            side = 1.0 if noise(slot, seed + 6) < 0.5 else -1.0
            x = thick[0] + side * (0.5 + 0.8 * noise(slot, seed + 7)) * u
            y = -0.06 * down
            again = 2.0 if noise(slot, seed + 8) < 0.35 else 1.0
            way = math.atan2(thick[1] - y, thick[0] - x)
            self.hawk = [x, y, way, thick[0], thick[1], 0.0, t, again, t]
            return
        x, y, way, ax, ay, away, born, passes, since = hawk
        if thick is not None:
            ax += 0.2 * (thick[0] - ax)
            ay += 0.2 * (thick[1] - ay)
        if not away:
            want, rate, speed = math.atan2(ay - y, ax - x), 1.6, 2.2 * v
            if math.hypot(ax - x, ay - y) < 0.3 * u or t - since > 5.0:
                away, since, passes = 1.0, t, passes - 1.0
        else:
            want = math.atan2(-0.45, 1.0 if math.cos(way) >= 0 else -1.0)
            rate, speed = (3.0 if y > 0.8 * down else 1.1), 1.7 * v
            if passes > 0 and t - since > 1.6:
                away, since = 0.0, t
        turn = (want - way + math.pi) % math.tau - math.pi
        way += max(-rate * DT, min(rate * DT, turn))
        x += speed * math.cos(way) * DT
        y += speed * math.sin(way) * DT
        out = x < -0.1 * across or x > 1.1 * across or y < -0.1 * down
        if (away and out) or t - born > long:
            self.hawk = None
        else:
            self.hawk = [x, y, way, ax, ay, away, born, passes, since]

    def _step(self, air: Air, scare: Scare | None) -> None:
        assert self.k is not None
        self.k += 1
        t = self.k * DT
        self.f = f = air.stage(t)
        n = len(self.x)
        across, down = 2.0 * air.w, 4.0 * air.over
        if not n or across <= 0:
            return
        u = self.reach(across, down)
        v0 = PACE * u
        # A rank follows where the lead was: a tribe trails along its way.
        leads = [
            [self.lead(j, across, down, t - LAGS[1] * rank) for rank in range(LAGS[0])]
            for j in range(len(LEADS))
        ]
        X, Y, Z, VX, VY, VZ = self.x, self.y, self.z, self.vx, self.vy, self.vz
        state, tribe, ups, downs = self.state, self.tribe, self.up, self.down
        birds = self.birds

        # Who takes off, who heads for its roost.
        if f < ROOST[0]:
            self.bed = False
        elif not self.bed:
            self._deal(air)
        for i in range(n):
            s = state[i]
            if s == DOWN:
                if ups[i] <= f < downs[i]:
                    self._launch(i, air, [rank[0] for rank in leads], u)
            elif s == UP and f >= downs[i]:
                state[i] = IN
        self._split(t)
        tribe = self.tribe
        if self.k % 10 == 0 or len(self.stray) != n:
            seed = self.seed
            self.stray = [
                noise(i, seed + int((t + bird.beat) / STRAY[0])) < STRAY[1]
                for i, bird in enumerate(birds[:n])
            ]
        strays = self.stray

        # The neighbours, of its tribe: the birds by cell, then by 27 cells.
        inv = 1.0 / max(3.0, HOOD * u)
        cells: dict[tuple[int, int, int, int], list[float]] = {}
        keys: list[Any] = [None] * n
        for i in range(n):
            if state[i] != UP:
                continue
            x, y, z = X[i], Y[i], Z[i]
            key = (
                int(x * inv + 64.0),
                int(y * inv + 64.0),
                int(z * inv + 64.0),
                tribe[i],
            )
            cell = cells.get(key)
            if cell is None:
                cells[key] = [1.0, x, y, z, VX[i], VY[i], VZ[i]]
            else:
                cell[0] += 1.0
                cell[1] += x
                cell[2] += y
                cell[3] += z
                cell[4] += VX[i]
                cell[5] += VY[i]
                cell[6] += VZ[i]
            keys[i] = key
        hood: dict[tuple[int, int, int, int], tuple[float, ...]] = {}
        get = cells.get
        span = (-1, 0, 1)
        most, thick = 0.0, None
        for key in cells:
            a, b, c, own = key
            m = sx = sy = sz = su = sv = sw = 0.0
            for da in span:
                for db in span:
                    for dc in span:
                        cell = get((a + da, b + db, c + dc, own))
                        if cell is not None:
                            m += cell[0]
                            sx += cell[1]
                            sy += cell[2]
                            sz += cell[3]
                            su += cell[4]
                            sv += cell[5]
                            sw += cell[6]
            hood[key] = (m, sx / m, sy / m, sz / m, su / m, sv / m, sw / m)
            if m > most:
                most, thick = m, (sx / m, sy / m)

        self._hunt(t, air, thick, u)
        # What scares: where, how far, how much.
        threats: list[tuple[float, float, float, float]] = []
        if scare is not None:
            gain = 0.5 + 0.5 * scare[2]
            threats.append((scare[0], scare[1], SHY * u * gain, gain))
        if (hawk := self.hawk) is not None:
            ahead = 0.45 * 2.0 * v0
            threats.append((hawk[0], hawk[1], WARY * u, 1.0))
            threats.append(
                (
                    hawk[0] + ahead * math.cos(hawk[2]),
                    hawk[1] + ahead * math.sin(hawk[2]),
                    0.75 * WARY * u,
                    0.8,
                )
            )

        dt = DT
        full = PACK * n
        coh = COHERE * v0 * inv
        apart = -PART * coh
        few = max(3.0, 2.5 * full / 27)  # birds in a cell, at most
        wall = WALL * v0 / u
        pull_ = LEAD * v0
        x0, x1 = 0.04 * across, 0.96 * across
        y0, y1 = 0.08 * down, 0.78 * down
        zm = 0.8 * u
        lo, hi = 0.7 * v0, 1.45 * v0
        glide = 0.9 * v0
        seconds = air.dusk
        sqrt, sin, cos = math.sqrt, math.sin, math.cos
        for i in range(n):
            s = state[i]
            if s == DOWN:
                continue
            x, y, z, vx, vy, vz = X[i], Y[i], Z[i], VX[i], VY[i], VZ[i]
            if s == IN:
                # To its roost: straight when the way is not steep,
                # else on along and down, and back.
                tx, ty = self.spot(i, air)
                left = (downs[i] + ROOST[2] - f) * seconds
                dx, dy = tx - x, ty - y
                far = sqrt(dx * dx + dy * dy)
                speed = max(glide, far / max(left, dt))
                if left <= dt or far <= speed * dt + 0.5:
                    state[i], X[i], Y[i] = DOWN, tx, ty
                    continue
                if dy > STEEP * abs(dx) and left > 4.0 * dy / v0 + 1.0:
                    wx, wy = (0.94 if vx >= 0 else -0.94), 0.34
                else:
                    wx, wy = dx / far, dy / far
                rate = min(1.0, max(3.0 * dt, 2.0 * dt / left))
                vx += (wx * speed - vx) * rate
                vy += (wy * speed - vy) * rate
                vz *= 0.8
            else:
                bird = birds[i]
                m, cx, cy, cz, ux, uy, uz = hood[keys[i]]
                stray = strays[i]
                # With its neighbours.
                g = ALIGN * 0.3 if stray else ALIGN
                ax, ay, az = g * (ux - vx), g * (uy - vy), g * (uz - vz)
                # Towards them when few, away when many.
                g = coh * (1.0 - m / full)
                if g < apart:
                    g = apart
                elif stray and g > 0:
                    g = 0.0
                ax += g * (cx - x)
                ay += g * (cy - y)
                az += g * (cz - z)
                # Never wing to wing.
                own = cells[keys[i]]
                if own[0] > few:
                    g = apart * (own[0] - few) / own[0]
                    ax += g * (own[1] / own[0] - x)
                    ay += g * (own[2] / own[0] - y)
                    az += g * (own[3] / own[0] - z)
                # After its lead, when far from it.
                lx, ly, lz = leads[tribe[i]][bird.rank]
                dx, dy, dz = lx - x, ly - y, lz - z
                d = sqrt(dx * dx + dy * dy + dz * dz)
                slack = (ROAM if stray else SLACK) * u
                if d > slack:
                    g = pull_ * min(1.0, (d - slack) / u) / d
                    ax += g * dx
                    ay += g * dy
                    az += g * dz
                # Its own mind.
                a = bird.beat + 0.8 * t * bird.pace
                g = (WANDER * 2.5 if stray else WANDER) * v0
                ax += g * cos(a)
                ay += 0.6 * g * sin(1.31 * a)
                az += g * sin(a)
                # Back into the view, and level.
                if x < x0:
                    ax += wall * (x0 - x)
                elif x > x1:
                    ax -= wall * (x - x1)
                if y < y0:
                    ay += wall * (y0 - y)
                elif y > y1:
                    ay -= 0.4 * wall * (y - y1)
                if z < -zm:
                    az -= wall * (z + zm)
                elif z > zm:
                    az -= wall * (z - zm)
                ay -= LEVEL * vy
                top = hi
                for hx, hy, r, gain in threats:
                    ex, ey = x - hx, y - hy
                    d2 = ex * ex + ey * ey
                    if d2 < r * r:
                        d = sqrt(d2) or 1.0
                        near = gain * (1.0 - d / r)
                        g = FEAR * v0 * near / d
                        ax += g * ex
                        ay += g * ey
                        top = max(top, hi + DASH * v0 * near)
                vx += ax * dt
                vy += ay * dt
                vz += az * dt
                # At about its speed: a bird does not stop.
                speed = sqrt(vx * vx + vy * vy + vz * vz) or 1.0
                want = speed + (bird.pace * v0 - speed) * 0.8 * dt
                want = max(lo, min(want, top))
                g = want / speed
                vx *= g
                vy *= g
                vz *= g
            X[i] = x + vx * dt
            Y[i] = y + vy * dt
            Z[i] = z + vz * dt
            VX[i], VY[i], VZ[i] = vx, vy, vz

    def fly(self, air: Air, t: float, scare: Scare | None = None) -> list[Dot]:
        """Step to `t` seconds; the birds to draw, in dots: in the
        air, or on their perch. `scare`: what they flee, a dot and how
        much, 0 to 1."""
        k = math.floor(t / DT + 1e-6)
        if self.k is None or not 0 <= k - self.k <= LEAP:
            self._start(k - WARM, air)
        else:
            self._fit(air)
        while self.k is not None and self.k < k:
            self._step(air, scare)
        out = []
        perched = len(air.roosts)
        night = self.f >= ROOST[0]
        for i, s in enumerate(self.state):
            if s != DOWN:
                out.append((self.x[i], self.y[i]))
            elif i < perched and (night or not (self.join and self.birds[i].group)):
                out.append(self.spot(i, air))
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
        second = SECOND if args.double else 0.0
        self.flock = Flock(rng, args.birds, second, args.join, args.hawk)
        self.sky = Sky(rng, 0.0) if args.night else None

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
        self,
        w: int,
        h: int,
        t: float,
        roosts: Sequence[tuple[int, int]] = (),
        scare: Scare | None = None,
    ) -> tuple[list[Dot], list[Dot]]:
        """The birds, then the hawk, in dots, the flock stepped to `t`
        seconds. `roosts`: dots to perch on at night; a bird without
        one goes into the land, and is not seen there. `scare`: the
        pointer, a dot and how much."""
        over = max(1, h - self.rows(h))
        air = Air(w, over, self.stage, self.args.dusk, lambda px: self.land.top(px, h), roosts)  # fmt: skip
        flown = self.flock.fly(air, t, scare)
        hawk = self.flock.hawk
        return flown, [(hawk[0], hawk[1])] if hawk is not None else []

    def fore(
        self,
        w: int,
        h: int,
        t: float,
        roosts: Sequence[tuple[int, int]] = (),
        scare: Scare | None = None,
    ) -> Grid:
        """The sun and the birds before it, `h` rows."""
        grid: Grid = [{} for _ in range(h)]
        if w <= 0 or h <= 0:
            return grid
        f = self.stage(t)
        cx, cy, colour = self.sun(w, h, t)
        r = self.radius(w, h)
        disc = _disc(cx, cy, r, self.land, h) if r > 0 else {}
        flown, stoop = self.birds(w, h, t, roosts, scare)
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
        hawk = self.flock.hawk
        for x, y in stoop:
            # Wings swept back: up in a stoop, down in a climb.
            flip = -1 if hawk is not None and math.sin(hawk[2]) < 0 else 1
            for ox, oy in HAWK_DOTS:
                px, py = math.floor(x) + ox, math.floor(y) + flip * oy
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
    pointer: list[float] | None = None  # the host mouse: a dot, when it moved
    try:
        pilot.subscribe("hover")
    except PtyrcError as err:  # a driver without the topic
        if err.code != "bad-params":
            raise

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
        for topic, data in sight.others:
            if topic != "hover":
                continue
            ids = [sprite.id if sprite is not None else None for sprite in sprites]
            if data.get("at") is None or data["sprite"] not in ids:
                pointer = None  # off the scene
                continue
            # The land's sprite is the last rows of the view.
            x, y = data["at"]
            y += h - rows if data["sprite"] == ids[2] else 0
            pointer = [2.0 * x + 1.0, 4.0 * y + 2.0, t]
        scare = None
        if pointer is not None:
            fright = max(STILL, 1.0 - (t - pointer[2]) / CALM)
            scare = (pointer[0], pointer[1], fright)
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
            cells = runs(scene.fore(w, h, t, roosts, scare), w, shown)
            # The whole view is its box: a hover is told anywhere on it.
            cells[0].append({"t": " " * (w - sum(len(run["t"]) for run in cells[0]))})
            put(1, cells, 0, False)
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
        default=130.0,
        metavar="N",
        help="birds over 80 by 24 cells [default: 130]",
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
        "-d",
        "--double",
        action="store_true",
        help="a second, smaller flock, on its own way",
    )
    out.add_argument(
        "-k",
        "--hawk",
        action="store_true",
        help="a hawk hunts now and then, the flock bursts",
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
