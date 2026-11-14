# SPDX-License-Identifier: GPL-3.0-or-later
"""Shore: a beach behind the text, seen from above; waves that run up
the sand and leave their lace, a tide that comes and goes.

    ptyrc-app shore                 # sand, swell, swash, the tide
    ptyrc-app shore -d -r           # shells and wrack, ripples in the sand
    ptyrc-app shore -c -b           # crabs, sandpipers: they flee the pointer
    ptyrc-app shore -w -g           # someone's footprints, gulls overhead
    ptyrc-app shore -n              # dusk, then the moon on the water
    ptyrc-app shore --all
    ptyrc-app shore --print 80x24   # no driver: the scene on stdout

The sand is at the top, the sea at the bottom. All of it is braille,
2 by 4 dots a cell; lengths are in dots, `y` down, towards the sea.

What it shows:

- A scene that is a function of the time: `Shore.picture` keeps no
  state but what the pointer did to the animals. A wave is a number:
  where its crest is, how far it ran, what it left, all follow from
  it (`Wave`). One sprite in view cells, behind the text, 8 times a
  second, sent when it changed.
- Few marks: a crest is a line of long strokes, a front one dot
  thick; the sand and the water between are left almost bare.
- A wave: a crest that comes in from the open sea, breaks at the
  still-water line, runs up the sand in lobes, slower on one side,
  and slides back. The foam is a net (`lace`), carried by the front.
- What it leaves: a line of foam at its highest reach, which thins
  out over the next waves, and is wiped by the first one to pass it;
  wet sand, darker.
- The tide: the still-water line goes up and down the pane in
  `--tide` seconds.
- The screen read as a map (`_cli.Sight`): the words are rocks, froth
  gathers over and under them in the water. Nothing is drawn in the
  cell on each side of a glyph.
- `-b`, `-w`: footprints, of sandpipers that follow the water down
  and flee it, of someone walking by. A print lasts until a wave
  that peaked after it was made reaches it.
- The pointer over the pane (`scare`, from the `hover` events), with
  `-b`, `-c`: sandpipers run from it, and fly, all those about, when
  it is on one; crabs run for their lives. One that rests is
  forgotten; they walk back.
- `-g`: the shadow of a gull, one request: a looped animation with a
  `motion` and a `ttl`.
- `-n`: the light, in `--day` seconds: the palette goes from day to
  dusk to night, cell by cell; at night the foam glows and the moon
  glitters on the water.
"""

from __future__ import annotations

import argparse
import math
import random
import sys
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any, NamedTuple

from ptyrc import Pilot, PtyrcError, Sprite, clock

from . import _cli
from ._strip import Pointer
from .life import DOTS
from .snake import walls_of
from .stars import Grid, clear, noise, runs

__all__ = [
    "FINDS",
    "Shore",
    "Wave",
    "app",
    "gull",
    "lace",
    "light",
    "main",
    "scare",
    "show",
    "swash",
    "tide",
]

Ink = tuple[int, int, int]
"""A colour by light: day, dusk, night."""

PERIOD = 1 / 8  # seconds a tick
AREA = 80 * 24  # cells `--sand` counts over
TILE = 64  # dots a side of the foam net

PEAK = 0.3  # share of its time on the sand a wave takes to run up
STAY = 1.05  # time a wave stays on the sand, in wave periods
LIFE = 4.0  # wave periods a line of foam lasts
DRY = 4.0  # wave periods the sand stays wet
HIGH, LOW = 0.52, 0.82  # the still-water line, down the pane
GULL = (9.0, 24.0)  # seconds between two gulls
WALK = 50.0  # seconds between two walkers
STEP = 0.5  # seconds a step of the walker
STRIDE = 5  # dots a step
TRAIL = 0.4  # seconds between two prints of a sandpiper
ROWS = 11  # windrows of sand down the pane
DUSK = 0.04  # share of a day a change of light takes
STILL = 2.0  # seconds a pointer that rests still scares
FADE = 3.0  # seconds its scare then takes to go
WARY = 9.0  # cells from the pointer a sandpiper runs from it
NEAR = 4.5  # cells from the pointer a sandpiper flies
SHY = 11.0  # cells from the pointer a crab runs from it
BOLT = 14.0  # cells a second of a crab that flees
RUN = 9.0  # cells a second of a sandpiper that flees
FLY = 0.9  # seconds a flight
FLEE = 13.0  # cells a flight

FOAM: tuple[Ink, ...] = (
    (231, 231, 123),
    (255, 230, 87),
    (195, 223, 51),
    (153, 217, 44),
)
LINE: tuple[Ink, ...] = (
    (254, 224, 37), (250, 223, 30), (246, 181, 30), (243, 245, 23), (241, 240, 23), (239, 239, 23),
)  # fmt: skip
#: A crest, from the shore out.
SEA: tuple[Ink, ...] = (
    (116, 222, 31), (74, 180, 25), (38, 138, 24), (32, 96, 24), (25, 60, 18), (24, 24, 17),
)  # fmt: skip
GLINT: Ink = (195, 222, 67)
SHALLOW: tuple[Ink, ...] = ((37, 137, 23), (80, 180, 30))  # deeper, nearer
SAND: tuple[Ink, ...] = (
    (137, 131, 238), (137, 131, 239), (101, 95, 238), (137, 131, 238),
    (101, 95, 239), (180, 173, 241),
)  # fmt: skip
WET: tuple[Ink, ...] = ((66, 96, 237), (67, 96, 238), (103, 95, 238))
FILM: Ink = (153, 216, 60)  # water thin on the sand a wave leaves
SUNK: tuple[Ink, ...] = ((66, 96, 23), (67, 60, 24), (73, 66, 23))
FROTH: tuple[Ink, ...] = ((153, 217, 44), (110, 174, 38))
WRACK: tuple[Ink, ...] = ((58, 58, 236), (64, 58, 237), (94, 94, 236), (100, 94, 238))
RIPPLE: tuple[Ink, Ink] = ((95, 95, 236), (60, 96, 236))  # dry, wet
PRINT: tuple[Ink, Ink] = ((180, 173, 243), (109, 138, 240))  # dry, wet
BIRD: Ink = (255, 230, 250)
CRAB: Ink = (209, 203, 131)
MOON: tuple[Ink, ...] = ((230, 230, 230), (229, 229, 229), (223, 223, 223))
SHADOW = 240

#: What the sea leaves, `-d`: glyph, colours, weight.
FINDS: tuple[tuple[str, tuple[Ink, ...], int], ...] = (
    ("•", ((245, 245, 240), (250, 181, 242), (102, 95, 238)), 6),  # pebbles
    ("°", ((224, 217, 245), (230, 223, 247)), 4),  # small shells
    ("∘", ((223, 216, 244), (255, 224, 248)), 3),
    ("◠", ((224, 217, 246), (217, 174, 243)), 3),  # a clam
    ("✶", ((209, 203, 131), (174, 167, 95)), 2),  # a starfish
    ("~", ((65, 58, 238), (71, 64, 239)), 2),  # kelp
)


def swash(p: float) -> float:
    """How far up a wave is, 0 to 1, `p` of its time on the sand: a
    fast run up to `PEAK`, a slow way back."""
    if not 0.0 < p < 1.0:
        return 0.0
    if p < PEAK:
        return math.sin(math.pi / 2 * p / PEAK)
    return 0.5 + 0.5 * math.cos(math.pi * (p - PEAK) / (1.0 - PEAK))


def tide(t: float, period: float, h: int) -> float:
    """The still-water line, in dots from the top of `h` rows, at `t`
    seconds: half-way and rising at 0."""
    mid, half = (HIGH + LOW) / 2, (LOW - HIGH) / 2
    return 4 * h * (mid - half * math.sin(math.tau * t / period))


def light(t: float, day: float) -> tuple[int, float]:
    """The light at `t` seconds, a day lasting `day`: the palette, 0
    day, 1 dusk, 2 night, and how far it is into the next, 0 to 1.
    The afternoon at 0."""
    f = (t / day + 0.3) % 1.0
    for at, palette in ((0.45, 0), (0.55, 1), (0.9, 2), (0.97, 1)):
        if at <= f < at + DUSK:
            return palette, (f - at) / DUSK
    if f < 0.45:
        return 0, 0.0
    if f < 0.55:
        return 1, 0.0
    return (2, 0.0) if f < 0.9 else (1, 0.0) if f < 0.97 else (0, 0.0)


def _after(palette: int, f: float) -> int:
    """The palette that follows `palette` at `f` of a day."""
    return {0: 1, 1: 2 if f < 0.75 else 0, 2: 1}[palette]


def scare(pointer: Pointer | None, t: float) -> tuple[int, int, float] | None:
    """The cell of the pointer at `t` seconds and how much it scares,
    0 to 1: all it can while it moves and `STILL` seconds after, less
    over the next `FADE`, and as it leaves. None: not at all."""
    if pointer is None or not pointer.trail:
        return None
    x, y, moved = pointer.trail[-1]
    rest = t - moved - STILL if pointer.at is not None else t - pointer.since
    much = 1.0 - max(0.0, rest) / FADE
    return (x, y, much) if much > 0.0 else None


@dataclass
class _Bird:
    """What the pointer did to a sandpiper."""

    off: float = 0.0  # cells from where it would be
    flight: tuple[float, float, float] | None = None  # from when, from, to
    face: int = 1
    down: float = -math.inf  # when it landed
    since: float = math.inf  # it was not where it would be: from, to
    until: float = -math.inf
    tracks: list[tuple[int, int, float]] = field(default_factory=list)


@dataclass
class _Crab:
    """What the pointer did to a crab."""

    off: float = 0.0  # cells from where it would be
    x: float | None = None  # where it was
    way: int = 0  # the side it runs to; 0: it does not


_LACE: list[list[float]] = []


def lace() -> list[list[float]]:
    """The foam net, `TILE` dots a side, it tiles: per dot a number in
    [0, 1), low on the walls between the bubbles; as many dots under
    `q` as `q` says."""
    if _LACE:
        return _LACE
    rng = random.Random(0x5EA)
    n = TILE
    seeds = [(rng.uniform(0, n), rng.uniform(0, n)) for _ in range(n * n // 60)]
    gaps: list[tuple[float, int]] = []
    for y in range(n):
        for x in range(n):
            a = b = 1e9
            for sx, sy in seeds:
                dx, dy = abs(x - sx), abs(y - sy)
                d = math.hypot(min(dx, n - dx), min(dy, n - dy))
                if d < a:
                    a, b = d, a
                elif d < b:
                    b = d
            gaps.append((b - a + 0.3 * rng.random(), y * n + x))
    out = [[0.0] * n for _ in range(n)]
    for rank, (_, at) in enumerate(sorted(gaps)):
        out[at // n][at % n] = rank / (n * n)
    _LACE.extend(out)
    return _LACE


_GRIT: list[list[float]] = []


def grit() -> list[list[float]]:
    """Noise, `TILE` dots a side: per dot a number in [0, 1)."""
    if not _GRIT:
        rng = random.Random(0x5A4D)
        _GRIT.extend([rng.random() for _ in range(TILE)] for _ in range(TILE))
    return _GRIT


class Wave(NamedTuple):
    """Wave `k` over `across` dot columns: `at`, the second its middle
    breaks; per column, `late`, the seconds it breaks after, and
    `run`, the dots it runs up the sand."""

    k: int
    at: float
    late: list[float]
    run: list[float]
    bend: float  # phase of the bends of its crest


class _Dots:
    """Braille dots written into a `Grid`; a cell takes the colour of
    its dot of highest rank."""

    def __init__(self, w: int, h: int) -> None:
        self.w, self.h = 2 * w, 4 * h
        self.cells: dict[tuple[int, int], list[Any]] = {}

    def dot(self, px: int, py: int, ink: Ink, rank: int = 0) -> None:
        if 0 <= px < self.w and 0 <= py < self.h:
            cell = self.cells.get((px >> 1, py >> 2))
            if cell is None:
                self.cells[px >> 1, py >> 2] = [DOTS[py & 3][px & 1], ink, rank]
                return
            cell[0] |= DOTS[py & 3][px & 1]
            if rank >= cell[2]:
                cell[1], cell[2] = ink, rank


class Shore:
    """The scene and its options. `seed`: same seed, same shore.
    `period`: seconds between two waves; `ebb`: seconds a tide takes;
    `grains`: grains of sand over 80 by 24 cells."""

    def __init__(
        self,
        rng: random.Random,
        period: float = 8.0,
        ebb: float = 300.0,
        grains: float = 420.0,
        day: float = 0.0,
        drift: bool = False,
        ripples: bool = False,
        crabs: bool = False,
        birds: bool = False,
        walker: bool = False,
    ) -> None:
        self.period, self.ebb, self.grains, self.day = period, ebb, grains, day
        self.drift, self.ripples, self.crabs = drift, ripples, crabs
        self.birds, self.walker = birds, walker
        self.key = rng.getrandbits(24)
        self._sand = random.Random(rng.getrandbits(32))
        self._finds = random.Random(rng.getrandbits(32))
        self.sand: list[tuple[float, float, int]] = []
        self.finds: list[tuple[float, float, int, int]] = []
        self._waves: dict[int, Wave] = {}
        self._across = 0
        self._seen = -math.inf  # the time of the last picture
        self._flock: list[_Bird] = []
        self._dug: list[_Crab] = []

    # -- the waves ---------------------------------------------------

    def reach(self, h: int) -> float:
        """Dots a wave of full size runs up the sand, on `h` rows."""
        return max(5.0, min(60.0, 0.19 * 4 * h))

    def spacing(self, h: int) -> float:
        """Dots between two crests at sea."""
        return max(8.0, 0.11 * 4 * h)

    def wave(self, k: int, across: int, h: int) -> Wave:
        """Wave `k`, over `across` dot columns of `h` rows."""
        if across != self._across or len(self._waves) > 64:
            self._waves.clear()
            self._across = across
        got = self._waves.get((k, h))  # type: ignore[call-overload]
        if got is not None:
            return got
        n = [noise(self.key + i, k) for i in range(8)]
        size = self.reach(h) * (0.5 + 0.5 * n[1] ** 0.8)
        skew = (n[2] - 0.5) * 0.012 * self.period / 8.0
        late, run = [], []
        lobe = 5.0 * self.reach(h)  # dots across a lobe
        for x in range(across):
            late.append(
                skew * (x - across / 2)
                + 0.045 * self.period * math.sin(x * math.tau / 130 + math.tau * n[3])
            )
            # Lobes: round towards the land, a cusp between two.
            run.append(
                size
                * (
                    0.62
                    + 0.3 * abs(math.cos(x * math.pi / lobe + math.pi * n[4]))
                    + 0.12 * abs(math.cos(x * math.pi / (0.43 * lobe) + math.pi * n[5]))
                    + 0.08 * math.sin(x * math.tau / (2.9 * lobe) + math.tau * n[6])
                )
            )
        at = (k + 0.3 * (n[0] - 0.5)) * self.period
        out = Wave(k, at, late, run, math.tau * n[7])
        self._waves[k, h] = out  # type: ignore[index]
        return out

    def edge(self, px: int, t: float, across: int, h: int) -> float:
        """The water's edge at the dot column `px`, at `t` seconds."""
        base = tide(t, self.ebb, h)
        stay = STAY * self.period
        out = base
        px = max(0, min(across - 1, px))
        k0 = math.floor(t / self.period)
        for k in range(k0 - 2, k0 + 2):
            one = self.wave(k, across, h)
            p = (t - one.at - one.late[px]) / stay
            if 0.0 < p < 1.0:
                out = min(out, base - one.run[px] * swash(p))
        return out

    # -- the picture -------------------------------------------------

    def picture(
        self,
        w: int,
        h: int,
        t: float = 0.0,
        rocks: Sequence[tuple[int, int]] = (),
        pointer: Pointer | None = None,
    ) -> Grid:
        """The shore over `w` by `h` cells at `t` seconds. `rocks`:
        the cells where froth gathers when the water covers them.
        `pointer`: what the animals flee, in cells; what it did to
        them is kept from a picture to the next, a second apart at
        most."""
        grid: Grid = [{} for _ in range(max(0, h))]
        if w <= 0 or h <= 0:
            return grid
        dt = t - self._seen
        self._seen = t
        if not 0.0 <= dt <= 1.0:
            dt = 0.0
            self._flock.clear()
            self._dug.clear()
        fear = scare(pointer, t)
        across, down = 2 * w, 4 * h
        period = self.period
        stay = STAY * period
        base = tide(t, self.ebb, h)
        net = lace()
        dots = _Dots(w, h)
        dot = dots.dot
        k0 = math.floor(t / period)
        gap = self.spacing(h)
        speed = gap / period  # of a crest at sea, dots a second
        far = int((down - base) / gap) + 2

        # Per column: the edge, how far the sand is wet, and per wave
        # on the sand its front; per wave gone, what it reached.
        edge = [base] * across
        wet = [base] * across
        back = [False] * across  # the edge is a wave on its way back
        fronts: list[tuple[Wave, list[float], list[float]]] = []
        marks: list[tuple[Wave, float, list[float]]] = []  # wave, peak, reach
        for k in range(k0 - int(LIFE + DRY) - 2, k0 + 2):
            one = self.wave(k, across, h)
            if t < one.at - 0.5 * period:
                continue
            was = tide(one.at, self.ebb, h)
            age = (t - one.at) / period
            front: list[float] = [0.0] * across
            share: list[float] = [0.0] * across
            reach: list[float] = [0.0] * across
            live = False
            fade = max(0.0, min(1.0, (age - DRY / 2) / (DRY / 2)))
            for x in range(across):
                p = (t - one.at - one.late[x]) / stay
                run = one.run[x]
                if p <= 0.0:
                    front[x] = reach[x] = down + 1.0
                    share[x] = -1.0
                    continue
                if p < 1.0:
                    live = True
                    y = base - run * swash(p)
                    front[x], share[x] = y, p
                    if y < edge[x]:
                        edge[x] = y
                        back[x] = p >= PEAK
                    top = y if p < PEAK else was - run
                else:
                    share[x] = 2.0
                    top = was - run
                reach[x] = top
                top += (base - top) * fade
                wet[x] = min(wet[x], top)
            if live:
                fronts.append((one, front, share))
            marks.append((one, one.at + PEAK * stay, reach))

        def washed(px: int, py: float, since: float) -> bool:
            """Whether a wave that peaked after `since` reached the
            dot, or the water is over it."""
            if py >= edge[px]:
                return True
            return any(
                peak + one.late[px] > since and reach[px] <= py
                for one, peak, reach in marks
            )

        self._ground(dots, w, h, edge, wet, back)
        if self.ripples:
            self._ripples(dots, across, h, edge, wet)
        if self.drift:
            self._wrack(dots, across, h)

        # The lines of foam the waves gone have left.
        for i, (one, peak, reach) in enumerate(marks):
            age = (t - peak) / (LIFE * period)
            if not 0.0 <= age < 1.0:
                continue
            ink = LINE[min(len(LINE) - 1, int(age * len(LINE)))]
            later = [r for _, _, r in marks[i + 1 :]]
            row = net[one.k % TILE]
            for x in range(across):
                y = reach[x]
                if y >= edge[x] or t < peak + one.late[x]:
                    continue
                keep = row[x % TILE]
                if keep < age or any(r[x] <= y + 0.5 for r in later):
                    continue
                dot(x, int(y), ink, 3)

        self._sea(dots, across, down, edge, base, t, k0, far, speed, h)

        # The waves on the sand: a front, the net behind it.
        for one, front, share in fronts:
            shift = one.k * 23
            for x in range(across):
                p = share[x]
                if not 0.0 < p < 1.0:
                    continue
                y = front[x]
                over = base - y  # dots of sand under the wave
                column = net[(x + shift) % TILE]
                if p < PEAK:
                    line, body, scale = 1.0, 0.3, 1.5 + 0.3 * over
                    pale = 0
                else:
                    q = (p - PEAK) / (1.0 - PEAK)
                    line = 0.7 * (1.0 - q) ** 1.5
                    pale = 1 + int(2.5 * q)
                    body = 0.25 * (1.0 - q) ** 3
                    scale = 1.5 + 0.35 * over
                y0 = math.floor(y)
                deep = min(int(over) + 3, 30)
                for d in range(deep):
                    py = y0 + d
                    if d < 2:
                        want = line if d == 0 else line * 0.4
                    else:
                        want = body * math.exp(-d / scale)
                        if want < 0.04:
                            break
                    if column[(py - y0 + shift) % TILE] < want:
                        dot(x, py, FOAM[min(3, d // 3 + pale)], 6 if d < 2 else 5)

        self._froth(dots, rocks, edge, base, t, h)
        glyphs: dict[tuple[int, int], tuple[str, Ink]] = {}
        if self.walker:
            self._walker(dots, across, h, t, wet, washed)
        if self.drift:
            self._drift(glyphs, w, h, edge, wet)
        if self.birds:
            self._birds(dots, glyphs, w, h, t, wet, washed, fear, dt)
        if self.crabs:
            self._crabs(glyphs, w, h, t, edge, fear, dt)
        if self.day and light(t, self.day)[0] == 2:
            self._moon(dots, across, down, edge, t)

        palette, blend = light(t, self.day) if self.day else (0, 0.0)
        after = _after(palette, (t / self.day + 0.3) % 1.0) if self.day else 0
        for (x, y), (bits, ink, _) in dots.cells.items():
            turn = blend > 0.0 and noise(x * 131 + y, 7) < blend
            grid[y][x] = (chr(0x2800 + bits), ink[after if turn else palette])
        for (x, y), (glyph, ink) in glyphs.items():
            if 0 <= x < w and 0 <= y < h:
                turn = blend > 0.0 and noise(x * 131 + y, 7) < blend
                grid[y][x] = (glyph, ink[after if turn else palette])
        return grid

    def _ground(
        self,
        dots: _Dots,
        w: int,
        h: int,
        edge: list[float],
        wet: list[float],
        back: list[bool],
    ) -> None:
        """The grains: dry, wet, or seen through shallow water; the
        wet sand, darker, a film of water where a wave just left."""
        across, down = 2 * w, 4 * h
        rough = grit()
        for x in range(across):
            column = rough[x % TILE]
            y1 = math.floor(edge[x])
            for py in range(max(0, math.ceil(wet[x])), y1):
                roll = column[py % TILE]
                if back[x] and y1 - py <= 4 and roll < 0.2 - 0.04 * (y1 - py):
                    dots.dot(x, py, FILM, 1)
                elif roll < 0.035:
                    dots.dot(x, py, WET[int(roll * 997) % len(WET)], 0)
        n = int(self.grains * w * h / AREA)
        rng = self._sand
        while len(self.sand) < n:
            u, v = rng.random(), rng.random()
            if rng.random() < 0.85:
                # Windrows: most grains lie along lines the wind drew.
                row = int(v * ROWS)
                v = (row + 0.5 + 0.3 * math.sin(math.tau * (3 * u + 0.37 * row))) / ROWS
                v += rng.gauss(0.0, 0.004)
            self.sand.append((u, v % 1.0, rng.getrandbits(16)))
        for u, v, key in self.sand[:n]:
            px, py = int(u * across), int(v * down)
            depth = py - edge[px]
            if depth < 0:
                if py >= wet[px]:
                    dots.dot(px, py, WET[key % len(WET)], 1)
                else:
                    dots.dot(px, py, SAND[key % len(SAND)], 1)
            elif depth < 10 and key % 2:
                dots.dot(px, py, SUNK[key % len(SUNK)], 0)

    def _sea(
        self,
        dots: _Dots,
        across: int,
        down: int,
        edge: list[float],
        base: float,
        t: float,
        k0: int,
        far: int,
        speed: float,
        h: int,
    ) -> None:
        """The crests on their way in, the water's stipple, glints."""
        period = self.period
        net = lace()
        dot = dots.dot
        rough = grit()
        for k in range(k0, k0 + far + 2):
            one = self.wave(k, across, h)
            row = net[(one.k * 7) % TILE]
            spots = rough[(one.k * 5) % TILE]
            drift = int(3.0 * t)
            for x in range(across):
                ahead = one.at + one.late[x] - t  # seconds before it breaks
                if ahead <= 0.0:
                    continue
                out = speed * ahead  # dots from the still-water line
                sway = 0.35 + 0.65 * min(1.0, out / (2.0 * speed * period))
                y = (
                    base
                    + out
                    + sway
                    * 0.2
                    * speed
                    * period
                    * (
                        math.sin(x * math.tau / 83 + one.bend)
                        + 0.5 * math.sin(x * math.tau / 31 + 2.0 * one.bend)
                    )
                )
                if y >= down or y <= edge[x] + 1:
                    continue
                near = ahead / period
                if near < 0.22:
                    # It breaks, here and there first: white, foam behind.
                    rise = 1.0 - near / 0.22
                    if spots[(x >> 3) % TILE] < 0.15 + rise:
                        ink = FOAM[0] if rise > 0.5 else FOAM[2]
                        dot(x, int(y), ink, 4)
                        if row[(x + drift) % TILE] < 0.6 * rise:
                            dot(x, int(y) - 1, ink, 4)
                        for d in range(1, 1 + int(3 * rise)):
                            if net[(x + k) % TILE][(d + drift) % TILE] < 0.25 * rise:
                                dot(x, int(y) + d, FOAM[3], 4)
                        continue
                # Long strokes, shorter and fewer farther out.
                stroke = row[((x + drift) >> 4) % TILE]
                if stroke > 0.92 - 0.3 * min(1.0, near / 3.0):
                    continue
                ink = SEA[min(len(SEA) - 1, int(out / (0.55 * speed * period)))]
                dot(x, int(y), ink, 2)
        # Shallow water over the sand, thinner as it deepens.
        slide = int(0.7 * t)
        for x in range(across):
            y0 = math.floor(edge[x]) + 1
            column = rough[(x * 5 + 11) % TILE]
            for d in range(1, min(13, down - y0)):
                if column[(y0 + d + slide) % TILE] < 0.06 - 0.005 * d:
                    dot(x, y0 + d, SHALLOW[d < 5], 1)
        # Short ripples between the crests.
        for j in range(int(max(0.0, down - base) * across / 600)):
            px = int(noise(j, 11) * across + 3.0 * math.sin(0.3 * t + j))
            py = int(base + 8 + noise(j, 12) * (down - base - 8))
            ink = SEA[min(len(SEA) - 1, 1 + int((py - base) / (0.55 * speed * period)))]
            for dx in range(3 + int(5 * noise(j, 13))):
                if 0 <= px + dx < across and py > edge[px + dx] + 8:
                    dot(px + dx, py, ink, 1)
        # Glints of the light, each on a beat of its own.
        cells = max(0.0, down - base) * across / 8
        for j in range(int(cells / 110)):
            beat = int(t / 0.6 + 8 * noise(j, 1))
            px = int(noise(j, beat) * across)
            py = int(edge[px] + 6 + noise(beat, j) * (down - edge[px] - 6))
            dot(px, py, GLINT, 2)

    def _froth(
        self,
        dots: _Dots,
        rocks: Sequence[tuple[int, int]],
        edge: list[float],
        base: float,
        t: float,
        h: int,
    ) -> None:
        """Foam over and under the words the water covers."""
        net = lace()
        size = self.reach(h)
        for x, y in rocks:
            px, py = 2 * x, 4 * y
            if px + 1 >= len(edge):
                continue
            depth = py + 2 - edge[px]
            if depth < 0:
                continue
            swell = 0.6 + 0.4 * math.sin(1.9 * t + 0.7 * x + 2.3 * y)
            want = (0.03 + 0.14 * math.exp(-max(0.0, py - base) / size)) * swell
            sway = int(2.5 * math.sin(0.8 * t + y))
            for dy in range(4):
                for dx in range(2):
                    if py + dy <= edge[px + dx]:
                        continue
                    if net[(px + dx + sway) % TILE][(py + dy) % TILE] < want:
                        dots.dot(px + dx, py + dy, FROTH[depth > size], 4)

    # -- the extras --------------------------------------------------

    def _ripples(
        self, dots: _Dots, across: int, h: int, edge: list[float], wet: list[float]
    ) -> None:
        """Ripple marks in the sand the tide uncovers."""
        top = 4 * h * HIGH - self.reach(h)
        low = 4 * h * LOW
        for j in range(int((low - top) / 9) + 1):
            y0 = top + 9 * j
            for x in range(across):
                if math.sin(0.19 * x + 2.4 * j + noise(self.key, j) * 9) < 0.45:
                    continue
                y = y0 + 1.4 * math.sin(x * math.tau / 19 + 1.9 * j)
                if y < edge[x] - 1:
                    dots.dot(x, int(y), RIPPLE[y >= wet[x]], 0)

    def _wrack(self, dots: _Dots, across: int, h: int) -> None:
        """Weed and bits along the high-water mark."""
        net = lace()
        y0 = 4 * h * HIGH - 0.9 * self.reach(h)
        a, b = noise(self.key, 91) * 9, noise(self.key, 92) * 9
        for x in range(across):
            clump = net[(x // 3) % TILE][17]
            if clump > 0.3:
                continue
            y = y0 + 2.2 * math.sin(x / 23 + a) + 1.2 * math.sin(x / 7 + b)
            ink = WRACK[int(clump * 13) % len(WRACK)]
            dots.dot(x, int(y), ink, 2)
            if net[x % TILE][29] < 0.5:
                dots.dot(x, int(y) + 1, ink, 2)
            if net[x % TILE][41] < 0.15:
                dots.dot(x, int(y) - 1, ink, 2)

    def _drift(
        self,
        glyphs: dict[tuple[int, int], tuple[str, Ink]],
        w: int,
        h: int,
        edge: list[float],
        wet: list[float],
    ) -> None:
        """Shells, pebbles, a starfish: dimmer under water."""
        n = int(11 * w * h / AREA)
        rng = self._finds
        weights = [weight for _, _, weight in FINDS]
        while len(self.finds) < n:
            kind = rng.choices(range(len(FINDS)), weights)[0]
            self.finds.append((rng.random(), rng.random(), kind, rng.getrandbits(8)))
        for u, v, kind, key in self.finds[:n]:
            x, y = int(u * w), int(v * h)
            glyph, inks, _ = FINDS[kind]
            depth = 4 * y + 2 - edge[min(len(edge) - 1, 2 * x)]
            if depth < 0:
                glyphs[x, y] = (glyph, inks[key % len(inks)])
            elif depth < 16:
                glyphs[x, y] = (glyph, SUNK[key % len(SUNK)])

    def _walker(
        self, dots: _Dots, across: int, h: int, t: float, wet: list[float], washed: Any
    ) -> None:
        """Footprints of someone who walks along the shore."""
        size = self.reach(h)
        for c in range(int(t // WALK) - 2, int(t // WALK) + 1):
            if c < 0:
                continue
            start = c * WALK
            way = 1 if noise(self.key + 5, c) < 0.5 else -1
            line = tide(start, self.ebb, h) - size * (1.0 + 0.25 * noise(self.key, c))
            bend = noise(self.key + 9, c) * math.tau
            steps = min(int((t - start) / STEP), across // STRIDE)
            for j in range(steps):
                made = start + j * STEP
                old = (t - made) / (2.5 * WALK)
                if noise(j, c) < old * old:
                    continue
                px = j * STRIDE
                if way < 0:
                    px = across - 1 - px
                if not 0 <= px < across - 1:
                    continue
                py = (
                    int(line + 3.0 * math.sin(px * math.tau / 120 + bend)) + (j & 1) * 2
                )
                if washed(px, py, made):
                    continue
                ink = PRINT[py >= wet[px]]
                dots.dot(px, py, ink, 2)
                dots.dot(px + way, py, ink, 2)

    def _birds(
        self,
        dots: _Dots,
        glyphs: dict[tuple[int, int], tuple[str, Ink]],
        w: int,
        h: int,
        t: float,
        wet: list[float],
        washed: Any,
        fear: tuple[int, int, float] | None,
        dt: float,
    ) -> None:
        """Sandpipers: they follow the water down and run from it;
        from the pointer too, and fly when it is on them."""
        across = 2 * w
        n = max(3, min(9, w // 16))

        def home(i: int, at: float) -> float:
            mid = w * (0.5 + 0.36 * math.sin(at * math.tau / 97 + self.key))
            return mid + (i - (n - 1) / 2) * 3.4 + 1.8 * math.sin(0.7 * at + 2.1 * i)

        def stand(i: int, x: float, at: float) -> tuple[int, int]:
            x = max(0, min(w - 1, round(x)))
            lag = 0.25 + 0.12 * (i % 3)
            y = int(self.edge(2 * x, at - lag, across, h)) >> 2
            return x, max(0, min(h - 1, y - 1 - (i & 1)))

        def place(i: int, at: float) -> tuple[int, int]:
            return stand(i, home(i, at), at)

        if len(self._flock) != n:
            self._flock = [_Bird() for _ in range(n)]
        flock = self._flock
        # Where each is, how far from the pointer, the side it flees to.
        spots: list[tuple[float, float, int]] = []
        for i, bird in enumerate(flock):
            x = home(i, t) + bird.off
            if fear is None:
                spots.append((x, math.inf, 1))
                continue
            d = math.hypot(x - fear[0], 2.0 * (stand(i, x, t)[1] - fear[1]))
            spots.append((x, d, 1 if (x - fear[0] or (i & 1) - 0.5) > 0 else -1))
        at, much = (fear[0], fear[2]) if fear else (0, 0.0)
        # One flies: those about it do.
        up = any(
            d < NEAR * much and bird.flight is None
            for bird, (_, d, _) in zip(flock, spots, strict=True)
        )
        for i, bird in enumerate(flock):
            x, d, way = spots[i]
            d -= 1.7 * (i % 4)  # each its own nerve
            hx = x - bird.off
            lift = 0.0
            if bird.flight is not None:
                since, a, b = bird.flight
                u = (t - since) / FLY
                if u >= 1.0:
                    bird.flight, bird.down, x = None, t, b
                elif u > 0.0:
                    x = a + (b - a) * u * u * (3 - 2 * u)
                    lift = math.sin(math.pi * u)
                    bird.face = 1 if b > a else -1
            elif d < NEAR * much or (
                up and d < (WARY + 3.0) * much and t - bird.down > 2 * FLY
            ):
                to = x + way * (FLEE + 6.0 * noise(i, int(8 * t)))
                wall = 1.0 if way < 0 else w - 2.0
                if abs(wall - x) < FLEE / 2:  # no room: over the pointer
                    to = x - way * (FLEE + 2 * abs(x - at))
                to = max(1.0, min(w - 2.0, to))
                wait = 0.3 * noise(i, int(t)) * max(0.0, d) / WARY
                bird.flight = (t + wait, x, to)
            elif d < WARY * much:
                x = max(0.0, min(w - 1.0, x + way * RUN * dt))
                bird.face = way
            elif bird.off and (
                fear is None
                or (
                    (hx - fear[0]) * (x - fear[0]) > 0
                    and abs(hx - fear[0]) > WARY * much + 2
                )
            ):
                step = min(abs(bird.off), 0.3 * RUN * dt)
                x -= math.copysign(step, bird.off)
                bird.face = -1 if bird.off > 0 else 1
            bird.off = x - hx
            cx, cy = stand(i, x, t)
            away = bird.off != 0.0 or bird.flight is not None
            if away:
                if t - bird.until > 25 * TRAIL:
                    bird.since = t
                bird.until = t
            # Its prints: where it would have been, and where it fled.
            for j in range(1, 26):
                made = t - j * TRAIL
                if bird.since <= made <= bird.until:
                    continue
                fx, fy = place(i, made)
                px, py = 2 * fx + (j & 1), 4 * fy + 2 + (j & 1)
                if (fx, fy) != (cx, cy) and not washed(px, py, made):
                    dots.dot(px, py, PRINT[py >= wet[px]], 2)
            tracks = bird.tracks
            while tracks and t - tracks[0][2] > 25 * TRAIL:
                tracks.pop(0)
            for px, py, made in tracks:
                if (px >> 1, py >> 2) != (cx, cy) and not washed(px, py, made):
                    dots.dot(px, py, PRINT[py >= wet[px]], 2)
            if away and not lift and (not tracks or t - tracks[-1][2] >= TRAIL / 2):
                odd = len(tracks) & 1
                tracks.append((2 * cx + odd, 4 * cy + 2 + odd, t))
            if lift:
                cy = max(0, cy - round(2.4 * lift))
                glyph = "v^"[int(8 * t + i) & 1]
            elif away:
                glyph = "›" if bird.face > 0 else "‹"
            else:
                before = place(i, t - 0.3)
                if before == (cx, cy):
                    glyph = "," if noise(i, int(2 * t)) < 0.4 else "›‹"[i & 1]
                else:
                    glyph = "›" if before[0] <= cx else "‹"
            glyphs[cx, cy] = (glyph, BIRD)

    def _crabs(
        self,
        glyphs: dict[tuple[int, int], tuple[str, Ink]],
        w: int,
        h: int,
        t: float,
        edge: list[float],
        fear: tuple[int, int, float] | None,
        dt: float,
    ) -> None:
        """Crabs: sideways, in bursts; under the sand when the water
        comes over them. They run from the pointer, to the side they
        took, past it when the side of the pane is near."""
        n = max(1, w // 12)
        size = self.reach(h)
        base = tide(t, self.ebb, h)
        if len(self._dug) != n:
            self._dug = [_Crab() for _ in range(n)]
        for i, crab in enumerate(self._dug):
            key = self.key + 977 * i
            beat, part = divmod(t / 2.6 + 5 * noise(key), 1.0)
            a, b = noise(key, int(beat)), noise(key, int(beat) + 1)
            ease = min(1.0, part / 0.35)
            ease = ease * ease * (3 - 2 * ease)
            hx = w * ((i + 0.3 + 0.4 * (a + (b - a) * ease)) / n)
            y = int(base - size * (0.7 + 0.5 * noise(key, 3))) >> 2
            fx = hx + crab.off
            was = fx if crab.x is None else crab.x
            d = math.inf
            if fear is not None:
                d = math.hypot(was - fear[0], 2.0 * (y - fear[1])) / fear[2]
                d -= 5.0 * noise(key, 7)  # each its own nerve
            if d < SHY:
                if not crab.way:
                    crab.way = 1 if was >= fear[0] else -1  # type: ignore[index]
                    if abs((0 if crab.way < 0 else w - 1) - was) < 3:
                        crab.way = -crab.way
                elif was in (0.0, w - 1.0):
                    crab.way = 1 if was == 0.0 else -1
                fx = was + crab.way * BOLT * (0.7 + 0.6 * noise(key, int(4 * t))) * dt
            elif d >= SHY + 2.0:
                crab.way = 0
                if fear is not None and abs(hx - fear[0]) < (SHY + 2.0) * fear[2]:
                    fx = was  # the pointer is where it would go
                else:
                    step = min(abs(crab.off), 0.3 * RUN * dt)
                    fx -= math.copysign(step, crab.off)
            else:
                fx = was
            fx = max(0.0, min(w - 1.0, fx))
            crab.off, crab.x = fx - hx, fx
            x = round(fx)
            if 0 <= y < h and 4 * y + 3 < edge[2 * x]:
                glyphs[x, y] = ("ж", CRAB)

    def _moon(
        self, dots: _Dots, across: int, down: int, edge: list[float], t: float
    ) -> None:
        """The moon on the water: a path of glitter, wider towards
        the open sea, that crosses the pane in a night."""
        f = (t / self.day + 0.3) % 1.0
        mid = across * (0.2 + 0.6 * (f - 0.55) / 0.35)
        for j in range(int(1.6 * down / 4)):
            beat = int(t / 0.45 + 9 * noise(j, 2))
            out = noise(j, 5) ** 0.8
            half = 2.0 + 0.2 * out * down
            px = int(mid + (noise(j, beat) + noise(beat, j) - 1.0) * half)
            if not 0 <= px < across:
                continue
            py = int(edge[px] + 3 + out * (down - edge[px] - 3))
            dots.dot(px, py, MOON[j % 3], 3)


def gull(rng: random.Random, w: int, h: int) -> dict[str, Any]:
    """The shadow of a gull that crosses a view of `w` by `h`, as
    arguments of `Pilot.draw` less the frame."""
    way = rng.choice((-1, 1))
    speed = rng.uniform(14.0, 22.0)  # cells a second
    looks = ("⠑⠢⠔⠊", "⠒⠢⠔⠒", "⠔⠒⠒⠢", "⠒⠢⠔⠒")
    return {
        "frames": [
            {"cells": [[{"t": look, "fg": SHADOW}]], "ms": 130} for look in looks
        ],
        "loop": True,
        "at": (0 if way > 0 else max(0, w - 4), rng.randrange(max(1, h - 1))),
        "space": "view",
        "motion": {
            "vx": way * speed,
            "vy": rng.uniform(-2.5, 2.5),
            "ax": 0.0,
            "ay": 0.0,
            "bounce": False,
        },
        "ttl": w / speed,
        "layer": "back",
        "z": 1,
    }


def rocks_of(rows: Sequence[str], w: int, h: int) -> list[tuple[int, int]]:
    """The cells over and under the glyphs of the text `rows`, in a
    view of `w` by `h`, that hold no glyph and are beside none."""
    walls = walls_of(rows)
    shut = {(x + dx, y) for x, y in walls for dx in (-1, 0, 1)}
    out = {(x + dx, y + dy) for x, y in walls for dx in (-1, 0, 1) for dy in (-1, 1)}
    return sorted((x, y) for x, y in out - shut if 0 <= x < w and 0 <= y < h)


def scene(args: argparse.Namespace, rng: random.Random) -> Shore:
    return Shore(
        rng,
        period=args.waves,
        ebb=args.tide,
        grains=args.sand,
        day=args.day if args.night else 0.0,
        drift=args.drift,
        ripples=args.ripples,
        crabs=args.crabs,
        birds=args.birds,
        walker=args.walker,
    )


def show(args: argparse.Namespace) -> int:
    """`--print`: the scene once on stdout, no driver."""
    w, h = args.print
    shore = scene(args, random.Random(args.seed))
    for line in runs(shore.picture(w, h, args.at), w):
        text = "".join(
            f"\x1b[38;5;{run['fg']}m{run['t']}\x1b[0m" if "fg" in run else run["t"]
            for run in line
        )
        sys.stdout.write(text + "\n")
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    shore = scene(args, rng)
    fx = random.Random(rng.getrandbits(32))
    frame = _cli.pick(pilot, args.frame)
    size = (0, 0)
    shown: list[int] = []
    rocks: list[tuple[int, int]] = []
    beach: Sprite | None = None
    sent: Any = None  # the cells of the beach, as sent
    start = clock.monotonic() - args.at
    pointer = Pointer(lambda: clock.monotonic() - start)
    sight = _cli.Sight(pilot, frame)
    if shore.birds or shore.crabs:
        try:
            pilot.subscribe("hover")
        except PtyrcError as err:  # a driver without the topic
            if err.code != "bad-params":
                raise
    flight = fx.uniform(*GULL) / 4  # when the next gull goes
    for _ in _cli.ticks(PERIOD, seconds):
        seen = sight.look()
        for topic, data in sight.others:
            # Over the beach, or off every sprite; over a gull: no news.
            if topic != "hover":
                continue
            if data.get("sprite") is None:
                pointer.leave()
            elif beach is not None and data["sprite"] == beach.id:
                pointer.move(*data["at"])
        if not seen:
            continue
        w, h = frame.size
        t = clock.monotonic() - start
        if (w, h) != size or sight.changed:
            size = (w, h)
            shown = clear(sight.rows, w, h)
            rocks = rocks_of(sight.rows, w, h)
        with sight.drawing(), pilot.batch():
            cells = runs(shore.picture(w, h, t, rocks, pointer), w, shown)
            # The whole view is its box: a hover is told anywhere on it.
            if (pad := w - sum(len(run["t"]) for run in cells[0])) > 0:
                cells[0].append({"t": " " * pad})
            if beach is None:
                beach = pilot.draw(
                    cells=cells, frame=frame.id, at=(0, 0), space="view", z=0
                )
            elif cells != sent:
                beach.update(content={"cells": cells})
            sent = cells
            if args.gulls and t >= flight:
                flight = t + fx.uniform(*GULL)
                pilot.draw(frame=frame.id, **gull(fx, w, h))


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
        "shore",
        "A beach behind the text, seen from above: waves that run up "
        "the sand and leave their lace, a tide.",
        frame="largest",
    )
    out.add_argument(
        "--waves",
        type=float,
        default=8.0,
        metavar="SECONDS",
        help="the time between two waves [default: 8]",
    )
    out.add_argument(
        "--tide",
        type=float,
        default=300.0,
        metavar="SECONDS",
        help="the time the tide takes to come in and go out [default: 300]",
    )
    out.add_argument(
        "--day",
        type=float,
        default=240.0,
        metavar="SECONDS",
        help="with -n, the time from an afternoon to the next [default: 240]",
    )
    out.add_argument(
        "--sand",
        type=float,
        default=420.0,
        metavar="N",
        help="grains of sand over 80 by 24 cells [default: 420]",
    )
    out.add_argument(
        "--at",
        type=float,
        default=0.0,
        metavar="SECONDS",
        help="start this long into the scene [default: 0]",
    )
    out.add_argument("--seed", type=int, help="same seed, same shore")
    out.add_argument(
        "-d", "--drift", action="store_true", help="shells, pebbles, a line of wrack"
    )
    out.add_argument(
        "-r", "--ripples", action="store_true", help="ripple marks in the sand"
    )
    out.add_argument(
        "-c", "--crabs", action="store_true", help="crabs, sideways on the wet sand"
    )
    out.add_argument(
        "-b", "--birds", action="store_true", help="sandpipers at the water's edge"
    )
    out.add_argument(
        "-w", "--walker", action="store_true", help="footprints the waves wipe"
    )
    out.add_argument(
        "-g", "--gulls", action="store_true", help="the shadow of a gull now and then"
    )
    out.add_argument(
        "-n", "--night", action="store_true", help="dusk, night, the moon on the water"
    )
    out.add_argument("-a", "--all", action="store_true", help="-d -r -c -b -w -g -n")
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
        args.drift = args.ripples = args.crabs = args.birds = True
        args.walker = args.gulls = args.night = True
    if args.waves <= 0 or args.tide <= 0 or args.day <= 0:
        out.error("--waves, --tide and --day must be positive")
    if args.sand < 0:
        out.error("--sand must not be negative")
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
