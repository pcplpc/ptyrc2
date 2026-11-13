# SPDX-License-Identifier: GPL-3.0-or-later
"""Life: Conway's game of life in braille, behind the text.

    ptyrc-app life                  # one colour, a soup, renewed when still
    ptyrc-app life -c -t            # colour by age, a trail where cells died
    ptyrc-app life -g -p            # patterns dropped in, particles
    ptyrc-app life -w               # the text is a wall
    ptyrc-app life --all

A cell of the terminal holds 2 by 4 cells of the world, the dots of a
braille glyph: a pane of 80 by 24 is a world of 160 by 96, a torus.

What it shows:

- One sprite in view cells, the whole pane, its content replaced at
  each generation; a space is transparent.
- A world in bits: a row is one integer, a generation a few operations
  on each (`Life.step`). The age of a cell (`-c`) and the time since
  its death (`-t`) are kept as masks, one a level.
- A glyph has one colour: with `-c` the age of its youngest cell, born
  white, then yellow, green, teal, old blue. With `-t` a glyph with no
  cell alive shows the cells that died there, grey, darker for 4
  generations.
- A still world: the population repeats (`Watch`). The soup is then
  made again; with `-g` a pattern is dropped instead, and one every 3
  to 8 s: spaceships, methuselahs, oscillators, a gun, each where the
  world is empty, turned at random.
- `-p`: particles, each one request with a `motion` (`_arcade.spark`):
  where a glyph loses 5 cells at once, at most 4 a generation, and
  where a pattern lands.
- The screen read as a map (`_cli.Sight`): nothing is drawn in the
  cell on each side of a glyph, the words stay apart. The world goes
  on under the text, unseen; with `-w` no cell lives there.
"""

from __future__ import annotations

import argparse
import random
from collections import deque
from collections.abc import Iterable, Sequence
from typing import Any

from ptyrc import Frame, Pilot, Sprite

from . import _arcade, _cli
from .snake import walls_of

__all__ = [
    "AGES",
    "GUESTS",
    "TRAIL",
    "Life",
    "Watch",
    "blasts",
    "drop",
    "main",
    "picture",
    "turned",
    "wall",
]

GREEN = "green"

AGES = (231, 228, 156, 114, 79, 73, 67, 61)
"""Colour of a glyph by the age of its youngest cell, in generations."""

TRAIL = (243, 240, 238, 236)
"""Colour of the cells that died, by generations since."""

SPARKS = (231, 228, 156, 114)

BLAST = 5  # cells a glyph loses at once to throw a particle
BLASTS = 4  # particles a generation, at most
STILL = 12  # a population that repeats at this distance is still
QUIET = 48  # generations still before the world is renewed
GUEST = (3.0, 8.0)  # seconds between two patterns dropped

DOTS = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))
#: Glyph of 2 bits a row, 4 rows, the top row lowest.
BRAILLE = tuple(
    " "
    if not bits
    else chr(
        0x2800
        + sum(DOTS[y][x] for y in range(4) for x in range(2) if bits >> (2 * y + x) & 1)
    )
    for bits in range(256)
)

GUESTS: dict[str, tuple[str, ...]] = {
    "glider": (".O.", "..O", "OOO"),
    "lwss": (".O..O", "O....", "O...O", "OOOO."),
    "mwss": ("...O..", ".O...O", "O.....", "O....O", "OOOOO."),
    "hwss": ("...OO..", ".O....O", "O......", "O.....O", "OOOOOO."),
    "r-pentomino": (".OO", "OO.", ".O."),
    "pi-heptomino": ("OOO", "O.O", "O.O"),
    "acorn": (".O.....", "...O...", "OO..OOO"),
    "diehard": ("......O.", "OO......", ".O...OOO"),
    "pentadecathlon": ("..O....O..", "OO.OOOO.OO", "..O....O.."),
    "pulsar": (
        "..OOO...OOO..",
        ".............",
        "O....O.O....O",
        "O....O.O....O",
        "O....O.O....O",
        "..OOO...OOO..",
        ".............",
        "..OOO...OOO..",
        "O....O.O....O",
        "O....O.O....O",
        "O....O.O....O",
        ".............",
        "..OOO...OOO..",
    ),
    "gun": (
        "........................O...........",
        "......................O.O...........",
        "............OO......OO............OO",
        "...........O...O....OO............OO",
        "OO........O.....O...OO..............",
        "OO........O...O.OO....O.O...........",
        "..........O.....O.......O...........",
        "...........O...O....................",
        "............OO......................",
    ),
}
"""Patterns `-g` drops, as rows; `O`: a cell."""

#: How often each is drawn.
WEIGHTS = {"glider": 4, "lwss": 3, "r-pentomino": 3, "acorn": 2, "gun": 1}


class Life:
    """A world of `w` by `h` cells, a torus. A row is an integer, bit
    `x` the cell `x`.

    `ages[k][y]`: the cells alive for more than `k` generations;
    `ages[0]` is the world. `gone[k][y]`: the cells dead for `k + 1`
    generations, never born again since. `died`: those of the last
    step, `gone[0]`."""

    def __init__(self, w: int, h: int, ages: int = 1, gone: int = 1) -> None:
        self.w, self.h = w, h
        self.ages = [[0] * h for _ in range(max(1, ages))]
        self.gone = [[0] * h for _ in range(max(1, gone))]
        self.generation = 0

    @property
    def rows(self) -> list[int]:
        return self.ages[0]

    @property
    def died(self) -> list[int]:
        return self.gone[0]

    @property
    def population(self) -> int:
        return sum(row.bit_count() for row in self.rows)

    def soup(self, rng: random.Random) -> None:
        """A cell in four alive, the rest forgotten."""
        w, h = self.w, self.h
        for plane in self.ages[1:] + self.gone:
            plane[:] = [0] * h
        self.rows[:] = [rng.getrandbits(w) & rng.getrandbits(w) for _ in range(h)]

    def put(self, art: Sequence[str], x: int, y: int) -> None:
        """Write a pattern, its top-left cell at `(x, y)`; it wraps."""
        w, h = self.w, self.h
        for dy, line in enumerate(art):
            for dx, c in enumerate(line):
                bit = 1 << (x + dx) % w
                row = (y + dy) % h
                for plane in self.ages[1:] + self.gone:
                    plane[row] &= ~bit
                if c == "O":
                    self.rows[row] |= bit
                else:
                    self.rows[row] &= ~bit

    def cells(self) -> set[tuple[int, int]]:
        return {
            (x, y) for y, row in enumerate(self.rows) for x in range(self.w) if row >> x & 1
        }  # fmt: skip

    def empty(self, x: int, y: int, w: int, h: int) -> bool:
        """No cell alive in the box; it wraps."""
        if w > self.w or h > self.h:
            return False
        mask = (1 << w) - 1
        x %= self.w
        mask = (mask << x | mask >> (self.w - x)) & ((1 << self.w) - 1)
        return not any(self.rows[(y + dy) % self.h] & mask for dy in range(h))

    def resize(self, w: int, h: int) -> None:
        """Cut, or grown with empty cells."""
        full = (1 << w) - 1
        for plane in self.ages + self.gone:
            plane[:] = [row & full for row in plane[:h]] + [0] * (h - len(plane))
        self.w, self.h = w, h

    def step(self, open_: Sequence[int] | None = None) -> None:
        """One generation. `open_`: a mask a row, the cells where life
        is allowed."""
        w, h, rows = self.w, self.h, self.rows
        if not w or not h:
            return
        full, top = (1 << w) - 1, w - 1
        # Per row, the count of the cell and its two neighbours across,
        # two bits (`three`), and of the neighbours alone (`two`).
        three, two = [], []
        for row in rows:
            left = (row << 1 | row >> top) & full
            right = row >> 1 | (row & 1) << top
            x, a = left ^ right, left & right
            two.append((x, a))
            three.append((x ^ row, a | x & row))
        new = []
        for y, row in enumerate(rows):
            u0, u1 = three[y - 1]
            d0, d1 = three[(y + 1) % h]
            m0, m1 = two[y]
            # Neighbours: `ones` + 2 * (u1 + d1 + m1 + carry).
            ones = u0 ^ d0 ^ m0
            carry = u0 & d0 | m0 & (u0 ^ d0)
            # Exactly one of the four: 2 or 3 neighbours.
            one = (u1 ^ d1 ^ m1 ^ carry) & ~(u1 & d1 | m1 & carry)
            new.append(one & (ones | row))
        if open_ is not None:
            new = [row & mask for row, mask in zip(new, open_, strict=True)]
        gone = self.gone
        for k in range(len(gone) - 1, 0, -1):
            gone[k] = [g & ~n for g, n in zip(gone[k - 1], new, strict=True)]
        gone[0] = [r & ~n for r, n in zip(rows, new, strict=True)]
        ages = self.ages
        for k in range(len(ages) - 1, 0, -1):
            ages[k] = [a & n for a, n in zip(ages[k - 1], new, strict=True)]
        ages[0] = new
        self.generation += 1


class Watch:
    """Tells a world gone still: its population has repeated at a
    distance of `STILL` generations, which holds every period up to 4,
    6 and 12 and a ship crossing, for `QUIET` generations."""

    def __init__(self) -> None:
        self.past: deque[int] = deque(maxlen=STILL)
        self.quiet = 0

    def reset(self) -> None:
        self.past.clear()
        self.quiet = 0

    def still(self, population: int) -> bool:
        past = self.past
        same = len(past) == STILL and past[0] == population
        self.quiet = self.quiet + 1 if same else 0
        past.append(population)
        return self.quiet >= QUIET or population == 0


def wall(rows: Sequence[str], w: int, h: int) -> list[int]:
    """Masks, one a row of the world, for a view of `w` by `h` terminal
    cells holding the text `rows`: closed under each glyph and under
    the cell on each side of it, so that the words stay apart."""
    full = (1 << 2 * w) - 1
    closed = [0] * h
    for x, y in walls_of(rows):
        if x < w and y < h:
            closed[y] |= 63 << 2 * x >> 2
    return [full & ~closed[y // 4] for y in range(4 * h)]


def turned(art: Sequence[str], rng: random.Random) -> list[str]:
    """`art` in one of its 8 orientations."""
    out = list(art)
    if rng.random() < 0.5:
        out = ["".join(col) for col in zip(*out, strict=True)]
    if rng.random() < 0.5:
        out = [line[::-1] for line in out]
    if rng.random() < 0.5:
        out.reverse()
    return out


def drop(
    life: Life, rng: random.Random, open_: Sequence[int] | None = None
) -> tuple[int, int] | None:
    """Put a pattern of `GUESTS` where the world is empty and open, 2
    cells around it too. Its middle, or None: no room found."""
    names = list(GUESTS)
    weights = [WEIGHTS.get(name, 2) for name in names]
    for _ in range(12):
        art = turned(GUESTS[rng.choices(names, weights)[0]], rng)
        w, h = len(art[0]) + 4, len(art) + 4
        if w > life.w or h > life.h:
            continue
        x, y = rng.randrange(life.w), rng.randrange(life.h)
        if not life.empty(x, y, w, h):
            continue
        if open_ is not None:
            mask = ((1 << w) - 1) << x
            if x + w > life.w or y + h > life.h:
                continue
            if any(open_[y + dy] & mask != mask for dy in range(h)):
                continue
        life.put(art, x + 2, y + 2)
        return ((x + w // 2) % life.w, (y + h // 2) % life.h)
    return None


class _Even(dict[int, int]):
    """Width to the mask of its even bits."""

    def __missing__(self, w: int) -> int:
        self[w] = sum(1 << x for x in range(0, w, 2))
        return self[w]


_EVEN = _Even()


def _glyph(quad: Sequence[int], shift: int) -> int:
    """Index in `BRAILLE` of the 2 columns from `shift` of 4 rows."""
    return (
        quad[0] >> shift & 3
        | (quad[1] >> shift & 3) << 2
        | (quad[2] >> shift & 3) << 4
        | (quad[3] >> shift & 3) << 6
    )


def picture(
    life: Life,
    colour: bool = False,
    trail: bool = False,
    shown: Sequence[int] | None = None,
) -> list[list[dict[str, Any]]]:
    """The world as rows of runs, a glyph for 2 by 4 cells. `colour`:
    by the age of the youngest cell of the glyph, as far as the world
    keeps ages. `trail`: where no cell lives, those that died, in the
    colour of the last. `shown`: a mask a row, the cells to draw."""
    ages, gone = life.ages, life.gone if trail else []
    even = _EVEN[life.w]
    out: list[list[dict[str, Any]]] = []
    for y in range(0, life.h - 3, 4):
        alive = ages[0][y : y + 4]
        older = [plane[y : y + 4] for plane in ages[1:]] if colour else []
        ghosts = [plane[y : y + 4] for plane in gone]
        seen = alive[0] | alive[1] | alive[2] | alive[3]
        for quad in ghosts:
            seen |= quad[0] | quad[1] | quad[2] | quad[3]
        if shown is not None:
            seen &= shown[y]  # the same mask for the 4 rows of a glyph
        runs: list[dict[str, Any]] = []
        text: list[str] = []
        ink: int | str = GREEN
        x = 0
        # Bit 2x: the glyph x has something to show.
        marks = (seen | seen >> 1) & even
        while marks:
            low = marks & -marks
            marks ^= low
            shift = low.bit_length() - 1
            bits = _glyph(alive, shift)
            color: int | str = GREEN
            if bits and colour:
                age = 0
                while age < len(older) and _glyph(older[age], shift) == bits:
                    age += 1
                color = AGES[min(age, len(AGES) - 1)]
            elif not bits:
                for k in range(len(ghosts) - 1, -1, -1):
                    if dead := _glyph(ghosts[k], shift):
                        bits |= dead
                        color = TRAIL[k]
            if color != ink and text:
                runs.append({"t": "".join(text), "fg": ink})
                text = []
            ink = color
            text.append(" " * (shift // 2 - x) + BRAILLE[bits])
            x = shift // 2 + 1
        if text:
            runs.append({"t": "".join(text), "fg": ink})
        out.append(runs or [{"t": " "}])
    return out


def blasts(life: Life) -> list[tuple[int, int]]:
    """Terminal cells, `(x, y)` in the view, that lost `BLAST` cells
    or more in the last step and kept none."""
    out = []
    died, rows = life.died, life.rows
    for y in range(0, life.h - 3, 4):
        d0, d1, d2, d3 = died[y : y + 4]
        if (d0 | d1 | d2 | d3).bit_count() < 2:
            continue
        r = rows[y] | rows[y + 1] | rows[y + 2] | rows[y + 3]
        seen = d0 | d1 | d2 | d3
        marks = (seen | seen >> 1) & _EVEN[life.w] & ~(r | r >> 1)
        while marks:
            low = marks & -marks
            shift = low.bit_length() - 1
            marks ^= low
            n = sum((d >> shift & 3).bit_count() for d in (d0, d1, d2, d3))
            if n >= BLAST:
                out.append((shift // 2, y // 4))
    return out


def sparks(
    pilot: Pilot,
    frame: Frame,
    rng: random.Random,
    cells: Iterable[tuple[int, int]],
    n: int,
    layer: str,
    **kw: Any,
) -> None:
    """`n` particles from each of `cells`, view cells. In a batch."""
    for at in cells:
        for one in _arcade.burst(n, rng, SPARKS, **kw):
            one["layer"] = layer
            pilot.draw(frame=frame.id, at=at, space="view", **one)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    fx = random.Random(args.seed)  # the particles do not change the world
    frame = _cli.pick(pilot, args.frame)
    layer = "front" if args.front else "back"
    sight = _cli.Sight(pilot, frame)
    life = Life(0, 0, len(AGES) if args.colour else 1, len(TRAIL) if args.trail else 1)
    watch = Watch()
    clear: list[int] | None = None  # the cells off the text
    sprite: Sprite | None = None
    guest = rng.uniform(*GUEST) * args.speed  # generations to the next
    for _ in _cli.ticks(1.0 / args.speed, seconds):
        if not sight.look():
            continue
        w, h = frame.size
        sized = (2 * w, 4 * h) != (life.w, life.h)
        if sized:
            fresh = not life.w
            life.resize(2 * w, 4 * h)
            if fresh:
                life.soup(rng)
            watch.reset()
        if sight.changed or sized:
            clear = wall(sight.rows, w, h)
        open_ = clear if args.walls else None
        life.step(open_)
        landed: tuple[int, int] | None = None
        guest -= 1
        if watch.still(life.population) or (args.guests and guest <= 0):
            if args.guests:
                landed = drop(life, rng, open_)
                guest = rng.uniform(*GUEST) * args.speed
            if not args.guests or (landed is None and not life.population):
                life.soup(rng)
            watch.reset()
        cells = picture(life, args.colour, args.trail, clear)
        with sight.drawing(), pilot.batch():
            if sprite is None:
                sprite = pilot.draw(
                    cells=cells, frame=frame.id, at=(0, 0), space="view", layer=layer
                )
            else:
                sprite.update(content={"cells": cells})
            if args.particles:
                hit = [
                    (x, y)
                    for x, y in blasts(life)
                    if clear is None or clear[4 * y] >> 2 * x & 1
                ]
                sparks(
                    pilot,
                    frame,
                    fx,
                    fx.sample(hit, min(len(hit), BLASTS)),
                    1,
                    layer,
                    glyphs="⠶⠒⠂",
                    speed=(2.0, 8.0),
                    life=(0.3, 0.7),
                    gravity=0.0,
                )
                if landed is not None:
                    at = (landed[0] // 2, landed[1] // 4)
                    sparks(pilot, frame, fx, [at], 14, layer, glyphs="⣿⠿⠶⠒⠂")


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "life", "Conway's game of life in braille, behind the text.", frame="largest"
    )
    out.add_argument(
        "--speed",
        type=float,
        default=10.0,
        help="generations per second [default: 10]",
    )
    out.add_argument("--seed", type=int, help="same seed, same world")
    out.add_argument(
        "-c", "--colour", action="store_true", help="a glyph coloured by its age"
    )
    out.add_argument(
        "-t", "--trail", action="store_true", help="the cells that died fade out"
    )
    out.add_argument(
        "-g",
        "--guests",
        action="store_true",
        help="patterns dropped in: ships, methuselahs, oscillators, a gun",
    )
    out.add_argument(
        "-p",
        "--particles",
        action="store_true",
        help="particles where cells die together and where a pattern lands",
    )
    out.add_argument(
        "-w", "--walls", action="store_true", help="no cell lives under the text"
    )
    out.add_argument("-a", "--all", action="store_true", help="-c -t -g -p -w")
    out.add_argument("--front", action="store_true", help="over the text")
    args = _cli.parse(out, argv)
    if args.all:
        args.colour = args.trail = args.guests = args.particles = args.walls = True
    if args.speed <= 0:
        out.error("--speed must be positive")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
