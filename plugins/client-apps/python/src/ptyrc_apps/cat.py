# SPDX-License-Identifier: GPL-3.0-or-later
"""Cat: a cat that lives on the text, and sleeps when nothing moves.

    ptyrc run vim notes.txt          # or less, a shell
    ptyrc-app cat
    ptyrc-app cat -f active --front
    ptyrc-app cat --count 3 --random     # three cats, each its own look
    ptyrc-app cat --food --mice          # plates and mice, now and then
    ptyrc-app cat -a                     # --random --food --mice, two cats

What it shows:

- Events instead of a clock: the app subscribes to `idle`, `cursor`,
  `screen` and `frames`, and sleeps on the socket between two things
  to do. A cat asleep sends nothing: its breath is a looped animation
  played by the driver.
- Topic `idle`: the driver says when the pane has been still for a
  while. The cat yawns, curls up and sleeps; it wakes when the pane
  is busy again.
- `space="frame"`: the cat stands on a line of text, not on a row of
  the window. Scroll and it goes with its line.
- `draw.list` (`Sprite.info`): `visible` tells that a scroll left the
  cat out of the view, `covered` that text was written over it. It
  jumps to another place then, as it does when the cursor comes near
  or when the line under its paws goes.
- The back layer: text written where the cat is stays readable.

A place to stand is a `ledge`: a row with text under a box of free
cells, three rows by eleven columns (`ledges`). The bottom of the
view is one when it is free.

`--count N` cats share a `Home`: one read of the text for all, and no
two on the same cells. Each has its own `Coat` (colours); `--random`
draws the coats and a `Skin` (ears, eyes, nose) for each.

The word `food` on screen gets a plate next to it, on its line: right
of it, left of it, else on it. The cats come and eat, one on each
side; an empty plate goes, and the word gets no other. `--food` lays a
plate on a ledge now and then, for a while. `--mice`: a mouse walks a
ledge now and then; a cat that sees it pounces, the mouse runs, and
is caught when the cat lands on it. `-a`: `--random --food --mice`,
and two cats unless `--count` says otherwise.
"""

from __future__ import annotations

import argparse
import math
import random
import re
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Frame, Pilot, PtyrcError, Sprite, clock

from . import _cli
from ._reef import mirror, pad, paint

__all__ = [
    "COATS",
    "COVERED",
    "EARS",
    "EYES",
    "GAP",
    "GROUND",
    "MOUSE",
    "MW",
    "NOSES",
    "PH",
    "PLAIN",
    "PLATE",
    "POSES",
    "PW",
    "Cat",
    "Coat",
    "H",
    "Home",
    "Ledge",
    "Mouse",
    "Plate",
    "Skin",
    "W",
    "arc",
    "dress",
    "ledges",
    "look",
    "main",
    "mouse_look",
    "near",
    "perch",
    "plate_look",
]

W, H = 11, 3  # the box of every pose
STEP = 0.13  # seconds a step of the walk; one column
HOP = 0.05  # seconds between two points of a jump
NEAR = (7, 2)  # columns, rows: the cursor that close is a fright
READ = 0.3  # seconds, at least, between two reads of the text
AWAY = (2.0, 4.0)  # seconds out of the view before it comes back
GROUND = 5  # cells of text, at least, under the cat
COVERED = 0.2  # share of the cat under text that makes it move
PW, PH = 5, 1  # the box of a plate
GAP = 3  # cells from a word to its plate: the cursor after it scares no eater
MW = 9  # cells of a mouse, on one row
BITE = 2.5  # seconds a portion lasts
EMPTY = 3.0  # seconds an empty plate stays
PLATES = 3  # at most, at once
SERVED = (25.0, 45.0)  # seconds a plate of `--food` stays
FOOD = (20.0, 50.0)  # seconds between two of them
MICE = (15.0, 40.0)  # seconds between two mice
VISIT = (10.0, 20.0)  # seconds a mouse stays
SCURRY = (0.1, 0.045)  # seconds a column of a mouse: at ease, in flight
WITS = (0.05, 0.55)  # seconds before a mouse runs from a pounce
TRIES = 2  # pounces of a cat at one mouse
MAX_CATS = 12


@dataclass(frozen=True)
class Coat:
    """The colours of a cat, of the 256."""

    fur: int
    shade: int  # tail, chin
    eyes: int
    nose: int


COATS = (
    Coat(215, 208, 114, 211),  # ginger
    Coat(250, 244, 117, 217),  # grey
    Coat(255, 252, 81, 218),  # white
    Coat(180, 137, 150, 211),  # tabby
    Coat(223, 95, 39, 181),  # siamese
    Coat(243, 239, 220, 211),  # black
    Coat(110, 67, 228, 217),  # blue
    Coat(255, 208, 114, 211),  # calico
)


@dataclass(frozen=True)
class Skin:
    """The features of a cat: what `POSES` is drawn with."""

    ears: str = "/\\_/\\"
    eyes: str = "oO"  # at ease, wide open
    nose: str = "."


PLAIN = Skin()
EARS = ("/\\_/\\", "|\\_/|", "/^_^\\", "/\\-/\\")
EYES = ("oO", "•●", "uO", "*O")
NOSES = (".", "w", "_", "x", "v")

# Facing right. Each frame: three rows of `W` cells, then milliseconds.
POSES: dict[str, tuple[tuple[tuple[str, str, str], int], ...]] = {
    "sit": (
        (("     /\\_/\\ ", "    ( o.o )", "    ~> ^ < "), 2600),
        (("     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "), 140),
        (("     /\\_/\\ ", "    ( o.o )", "    _> ^ < "), 900),
        (("     /\\_/\\ ", "    ( o.o )", "    ~> ^ < "), 500),
        (("     /\\_/\\ ", "    ( o.o )", "    _> ^ < "), 1400),
    ),
    "alert": ((("     /\\_/\\ ", "    ( O.O )", "    _> ^ < "), 400),),
    "groom": (
        (("     /\\_/\\ ", "    ( -.o )", "    ~>\\^ < "), 260),
        (("     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "), 260),
        (("     /\\_/\\ ", "    ( -.o )", "    ~>\\^ < "), 260),
        (("     /\\_/\\ ", "    ( o.o )", "    _> ^ < "), 700),
    ),
    "yawn": (
        (("     /\\_/\\ ", "    ( -.- )", "    ~> ^ < "), 300),
        (("     /\\_/\\ ", "    ( -o- )", "    ~> ^ < "), 300),
        (("     /\\_/\\ ", "    ( -O- )", "    _> ^ < "), 500),
        (("     /\\_/\\ ", "    ( -o- )", "    _> ^ < "), 300),
    ),
    "sleep": (
        (("           ", "  __/\\_/\\  ", " (__( -.- )"), 1100),
        (("    z      ", "  __/\\_/\\  ", " (__( -.- )"), 900),
        (("   Z       ", " ___/\\_/\\  ", "(___( -.- )"), 900),
        (("  z        ", " ___/\\_/\\  ", "(___( -.- )"), 700),
    ),
    "walk": (
        (("      /\\_/\\", " ~.___(o.o)", "   /|   |\\ "), 0),
        (("      /\\_/\\", " -.___(o.o)", "   |\\   /| "), 0),
    ),
    "leap": ((("      /\\_/\\", " ~-=--(O.O)", "  ''     ``"), 0),),
    # The head down, over what is right of the box.
    "eat": (
        (("           ", " ~._/\\_/\\  ", " (__( o.o )"), 350),
        (("           ", " ~._/\\_/\\  ", " (__( -.- )"), 200),
        (("           ", " -._/\\_/\\  ", " (__( >.< )"), 200),
        (("           ", " ~._/\\_/\\  ", " (__( -.- )"), 200),
    ),
}

# A plate, by portions left.
PLATE = ("\\.../", "\\.o./", "\\.o./", "\\oOo/", "\\oOo/")
PLATE_INK = {"": 75, "oO": 173, ".": 130}

# Facing right: at rest, then the two steps of its walk.
MOUSE = ('~-(,_,"°>', '~~(,_,"°>', '-~(_,_"°>')
MOUSE_INK = {"": 250, "~-": 217, ">": 211, "°": 231, ",": 245}

_FACE = re.compile(r"\((\s?)(\S)(\S)(\S)(\s?)\)")
_WORD = re.compile(r"\bfood\b", re.IGNORECASE)


def dress(rows: Sequence[str], skin: Skin) -> tuple[str, ...]:
    """`rows` of `POSES` with the ears, the eyes and the nose of `skin`."""
    eyes = dict(zip("oO", skin.eyes, strict=True))

    def face(m: re.Match[str]) -> str:
        a, left, mid, right, b = m.groups()
        mid = skin.nose if mid == "." else mid
        return f"({a}{eyes.get(left, left)}{mid}{eyes.get(right, right)}{b})"

    return tuple(_FACE.sub(face, row.replace(EARS[0], skin.ears)) for row in rows)


def look(
    pose: str,
    right: bool = True,
    step: int | None = None,
    coat: Coat = COATS[0],
    skin: Skin = PLAIN,
) -> dict[str, Any]:
    """A pose as content for `Sprite.set`: a looped animation, or its
    frame `step` alone (the walk: the client is the clock)."""
    ink = {
        "": coat.fur,
        "oO" + skin.eyes: coat.eyes,
        "^wxv": coat.nose,
        "zZ": 244,
        "~_.": coat.shade,
    }

    def cells(rows: Sequence[str]) -> list[list[dict[str, Any]]]:
        rows = dress(rows, skin)
        return paint(rows if right else mirror(rows), ink, bold=True)

    frames = POSES[pose]
    if step is not None or len(frames) == 1:
        return {"cells": cells(frames[(step or 0) % len(frames)][0])}
    return {
        "frames": [{"cells": cells(rows), "ms": ms} for rows, ms in frames],
        "loop": True,
    }


def plate_look(left: int) -> dict[str, Any]:
    """A plate with `left` portions, as content for `Sprite.set`."""
    return {"cells": paint((PLATE[left],), PLATE_INK, bold=True)}


def mouse_look(right: bool = True, step: int | None = None) -> dict[str, Any]:
    """A mouse: the frame `step` of its walk, or at rest, its tail
    moving."""

    def cells(row: str) -> list[list[dict[str, Any]]]:
        return paint((row,) if right else mirror((row,)), MOUSE_INK)

    if step is not None:
        return {"cells": cells(MOUSE[1 + step % 2])}
    return {
        "frames": [{"cells": cells(row), "ms": 400} for row in MOUSE[:2]],
        "loop": True,
    }


@dataclass(frozen=True)
class Ledge:
    """Where the cat may stand: its left column from `x0` to `x1`,
    its paws on the row above `g`. Frame cells."""

    g: int
    x0: int
    x1: int

    def has(self, x: int) -> bool:
        return self.x0 <= x <= self.x1


def ledges(rows: Sequence[str], box: tuple[int, int, int, int]) -> list[Ledge]:
    """The ledges of a view. `rows`: its text; `box`: its frame cells
    (x0, y0, x1, y1), ends excluded. Under the cat, text on `GROUND`
    cells at least; around it, its box free and a free column on each
    side. The floor of the view counts, with nothing under it."""
    x0, y0, x1, y1 = box
    width, height = x1 - x0, y1 - y0
    rows = pad([*rows, *[""] * height][:height], width) + ["#" * width]
    ink = [[not ch.isspace() for ch in row] for row in rows]
    out = []
    for g in range(H, height + 1):
        free = [not any(ink[y][x] for y in range(g - H, g)) for x in range(width)]
        run: list[int] = []
        for x in range(width - W + 2):
            ok = (
                x <= width - W
                and all(free[max(0, x - 1) : x + W + 1])
                and sum(ink[g][x : x + W]) >= GROUND
            )
            if ok:
                run.append(x)
            elif run:
                out.append(Ledge(y0 + g, x0 + run[0], x0 + run[-1]))
                run = []
    return out


def perch(spots: Sequence[Ledge], x: int, g: int) -> Ledge | None:
    """The ledge the cat at `x` above row `g` stands on, if any."""
    return next((s for s in spots if s.g == g and s.has(x)), None)


def near(x: int, g: int, cursor: tuple[int, int] | None) -> bool:
    """The cursor is in the cat's box, or `NEAR` it."""
    if cursor is None:
        return False
    dx, dy = NEAR
    return x - dx <= cursor[0] < x + W + dx and g - H - dy <= cursor[1] < g + dy


def arc(a: tuple[int, int], b: tuple[int, int]) -> list[tuple[int, int]]:
    """The cells of a jump from `a` to `b`, one per `HOP`: a parabola,
    higher and longer for a long jump; `b` is the last."""
    far = math.hypot(b[0] - a[0], 2 * (b[1] - a[1]))
    n = max(4, min(16, round(4 + far / 6)))
    rise = min(4.0, 1.5 + far / 20)
    out = []
    for i in range(1, n + 1):
        t = i / n
        lift = 4 * rise * t * (1 - t)
        out.append(
            (round(a[0] + (b[0] - a[0]) * t), round(a[1] + (b[1] - a[1]) * t - lift))
        )
    return out


class Plate:
    """A plate: its left column `x`, on the row above `g`; frame cells.
    `word`: the cell of the word it was laid for, None for one of
    `--food`. `seats`: who eats there, by the side the cat faces."""

    def __init__(
        self, x: int, g: int, word: tuple[int, int] | None, until: float
    ) -> None:
        self.x, self.g = x, g
        self.word = word
        self.until = until  # when it goes
        self.left = len(PLATE) - 1  # portions
        self.seats: dict[bool, Cat] = {}
        self.sprite: Sprite | None = None


class Mouse:
    """A mouse on a ledge: `x`, its left column, from `lo` to `hi`, on
    the row above `g`. It walks to a `goal`, stops, and walks again;
    after its visit, or scared, the goal is an end of the ledge, where
    it goes (`out`)."""

    def __init__(self, home: Home, ledge: Ledge, now: float) -> None:
        rng = home.rng
        self.home = home
        self.g = ledge.g
        self.lo, self.hi = ledge.x0, ledge.x1 + W - MW
        self.x = rng.choice((self.lo, self.hi))
        self.goal = rng.randint(self.lo, self.hi)
        self.right = self.goal >= self.x
        self.still = False  # stopped at its goal
        self.pace = SCURRY[0]
        self.out = False
        self.due = now
        self.leave = now + rng.uniform(*VISIT)
        self.fright = math.inf  # when it runs
        self.threat = 0  # the column it runs from
        self.sprite: Sprite | None = None

    def show(self, step: int | None = None) -> None:
        self.sprite = self.home.put(
            self.sprite, self.x, self.g - 1, mouse_look(self.right, step), z=-1
        )

    def head(self, goal: int, out: bool = False) -> None:
        self.goal, self.out = max(self.lo, min(goal, self.hi)), out
        self.right = self.goal >= self.x

    def scare(self, x: int, now: float) -> None:
        """A cat jumps to column `x`: run from it, after a moment."""
        self.threat = x
        self.fright = min(self.fright, now + self.home.rng.uniform(*WITS))
        self.due = min(self.due, self.fright)

    def step(self, now: float) -> None:
        rng = self.home.rng
        if now >= self.fright:
            self.fright, self.pace, self.still = math.inf, SCURRY[1], False
            away = 2 * self.x + MW >= 2 * self.threat + W
            self.head(self.hi if away else self.lo, out=True)
        elif self.still:
            # The stop is over: another walk, or the nearest way out.
            self.still = False
            if now >= self.leave:
                near_lo = self.x - self.lo <= self.hi - self.x
                self.head(self.lo if near_lo else self.hi, out=True)
            else:
                self.head(rng.randint(self.lo, self.hi))
        if self.x == self.goal:
            if self.out:
                return self.home.shoo(now)
            self.still = True
            self.show()
            self.due = now + rng.uniform(0.6, 2.2)
            return
        self.x += 1 if self.goal > self.x else -1
        self.show(self.x)
        self.due = now + self.pace


class Cat:
    """A cat of a `Home`: where it stands (`x`, left column; `g`, the
    row under its paws; frame cells), what it does (`act`) and until
    when."""

    def __init__(
        self, home: Home, coat: Coat = COATS[0], skin: Skin = PLAIN, z: int = 0
    ) -> None:
        self.home = home
        self.rng = home.rng
        self.coat, self.skin, self.z = coat, skin, z
        self.sprite: Sprite | None = None
        self.x = self.g = 0
        self.right = True
        self.act = "away"
        self.due = 0.0  # when the act ends, or its next step
        self.goal = 0  # the walk: the column to reach
        self.path: list[tuple[int, int]] = []  # the jump: cells left
        self.landing: tuple[int, int] | None = None
        self.meal: Plate | None = None  # the plate it eats at, or goes to
        self.prey = False  # the jump is a pounce
        self.tries = 0  # pounces at the mouse there is

    # -- what the cat knows -------------------------------------------

    def claims(self) -> list[tuple[int, int]]:
        """The places it holds: where it is, where it goes."""
        if self.act == "away":
            return []
        out = [(self.x, self.g)]
        if self.act == "leap" and self.landing is not None:
            out.append(self.landing)
        elif self.act == "walk":
            out.append((self.goal, self.g))
        return out

    def pick(self, far: bool = False) -> tuple[int, Ledge] | None:
        """A place to go: a column of a ledge, away from the cursor and
        from the other cats; `far`: the farthest of a few from where it
        stands."""
        home = self.home
        spots = [
            (x, s)
            for s in home.spots
            for x in {s.x0, s.x1, self.rng.randint(s.x0, s.x1)}
            if not near(x, s.g, home.cursor)
            and not home.taken(self, x, s.g)
            and not home.laid(x, s.g)
        ]
        if not spots:
            return None
        some = self.rng.sample(spots, min(6, len(spots)))
        if not far:
            return some[0]
        return max(some, key=lambda p: math.hypot(p[0] - self.x, 2 * (p[1].g - self.g)))

    # -- what the cat does --------------------------------------------

    def show(self, pose: str, step: int | None = None) -> None:
        """Pose and place, in one request: one repaint."""
        content = look(pose, self.right, step, self.coat, self.skin)
        self.sprite = self.home.put(self.sprite, self.x, self.g - H, content, z=self.z)

    def rest(self, pose: str, now: float, seconds: float | None) -> None:
        """A pose held `seconds`; None: until something happens."""
        self.act, self.due = pose, math.inf if seconds is None else now + seconds
        self.show(pose)

    def walk(self, goal: int, now: float) -> None:
        self.act, self.goal, self.due = "walk", goal, now
        self.right = goal >= self.x

    def jump(self, to: tuple[int, int], now: float, start: tuple[int, int]) -> None:
        """Leap from `start` (left column, row under the paws) to `to`."""
        self.act, self.due, self.landing = "leap", now, to
        self.right = to[0] >= start[0]
        self.path = arc(start, to)

    def quit(self) -> None:
        """Leave the plate, forget the mouse."""
        if self.meal is not None:
            seats = self.meal.seats
            for side in [k for k, cat in seats.items() if cat is self]:
                del seats[side]
        self.meal, self.prey = None, False

    def flee(self, now: float) -> None:
        """Somewhere else, at once; nowhere to go: stay."""
        if self.act in ("leap", "away"):
            return
        home = self.home
        if home.stale or now - home.read > 2.0:
            home.survey(now)
        if (to := self.pick(far=True)) is not None:
            self.quit()
            self.jump((to[0], to[1].g), now, (self.x, self.g))

    def arrive(self, now: float) -> None:
        """Into the view from above it, on a ledge."""
        home = self.home
        fresh = not home.stale and home.spots and now - home.read < 1.0
        if not (fresh or home.survey(now)) or (to := self.pick()) is None:
            self.due = now + 0.3
            return
        x, g = to[0], to[1].g
        self.jump((x, g), now, (x, g))
        top = max(home.box()[1] + H, g - 6)
        self.path = [(x, y) for y in range(top, g + 1)]  # straight down

    def leave(self, now: float) -> None:
        """Out of the view: wait, then come back in it."""
        self.quit()
        self.act, self.due = "away", now + self.rng.uniform(*AWAY)

    def dine(self, plate: Plate, right: bool, now: float) -> None:
        """To a side of `plate`; `right`: the one where it faces right."""
        plate.seats[right] = self
        self.meal = plate
        x = plate.x - W if right else plate.x + PW
        self.jump((x, plate.g), now, (self.x, self.g))

    def hunt(self, to: tuple[int, int], now: float) -> None:
        """Pounce: `to` is where the mouse is, or will be."""
        self.tries += 1
        self.jump(to, now, (self.x, self.g))
        self.prey = True
        if (mouse := self.home.mouse) is not None:
            mouse.scare(to[0], now)

    def land(self, now: float) -> None:
        """The jump is over."""
        home = self.home
        self.x, self.g = self.landing or (self.x, self.g)
        home.stale = True  # a look around, soon
        if self.meal is not None:
            self.right = self.meal.x > self.x
            self.rest("eat", now, BITE)
        elif self.prey and home.caught(self):
            self.prey = False
            home.shoo(now)
            self.rest("eat", now, 2.0)
        else:
            self.prey = False
            self.rest("alert", now, 0.9)

    def stir(self, now: float) -> None:
        """The pane went idle, or busy."""
        if not self.home.idle and self.act in ("sleep", "yawn"):
            self.rest("alert", now, 0.6)
        elif self.home.idle and self.act in ("sit", "groom"):
            self.due = min(self.due, now + self.rng.uniform(0.5, 2.0))

    def check(self, info: dict[str, Any] | None, now: float) -> bool:
        """`info`: its entry in `draw.list`. False when it is not seen:
        it left."""
        if not info:  # its rows were forgotten: the driver took it
            self.sprite = None
        if not info or not info["visible"]:
            self.leave(now)
            return False
        return True

    def step(self, now: float) -> None:
        """The act is due: its next step, or what comes after."""
        rng, home = self.rng, self.home
        if self.act == "away":
            self.arrive(now)
        elif self.act == "leap":
            if self.path:
                self.x, self.g = self.path.pop(0)
                self.show("leap")
                self.due = now + HOP
            else:
                self.land(now)
        elif self.act == "walk":
            ahead = self.x + (1 if self.goal > self.x else -1)
            if self.x == self.goal or home.taken(self, ahead, self.g):
                self.goal = self.x
                return self.rest("sit", now, rng.uniform(2.0, 6.0))
            self.x = ahead
            self.show("walk", self.x)
            self.due = now + STEP
        elif self.act == "eat" and self.meal is not None and self.meal.left:
            home.bite(self.meal, now)
            self.due = now + BITE
        elif self.act == "eat":  # the plate is empty, the mouse eaten
            self.quit()
            self.rest("groom", now, rng.uniform(2.5, 5.0))
        elif (aim := home.scent(self)) is not None:
            self.hunt(aim, now)
        elif (seat := home.seat(self)) is not None:
            self.dine(*seat, now)
        elif self.act == "yawn":
            self.rest("sleep", now, None)
        elif home.idle:
            self.rest("yawn", now, 1.4)
        else:
            self.wander(now)

    def wander(self, now: float) -> None:
        """Awake with nothing to do: groom, walk the ledge, hop to
        another, or sit a while more."""
        rng, roll, home = self.rng, self.rng.random(), self.home
        here = perch(home.spots, self.x, self.g)
        if roll < 0.25 and self.act != "groom":
            self.rest("groom", now, rng.uniform(2.5, 5.0))
        elif roll < 0.65 and here is not None and here.x1 > here.x0:
            goal = rng.randint(here.x0, here.x1)
            if abs(goal - self.x) < 3:
                goal = here.x0 if self.x - here.x0 > here.x1 - self.x else here.x1
            free = not home.taken(self, goal, here.g)
            if free and not near(goal, here.g, home.cursor):
                return self.walk(goal, now)
            self.rest("sit", now, rng.uniform(2.0, 5.0))
        elif roll < 0.85 and (to := self.pick()) is not None:
            self.jump((to[0], to[1].g), now, (self.x, self.g))
        else:
            self.rest("sit", now, rng.uniform(2.0, 6.0))


class Home:
    """What the cats share: the frame, its text and its ledges, the
    cursor, whether the pane is idle; the plates and the mouse.

    `count` cats, a coat each; `varied`: coats and skins drawn. `food`,
    `mice`: plates and mice now and then, from `now` on."""

    def __init__(
        self,
        pilot: Pilot,
        frame: Frame,
        rng: random.Random,
        front: bool = False,
        *,
        count: int = 1,
        varied: bool = False,
        food: bool = False,
        mice: bool = False,
        now: float = 0.0,
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.layer = "front" if front else "back"
        self.idle = False
        self.cursor: tuple[int, int] | None = None
        self.rows: list[str] = []  # the text of the view, when read
        self.view = (0, 0, 0, 0)  # and its box
        self.spots: list[Ledge] = []
        self.read = 0.0  # time of the last read of the text
        self.stale = True  # the text changed since
        coats = rng.sample(COATS, len(COATS)) if varied else COATS
        self.cats = []
        for i in range(count):
            skin = PLAIN
            if varied:
                skin = Skin(rng.choice(EARS), rng.choice(EYES), rng.choice(NOSES))
            cat = Cat(self, coats[i % len(coats)], skin, z=i)
            cat.due = now + i * rng.uniform(0.5, 1.5) if i else 0.0
            self.cats.append(cat)
        self.plates: list[Plate] = []
        self.eaten: set[tuple[int, int]] = set()  # words whose plate is empty
        self.mouse: Mouse | None = None
        self.next_food = now + rng.uniform(4.0, 10.0) if food else math.inf
        self.next_mouse = now + rng.uniform(3.0, 8.0) if mice else math.inf

    # -- the place ----------------------------------------------------

    def box(self) -> tuple[int, int, int, int]:
        (ox, oy), (w, h) = self.frame.origin, self.frame.size
        return (ox, oy, ox + w, oy + h)

    def survey(self, now: float) -> bool:
        """Read the view, find its ledges, lay the plates of its words;
        False while the frame cannot be drawn on."""
        if not _cli.on_screen(self.frame):
            return False
        with _cli.in_sight():
            box = self.box()
            width, height = box[2] - box[0], box[3] - box[1]
            rows = pad([*self.frame.text(), *[""] * height][:height], width)
            self.rows, self.view = rows, box
            self.spots = ledges(rows, box)
            self.read, self.stale = now, False
            self.lay(now)
            return True
        return False

    def free(self, x: int, y: int, w: int, h: int) -> bool:
        """The box at (`x`, `y`) is in the view as read, with no text."""
        x0, y0, x1, y1 = self.view
        if x < x0 or y < y0 or x + w > x1 or y + h > y1:
            return False
        rows = self.rows[y - y0 : y - y0 + h]
        return all(row[x - x0 : x - x0 + w].isspace() for row in rows)

    def taken(self, cat: Cat, x: int, g: int) -> bool:
        """Another cat holds cells of the box at `x` above `g`, or next
        to it."""
        return any(
            abs(x - ox) <= W and abs(g - og) < H
            for other in self.cats
            if other is not cat
            for ox, og in other.claims()
        )

    def put(
        self, sprite: Sprite | None, x: int, y: int, content: dict[str, Any], z: int = 0
    ) -> Sprite:
        """Draw at (`x`, `y`), or move and redraw `sprite`: one repaint."""
        if sprite is None:
            return self.pilot.draw(
                frame=self.frame.id,
                at=(x, y),
                space="frame",
                layer=self.layer,
                z=z,
                **content,
            )
        with self.pilot.batch():
            sprite.update(anchor={"at": [x, y], "space": "frame"})
            sprite.set(**content)
        return sprite

    def call(self, now: float, asleep: float = 1.0) -> None:
        """Something new to see: the cats at rest look soon; of those
        asleep, a share."""
        for cat in self.cats:
            if cat.act in ("sit", "groom", "alert") or (
                cat.act in ("sleep", "yawn") and self.rng.random() < asleep
            ):
                cat.due = min(cat.due, now + self.rng.uniform(0.2, 1.2))

    # -- the plates ---------------------------------------------------

    def chairs(self, x: int, g: int) -> list[tuple[bool, int]]:
        """The sides of a plate at `x` above `g` with room for a cat:
        whether it faces right there, and its column."""
        sides = ((True, x - W), (False, x + PW))
        return [(r, cx) for r, cx in sides if self.free(cx, g - H, W, H)]

    def room(self, x: int, g: int) -> bool:
        """A plate fits at `x` above `g`: no text, no other plate."""
        return self.free(x, g - PH, PW, PH) and not any(
            abs(x - p.x) < PW + 1 and abs(g - p.g) < PH for p in self.plates
        )

    def laid(self, x: int, g: int) -> bool:
        """A plate is in the box of a cat at `x` above `g`."""
        return any(
            abs(g - p.g) < H and x < p.x + PW and p.x < x + W for p in self.plates
        )

    def beside(self, x: int, y: int, n: int) -> tuple[int, int] | None:
        """Where the plate of the word at (`x`, `y`), `n` cells, goes:
        right of it, left of it, or on it; first where a cat can sit."""
        x0, _, x1, _ = self.view
        over = max(x0, min(x + (n - PW) // 2, x1 - PW))
        sites = [
            site
            for site in ((x + n + GAP, y + 1), (x - PW - GAP, y + 1), (over, y))
            if self.room(*site)
        ]
        seated = (site for site in sites if self.chairs(*site))
        return next(seated, sites[0] if sites else None)

    def dish(
        self, at: tuple[int, int], word: tuple[int, int] | None, until: float
    ) -> Plate:
        plate = Plate(*at, word, until)
        self.plates.append(plate)
        plate.sprite = self.put(None, plate.x, plate.g - PH, plate_look(plate.left), -1)
        return plate

    def lay(self, now: float) -> None:
        """After a read: a plate for each `food` in the view; those
        whose word went, or with text over them, go. The mouse goes
        with its ledge."""
        x0, y0 = self.view[:2]
        words = {
            (x0 + m.start(), y0 + y): len(m[0])
            for y, row in enumerate(self.rows)
            for m in _WORD.finditer(row)
        }
        for plate in [*self.plates]:
            gone = plate.word is not None and plate.word not in words
            if gone or not self.free(plate.x, plate.g - PH, PW, PH):
                self.clear(plate, now)
        self.eaten &= words.keys()
        laid = {plate.word for plate in self.plates} | self.eaten
        for word, n in words.items():
            if len(self.plates) >= PLATES:
                break
            if word not in laid and (at := self.beside(*word, n)) is not None:
                self.dish(at, word, math.inf)
                self.call(now)
        mouse = self.mouse
        if mouse is not None and not any(
            s.g == mouse.g and s.x0 <= mouse.lo and mouse.hi <= s.x1 + W - MW
            for s in self.spots
        ):
            self.shoo(now)

    def serve(self, now: float) -> None:
        """A plate of `--food`, on a ledge with room for a cat."""
        rng = self.rng
        self.next_food = now + 5.0  # nowhere to put it: soon again
        if len(self.plates) >= PLATES or (self.stale and not self.survey(now)):
            return
        for s in rng.sample(self.spots, len(self.spots)):
            x = rng.randint(s.x0, s.x1 + W - PW)
            busy = any(
                og == s.g and ox < x + PW + 1 and x < ox + W + 1
                for cat in self.cats
                for ox, og in cat.claims()
            )
            if busy or near(x, s.g, self.cursor) or not self.room(x, s.g):
                continue
            if self.chairs(x, s.g):
                self.dish((x, s.g), None, now + rng.uniform(*SERVED))
                self.next_food = math.inf  # after this one
                return self.call(now)

    def seat(self, cat: Cat) -> tuple[Plate, bool] | None:
        """The nearest free side of a plate with food left."""
        seats = [
            (math.hypot(cx - cat.x, 2 * (plate.g - cat.g)), i, plate, right)
            for i, plate in enumerate(self.plates)
            if plate.left
            for right, cx in self.chairs(plate.x, plate.g)
            if right not in plate.seats
            and not near(cx, plate.g, self.cursor)
            and not self.taken(cat, cx, plate.g)
        ]
        return min(seats)[2:] if seats else None

    def bite(self, plate: Plate, now: float) -> None:
        """A portion less; the last: the plate goes soon, for good."""
        plate.left -= 1
        plate.sprite = self.put(
            plate.sprite, plate.x, plate.g - PH, plate_look(plate.left), -1
        )
        if not plate.left:
            plate.until = now + EMPTY
            if plate.word is not None:
                self.eaten.add(plate.word)

    def clear(self, plate: Plate, now: float) -> None:
        """Take `plate` away; who was eating looks up."""
        self.plates.remove(plate)
        if plate.sprite is not None:
            plate.sprite.remove()
        for cat in [*plate.seats.values()]:
            cat.quit()
            if cat.act == "eat":
                cat.rest("alert", now, 0.6)
        if plate.word is None:
            self.next_food = now + self.rng.uniform(*FOOD)

    # -- the mouse ----------------------------------------------------

    def loose(self, now: float) -> None:
        """A mouse, at an end of a long ledge."""
        self.next_mouse = now + 5.0  # no ledge for it: soon again
        if self.stale and not self.survey(now):
            return
        long = [s for s in self.spots if s.x1 + W - MW - s.x0 >= 16]
        if long:
            self.mouse = Mouse(self, self.rng.choice(long), now)
            for cat in self.cats:
                cat.tries = 0
            self.call(now, asleep=0.5)

    def shoo(self, now: float) -> None:
        """The mouse goes: out, or caught."""
        if self.mouse is not None and self.mouse.sprite is not None:
            self.mouse.sprite.remove()
        self.mouse = None
        self.next_mouse = now + self.rng.uniform(*MICE)

    def scent(self, cat: Cat) -> tuple[int, int] | None:
        """Where `cat` lands to catch the mouse: its head over it, a
        little ahead of a mouse that walks. None: no mouse, enough
        pounces, or no room."""
        mouse = self.mouse
        if mouse is None or cat.tries >= TRIES:
            return None
        ahead = 0
        if mouse.x != mouse.goal:
            ahead = self.rng.randint(0, 4) * (1 if mouse.goal > mouse.x else -1)
        behind = 2 * cat.x + W <= 2 * mouse.x + MW  # the cat comes from the left
        x = mouse.x + ahead + (MW - W if behind else 0)
        x = max(mouse.lo, min(x, mouse.hi + MW - W))
        if near(x, mouse.g, self.cursor) or self.taken(cat, x, mouse.g):
            return None
        return (x, mouse.g)

    def caught(self, cat: Cat) -> bool:
        """`cat` stands on the mouse."""
        mouse = self.mouse
        return (
            mouse is not None
            and mouse.g == cat.g
            and mouse.x < cat.x + W
            and cat.x < mouse.x + MW
        )

    # -- what happens -------------------------------------------------

    def on(self, topic: str, data: Any, now: float) -> None:
        """An event of the driver."""
        if topic == "idle" and data["frame"] == self.frame.id:
            self.idle = bool(data["idle"])
            for cat in self.cats:
                cat.stir(now)
        elif topic == "cursor":
            # In frame cells when the cursor is in the cats' frame: no
            # origin to keep up to date.
            if not data.get("visible", True):
                self.cursor = None
            elif data.get("frame") == self.frame.id:
                self.cursor = tuple(data["pos"])
            else:
                self.cursor = self.frame.to_frame(data["x"], data["y"])
            for cat in self.cats:
                if near(cat.x, cat.g, self.cursor):
                    cat.flee(now)
        elif topic in ("screen", "frames"):
            self.stale = True

    def check(self, now: float) -> None:
        """After a change of the text or of the view: is each cat seen,
        and is it still standing on something? A cat at a plate needs
        no ledge."""
        cats = [
            cat
            for cat in self.cats
            if cat.act not in ("leap", "away") and cat.sprite is not None
        ]
        infos: list[dict[str, Any] | None] = []
        if cats:
            with self.pilot.batch():
                infos = [cat.sprite.info() for cat in cats if cat.sprite is not None]
        seen = [
            (cat, info)
            for cat, info in zip(cats, infos, strict=True)
            if cat.check(info, now)
        ]
        if not self.survey(now):
            return
        for cat, info in seen:
            off = cat.meal is None and perch(self.spots, cat.x, cat.g) is None
            if info is not None and (info["covered"] > COVERED or off):
                cat.flee(now)

    def step(self, now: float) -> None:
        """What is due: a look at the text, the cats, the mouse, the
        plates."""
        if self.stale and now - self.read >= READ:
            self.stale = False
            self.check(now)
        for cat in self.cats:
            if now >= cat.due:
                cat.step(now)
        if self.mouse is not None:
            if now >= self.mouse.due:
                self.mouse.step(now)
        elif now >= self.next_mouse:
            self.loose(now)
        for plate in [*self.plates]:
            if now >= plate.until:
                if plate.left and plate.seats:
                    plate.until = now + 2 * BITE  # not under their nose
                else:
                    self.clear(plate, now)
        if now >= self.next_food:
            self.serve(now)

    def due(self, now: float) -> float:
        """When `step` has something to do."""
        times = [cat.due for cat in self.cats]
        times += [plate.until for plate in self.plates]
        times += [self.next_food, now + READ if self.stale else math.inf]
        times.append(self.next_mouse if self.mouse is None else self.mouse.due)
        return min(times)

    def move(self, frame: Frame, now: float) -> None:
        """To `frame`, which took the place of this one: all leave, the
        cats come back."""
        if frame.id != self.frame.id:
            for cat in self.cats:
                if cat.sprite is not None:
                    cat.sprite.remove()
                    cat.sprite = None
        self.frame = frame
        for plate in [*self.plates]:
            self.clear(plate, now)
        if self.mouse is not None:
            self.shoo(now)
        for cat in self.cats:
            cat.leave(now)


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    rng = random.Random(args.seed)
    frame = _cli.pick(pilot, args.frame)
    start = clock.monotonic()
    home = Home(
        pilot,
        frame,
        rng,
        args.front,
        count=args.count,
        varied=args.random,
        food=args.food,
        mice=args.mice,
        now=start,
    )
    # With a snapshot the first events tell what already is: whether
    # the frame is idle, where the cursor is. Nothing is asked.
    pilot.subscribe("idle", "cursor", "screen", "frames", snapshot=True)
    end = None if seconds is None else start + seconds
    lost = 0.0  # since when the frame is out of sight
    # Until the session ends: a closed connection raises out of `events`.
    while True:
        now = clock.monotonic()
        if end is not None and now >= end:
            return
        try:
            with _cli.in_sight():
                home.step(now)
        except PtyrcError as exc:
            if exc.code != "no-frame":
                raise
        # The frame may be replaced (a resize of a pane with little
        # text): after a while out of sight, look for it again.
        if frame.usable:
            lost = 0.0
        elif not lost:
            lost = now
        elif now - lost > 2.0:
            lost = 0.0
            frame = _cli.pick(pilot, args.frame)
            home.move(frame, now)
        # Sleep on the socket until the next thing to do, or an event.
        wake = home.due(now)
        if not frame.usable:
            wake = min(wake, now + 0.5)
        if end is not None:
            wake = min(wake, end)
        timeout = None if wake == math.inf else max(0.001, wake - clock.monotonic())
        for topic, data in pilot.events(timeout=timeout):
            home.on(topic, data, clock.monotonic())
            break


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser(
        "cat", "Cats on the text; they sleep when nothing moves.", frame="largest"
    )
    out.add_argument("--front", action="store_true", help="in front of the text")
    out.add_argument("--seed", type=int, help="same seed, same cats")
    out.add_argument("--count", type=int, help="cats [default: 1; 2 with -a]")
    out.add_argument(
        "-r", "--random", action="store_true", help="coats and features drawn"
    )
    out.add_argument("--food", action="store_true", help="a plate now and then")
    out.add_argument("--mice", action="store_true", help="a mouse now and then")
    out.add_argument(
        "-a", "--all", action="store_true", help="--random --food --mice, two cats"
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.random = args.food = args.mice = True
    if args.count is None:
        args.count = 2 if args.all else 1
    if not 1 <= args.count <= MAX_CATS:
        out.error(f"--count must be between 1 and {MAX_CATS}")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
