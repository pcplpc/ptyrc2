# SPDX-License-Identifier: GPL-3.0-or-later
"""Pond: still water behind the text, seen from above; rings that
widen and break against the words, lily pads.

    ptyrc-app pond                  # water, lily pads, a drop now and then
    ptyrc-app pond -k -l            # koi under the surface, lotus in bloom
    ptyrc-app pond -r -m            # showers, tonight's moon in the water
    ptyrc-app pond -p -i            # what the season sheds, insects
    ptyrc-app pond --all
    ptyrc-app pond --print 80x24    # no driver: the scene on stdout

The pond is a map: a pad has a place in it, `(u, v)` in [0, 1), and
the map is laid over the pane, whatever its size. All of it is braille,
2 by 4 dots a cell; a dot is square, a ring is round.

What it shows:

- A scene that is a function of the time: a drop is drawn from the
  second it falls in (`drops`), so `Pond.picture` keeps no state but
  the koi. One sprite in view cells, behind the text, 8 times a
  second, sent when it changed.
- The screen read as a map (`_cli.Sight`): the words are stones. A
  drop on one makes no ring; a ring stops against them, and closes
  again behind a small one (`shadow`). Nothing is drawn in the cell
  on each side of a glyph. The koi steer clear of them.
- Rings: a drop sends a few. As one fades it loses arcs, each on its
  own bearing, then thins to one dot in two, one in three.
- `-m`: the moon in the water, tonight's phase (`stars.phase_of`),
  its rows shaken by the swell, a glitter over and under it; it
  crosses the pond in `--night` seconds.
- `-p`: the season of the day (`_sky.season_of`): petals in spring,
  leaves in autumn, adrift on a slow current.
- `-i`: water striders, a ring at each push; dragonflies, sprites
  the driver plays, moved by the client.
"""

from __future__ import annotations

import argparse
import datetime
import functools
import math
import random
import sys
from collections import deque
from collections.abc import Callable, Iterator, Mapping, Sequence
from typing import Any, NamedTuple

from ptyrc import Pilot, Sprite, clock

from . import _cli
from ._sky import SEASONS, season_of
from .life import DOTS
from .snake import walls_of
from .stars import Grid, clear, noise, phase_of, runs

__all__ = [
    "COATS",
    "Fly",
    "Koi",
    "Pad",
    "Pond",
    "Ripple",
    "drops",
    "index",
    "main",
    "moon_at",
    "shadow",
    "show",
]

Walls = Mapping[int, Sequence[int]]
"""The glyphs of the view: per row, the columns that hold one."""

PERIOD = 1 / 8  # seconds a tick
AREA = 80 * 24  # cells the densities count over

RING = (153, 117, 111, 75, 68, 61, 60, 23)  # a ring, as it fades
SHEEN = (18, 24, 31)  # light on the swell, by level
GREENS = (22, 28, 29, 34, 35, 64, 70)  # pads
BLOOMS = "✿❀✾"
LOTUS = (218, 212, 225, 231)
MOON = (230, 186, 144, 101)  # a row in place, one shaken; the glitter, as it fades
STRIDER = 180
FLIES = ((51, 45), (203, 197), (120, 84))  # a dragonfly: wings out, wings in
#: What floats by, a season: colours, dots a piece, pieces over `AREA`.
ADRIFT: dict[str, tuple[tuple[int, ...], int, float]] = {
    "spring": ((218, 225, 212, 231), 2, 16.0),
    "summer": ((231, 229, 151), 2, 7.0),
    "autumn": ((208, 172, 178, 166, 130), 3, 12.0),
    "winter": ((137, 144, 180), 3, 5.0),
}
#: Coats of a koi, head to tail.
COATS: tuple[tuple[int, ...], ...] = (
    (208,),
    (214, 214, 208, 208),
    (255, 255, 202, 202, 202, 255, 255, 202, 202, 255),
    (255, 202, 202, 255, 255, 255, 202, 255),
    (220, 220, 214),
    (255, 255, 240, 255, 240, 240, 255),
    (255,),
)
#: Half the width of a koi along its length, in dots; the last two the tail.
BODY = (1.0, 1.5, 1.8, 1.9, 1.8, 1.6, 1.3, 1.0, 0.8, 0.6, 0.5, 0.9, 1.4)

SPEED = 5.0  # of a ring, dots per second
GAP = 3.5  # dots from a ring to the next of its drop
ARCS = 40  # bearings a ring breaks along
BINS = 96  # bearings of a shadow
RAIN = 260.0  # drops a minute over `AREA`, at the height of a shower
SHOWER = 50.0  # seconds from a shower to the next
RISEN = 0.2  # share of its way the moon has made at the start
DOWN = 0.2  # share of a night the moon stays out
DISC = 6.0  # radius of the moon, dots
KISS = (6.0, 20.0)  # seconds between two rings of a koi

_UNIT = tuple(
    (math.cos(math.tau * i / 512), math.sin(math.tau * i / 512)) for i in range(512)
)


class Ripple(NamedTuple):
    """A drop: where, in dots, and when; its rings go for `life`
    seconds, `rings` of them, at `speed` dots per second."""

    key: int
    x: float
    y: float
    t0: float
    life: float
    rings: int
    speed: float = SPEED


def drops(
    seed: int,
    t: float,
    w: int,
    h: int,
    rate: float | Callable[[int], float],
    life: tuple[float, float] = (4.0, 9.0),
) -> Iterator[Ripple]:
    """The drops whose rings still go at `t` over `w` by `h` cells:
    `rate` a minute over `AREA`, or a function of the second. A drop
    is drawn from the second it falls in: the same at every call."""
    low, high = life
    share = w * h / AREA / 60.0
    for second in range(math.floor(t - high), math.floor(t) + 1):
        mean = (rate(second) if callable(rate) else rate) * share
        slot = seed * 0x2545F491 + second
        count = int(mean) + (noise(slot, 0) < mean - int(mean))
        for j in range(count):
            key = int(noise(slot, j + 1) * 2**32)
            t0 = second + noise(key, 1)
            # Most drops are small.
            size = noise(key, 2) ** 2
            lasts = low + (high - low) * size
            if t0 <= t < t0 + lasts:
                yield Ripple(
                    key,
                    noise(key, 3) * 2 * w,
                    noise(key, 4) * 4 * h,
                    t0,
                    lasts,
                    1 + round(2 * size + (high > 4.0) * 0.6),
                    SPEED if high > 4.0 else 4.0,
                )


def index(rows: Sequence[str], w: int, h: int) -> dict[int, list[int]]:
    """The glyphs of a view of `w` by `h` cells holding the text `rows`."""
    out: dict[int, list[int]] = {}
    for x, y in walls_of(rows):
        if x < w and y < h:
            out.setdefault(y, []).append(x)
    return out


def shadow(
    x: float, y: float, reach: float, walls: Walls
) -> tuple[list[float], list[float]] | None:
    """What the words hide of the rings of a drop at the dot `(x, y)`,
    as far as `reach` dots: per bearing, `BINS` a turn, the distances
    a ring is hidden from and to. A ring closes again behind a stone,
    three times its width further; behind a line it does not. None: the
    drop fell on a word."""
    near = [math.inf] * BINS
    for row in range(int(y - reach) >> 2, (int(y + reach) >> 2) + 1):
        for col in walls.get(row, ()):
            dx, dy = 2 * col + 1 - x, 4 * row + 2 - y
            d = math.hypot(dx, dy)
            if d > reach + 3:
                continue
            if d < 3:
                return None
            half = math.asin(min(1.0, 2.3 / d))
            mid = math.atan2(dy, dx)
            first = math.floor((mid - half) / math.tau * BINS)
            last = math.floor((mid + half) / math.tau * BINS)
            for b in range(first, last + 1):
                near[b % BINS] = min(near[b % BINS], d - 2)
    far = [math.inf] * BINS
    free = next((b for b in range(BINS) if near[b] == math.inf), None)
    if free is None:
        return near, far
    run: list[int] = []
    for b in range(free + 1, free + BINS + 1):
        b %= BINS
        if near[b] < math.inf:
            run.append(b)
            continue
        width = sum(near[i] for i in run) * math.tau / BINS
        for i in run:
            far[i] = near[i] + max(6.0, 3 * width)
        run = []
    return near, far


def moon_at(w: int, h: int, r: float, f: float) -> tuple[float, float]:
    """The middle of the moon in the water, in dots, `f` of its way
    over `w` by `h` cells: from the left to the right, on a shallow arc."""
    return (
        -r + f * (2 * w + 2 * r),
        4 * h * (0.55 - 0.25 * math.sin(math.pi * f)),
    )


class _Water:
    """Braille dots, one colour a cell: the last to write has it."""

    def __init__(self, w: int, h: int) -> None:
        self.w = w
        self.across, self.down = 2 * w, 4 * h
        self.cells: dict[int, list[int]] = {}

    def dot(self, px: int, py: int, colour: int) -> None:
        if 0 <= px < self.across and 0 <= py < self.down:
            key = (py >> 2) * self.w + (px >> 1)
            cell = self.cells.get(key)
            if cell is None:
                self.cells[key] = [DOTS[py & 3][px & 1], colour]
            else:
                cell[0] |= DOTS[py & 3][px & 1]
                cell[1] = colour

    def disc(self, cx: float, cy: float, r: float, colour: int) -> None:
        for py in range(math.floor(cy - r), math.ceil(cy + r) + 1):
            for px in range(math.floor(cx - r), math.ceil(cx + r) + 1):
                if (px + 0.5 - cx) ** 2 + (py + 0.5 - cy) ** 2 <= r * r:
                    self.dot(px, py, colour)

    def into(self, grid: Grid) -> None:
        for key, (bits, colour) in self.cells.items():
            grid[key // self.w][key % self.w] = (chr(0x2800 + bits), colour)


class Pad(NamedTuple):
    """A lily pad of the map: `(u, v)` the middle of its cluster, `(ox,
    oy)` dots from it; `r` its radius in dots, `turn` where its notch
    points, `bloom` a glyph of `BLOOMS` plus one, or 0."""

    u: float
    v: float
    ox: float
    oy: float
    r: float
    turn: float
    phase: float
    green: int
    bloom: int
    petal: int


@functools.cache
def _leaf(r10: int, turn: int) -> tuple[tuple[int, int], ...]:
    """The dots of a pad of radius `r10` tenths of a dot, about its
    middle: a disc less a notch towards `turn`, of 32 a turn."""
    r = r10 / 10
    out = []
    span = math.ceil(r)
    for dy in range(-span, span + 1):
        for dx in range(-span, span + 1):
            d = math.hypot(dx, dy)
            if d > r:
                continue
            off = (math.atan2(dy, dx) - math.tau * turn / 32) % math.tau
            if d > 0.9 and min(off, math.tau - off) < 0.34:
                continue
            out.append((dx, dy))
    return tuple(out)


class Koi:
    """A koi under the surface: it wanders, turns away from the words
    and the banks, and trails its body along the way it came."""

    def __init__(self, rng: random.Random, w: int, h: int) -> None:
        self.rng = rng
        self.x = rng.uniform(0, 2 * w)
        self.y = rng.uniform(0, 4 * h)
        self.heading = rng.uniform(0, math.tau)
        self.turn = 0.0  # radians per second
        self.speed = rng.uniform(2.6, 4.6)  # dots per second
        self.size = rng.uniform(1.2, 1.7)
        self.coat = rng.choice(COATS)
        self.beat = rng.uniform(0, math.tau)
        self.kiss = rng.uniform(2.0, KISS[1])  # seconds to its next ring
        #: Where its head was, a dot apart, the last first.
        self.trail: deque[tuple[float, float]] = deque(maxlen=24)
        self.trail.append((self.x, self.y))

    def step(
        self, dt: float, w: int, h: int, stone: Callable[[float, float], bool]
    ) -> None:
        """Swim for `dt` seconds over `w` by `h` cells; `stone(x, y)`:
        the dot is under a word."""
        rng = self.rng
        across, down = 2 * w, 4 * h

        def shut(angle: float) -> bool:
            x = self.x + 10 * math.cos(self.heading + angle)
            y = self.y + 10 * math.sin(self.heading + angle)
            return not (-4 <= x < across + 4 and -4 <= y < down + 4) or stone(x, y)

        self.turn += (rng.gauss(0.0, 2.2) - 1.4 * self.turn) * dt
        if not (0 <= self.x < across and 0 <= self.y < down):
            # Out, after a resize: back to the middle.
            home = math.atan2(down / 2 - self.y, across / 2 - self.x)
            off = (home - self.heading + math.pi) % math.tau - math.pi
            self.turn = max(-1.6, min(1.6, 2 * off))
        else:
            left, ahead, right = shut(-0.6), shut(0.0), shut(0.6)
            if left and not right:
                self.turn += 5 * dt
            elif right and not left:
                self.turn -= 5 * dt
            elif ahead:
                self.turn += (5 if self.turn >= 0 else -5) * dt
        self.turn = max(-1.6, min(1.6, self.turn))
        self.heading += self.turn * dt
        self.x += self.speed * dt * math.cos(self.heading)
        self.y += self.speed * dt * math.sin(self.heading)
        px, py = self.trail[0]
        gone = math.hypot(self.x - px, self.y - py)
        if gone > 8:  # a jump: the tail does not stretch over it
            self.trail.clear()
            self.trail.append((self.x, self.y))
        while gone >= 1.0:
            px += (self.x - px) / gone
            py += (self.y - py) / gone
            self.trail.appendleft((px, py))
            gone -= 1.0
        self.kiss -= dt

    def draw(self, water: _Water, t: float) -> None:
        """Its body, tail first: a disc a point of `BODY`, swung
        across its way more and more towards the tail."""
        n = len(BODY)
        trail = self.trail
        for i in range(n - 1, -1, -1):
            j = round(i * self.size)
            if j >= len(trail):
                continue
            x, y = trail[j]
            ax, ay = trail[max(0, j - 1)]
            bx, by = trail[min(len(trail) - 1, j + 1)]
            way = math.hypot(ax - bx, ay - by) or 1.0
            f = i / n
            swing = 1.3 * f * f * math.sin(1.4 * self.speed * t + self.beat - 5 * f)
            x += swing * (by - ay) / way
            y += swing * (ax - bx) / way
            colour = self.coat[i * len(self.coat) // n]
            water.disc(x, y, max(0.8, BODY[i] * self.size**0.7), colour)


class Pond:
    """The scene: what the water shows at a time.

    `pads`: lily pads over `AREA`; `drops`: drops a minute over it.
    `koi`, `lotus`, `rain`, `moon`, `adrift`, `striders`: what `-k -l -r
    -m -p -i` add. `phase`: of the moon; `night`: seconds it takes to
    cross; `season`: one of `SEASONS`. The same seed gives the same
    pond, at every size."""

    def __init__(
        self,
        rng: random.Random,
        *,
        pads: float = 9.0,
        drops: float = 24.0,
        koi: bool = False,
        lotus: bool = False,
        rain: bool = False,
        moon: bool = False,
        adrift: bool = False,
        striders: bool = False,
        phase: float = 0.5,
        night: float = 300.0,
        season: str = "summer",
    ) -> None:
        self.density = pads
        self.rate = drops
        self.lotus, self.rain, self.moon = lotus, rain, moon
        self.adrift, self.striders = adrift, striders
        self.phase, self.night, self.season = phase, night, season
        self.seed = rng.getrandbits(30)
        self._pads = random.Random(rng.getrandbits(32))
        self._fish = random.Random(rng.getrandbits(32))
        self.current = rng.uniform(0, math.tau)  # where the water goes
        self.pads: list[Pad] = []
        self._cluster: list[Pad] = []
        self._left = 0
        self.koi: list[Koi] | None = [] if koi else None
        self.kisses: list[Ripple] = []
        self._walls: Walls | None = None
        self._shadows: dict[int, tuple[list[float], list[float]] | None] = {}

    # The map.

    def _pad(self) -> Pad:
        rng = self._pads
        if self._left <= 0:
            self._cluster = []
            self._left = rng.choice((1, 2, 2, 3, 3, 4, 5))
            self._at = rng.random(), rng.random()
        self._left -= 1
        r = 2.4 + 4.2 * rng.random() ** 1.6
        ox = oy = 0.0
        for tries in range(48):
            if all(
                math.hypot(ox - p.ox, oy - p.oy) >= r + p.r + 0.6 for p in self._cluster
            ):
                break
            # Further out at each try: one fits in the end.
            far = r + 3.0 + 2.0 * len(self._cluster) + tries
            ox, oy = rng.uniform(-far, far), rng.uniform(-far, far)
        pad = Pad(
            *self._at,
            ox,
            oy,
            r,
            rng.uniform(0, math.tau),
            rng.uniform(0, math.tau),
            rng.choice(GREENS),
            rng.randrange(1, len(BLOOMS) + 1) * (r >= 3.4 and rng.random() < 0.45),
            rng.choice(LOTUS),
        )
        self._cluster.append(pad)
        return pad

    def leaves(self, w: int, h: int, t: float = 0.0) -> list[tuple[int, int, Pad]]:
        """The pads over `w` by `h` cells at `t`, each with the dot of
        its middle: it sways by a dot."""
        n = int(self.density * w * h / AREA)
        while len(self.pads) < n:
            self.pads.append(self._pad())
        return [
            (
                round(p.u * 2 * w + p.ox + 0.7 * math.sin(0.31 * t + p.phase)),
                round(p.v * 4 * h + p.oy + 0.7 * math.cos(0.23 * t + 2 * p.phase)),
                p,
            )
            for p in self.pads[:n]
        ]

    # What moves.

    def shower(self, second: float) -> float:
        """Drops a minute over `AREA` at `second`, with `rain`: showers
        come and go."""
        wave = 0.5 + 0.5 * math.sin(math.tau * second / SHOWER + self.seed)
        return RAIN * (0.12 + 0.88 * wave * wave)

    def skaters(
        self, w: int, h: int, t: float, dry: Callable[[float, float], bool]
    ) -> Iterator[tuple[float, float, Ripple]]:
        """The water striders at `t`: where each is, in dots, and the
        ring of its last push. One rests, then darts to a place near
        its home that `dry` does not hold."""
        for i in range(max(1, int(3 * w * h / AREA))):
            key = int(noise(self.seed, 7000 + i) * 2**32)
            rest = 1.6 + 2.2 * noise(key, 1)
            beat = t / rest + noise(key, 2)
            hop = math.floor(beat)

            def place(hop: int, key: int = key) -> tuple[float, float]:
                x = y = 0.0
                for k in range(4):
                    x = noise(key, 3) * 2 * w + 18 * (noise(key, 8 * hop + k) - 0.5)
                    y = noise(key, 4) * 4 * h + 18 * (noise(key, 8 * hop + k + 4) - 0.5)
                    if not dry(x, y):
                        break
                return x, y

            (x0, y0), (x1, y1) = place(hop - 1), place(hop)
            since = (beat - hop) * rest
            f = min(1.0, since / 0.3)
            f = f * (2 - f)
            yield (
                x0 + (x1 - x0) * f,
                y0 + (y1 - y0) * f,
                Ripple(key + hop, x0, y0, t - since, 1.5, 1, 3.0),
            )

    def step(
        self, dt: float, w: int, h: int, t: float, walls: Walls | None = None
    ) -> None:
        """Move the koi by `dt` seconds, to the time `t`."""
        self.kisses = [one for one in self.kisses if t < one.t0 + one.life]
        if self.koi is None:
            return
        want = min(7, max(1, round(2 * math.sqrt(w * h / AREA))))
        while len(self.koi) < want:
            self.koi.append(Koi(self._fish, w, h))
        del self.koi[want:]

        def stone(x: float, y: float) -> bool:
            return walls is not None and int(x) >> 1 in walls.get(int(y) >> 2, ())

        for fish in self.koi:
            fish.step(dt, w, h, stone)
            if fish.kiss <= 0:
                fish.kiss = self._fish.uniform(*KISS)
                key = self._fish.getrandbits(30)
                self.kisses.append(Ripple(key, fish.x, fish.y, t, 4.5, 2, 4.0))

    # The picture.

    def _rings(
        self, water: _Water, ripples: Sequence[Ripple], t: float, walls: Walls | None
    ) -> None:
        if walls is not self._walls:
            self._walls, self._shadows = walls, {}
        shadows: dict[int, tuple[list[float], list[float]] | None] = {}
        rings: list[tuple[float, float, int, Ripple]] = []
        for one in ripples:
            shade = None
            if walls:
                if one.key in self._shadows:
                    shade = self._shadows[one.key]
                else:
                    reach = one.speed * one.life
                    shade = shadow(one.x, one.y, reach, walls)
                shadows[one.key] = shade
                if shade is None:
                    continue
            age = t - one.t0
            if not 0 <= age < one.life:
                continue
            for k in range(one.rings):
                r = one.speed * age - GAP * k
                if r >= 0:
                    strength = (1 - age / one.life) * 0.8**k
                    rings.append((strength, r, k, one))
        self._shadows = shadows
        # The strongest last: it has the cells it shares.
        rings.sort(key=lambda ring: ring[0])
        for strength, r, k, one in rings:
            colour = RING[int((1 - strength) ** 1.5 * (len(RING) - 0.01))]
            if r < 1.0:
                water.dot(math.floor(one.x), math.floor(one.y), colour)
                continue
            shade = shadows.get(one.key)
            whole = min(1.0, 0.6 + strength)
            # Fainter: every other dot, then one in three.
            step = 1 if strength > 0.45 else 2 if strength > 0.2 else 3
            arcs = [noise(one.key, 64 * k + a) < whole for a in range(ARCS)]
            n = max(8, int(math.tau * r))
            for i in range(0, n, step):
                if not arcs[i * ARCS // n]:
                    continue
                if shade is not None:
                    b = i * BINS // n
                    if shade[0][b] <= r < shade[1][b]:
                        continue
                cos, sin = _UNIT[i * 512 // n]
                water.dot(
                    math.floor(one.x + r * cos), math.floor(one.y + r * sin), colour
                )

    def _sheen(self, water: _Water, w: int, h: int, t: float) -> None:
        """Light on the swell: dashes that show where two waves meet."""
        for i in range(int(170 * w * h / AREA)):
            x = int(noise(self.seed, 2 * i + 100000) * 2 * w)
            y = int(noise(self.seed, 2 * i + 100001) * 4 * h)
            wave = math.sin(0.07 * x + 0.045 * y - 0.5 * t)
            wave += math.sin(-0.035 * x + 0.09 * y - 0.37 * t + 1.7)
            level = int((wave - 1.1 - 0.2 * noise(i, 5)) / 0.25)
            if level < 0:
                continue
            level = min(level, len(SHEEN) - 1)
            for dx in range(1 + level + (i & 1)):
                water.dot(x + dx, y, SHEEN[level])

    def _moon(self, water: _Water, w: int, h: int, t: float) -> None:
        f = (t / self.night + RISEN) % (1.0 + DOWN)
        if f > 1.0:
            return
        r, phase = DISC, self.phase
        cx, cy = moon_at(w, h, r, f)
        edge = math.cos(math.tau * phase)
        lit = 0.5 - 0.5 * edge  # the share of the disc that shows
        for py in range(math.floor(cy - 2.6 * r), math.ceil(cy + 2.6 * r) + 1):
            y = py + 0.5 - cy
            shift = round(
                1.5 * math.sin(0.8 * py + 1.9 * t)
                + 0.8 * math.sin(0.31 * py - 1.3 * t + 2.0)
            )
            if abs(y) >= r:
                # The glitter: dashes over and under, fewer further out.
                out = (abs(y) - r) / (1.6 * r)
                if noise(py, int(1.5 * t + 7 * noise(py))) < out + 0.35 / (lit + 0.05):
                    continue
                reach = max(1, round(lit * r * (1 - out) * noise(py, 3)))
                for px in range(-reach, reach + 1):
                    water.dot(round(cx) + 2 * shift + px, py, MOON[2 + (out > 0.5)])
                continue
            if math.sin(0.9 * py - 2.3 * t) > 0.86:
                continue
            half = math.sqrt(r * r - y * y)
            for px in range(math.floor(cx - half), math.ceil(cx + half) + 1):
                x = px + 0.5 - cx
                if abs(x) > half:
                    continue
                if x > edge * half if phase < 0.5 else x < -edge * half:
                    water.dot(px + shift, py, MOON[abs(shift) > 1])

    def _drift(self, water: _Water, w: int, h: int, t: float) -> None:
        colours, size, density = ADRIFT[self.season]
        across, down = 2 * w, 4 * h
        for i in range(int(density * w * h / AREA)):
            key = int(noise(self.seed, 5000 + i) * 2**32)
            pace = 0.5 + 0.9 * noise(key, 1)
            sway = math.tau * noise(key, 2)
            x = noise(key, 3) * across + pace * t * math.cos(self.current)
            y = noise(key, 4) * down + pace * t * math.sin(self.current)
            x += 3.0 * math.sin(0.17 * t + sway)
            y += 2.0 * math.cos(0.13 * t + sway)
            px, py = math.floor(x % across), math.floor(y % down)
            colour = colours[key % len(colours)]
            # It turns as it goes.
            spin = int(0.12 * t + 4 * noise(key, 5)) % 4
            dx, dy = ((1, 0), (1, 1), (0, 1), (-1, 1))[spin]
            for k in range(size):
                water.dot(px + k * dx, py + k * dy, colour)

    def picture(
        self, w: int, h: int, t: float = 0.0, walls: Walls | None = None
    ) -> Grid:
        """The pond over `w` by `h` cells at `t` seconds. `walls`: the
        glyphs of the view, as `index` gives them."""
        grid: Grid = [{} for _ in range(max(0, h))]
        if w <= 0 or h <= 0:
            return grid
        water = _Water(w, h)
        leaves = self.leaves(w, h, t)

        def dry(x: float, y: float) -> bool:
            return any(math.hypot(x - px, y - py) < p.r + 1.5 for px, py, p in leaves)

        self._sheen(water, w, h, t)
        ripples = [
            one for one in drops(self.seed, t, w, h, self.rate) if not dry(one.x, one.y)
        ]
        if self.rain:
            ripples += [
                one
                for one in drops(self.seed + 1, t, w, h, self.shower, (1.2, 2.4))
                if not dry(one.x, one.y)
            ]
        ripples += self.kisses
        skaters = list(self.skaters(w, h, t, dry)) if self.striders else []
        ripples += [push for _, _, push in skaters]
        self._rings(water, ripples, t, walls)
        for fish in self.koi or ():
            fish.draw(water, t)
        if self.moon:
            self._moon(water, w, h, t)
        if self.adrift:
            self._drift(water, w, h, t)
        for px, py, pad in leaves:
            turn = pad.turn + 0.3 * math.sin(0.11 * t + pad.phase)
            for dx, dy in _leaf(round(10 * pad.r), round(turn / math.tau * 32) % 32):
                water.dot(px + dx, py + dy, pad.green)
        for x, y, _ in skaters:
            px, py = math.floor(x), math.floor(y)
            for dx, dy in ((0, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)):
                water.dot(px + dx, py + dy, STRIDER)
        water.into(grid)
        if self.lotus:
            for px, py, pad in leaves:
                if pad.bloom and 0 <= px < 2 * w and 0 <= py < 4 * h:
                    grid[py >> 2][px >> 1] = (BLOOMS[pad.bloom - 1], pad.petal)
        return grid


class Fly:
    """A dragonfly: it hovers, then darts to another place."""

    def __init__(self, rng: random.Random, w: int, h: int) -> None:
        self.rng = rng
        self.x, self.y = rng.uniform(0, w), rng.uniform(0, h)
        self.goal = (self.x, self.y)
        self.wait = rng.uniform(0.5, 3.0)  # seconds it still hovers
        self.inks = rng.choice(FLIES)
        self.sprite: Sprite | None = None
        self.cell: tuple[int, int] | None = None

    def frames(self) -> list[dict[str, Any]]:
        out, wings = self.inks
        return [
            {"cells": [[{"t": "‡", "fg": out, "bold": True}]], "ms": 90},
            {"cells": [[{"t": "╪", "fg": wings}]], "ms": 90},
        ]

    def step(self, dt: float, w: int, h: int) -> tuple[int, int]:
        """Fly for `dt` seconds; the cell it is in."""
        rng = self.rng
        gx, gy = self.goal
        far = math.hypot(gx - self.x, 2 * (gy - self.y))
        if far > 0.5:
            go = min(1.0, 34.0 * dt / far)  # cells a second, a row two
            self.x += (gx - self.x) * go
            self.y += (gy - self.y) * go
        else:
            self.wait -= dt
            if self.wait <= 0:
                self.wait = rng.uniform(0.8, 5.0)
                if rng.random() < 0.35:  # a long way; else a hop
                    self.goal = (rng.uniform(0, w), rng.uniform(0, h))
                else:
                    self.goal = (
                        self.x + rng.uniform(-9, 9),
                        self.y + rng.uniform(-3, 3),
                    )
        self.goal = (
            max(0.0, min(w - 0.01, self.goal[0])),
            max(0.0, min(h - 0.01, self.goal[1])),
        )
        return (max(0, min(w - 1, int(self.x))), max(0, min(h - 1, int(self.y))))


def scene(args: argparse.Namespace, rng: random.Random) -> Pond:
    phase = args.phase
    if phase is None:
        phase = phase_of(datetime.datetime.now(datetime.UTC))
    season = args.season
    if season is None:
        season = season_of(datetime.date.today().timetuple().tm_yday)
    return Pond(
        rng,
        pads=args.pads,
        drops=args.drops,
        koi=args.koi,
        lotus=args.lotus,
        rain=args.rain,
        moon=args.moon,
        adrift=args.petals,
        striders=args.insects,
        phase=phase,
        night=args.night,
        season=season,
    )


def show(args: argparse.Namespace) -> int:
    """`--print`: the scene once on stdout, no driver."""
    w, h = args.print
    pond = scene(args, random.Random(args.seed))
    for i in range(80):  # the koi have come some way
        pond.step(PERIOD, w, h, (i - 80) * PERIOD)
    for line in runs(pond.picture(w, h, 0.0), w):
        text = "".join(
            f"\x1b[38;5;{run['fg']}m{run['t']}\x1b[0m" if "fg" in run else run["t"]
            for run in line
        )
        sys.stdout.write(text + "\n")
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    pond = scene(args, rng)
    fx = random.Random(rng.getrandbits(32))
    frame = _cli.pick(pilot, args.frame)
    sight = _cli.Sight(pilot, frame)
    size = (0, 0)
    shown: list[int] = []
    walls: dict[int, list[int]] = {}
    water: Sprite | None = None
    sent: Any = None  # the cells of the water, as sent
    flies: list[Fly] = []
    start = clock.monotonic()
    for dt in _cli.ticks(PERIOD, seconds):
        if not sight.look():
            continue
        w, h = frame.size
        t = clock.monotonic() - start
        if (w, h) != size or sight.changed:
            size = (w, h)
            shown = clear(sight.rows, w, h)
            walls = index(sight.rows, w, h)
        pond.step(dt, w, h, t, walls)
        with sight.drawing(), pilot.batch():
            cells = runs(pond.picture(w, h, t, walls), w, shown)
            if water is None:
                water = pilot.draw(
                    cells=cells, frame=frame.id, at=(0, 0), space="view", z=0
                )
            elif cells != sent:
                water.update(content={"cells": cells})
            sent = cells
            if args.insects:
                want = max(1, min(4, w * h // 2400))
                while len(flies) < want:
                    flies.append(Fly(fx, w, h))
                for fly in flies[want:]:
                    if fly.sprite is not None:
                        fly.sprite.remove()
                del flies[want:]
                for fly in flies:
                    cell = fly.step(dt, w, h)
                    if fly.sprite is None:
                        fly.sprite = pilot.draw(
                            frames=fly.frames(),
                            loop=True,
                            frame=frame.id,
                            at=cell,
                            space="view",
                            z=3,
                        )
                    elif cell != fly.cell:
                        fly.sprite.move(*cell)
                    fly.cell = cell


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
        "pond",
        "Still water behind the text, seen from above: rings that widen "
        "and break against the words, lily pads.",
        frame="largest",
    )
    out.add_argument(
        "--pads",
        type=float,
        default=9.0,
        metavar="N",
        help="lily pads over 80 by 24 cells [default: 9]",
    )
    out.add_argument(
        "--drops",
        type=float,
        default=24.0,
        metavar="N",
        help="drops a minute over 80 by 24 cells [default: 24]",
    )
    out.add_argument(
        "--night",
        type=float,
        default=300.0,
        metavar="SECONDS",
        help="the time the moon takes to cross the pond, with -m [default: 300]",
    )
    out.add_argument(
        "--phase",
        type=float,
        metavar="F",
        help="phase of the moon: 0 new, 0.25 first quarter, 0.5 full "
        "[default: tonight's]",
    )
    out.add_argument(
        "--season",
        choices=SEASONS,
        help="what floats by, with -p [default: today's]",
    )
    out.add_argument("--seed", type=int, help="same seed, same pond")
    out.add_argument("-k", "--koi", action="store_true", help="koi under the surface")
    out.add_argument(
        "-l", "--lotus", action="store_true", help="lotus in bloom on the pads"
    )
    out.add_argument(
        "-r", "--rain", action="store_true", help="showers that come and go"
    )
    out.add_argument("-m", "--moon", action="store_true", help="the moon in the water")
    out.add_argument(
        "-p",
        "--petals",
        action="store_true",
        help="what the season sheds, adrift: petals, leaves",
    )
    out.add_argument(
        "-i",
        "--insects",
        action="store_true",
        help="water striders, dragonflies",
    )
    out.add_argument("-a", "--all", action="store_true", help="-k -l -r -m -p -i")
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
        args.koi = args.lotus = args.rain = True
        args.moon = args.petals = args.insects = True
    if args.night <= 0:
        out.error("--night must be positive")
    if args.pads < 0 or args.drops < 0:
        out.error("--pads and --drops must not be negative")
    if args.phase is not None:
        args.phase %= 1.0
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
