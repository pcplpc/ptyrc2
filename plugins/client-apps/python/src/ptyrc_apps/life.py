# SPDX-License-Identifier: GPL-3.0-or-later
"""Life: Conway's game of life in braille, behind the text.

    ptyrc-app life                  # one colour, a soup, renewed when still
    ptyrc-app life -c -t            # colour by age, a trail where cells died
    ptyrc-app life -g -p            # patterns dropped in, particles
    ptyrc-app life -g --rate 6      # more of them, sooner
    ptyrc-app life -g --kinds ship,gun
    ptyrc-app life -w               # the text is a wall
    ptyrc-app life --cursor         # the pointer clears the cells near it
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
  made again; with `-g` a pattern is dropped instead, and the world
  starts with patterns, no soup.
- `-g`: patterns of a library (`_patterns`, 240 of them: still lifes,
  oscillators, spaceships, methuselahs, guns, puffers), dropped by a
  `Host`. The world is mapped in blocks of 4 by 4 cells, each with
  its distance to the nearest cell alive (`room`); a pattern goes to
  the middle of a hole, the widest holes first, and the wider the
  hole the larger the pattern. A ship is turned to the way with the
  most room ahead. A pattern dropped is not dropped again soon. A
  kind that fills the world (methuselah, gun, puffer) waits for it to
  be calm. A pattern too large for any hole replaces the cells of a
  place that has few; so does a small one when no hole is left.
- `--rate`: patterns a second into an empty world; into a crowded one,
  4 times fewer. `--rate 0`: only when the world is still. `--kinds`:
  the kinds dropped, every one unless told; `filler` grows over the
  whole world.
- `--cursor`: the pointer of the host over the pane, no button down
  (the `hover` events): where it moves, the cells within 6 of it die
  (`Life.wipe`). They leave a trail (`-t`) and particles (`-p`) as
  the others do.
- `-p`: particles, each one request with a `motion` (`_arcade.spark`):
  where a glyph loses 5 cells at once, at most 4 a generation, and
  where a pattern lands.
- The screen read as a map (`_cli.Sight`): nothing is drawn in the
  cell on each side of a glyph, the words stay apart. The world goes
  on under the text, unseen; with `-w` no cell lives there.
"""

from __future__ import annotations

import argparse
import math
import random
from collections import deque
from collections.abc import Iterable, Sequence
from typing import Any, NamedTuple

from ptyrc import Frame, Pilot, PtyrcError, Sprite

from . import _arcade, _cli, _patterns
from ._patterns import FILLER, GUN, OSCILLATOR, PUFFER, SEED, SHIP, Pattern
from .snake import walls_of

__all__ = [
    "AGES",
    "GUESTS",
    "SHARES",
    "TRAIL",
    "Host",
    "Landing",
    "Life",
    "Watch",
    "blasts",
    "drop",
    "main",
    "picture",
    "room",
    "touched",
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
RATE = 3.0  # patterns a second dropped in an empty world
CROWD = 4.0  # times fewer in a world with no hole
BLOCK = 4  # cells a side of a block of the map of room
MARGIN = 2  # cells kept empty around a pattern dropped
VOID = 3  # blocks from any cell: a block of a hole
SITES = 8  # places tried for a pattern
PICKS = 4  # patterns tried at a place
AHEAD = 16  # blocks a ship looks ahead
RECENT = 40  # patterns remembered, not to drop one again soon
STALE = 0.1  # weight of one remembered
CALM = 4  # blocks 2 away from any cell for one of the map: a calm world
OPENING = 24  # patterns dropped at once in a new world, at most
SMALL = 8  # cells a side, at most, of a pattern dropped in a full world
SPARSE = 16  # cells of a place for one alive, at least, to drop over them
REACH = 6  # cells around the pointer that die

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

GUESTS: dict[str, tuple[str, ...]] = {p.name: p.art for p in _patterns.LIBRARY.values()}
"""Patterns `-g` drops, as rows; `O`: a cell."""

SHARES: dict[str, float] = {
    SHIP: 30,
    SEED: 22,
    OSCILLATOR: 26,
    PUFFER: 7,
    GUN: 5,
    _patterns.STILL: 6,
    FILLER: 4,
}
"""How often a kind is dropped, of 100."""

LIVELY = (SEED, PUFFER, GUN, FILLER)
"""The kinds that fill the world: held back until it is calm."""

KINDS = tuple(SHARES)
"""The kinds dropped unless told: every one."""


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

    def span(self, x: int, w: int) -> int:
        """Mask of the `w` cells of a row from `x`; it wraps."""
        mask = (1 << w) - 1
        x %= self.w
        return (mask << x | mask >> (self.w - x)) & ((1 << self.w) - 1)

    def empty(self, x: int, y: int, w: int, h: int) -> bool:
        """No cell alive in the box; it wraps."""
        if w > self.w or h > self.h:
            return False
        mask = self.span(x, w)
        return not any(self.rows[(y + dy) % self.h] & mask for dy in range(h))

    def count(self, x: int, y: int, w: int, h: int) -> int:
        """Cells alive in the box; it wraps."""
        mask = self.span(x, w)
        return sum((self.rows[(y + dy) % self.h] & mask).bit_count() for dy in range(h))

    def wipe(self, x: int, y: int, r: int) -> int:
        """Kill the cells within `r` of `(x, y)`: they are `died`. It
        does not wrap. How many."""
        n = 0
        full = (1 << self.w) - 1
        for row in range(max(0, y - r), min(self.h, y + r + 1)):
            half = math.isqrt(r * r - (row - y) ** 2)
            lo = max(0, x - half)
            mask = ((1 << max(0, x + half + 1 - lo)) - 1) << lo & full
            if dead := self.rows[row] & mask:
                n += dead.bit_count()
                for plane in self.ages:
                    plane[row] &= ~dead
                for plane in self.gone[1:]:
                    plane[row] &= ~dead
                self.died[row] |= dead
        return n

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
    return _patterns.turn(list(art), rng.randrange(8))


def room(life: Life, open_: Sequence[int] | None = None) -> list[list[int]]:
    """The world in blocks of `BLOCK` by `BLOCK` cells, `[y][x]`: the
    distance of each, in blocks, by any of the 8 ways and around the
    torus, to the nearest block that holds a cell alive or one closed
    by `open_`; 0 for such a block. None such: the longer side of the
    map everywhere."""
    w, h = life.w, life.h
    gw, gh = -(-w // BLOCK), -(-h // BLOCK)
    full, first = (1 << w) - 1, _EVERY[BLOCK][w]
    front = []
    for top in range(0, h, BLOCK):
        row = 0
        for y in range(top, min(top + BLOCK, h)):
            row |= life.rows[y]
            if open_ is not None:
                row |= ~open_[y] & full
        cells = row
        for k in range(1, BLOCK):
            row |= cells >> k
        row &= first
        busy = 0
        while row:
            low = row & -row
            row ^= low
            busy |= 1 << (low.bit_length() - 1) // BLOCK
        front.append(busy)
    far = [[max(gw, gh)] * gw for _ in range(gh)]
    wide, top = (1 << gw) - 1, gw - 1
    done = [0] * gh
    d = 0
    while any(front):
        for by, row in enumerate(front):
            while row:
                low = row & -row
                row ^= low
                far[by][low.bit_length() - 1] = d
        done = [a | b for a, b in zip(done, front, strict=True)]
        across = [
            (m | m << 1 | m >> top | m >> 1 | (m & 1) << top) & wide for m in done
        ]
        front = [
            (across[by - 1] | across[by] | across[(by + 1) % gh]) & ~done[by]
            for by in range(gh)
        ]
        d += 1
    return far


class Landing(NamedTuple):
    """A pattern dropped: its name, the cell of its middle."""

    name: str
    x: int
    y: int


class Host:
    """Drops patterns of `kinds` where the world has room.

    `void`: how empty the world was at the last drop, 0 to 1; 1 when
    half of its blocks were `VOID` away from any cell. `calm`: how
    settled, 0 in a soup, 1 in its ashes. `recent`: the names of the
    last ones dropped."""

    def __init__(self, rng: random.Random, kinds: Iterable[str] = KINDS) -> None:
        self.rng = rng
        self.void = 0.0
        self.calm = 0.0
        kinds = set(kinds)
        guests = [p for p in _patterns.LIBRARY.values() if p.kind in kinds]
        of = {kind: sum(p.kind == kind for p in guests) for kind in kinds}
        #: Each pattern, the share of its kind split between its like.
        self.guests = [(p, SHARES[p.kind] / of[p.kind]) for p in guests]
        self.recent: deque[str] = deque(maxlen=min(RECENT, len(guests) // 2))

    def wait(self, rate: float) -> float:
        """Seconds to the next drop: `1 / rate` in an empty world,
        `CROWD` times that in a full one, half as much more or less."""
        if rate <= 0:
            return float("inf")
        crowd = 1 + (CROWD - 1) * (1 - self.void)
        return self.rng.uniform(0.5, 1.5) * crowd / rate

    def open(
        self,
        life: Life,
        open_: Sequence[int] | None = None,
        seen: Sequence[int] | None = None,
    ) -> int:
        """Drop patterns in a new world, up to `OPENING`, until half of
        it is no hole any more. How many."""
        n = 0
        while n < OPENING and self.drop(life, open_, seen) and self.void > 0.5:
            n += 1
        return n

    def weight(self, p: Pattern, share: float) -> float:
        """The chance of `p` now: less when dropped of late, and for a
        lively one when the world is not calm."""
        if p.name in self.recent:
            share *= STALE
        if p.kind in LIVELY:
            share *= self.calm**2
        return share

    def drop(
        self,
        life: Life,
        open_: Sequence[int] | None = None,
        seen: Sequence[int] | None = None,
    ) -> Landing | None:
        """Put a pattern in a hole of the world, `MARGIN` cells empty
        around it: a wide hole rather than a narrow one, a large
        pattern in a wide hole. One too large for any hole has its
        chance too: it replaces the cells of a place that has few.
        `open_`: a mask a row, the cells where a pattern may go.
        `seen`: those where it had better go; elsewhere when they
        have no room. With no room at all, a small pattern over the
        fewest cells. None: no place found, the world is as it was."""
        if not life.w or not life.h or not self.guests:
            return None
        masks = [open_]
        if seen is not None and seen is not open_:
            best = seen
            if open_ is not None:
                best = [a & b for a, b in zip(seen, open_, strict=True)]
            masks.insert(0, best)
        for n, shut in enumerate(masks):
            far = room(life, shut)
            if not n:
                blocks = len(far) * len(far[0])
                near = sum(d >= 2 for row in far for d in row)
                holes = sum(d >= VOID for row in far for d in row)
                self.calm = min(1.0, CALM * near / blocks)
                self.void = min(1.0, 2 * holes / blocks)
            landing = self.settle(life, far, shut)
            if landing is not None:
                return landing
        rng = self.rng
        small = [
            (p, self.weight(p, share))
            for p, share in self.guests
            if max(p.w, p.h) <= SMALL and p.kind in (SHIP, SEED)
        ]
        cells = [(rng.randrange(life.w), rng.randrange(life.h)) for _ in range(SITES)]
        return self.squeeze(life, open_, small, cells, 1)

    def settle(
        self, life: Life, far: Sequence[Sequence[int]], open_: Sequence[int] | None
    ) -> Landing | None:
        """A pattern in the middle of a hole of the map `far`."""
        rng = self.rng
        sites = [
            (bx, by, d) for by, row in enumerate(far) for bx, d in enumerate(row) if d
        ]
        if not sites:
            return None
        sites = rng.choices(sites, [d**3 for _, _, d in sites], k=SITES)
        guests = [(p, self.weight(p, share)) for p, share in self.guests]
        # The patterns too large for the widest hole: by their share.
        widest = (2 * max(d for row in far for d in row) + 1) * BLOCK - 2 * MARGIN
        large = [(p, weight) for p, weight in guests if max(p.w, p.h) > widest]
        share = sum(weight for _, weight in large)
        if rng.random() * sum(weight for _, weight in guests) < share:
            cells = [
                (bx * BLOCK + BLOCK // 2, by * BLOCK + BLOCK // 2)
                for bx, by, _ in sites
            ]
            landing = self.squeeze(life, open_, large, cells, MARGIN)
            if landing is not None:
                return landing
        for bx, by, d in sites:
            # Empty for sure: `2 * d - 1` blocks across. Tried: 2 more.
            reach = (2 * d + 1) * BLOCK - 2 * MARGIN
            sure = max(1, (2 * d - 1) * BLOCK - 2 * MARGIN)
            fits, weights = [], []
            for p, weight in guests:
                if weight and max(p.w, p.h) <= reach:
                    fits.append(p)
                    fill = (p.w * p.h) ** 0.5 / sure
                    weights.append(weight * min(1.0, max(0.1, fill)))
            if not fits:
                continue
            for p in rng.choices(fits, weights, k=PICKS):
                k = self.aim(p, far, bx, by)
                art = _patterns.turn(p.art, k)
                w, h = len(art[0]), len(art)
                x = bx * BLOCK + (BLOCK - w) // 2
                y = by * BLOCK + (BLOCK - h) // 2
                box = (x - MARGIN, y - MARGIN, w + 2 * MARGIN, h + 2 * MARGIN)
                if life.empty(*box) and _free(life, open_, *box):
                    return self.land(life, p, art, x, y)
        return None

    def aim(self, p: Pattern, far: Sequence[Sequence[int]], bx: int, by: int) -> int:
        """An orientation for `p` at the block `(bx, by)`: for a ship
        one with the most empty blocks ahead, up to `AHEAD`."""
        if p.move == (0, 0):
            return self.rng.randrange(8)
        gw, gh = len(far[0]), len(far)
        runs = []
        for k in range(8):
            dx, dy = _patterns.heading(p.move, k)
            dx, dy = (dx > 0) - (dx < 0), (dy > 0) - (dy < 0)
            n = 0
            while n < AHEAD and far[(by + (n + 1) * dy) % gh][(bx + (n + 1) * dx) % gw]:
                n += 1
            runs.append(n)
        return self.rng.choice([k for k in range(8) if runs[k] == max(runs)])

    def squeeze(
        self,
        life: Life,
        open_: Sequence[int] | None,
        guests: Sequence[tuple[Pattern, float]],
        cells: Iterable[tuple[int, int]],
        margin: int,
    ) -> Landing | None:
        """One of `guests`, each with its weight, around the one of
        `cells` with the fewest cells alive: they are removed, with
        those `margin` around. None when they are more than one in
        `SPARSE`."""
        rng = self.rng
        weights = [weight for _, weight in guests]
        if not any(weights):
            return None
        p = rng.choices([p for p, _ in guests], weights)[0]
        art = turned(p.art, rng)
        w, h = len(art[0]) + 2 * margin, len(art) + 2 * margin
        if w > life.w or h > life.h:
            return None
        places = []
        for cx, cy in cells:
            x, y = cx - w // 2, cy - h // 2
            if _free(life, open_, x, y, w, h):
                places.append((life.count(x, y, w, h), x, y))
        if not places:
            return None
        n, x, y = min(places)
        if n * SPARSE > w * h:
            return None
        life.put(["." * w] * h, x, y)
        return self.land(life, p, art, x + margin, y + margin)

    def land(
        self, life: Life, p: Pattern, art: Sequence[str], x: int, y: int
    ) -> Landing:
        life.put(art, x, y)
        self.recent.append(p.name)
        return Landing(
            p.name, (x + len(art[0]) // 2) % life.w, (y + len(art) // 2) % life.h
        )


def _free(
    life: Life, open_: Sequence[int] | None, x: int, y: int, w: int, h: int
) -> bool:
    """The whole box is open; it wraps."""
    if open_ is None:
        return True
    mask = life.span(x, w)
    return all(open_[(y + dy) % life.h] & mask == mask for dy in range(h))


def drop(
    life: Life, rng: random.Random, open_: Sequence[int] | None = None
) -> Landing | None:
    """`Host.drop`, once."""
    return Host(rng).drop(life, open_)


class _Every(dict[int, int]):
    """Width to the mask of one bit in `step`, from the lowest."""

    def __init__(self, step: int) -> None:
        self.step = step

    def __missing__(self, w: int) -> int:
        self[w] = sum(1 << x for x in range(0, w, self.step))
        return self[w]


_EVERY = {2: _Every(2), BLOCK: _Every(BLOCK)}
_EVEN = _EVERY[2]


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
        # Bit 2x, a level: the glyph x has a cell younger than it; a
        # cell that died then. The glyph of the dead of every level.
        young = [
            (m | m >> 1) & even
            for m in (
                (alive[0] ^ quad[0])
                | (alive[1] ^ quad[1])
                | (alive[2] ^ quad[2])
                | (alive[3] ^ quad[3])
                for quad in older
            )
        ]
        dead = []
        past = [0, 0, 0, 0]
        for quad in ghosts:
            m = quad[0] | quad[1] | quad[2] | quad[3]
            seen |= m
            dead.append((m | m >> 1) & even)
            past = [p | row for p, row in zip(past, quad, strict=True)]
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
                for mask in young:
                    if mask & low:
                        break
                    age += 1
                color = AGES[min(age, len(AGES) - 1)]
            elif not bits:
                bits = _glyph(past, shift)
                for k, mask in enumerate(dead):
                    if mask & low:
                        color = TRAIL[k]  # of the last to die
                        break
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


def touched(events: Iterable[tuple[str, Any]], frame: Frame) -> list[tuple[int, int]]:
    """The cells of the world under the pointer, of the `hover` events
    over the view of `frame`: the middle of each terminal cell."""
    rx, ry, w, h = frame.rect
    out = []
    for topic, data in events:
        if topic != "hover" or data.get("sprite") is None:
            continue
        x, y = data["x"] - rx, data["y"] - ry
        if 0 <= x < w and 0 <= y < h:
            out.append((2 * x + 1, 4 * y + 2))
    return out


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
    host = Host(rng, args.kinds) if args.guests else None
    guest = host.wait(args.rate) * args.speed if host else 0.0  # generations
    if args.cursor:
        try:
            pilot.subscribe("hover")
        except PtyrcError as err:  # a driver without the topic
            if err.code != "bad-params":
                raise
    for _ in _cli.ticks(1.0 / args.speed, seconds):
        if not sight.look():
            continue
        w, h = frame.size
        sized = (2 * w, 4 * h) != (life.w, life.h)
        fresh = not life.w
        if sized:
            life.resize(2 * w, 4 * h)
            watch.reset()
        if sight.changed or sized:
            clear = wall(sight.rows, w, h)
        open_ = clear if args.walls else None
        if fresh and host:
            host.open(life, open_, clear)
        elif fresh:
            life.soup(rng)
        life.step(open_)
        landed: Landing | None = None
        guest -= 1
        if watch.still(life.population) or (host and guest <= 0):
            if host:
                landed = host.drop(life, open_, clear)
                guest = host.wait(args.rate) * args.speed
            if not host or (landed is None and not life.population):
                life.soup(rng)
            watch.reset()
        for x, y in touched(sight.others, frame):
            life.wipe(x, y, REACH)
        cells = picture(life, args.colour, args.trail, clear)
        if args.cursor and (pad := w - sum(len(run["t"]) for run in cells[0])) > 0:
            # The whole view is its box: a hover is told anywhere on it.
            cells[0].append({"t": " " * pad})
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
                    at = (landed.x // 2, landed.y // 4)
                    sparks(pilot, frame, fx, [at], 14, layer, glyphs="⣿⠿⠶⠒⠂")


def _kinds(text: str) -> tuple[str, ...]:
    """`ship,gun` to the kinds; `all`: every one."""
    names = [name.strip() for name in text.split(",") if name.strip()]
    if "all" in names:
        return tuple(SHARES)
    for name in names:
        if name not in SHARES:
            raise argparse.ArgumentTypeError(f"no kind {name!r}: {', '.join(SHARES)}")
    if not names:
        raise argparse.ArgumentTypeError("no kind")
    return tuple(names)


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
        help="patterns dropped in, where the world is empty first",
    )
    out.add_argument(
        "--rate",
        type=float,
        default=RATE,
        help="with -g: patterns a second into an empty world, 4 times fewer "
        f"into a crowded one; 0: only when it is still [default: {RATE:g}]",
    )
    out.add_argument(
        "--kinds",
        type=_kinds,
        default=KINDS,
        metavar="KIND,...",
        help=f"with -g: the kinds dropped, of {', '.join(SHARES)}; or all "
        f"[default: {','.join(KINDS)}]",
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
    out.add_argument(
        "--cursor",
        action="store_true",
        help="the pointer of the host, moved over the pane, clears the cells near it",
    )
    out.add_argument("-a", "--all", action="store_true", help="-c -t -g -p -w --cursor")
    out.add_argument("--front", action="store_true", help="over the text")
    args = _cli.parse(out, argv)
    if args.all:
        args.colour = args.trail = args.guests = args.particles = args.walls = True
        args.cursor = True
    if args.speed <= 0:
        out.error("--speed must be positive")
    if args.rate < 0:
        out.error("--rate must not be negative")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
