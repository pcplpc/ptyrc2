# SPDX-License-Identifier: GPL-3.0-or-later
"""Stars: a night sky behind the text, a moon that crosses it, grass
in a breeze along the bottom.

    ptyrc-app stars                 # stars, tonight's moon, grass
    ptyrc-app stars -t -r           # they twinkle, the sky turns
    ptyrc-app stars -c -g           # constellations, the milky way
    ptyrc-app stars -m -b -w        # meteors, fireflies, a meadow
    ptyrc-app stars --all
    ptyrc-app stars --print 80x24   # no driver: the scene on stdout

The sky is a map: a star has a place in it, `(u, v)` in [0, 1), and
the map is laid over the pane, whatever its size. The moon, the faint
stars and the grass are braille, 2 by 4 dots a cell: the moon moves
half a cell at a time, a blade leans by one dot.

What it shows:

- A scene that is a function of the time: `Sky.picture`, `moon`,
  `Grass.picture` take the seconds since the start and keep no state;
  the app only asks what the scene is now, and sends what changed.
- Two sprites in view cells, behind the text: the sky, looked at
  twice a second, and the grass over it, 6 times a second. The moon
  rises and sets behind the grass.
- The moon: its phase is tonight's (`phase_of`); only the lit side is
  drawn, the dark one hides the stars behind it. It crosses the sky
  on an arc in `--night` seconds, then stays down for a fifth of that.
- The breeze (`breeze`): a wave that travels along the grass, a gust
  now and then, a flutter of each blade.
- `-m`, `-b`: sprites the driver plays. A meteor is one request, a
  finite animation with a `motion`; a firefly a looped animation,
  moved by the client when it changes cell.
- The screen read as a map (`_cli.Sight`): nothing is drawn in the
  cell on each side of a glyph, the words stay apart.
"""

from __future__ import annotations

import argparse
import datetime
import itertools
import math
import random
import sys
from collections.abc import Sequence
from typing import Any, NamedTuple

from ptyrc import Pilot, Sprite, clock

from . import _cli
from .life import DOTS
from .snake import walls_of

__all__ = [
    "FIGURES",
    "Bug",
    "Grass",
    "Sky",
    "Star",
    "breeze",
    "clear",
    "main",
    "meteor",
    "moon",
    "moon_at",
    "phase_of",
    "runs",
    "show",
]

Grid = list[dict[int, tuple[str, int]]]
"""A picture: per row, column to glyph and colour."""

PERIOD = 1 / 6  # seconds a tick: the grass
SKY = 3  # ticks between two looks at the sky
AREA = 80 * 24  # cells `--stars` counts over

GLYPHS = ("", "·", "⋆", "✦")  # by level; level 0 is a braille dot
GREYS = (242, 248, 253, 231)
#: Tints, by level: blue, warm, red.
TINTS = ((67, 110, 153, 195), (137, 180, 223, 230), (131, 174, 217, 224))
HAZE = (146, 103, 60, 239)  # the milky way, from its middle out
LINE = 238  # the lines of a constellation
MOON = 230
GREENS = ((22, 28, 28, 34), (28, 34, 64, 70), (64, 70, 106, 70))  # by row, bottom up
PETALS = (213, 226, 231, 208, 141)
GLOW = (58, 148, 190)

BEAT = 0.4  # seconds a star keeps its look, with `-t`
TURN = 4.0  # nights a turn of the sky takes, with `-r`
DOWN = 0.2  # share of a night the moon stays down
RISEN = 0.2  # share of its way the moon has made at the start
METEOR = (4.0, 15.0)  # seconds between two meteors
SYNODIC = 29.530588853  # days from a new moon to the next
NEW_MOON = datetime.datetime(2000, 1, 6, 18, 14, tzinfo=datetime.UTC)

FIGURES: dict[str, tuple[tuple[tuple[int, int, int], ...], tuple[int, ...]]] = {
    "dipper": (
        ((0, 3, 2), (5, 1, 3), (9, 2, 3), (13, 3, 2), (14, 8, 2), (21, 9, 3), (22, 3, 3)),
        (0, 1, 2, 3, 4, 5, 6, 3),
    ),
    "orion": (
        ((2, 0, 3), (14, 1, 3), (5, 9, 2), (8, 8, 3), (11, 7, 2), (4, 17, 2), (15, 16, 3)),
        (0, 2, 3, 4, 1, 0, -1, 2, 5, 6, 4),
    ),
    "cassiopeia": (
        ((0, 0, 2), (5, 7, 3), (10, 2, 3), (15, 8, 2), (20, 1, 2)),
        (0, 1, 2, 3, 4),
    ),
    "cygnus": (
        ((10, 0, 3), (10, 6, 2), (10, 17, 2), (2, 4, 2), (18, 9, 2)),
        (0, 1, 2, -1, 3, 1, 4),
    ),
    "crux": (
        ((5, 0, 2), (5, 13, 3), (0, 6, 3), (11, 5, 2)),
        (0, 1, -1, 2, 3),
    ),
}  # fmt: skip
"""Constellations `-c` draws: stars `(x, y, level)` in dots, and the
line through them as indices, -1 lifting the pen."""


def _mix(n: int) -> int:
    n &= 0xFFFFFFFF
    n = ((n ^ n >> 16) * 0x45D9F3B) & 0xFFFFFFFF
    n = ((n ^ n >> 16) * 0x45D9F3B) & 0xFFFFFFFF
    return n ^ n >> 16


def noise(a: int, b: int = 0) -> float:
    """A number in [0, 1), the same for the same `(a, b)`."""
    return _mix(_mix(a) + b * 0x9E3779B1) / 2**32


def clear(rows: Sequence[str], w: int, h: int) -> list[int]:
    """A mask a row of a view of `w` by `h` cells holding the text
    `rows`, bit `x` the cell `x`: set where the scene is drawn, off
    under each glyph and under the cell on each side of it."""
    closed = [0] * h
    for x, y in walls_of(rows):
        if x < w and y < h:
            closed[y] |= 7 << x >> 1
    full = (1 << w) - 1
    return [full & ~row for row in closed]


def runs(
    grid: Grid, w: int, shown: Sequence[int] | None = None
) -> list[list[dict[str, Any]]]:
    """`grid` as the `cells` of a sprite `w` wide: rows of runs, one a
    colour, a space transparent. `shown`: a mask a row, as `clear`."""
    out: list[list[dict[str, Any]]] = []
    for y, row in enumerate(grid):
        mask = -1 if shown is None else shown[y]
        line: list[dict[str, Any]] = []
        text: list[str] = []
        ink, at = 0, 0
        for x in sorted(row):
            if not 0 <= x < w or not mask >> x & 1:
                continue
            glyph, colour = row[x]
            if colour != ink and text:
                line.append({"t": "".join(text), "fg": ink})
                text = []
            ink = colour
            text.append(" " * (x - at) + glyph)
            at = x + 1
        if text:
            line.append({"t": "".join(text), "fg": ink})
        out.append(line or [{"t": " "}])
    return out


class _Dots:
    """Braille dots of one colour a cell, written into a `Grid`.
    `wrap`: a dot out on a side comes back on the other."""

    def __init__(self, w: int, h: int, wrap: bool = False) -> None:
        self.w, self.h, self.wrap = w, h, wrap
        self.cells: dict[tuple[int, int], list[int]] = {}

    def dot(self, px: int, py: int, colour: int) -> None:
        """Set the dot `(px, py)`; the cell takes `colour`."""
        if self.wrap and self.w:
            px %= 2 * self.w
        if 0 <= px < 2 * self.w and 0 <= py < 4 * self.h:
            cell = self.cells.setdefault((px >> 1, py >> 2), [0, colour])
            cell[0] |= DOTS[py & 3][px & 1]
            cell[1] = colour

    def line(self, a: tuple[int, int], b: tuple[int, int], colour: int) -> None:
        """Every other dot from `a` to `b`."""
        n = max(abs(b[0] - a[0]), abs(b[1] - a[1]), 1)
        for i in range(0, n + 1, 2):
            self.dot(
                a[0] + round((b[0] - a[0]) * i / n),
                a[1] + round((b[1] - a[1]) * i / n),
                colour,
            )

    def into(self, grid: Grid) -> None:
        for (x, y), (bits, colour) in self.cells.items():
            grid[y][x] = (chr(0x2800 + bits), colour)


class Star(NamedTuple):
    """A star of the map. `level`: 0 a dot, to 3 the brightest; `tint`:
    0 grey, else a row of `TINTS` plus one."""

    u: float
    v: float
    level: int
    tint: int
    key: int


class Sky:
    """A map of the sky: stars at `(u, v)` in [0, 1), `u` across; the
    same seed gives the same map, at every size. `density`: stars over
    80 by 24 cells. `galaxy`: the milky way, a band of faint dots.
    `figures`: three constellations of `FIGURES`."""

    def __init__(
        self,
        rng: random.Random,
        density: float = 70.0,
        galaxy: bool = False,
        figures: bool = False,
    ) -> None:
        self.density = density
        self.galaxy = galaxy
        self._stars = random.Random(rng.getrandbits(32))
        self._haze = random.Random(rng.getrandbits(32))
        self.stars: list[Star] = []
        self.haze: list[tuple[float, float, int]] = []
        self.bend = rng.uniform(0.0, 1.0), rng.uniform(0.3, 0.5)
        self.figures: list[tuple[float, float, str]] = []
        if figures:
            names = rng.sample(sorted(FIGURES), 3)
            for i, name in enumerate(names):
                u = (0.06 + i / 3 + rng.uniform(0.0, 0.1)) % 1.0
                self.figures.append((u, rng.uniform(0.04, 0.4), name))

    def _star(self) -> Star:
        rng = self._stars
        level = rng.choices((0, 1, 2, 3), (58, 27, 11, 4))[0]
        tint = rng.choice((1, 1, 2, 3)) if rng.random() < 0.2 else 0
        # More stars high in the sky than over the horizon.
        return Star(rng.random(), rng.random() ** 1.3, level, tint, rng.getrandbits(30))

    def _dust(self) -> tuple[float, float, int]:
        rng = self._haze
        u, off = rng.random(), rng.gauss(0.0, 0.07)
        phase, mid = self.bend
        v = mid + 0.22 * math.sin(math.tau * (u + phase)) + off
        return u, v, HAZE[min(3, int(abs(off) / 0.045))]

    def picture(
        self, w: int, h: int, t: float = 0.0, twinkle: bool = False, turn: float = 0.0
    ) -> Grid:
        """The sky over `w` by `h` cells at `t` seconds. `twinkle`: a
        star changes level now and then, each on its own beat. `turn`:
        how much of a turn the sky has made, to the right."""
        grid: Grid = [{} for _ in range(max(0, h))]
        if w <= 0 or h <= 0:
            return grid
        across, down = 2 * w, 4 * h
        share = w * h / AREA
        dots = _Dots(w, h, wrap=True)
        bright: dict[tuple[int, int], tuple[int, int]] = {}

        def place(u: float, v: float) -> tuple[int, int]:
            return int((u + turn) % 1.0 * across), int(v * down)

        def star(px: int, py: int, level: int, tint: int, key: int) -> None:
            if twinkle:
                roll = noise(key, int(t / BEAT + 16 * noise(key)))
                level += (roll > 0.93) - (roll < 0.14)
            level = min(level, 3)
            if level < 0 or not (0 <= px < across and 0 <= py < down):
                return
            colour = TINTS[tint - 1][level] if tint else GREYS[level]
            cell = (px >> 1, py >> 2)
            if level == 0:
                dots.dot(px, py, colour)
            elif bright.get(cell, (0, 0))[0] <= level:
                bright[cell] = (level, colour)

        for u, v, name in self.figures:
            x0, y0 = place(u, v)
            points, path = FIGURES[name]
            for a, b in itertools.pairwise(path):
                if a >= 0 and b >= 0:
                    xa, ya, _ = points[a]
                    xb, yb, _ = points[b]
                    dots.line((x0 + xa, y0 + ya), (x0 + xb, y0 + yb), LINE)
        if self.galaxy:
            n = int(6 * self.density * share)
            while len(self.haze) < n:
                self.haze.append(self._dust())
            for u, v, colour in self.haze[:n]:
                dots.dot(*place(u, v), colour)
        n = int(self.density * share)
        while len(self.stars) < n:
            self.stars.append(self._star())
        for one in self.stars[:n]:
            star(*place(one.u, one.v), one.level, one.tint, one.key)
        for i, (u, v, name) in enumerate(self.figures):
            x0, y0 = place(u, v)
            for j, (x, y, level) in enumerate(FIGURES[name][0]):
                star((x0 + x) % across, y0 + y, level, 0, 977 * i + j)
        dots.into(grid)
        for (x, y), (level, colour) in bright.items():
            grid[y][x] = (GLYPHS[level], colour)
        return grid


def phase_of(when: datetime.datetime) -> float:
    """The phase of the moon at `when`: 0 new, 0.25 the first quarter,
    0.5 full; from the mean month, right within a day."""
    days = (when - NEW_MOON).total_seconds() / 86400.0
    return days / SYNODIC % 1.0


def moon_at(w: int, h: int, r: float, f: float) -> tuple[float, float]:
    """The middle of the moon, in dots, `f` of its way over `w` by `h`
    cells: an arc from under the left of the horizon to under its
    right, a radius from the top at its highest."""
    low, top = 4.0 * h + r, r + 2.0
    return (
        -r + f * (2 * w + 2 * r),
        low - (low - top) * math.sin(math.pi * f),
    )


def moon(grid: Grid, cx: float, cy: float, r: float, phase: float) -> None:
    """Draw a moon of radius `r` dots, its middle at the dot `(cx,
    cy)`, over `grid`: the lit side, as seen from the north. The dark
    side is not drawn; it hides what was in its cells."""
    h = len(grid)
    edge = math.cos(math.tau * phase)
    lit: dict[tuple[int, int], int] = {}
    dark: dict[tuple[int, int], int] = {}
    for py in range(max(0, math.floor(cy - r)), min(4 * h, math.ceil(cy + r) + 1)):
        y = py + 0.5 - cy
        if abs(y) >= r:
            continue
        half = math.sqrt(r * r - y * y)
        for px in range(max(0, math.floor(cx - half)), math.ceil(cx + half) + 1):
            x = px + 0.5 - cx
            if abs(x) > half:
                continue
            on = x > edge * half if phase < 0.5 else x < -edge * half
            side = lit if on else dark
            cell = (px >> 1, py >> 2)
            side[cell] = side.get(cell, 0) | DOTS[py & 3][px & 1]
    for x, y in dark:
        grid[y].pop(x, None)
    for (x, y), bits in lit.items():
        grid[y][x] = (chr(0x2800 + bits), MOON)


def breeze(x: float, t: float, wind: float = 1.0) -> float:
    """How far the breeze leans a blade 4 dots high at the dot column
    `x`, in dots, at `t` seconds: a steady lean, a wave that travels
    with the wind, a gust now and then."""
    way = 1.0 if wind >= 0 else -1.0
    wave = math.sin(0.11 * x - 1.3 * way * t)
    gust = max(0.0, math.sin(0.023 * x - 0.31 * way * t)) ** 3
    return wind * (0.25 + 0.45 * wave + 0.9 * gust)


class Grass:
    """Grass along the bottom: a blade a dot column, up to 7 dots
    high, on 2 rows of cells. `wild`: a meadow, up to 11 dots on 3
    rows, a flower on some of the tallest."""

    def __init__(self, rng: random.Random, wild: bool = False) -> None:
        self.rng = random.Random(rng.getrandbits(32))
        self.wild = wild
        self.rows = 3 if wild else 2
        #: Per dot column: height, phase of its flutter, petal or 0.
        self.blades: list[tuple[int, float, int]] = []

    def _blade(self) -> tuple[int, float, int]:
        rng = self.rng
        if rng.random() < 0.08:
            return (0, 0.0, 0)
        if self.wild:
            height = 2 + int(rng.random() ** 1.4 * 10)
        else:
            height = 1 + int(rng.random() ** 1.6 * 7)
        petal = rng.choice(PETALS) if height >= 9 and rng.random() < 0.3 else 0
        return (height, rng.uniform(0.0, math.tau), petal)

    def picture(self, w: int, t: float = 0.0, wind: float = 1.0) -> Grid:
        """The grass over `w` cells at `t` seconds, `rows` rows; a
        blade bends more towards its tip."""
        rows = self.rows
        grid: Grid = [{} for _ in range(rows)]
        while len(self.blades) < 2 * w:
            self.blades.append(self._blade())
        dots = _Dots(w, rows)
        heads: list[tuple[int, int, int]] = []
        for i, (height, phase, petal) in enumerate(self.blades[: 2 * w]):
            if not height:
                continue
            lean = breeze(i, t, wind) * height / 4
            if wind:
                lean += 0.3 * math.sin(2.1 * t + phase)
            for j in range(height):
                px = i + round(lean * (j / height) ** 1.5)
                py = 4 * rows - 1 - j
                dots.dot(px, py, 0)
                if petal and j == height - 1 and 0 <= px < 2 * w:
                    heads.append((px >> 1, py >> 2, petal))
        for (x, y), (bits, _) in dots.cells.items():
            greens = GREENS[rows - 1 - y]
            grid[y][x] = (chr(0x2800 + bits), greens[int(4 * noise(x, y))])
        for x, y, petal in heads:
            grid[y][x] = ("*•°"[int(3 * noise(x, petal))], petal)
        return grid


def meteor(rng: random.Random, w: int, h: int) -> dict[str, Any]:
    """A shooting star over a view of `w` by `h`, as arguments of
    `Pilot.draw` less the frame: a trail that falls two cells a row
    and fades."""
    way = rng.choice((-1, 1))
    # The tail on the first row, the head on the second.
    art = ("⠑⢄", "  ⠑⢄") if way > 0 else ("  ⡠⠊", "⡠⠊")
    seconds = rng.uniform(0.35, 0.7)
    speed = rng.uniform(10.0, 18.0)  # rows per second

    def cells(inks: Sequence[int]) -> list[list[dict[str, Any]]]:
        return [
            [{"t": art[0], "fg": inks[0]}],
            [{"t": art[1], "fg": inks[1], "bold": True}],
        ]

    looks = ((244, 231), (240, 253), (237, 246))
    return {
        "frames": [
            {"cells": cells(inks), "ms": int(seconds * 1000 / len(looks))}
            for inks in looks
        ],
        "loop": False,
        "at": (rng.randrange(w), rng.randrange(max(1, h // 3))),
        "space": "view",
        "motion": {
            "vx": 2 * way * speed,
            "vy": speed,
            "ax": 0.0,
            "ay": 0.0,
            "bounce": False,
        },
        "ttl": seconds + 0.3,
        "layer": "back",
        "z": 1,
    }


class Bug:
    """A firefly: it wanders over the grass, and glows now and then."""

    def __init__(self, rng: random.Random, w: int, h: int, rows: int) -> None:
        self.rng = rng
        self.x = rng.uniform(0, w)
        self.y = rng.uniform(*self.band(h, rows))
        self.vx = self.vy = 0.0
        self.sprite: Sprite | None = None
        self.cell: tuple[int, int] | None = None

    @staticmethod
    def band(h: int, rows: int) -> tuple[float, float]:
        """The rows it keeps to: the top of the grass and 4 above."""
        return (max(0.0, h - rows - 4.0), max(0.0, h - rows + 0.9))

    def frames(self) -> list[dict[str, Any]]:
        dark = self.rng.randrange(900, 3200)
        looks = (("·", 0, 140), ("•", 1, 160), ("•", 2, 380), ("•", 1, 160), ("·", 0, 140))  # fmt: skip
        return [
            {"cells": [[{"t": " "}]], "ms": dark},
            *({"cells": [[{"t": t, "fg": GLOW[c]}]], "ms": ms} for t, c, ms in looks),
        ]

    def step(self, dt: float, w: int, h: int, rows: int) -> tuple[int, int]:
        """Wander for `dt` seconds; the cell it is in."""
        rng = self.rng
        self.vx = max(-2.0, min(2.0, self.vx + rng.uniform(-3, 3) * dt))
        self.vy = max(-0.6, min(0.6, self.vy + rng.uniform(-1.5, 1.5) * dt))
        low, high = self.band(h, rows)
        self.x = (self.x + self.vx * dt) % max(1, w)
        self.y += self.vy * dt
        if not low <= self.y <= high:
            self.y = max(low, min(high, self.y))
            self.vy = -self.vy
        return (min(w - 1, int(self.x)), min(h - 1, int(self.y)))


class Night:
    """The scene and its options: what to draw at a time."""

    def __init__(self, args: argparse.Namespace, rng: random.Random) -> None:
        self.args = args
        self.sky = Sky(rng, args.stars, args.galaxy, args.constellations)
        self.grass = Grass(rng, args.wild)
        self.phase = args.phase
        if self.phase is None:
            self.phase = phase_of(datetime.datetime.now(datetime.UTC))

    def above(self, w: int, h: int, t: float) -> Grid:
        """The sky and the moon, `h` rows."""
        args = self.args
        night = args.night
        turn = t / (TURN * night) if args.turn else 0.0
        # No star in the last row: it is grass.
        grid = self.sky.picture(w, h - 1, t, args.twinkle, turn)
        grid.append({})
        f = (t / night + RISEN) % (1.0 + DOWN)
        if args.moon > 0 and f <= 1.0:
            moon(grid, *moon_at(w, h, args.moon, f), args.moon, self.phase)
        return grid

    def below(self, w: int, t: float) -> Grid:
        """The grass, `grass.rows` rows."""
        return self.grass.picture(w, t, self.args.wind)


def show(args: argparse.Namespace) -> int:
    """`--print`: the scene once on stdout, no driver."""
    w, h = args.print
    night = Night(args, random.Random(args.seed))
    grid = night.above(w, h, 0.0)
    low = night.below(w, 0.0)
    for row, grass in zip(grid[max(0, h - len(low)) :], low[-h:], strict=False):
        row.update(grass)
    for line in runs(grid, w):
        text = "".join(
            f"\x1b[38;5;{run['fg']}m{run['t']}\x1b[0m" if "fg" in run else run["t"]
            for run in line
        )
        sys.stdout.write(text + "\n")
    return 0


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    night = Night(args, rng)
    fx = random.Random(rng.getrandbits(32))
    frame = _cli.pick(pilot, args.frame)
    sight = _cli.Sight(pilot, frame)
    size = (0, 0)
    shown: list[int] = []
    sky: Sprite | None = None
    grass: Sprite | None = None
    sent: list[Any] = [None, None]  # the cells of the two, as sent
    bugs: list[Bug] = []
    start = clock.monotonic()
    shot = fx.uniform(1.0, METEOR[0])  # when the next meteor goes
    for tick, dt in enumerate(_cli.ticks(PERIOD, seconds)):
        if not sight.look():
            continue
        w, h = frame.size
        t = clock.monotonic() - start
        rows = min(night.grass.rows, h)
        sized = (w, h) != size
        if sized or sight.changed:
            size = (w, h)
            shown = clear(sight.rows, w, h)
        with sight.drawing(), pilot.batch():
            if sized or sight.changed or tick % SKY == 0:
                cells = runs(night.above(w, h, t), w, shown)
                if sky is None:
                    sky = pilot.draw(
                        cells=cells, frame=frame.id, at=(0, 0), space="view", z=0
                    )
                elif cells != sent[0]:
                    sky.update(content={"cells": cells})
                sent[0] = cells
            cells = runs(night.below(w, t)[-rows:], w, shown[h - rows :])
            if grass is None:
                grass = pilot.draw(
                    cells=cells, frame=frame.id, at=(0, h - rows), space="view", z=2
                )
            elif sized:
                grass.update(
                    anchor={"at": [0, h - rows], "space": "view"},
                    content={"cells": cells},
                )
            elif cells != sent[1]:
                grass.update(content={"cells": cells})
            sent[1] = cells
            if args.meteors and t >= shot:
                shot = t + fx.uniform(*METEOR)
                pilot.draw(frame=frame.id, **meteor(fx, w, h))
            if args.bugs:
                want = max(2, w // 14)
                while len(bugs) < want:
                    bugs.append(Bug(fx, w, h, rows))
                for bug in bugs[want:]:
                    if bug.sprite is not None:
                        bug.sprite.remove()
                del bugs[want:]
                for bug in bugs:
                    cell = bug.step(dt, w, h, rows)
                    if bug.sprite is None:
                        bug.sprite = pilot.draw(
                            frames=bug.frames(),
                            loop=True,
                            frame=frame.id,
                            at=cell,
                            space="view",
                            z=3,
                        )
                    elif cell != bug.cell:
                        bug.sprite.move(*cell)
                    bug.cell = cell


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
        "stars",
        "A night sky behind the text: stars, a moon that crosses it, "
        "grass in a breeze.",
        frame="largest",
    )
    out.add_argument(
        "--stars",
        type=float,
        default=70.0,
        metavar="N",
        help="stars over 80 by 24 cells [default: 70]",
    )
    out.add_argument(
        "--night",
        type=float,
        default=300.0,
        metavar="SECONDS",
        help="the time the moon takes to cross the sky [default: 300]",
    )
    out.add_argument(
        "--moon",
        type=float,
        default=5.0,
        metavar="DOTS",
        help="radius of the moon, 2 dots a column; 0: no moon [default: 5]",
    )
    out.add_argument(
        "--phase",
        type=float,
        metavar="F",
        help="phase of the moon: 0 new, 0.25 first quarter, 0.5 full "
        "[default: tonight's]",
    )
    out.add_argument(
        "--wind",
        type=float,
        default=1.0,
        help="strength of the breeze, < 0 to the left; 0: none [default: 1]",
    )
    out.add_argument("--seed", type=int, help="same seed, same sky")
    out.add_argument("-t", "--twinkle", action="store_true", help="the stars twinkle")
    out.add_argument(
        "-r", "--turn", action="store_true", help="the sky turns, slower than the moon"
    )
    out.add_argument(
        "-c",
        "--constellations",
        action="store_true",
        help="three constellations, their lines drawn",
    )
    out.add_argument(
        "-g", "--galaxy", action="store_true", help="the milky way, a band of dust"
    )
    out.add_argument(
        "-m", "--meteors", action="store_true", help="a shooting star now and then"
    )
    out.add_argument(
        "-b", "--bugs", action="store_true", help="fireflies over the grass"
    )
    out.add_argument(
        "-w", "--wild", action="store_true", help="a meadow: taller grass, flowers"
    )
    out.add_argument("-a", "--all", action="store_true", help="-t -r -c -g -m -b -w")
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
        args.twinkle = args.turn = args.constellations = args.galaxy = True
        args.meteors = args.bugs = args.wild = True
    if args.night <= 0:
        out.error("--night must be positive")
    if args.stars < 0 or args.moon < 0:
        out.error("--stars and --moon must not be negative")
    if args.phase is not None:
        args.phase %= 1.0
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
