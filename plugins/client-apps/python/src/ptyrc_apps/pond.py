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
  the koi and the pointer. One sprite in view cells, behind the text,
  20 times a second, sent when it changed. Few things, dull colours.
- A slow current (`--current`): the light on the swell, what floats
  and the striders at rest go with it.
- The pointer over the pane (`Pond.touch`, from the `hover` events):
  feared while it moves and `CALM` seconds after. The koi turn their
  tail to it and dart, a ring where they took fright; the striders
  slide off; a dragonfly leaves for another place.
- The screen read as a map (`_cli.Sight`): the words are stones. A
  drop on one makes no ring; a ring stops against them, and closes
  again behind a small one (`shadow`). Nothing is drawn in the cell
  on each side of a glyph. The koi steer clear of them.
- Rings: a drop sends one or two, that leave fast and slow down
  (`Ripple.ring`). Whole as it leaves, a ring then loses arcs, each on
  its own bearing, and thins to one dot in two, one in three.
- `-m`: the moon in the water, tonight's phase (`stars.phase_of`),
  its rows shaken by the swell, a glitter over and under it; it
  crosses the pond in `--night` seconds.
- `-p`: the season of the day (`_sky.season_of`): petals in spring,
  leaves in autumn, adrift on the current.
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

from ptyrc import Pilot, PtyrcError, Sprite, clock

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

PERIOD = 1 / 20  # seconds a tick
AREA = 80 * 24  # cells the densities count over

# Colours of the 256: greys and the dull steps of the cube.
RING = (152, 109, 103, 67, 66, 60, 59, 238)  # a ring, as it fades
SHEEN = (237, 239, 242)  # light on the swell, by level
GREENS = (65, 65, 71, 101, 108)  # pads
BLOOMS = "✿❀✾"
LOTUS = (224, 181, 182, 230)
MOON = (230, 187, 144, 101)  # a row in place, one shaken; the glitter, as it fades
STRIDER = 144
FLIES = ((116, 73), (174, 131), (151, 108))  # a dragonfly: wings out, wings in
#: What floats by, a season: colours, dots a piece, pieces over `AREA`.
ADRIFT: dict[str, tuple[tuple[int, ...], int, float]] = {
    "spring": ((224, 225, 182, 231), 2, 8.0),
    "summer": ((230, 187, 151), 2, 4.0),
    "autumn": ((173, 137, 179, 180, 131), 3, 7.0),
    "winter": ((138, 144, 102), 3, 3.0),
}
#: Coats of a koi, head to tail.
COATS: tuple[tuple[int, ...], ...] = (
    (173,),
    (216, 216, 173, 173),
    (254, 254, 167, 167, 167, 254, 254, 167, 167, 254),
    (254, 167, 167, 254, 254, 254, 167, 254),
    (222, 222, 179),
    (254, 254, 245, 254, 245, 245, 254),
    (253,),
)
#: Half the width of a koi along its length, in dots; the last two the tail.
BODY = (1.0, 1.5, 1.8, 1.9, 1.8, 1.6, 1.3, 1.0, 0.8, 0.6, 0.5, 0.9, 1.4)

REACH = 12.0  # dots a ring is wide by when it dies
LAG = 0.22  # share of the life of a drop from a ring to the next
ARCS = 40  # bearings a ring breaks along
FLOW = 0.6  # the current, dots per second
CALM = 3.0  # seconds a pointer that rests is feared
SHY = 26.0  # dots a koi keeps from the pointer
SKIT = 16.0  # dots a strider keeps from it
WARY = 7.0  # cells a dragonfly keeps from it
BINS = 96  # bearings of a shadow
RAIN = 110.0  # drops a minute over `AREA`, at the height of a shower
SHOWER = 50.0  # seconds from a shower to the next
RISEN = 0.2  # share of its way the moon has made at the start
DOWN = 0.2  # share of a night the moon stays out
#: Of a drop of rain: its life, its reach, its rings, as `drops` takes them.
SHOWERS = ((0.7, 1.3), (2.5, 6.0), 1)
DISC = 6.0  # radius of the moon, dots
KISS = (6.0, 20.0)  # seconds between two rings of a koi

_UNIT = tuple(
    (math.cos(math.tau * i / 512), math.sin(math.tau * i / 512)) for i in range(512)
)


class Ripple(NamedTuple):
    """A drop: where, in dots, and when; `rings` rings that go for
    `life` seconds, the first `reach` dots wide by then."""

    key: int
    x: float
    y: float
    t0: float
    life: float
    rings: int
    reach: float = REACH

    def ring(self, age: float, k: int = 0) -> tuple[float, float] | None:
        """Ring `k` at `age` seconds: its radius in dots, its strength
        from 1 to 0. It leaves fast and slows down: the cube of what is
        left of its time. None: not out yet, or gone."""
        late = LAG * k
        a = (age / self.life - late) / (1 - late)
        if not 0 <= a < 1:
            return None
        return self.reach * (1 - 0.3 * k) * (1 - (1 - a) ** 3), (1 - a) * 0.75**k


def drops(
    seed: int,
    t: float,
    w: int,
    h: int,
    rate: float | Callable[[int], float],
    life: tuple[float, float] = (1.4, 3.2),
    reach: tuple[float, float] = (6.0, 20.0),
    rings: int = 2,
) -> Iterator[Ripple]:
    """The drops whose rings still go at `t` over `w` by `h` cells:
    `rate` a minute over `AREA`, or a function of the second. A drop
    is drawn from the second it falls in: the same at every call.
    `life`, `reach`: of the smallest, of the largest; `rings`: at most."""
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
                    1 + (rings > 1 and size > 0.25) + (rings > 2 and size > 0.6),
                    reach[0] + (reach[1] - reach[0]) * size,
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


@functools.lru_cache(maxsize=512)
def _arcs(key: int, k: int) -> tuple[float, ...]:
    """Per bearing of ring `k` of the drop `key`, what it takes to
    show there: a number in [0, 1)."""
    return tuple(noise(key, 64 * k + a) for a in range(ARCS))


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
        self.dash = 0.0  # its fright, 1 to 0: it swims faster
        #: Where its head was, a dot apart, the last first.
        self.trail: deque[tuple[float, float]] = deque(maxlen=24)
        self.trail.append((self.x, self.y))

    def step(
        self,
        dt: float,
        w: int,
        h: int,
        stone: Callable[[float, float], bool],
        scare: tuple[float, float, float] | None = None,
    ) -> None:
        """Swim for `dt` seconds over `w` by `h` cells; `stone(x, y)`:
        the dot is under a word. `scare`: the pointer, as `Pond.fear`;
        within `SHY` dots of it the koi turns its tail to it and darts."""
        rng = self.rng
        across, down = 2 * w, 4 * h

        def shut(angle: float) -> bool:
            x = self.x + 10 * math.cos(self.heading + angle)
            y = self.y + 10 * math.sin(self.heading + angle)
            return not (-4 <= x < across + 4 and -4 <= y < down + 4) or stone(x, y)

        self.turn += (rng.gauss(0.0, 2.2) - 1.4 * self.turn) * dt
        self.dash *= math.exp(-dt / 0.8)
        limit = 1.6
        if scare is not None:
            sx, sy, fright = scare
            if math.hypot(self.x - sx, self.y - sy) < SHY:
                away = math.atan2(self.y - sy, self.x - sx)
                off = (away - self.heading + math.pi) % math.tau - math.pi
                limit = 1.6 + 4.4 * fright
                self.turn = max(-limit, min(limit, 6 * off))
                self.dash = max(self.dash, fright)
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
        self.turn = max(-limit, min(limit, self.turn))
        self.heading += self.turn * dt
        pace = self.speed * (1 + 2.5 * self.dash) * dt
        self.x += pace * math.cos(self.heading)
        self.y += pace * math.sin(self.heading)
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
    cross; `season`: one of `SEASONS`; `current`: dots a second the
    water goes by. The same seed gives the same pond, at every size.

    `touch`: the pointer came on a dot. What lives here fears it for
    `CALM` seconds after its last move (`fear`): the koi dart away, the
    striders slide off."""

    def __init__(
        self,
        rng: random.Random,
        *,
        pads: float = 6.0,
        drops: float = 10.0,
        koi: bool = False,
        lotus: bool = False,
        rain: bool = False,
        moon: bool = False,
        adrift: bool = False,
        striders: bool = False,
        phase: float = 0.5,
        night: float = 300.0,
        season: str = "summer",
        current: float = FLOW,
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
        #: The current, dots per second across and down.
        self.flow = (current * math.cos(self.current), current * math.sin(self.current))
        self.pointer: tuple[float, float, float] | None = None
        self.pads: list[Pad] = []
        self._cluster: list[Pad] = []
        self._left = 0
        self.koi: list[Koi] | None = [] if koi else None
        self.kisses: list[Ripple] = []
        self._walls: Walls | None = None
        self._shadows: dict[int, tuple[list[float], list[float]] | None] = {}
        # What does not change with the time, by size: worked out once.
        self._glints: tuple[Any, list[tuple[float, float, float]]] = (None, [])
        self._adrift: tuple[Any, list[tuple[Any, ...]]] = (None, [])

    # The pointer.

    def touch(self, x: float, y: float, t: float) -> None:
        """The pointer came on the dot `(x, y)` at `t`."""
        self.pointer = (x, y, t)

    def fear(self, t: float) -> tuple[float, float, float] | None:
        """The pointer as felt at `t`: its dot, and the fright from 1,
        as it moves, to 0, `CALM` seconds after. None: none is felt."""
        if self.pointer is None:
            return None
        x, y, t0 = self.pointer
        f = 1 - (t - t0) / CALM
        if not 0 < f <= 1:
            return None
        return x, y, f * f * (3 - 2 * f)

    # The map.

    def _pad(self) -> Pad:
        rng = self._pads
        if self._left <= 0:
            self._cluster = []
            self._left = rng.choice((1, 2, 2, 3, 3, 4, 5))
            self._at = rng.random(), rng.random()
        self._left -= 1
        r = 2.4 + 3.2 * rng.random() ** 1.6
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
        ring of its last push. One rests, adrift on the current, then
        darts to a place near its home that `dry` does not hold."""
        fx, fy = self.flow
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
            x0, y0 = x0 + fx * rest, y0 + fy * rest  # where the rest left it
            since = (beat - hop) * rest
            f = min(1.0, since / 0.25)
            f = f * (2 - f)
            yield (
                x0 + (x1 - x0) * f + fx * since,
                y0 + (y1 - y0) * f + fy * since,
                Ripple(key + hop, x0, y0, t - since, 0.9, 1, 4.0),
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

        scare = self.fear(t)
        for fish in self.koi:
            calm = fish.dash < 0.15
            fish.step(dt, w, h, stone, scare)
            # A ring where it is, now and then; one where it took fright.
            if fish.kiss <= 0 or (calm and fish.dash > 0.5):
                fish.kiss = self._fish.uniform(*KISS)
                key = self._fish.getrandbits(30)
                self.kisses.append(Ripple(key, fish.x, fish.y, t, 2.0, 2, 9.0))

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
                    shade = shadow(one.x, one.y, one.reach, walls)
                shadows[one.key] = shade
                if shade is None:
                    continue
            age = t - one.t0
            if not 0 <= age < one.life:
                continue
            for k in range(one.rings):
                ring = one.ring(age, k)
                if ring is not None:
                    rings.append((ring[1], ring[0], k, one))
        self._shadows = shadows
        # The strongest last: it has the cells it shares.
        rings.sort(key=lambda ring: ring[0])
        for strength, r, k, one in rings:
            colour = RING[int((1 - strength) ** 1.3 * (len(RING) - 0.01))]
            if r < 1.0:
                water.dot(math.floor(one.x), math.floor(one.y), colour)
                continue
            shade = shadows.get(one.key)
            whole = min(1.0, 0.5 + strength)
            # Whole as it leaves; then every other dot, then one in three.
            step = 1 if strength > 0.8 or r < 3 else 2 if strength > 0.35 else 3
            arcs = [arc < whole for arc in _arcs(one.key, k)]
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
        """Light on the swell: dashes adrift on the current, that show
        where two waves meet."""
        fx, fy = self.flow
        if self._glints[0] != (w, h):
            self._glints = (
                (w, h),
                [
                    (
                        noise(self.seed, 2 * i + 100000) * 2 * w,
                        noise(self.seed, 2 * i + 100001) * 4 * h,
                        0.2 * noise(i, 5),
                    )
                    for i in range(int(90 * w * h / AREA))
                ],
            )
        for x0, y0, dull in self._glints[1]:
            x = int((x0 + fx * t) % (2 * w))
            y = int((y0 + fy * t) % (4 * h))
            wave = math.sin(0.07 * x + 0.045 * y - 0.5 * t)
            wave += math.sin(-0.035 * x + 0.09 * y - 0.37 * t + 1.7)
            level = int((wave - 1.15 - dull) / 0.25)
            if level < 0:
                continue
            level = min(level, len(SHEEN) - 1)
            for dx in range(1 + level):
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
        if self._adrift[0] != (w, h, self.season):
            keys = [
                int(noise(self.seed, 5000 + i) * 2**32)
                for i in range(int(density * w * h / AREA))
            ]
            self._adrift = (
                (w, h, self.season),
                [
                    (
                        0.8 + 0.8 * noise(key, 1),
                        math.tau * noise(key, 2),
                        noise(key, 3) * across,
                        noise(key, 4) * down,
                        colours[key % len(colours)],
                        4 * noise(key, 5),
                    )
                    for key in keys
                ],
            )
        for pace, sway, x, y, colour, turn in self._adrift[1]:
            x += pace * t * self.flow[0]
            y += pace * t * self.flow[1]
            x += 3.0 * math.sin(0.17 * t + sway)
            y += 2.0 * math.cos(0.13 * t + sway)
            px, py = math.floor(x % across), math.floor(y % down)
            # It turns as it goes.
            spin = int(0.12 * t + turn) % 4
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
                for one in drops(self.seed + 1, t, w, h, self.shower, *SHOWERS)
                if not dry(one.x, one.y)
            ]
        ripples += self.kisses
        skaters = list(self.skaters(w, h, t, dry)) if self.striders else []
        scare = self.fear(t)
        if scare is not None:
            # A strider slides off the pointer, and back as it calms.
            sx, sy, fright = scare
            for i, (x, y, push) in enumerate(skaters):
                d = math.hypot(x - sx, y - sy) or 1.0
                if d < SKIT:
                    k = fright * (SKIT - d) / d
                    skaters[i] = (x + k * (x - sx), y + k * (y - sy), push)
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

    def step(
        self,
        dt: float,
        w: int,
        h: int,
        scare: tuple[float, float, float] | None = None,
    ) -> tuple[int, int]:
        """Fly for `dt` seconds; the cell it is in. `scare`: the pointer,
        a cell and a fright: it leaves a place within `WARY` cells of it."""
        rng = self.rng
        if scare is not None and scare[2] > 0.3:
            sx, sy, _ = scare
            dx, dy = self.goal[0] - sx, 2 * (self.goal[1] - sy)
            if math.hypot(dx, dy) < WARY:
                way = math.atan2(dy, dx) if dx or dy else rng.uniform(0, math.tau)
                way += rng.uniform(-0.7, 0.7)
                far = rng.uniform(1.6, 2.6) * WARY
                self.goal = (sx + far * math.cos(way), sy + far * math.sin(way) / 2)
                self.wait = rng.uniform(0.8, 3.0)
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
        current=args.current,
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
    try:
        pilot.subscribe("hover")
    except PtyrcError as err:  # a driver without the topic
        if err.code != "bad-params":
            raise
    start = clock.monotonic()
    for dt in _cli.ticks(PERIOD, seconds):
        if not sight.look():
            continue
        w, h = frame.size
        t = clock.monotonic() - start
        for topic, data in sight.others:
            if topic != "hover" or data.get("at") is None:
                continue
            # On the water, or on a dragonfly over it.
            x, y = data["at"]
            if water is None or data["sprite"] != water.id:
                over = [f for f in flies if f.sprite and f.sprite.id == data["sprite"]]
                if not over or over[0].cell is None:
                    continue
                x, y = x + over[0].cell[0], y + over[0].cell[1]
            pond.touch(2 * x + 1, 4 * y + 2, t)
        if (w, h) != size or sight.changed:
            size = (w, h)
            shown = clear(sight.rows, w, h)
            walls = index(sight.rows, w, h)
        pond.step(dt, w, h, t, walls)
        with sight.drawing(), pilot.batch():
            cells = runs(pond.picture(w, h, t, walls), w, shown)
            # The whole view is its box: a hover is told anywhere on it.
            cells[0].append({"t": " " * (w - sum(len(run["t"]) for run in cells[0]))})
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
                scare = pond.fear(t)
                if scare is not None:
                    scare = ((scare[0] - 1) / 2, (scare[1] - 2) / 4, scare[2])
                for fly in flies:
                    cell = fly.step(dt, w, h, scare)
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
        default=6.0,
        metavar="N",
        help="lily pads over 80 by 24 cells [default: 6]",
    )
    out.add_argument(
        "--drops",
        type=float,
        default=10.0,
        metavar="N",
        help="drops a minute over 80 by 24 cells [default: 10]",
    )
    out.add_argument(
        "--current",
        type=float,
        default=FLOW,
        metavar="SPEED",
        help=f"the current, dots a second; 0: still water [default: {FLOW}]",
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
    if args.pads < 0 or args.drops < 0 or args.current < 0:
        out.error("--pads, --drops and --current must not be negative")
    if args.phase is not None:
        args.phase %= 1.0
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
