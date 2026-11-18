# SPDX-License-Identifier: GPL-3.0-or-later
"""Cat: a cat that lives on the text, and sleeps when nothing moves.

    ptyrc run vim notes.txt          # or less, a shell
    ptyrc-app cat
    ptyrc-app cat -f active --front
    ptyrc-app cat --count 3 --random     # three cats, each its own look
    ptyrc-app cat --food --mice          # plates and mice, now and then
    ptyrc-app cat --fish --toys          # a fish, a ball of yarn
    ptyrc-app cat --hunt                 # they hunt the host mouse
    ptyrc-app cat -a                     # all of these, two cats

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

`--count N` cats share a `Home`: one read of the view for all, and no
two on the same cells. The view is read as a shape (`Frame.shape`):
where there is ink, and the words of `WORDS` as shown. Each has its own `Coat` (colours); `--random`
draws the coats and a `Skin` (ears, eyes, nose) for each.

The word `food` on screen gets a plate next to it, on its line: right
of it, left of it, else on it. The cats come and eat, one on each
side; an empty plate goes, and the word gets no other. `--food` lays a
plate on a ledge now and then, for a while. `--mice`: a mouse walks a
ledge now and then; a cat that sees it pounces, the mouse runs, and
is caught when the cat lands on it.

The word `fish` gets a fish (`--fish`: one on a ledge now and then). A
cat lands by it, takes it, carries it away and eats it there; another
may come for it: it has it, or is hissed at. The word `toy` gets a
ball of yarn (`--toys`), braille. A cat bats it: it jumps, bounces,
rolls, falls off the end of its line to the one below; the cat runs
after it, and leaps down to it. Its thread is left along its way, and
sinks to the floor. Played out, the cat lies in the thread, the ball
goes, the thread after it.

The moves are paced for the eye: a run starts and ends slower, a
column every 3 frames of a 60 Hz screen at its fastest, its gait a
cycle of six poses, one a column; a step of a jump, a step of the
ball: 2 frames.

`--hunt`: the cats hunt the pointer of the host mouse (topic `hover`).
The driver tells its moves over sprites only: a sprite of nothing
covers the view (`Home.mat`). A cat looks at the pointer, runs to it,
wiggles, pounces, bats it; stands up under it and leaps when it is
over its box; looks for it when it goes; loses interest in one that
rests, and purrs if it rests on it. A sleeper is poked awake.

`-a`: every option, and two cats unless `--count` says otherwise.
"""

from __future__ import annotations

import argparse
import contextlib
import math
import random
import re
from collections.abc import Iterator, Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Frame, Pilot, PtyrcError, Sprite, clock

from . import _cli
from ._canvas import Braille
from ._reef import mirror, pad, paint

__all__ = [
    "BW",
    "COATS",
    "COVERED",
    "EARS",
    "EYES",
    "FISH",
    "FW",
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
    "REACH",
    "TICK",
    "WORDS",
    "Cat",
    "Coat",
    "Fish",
    "H",
    "Home",
    "Ledge",
    "Mouse",
    "Plate",
    "Skin",
    "W",
    "Yarn",
    "arc",
    "ball_look",
    "dress",
    "fish_look",
    "ledges",
    "look",
    "main",
    "mass",
    "mouse_look",
    "near",
    "perch",
    "plate_look",
    "wave",
]

W, H = 11, 3  # the box of every pose
STEP = 0.13  # seconds a step of the walk; one column
HOP = 1 / 30  # seconds between two points of a jump
RISE = 0.05  # seconds a row, straight up or down
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
RUN = 0.05  # seconds a step of a run, one column, at its fastest
EASE = (0.1, 1 / 12, 1 / 15)  # and of its first columns, of its last
NOTICE = 0.3  # seconds a cat looks at the pointer before it goes
FUN = 8.0  # seconds its interest takes to fall to 1/e
BORED = 0.15  # interest under which it turns away
POUNCE = 14  # columns, at most, of a pounce along a ledge
REACH = 3  # rows over its box a cat leaps up to
POKE = (12, 3)  # columns, rows: the pointer that close wakes a sleeper
FW = 6  # cells of a fish, on one row
STEALS = 2  # tries of the others at one fish
LYING = (20.0, 40.0)  # seconds a fish of `--fish`, a ball of `--toys` wait
FISHES = (25.0, 60.0)  # seconds between two fishes
TOYS = (25.0, 60.0)  # seconds between two balls
PLAYS = (3, 5)  # bats a ball of yarn takes
UNDONE = 2.5  # seconds a ball stays once played out
BW = 2  # cells of a ball, on one row: 4 dots by 4
TICK = 1 / 30  # seconds a step of the ball, of its thread
# The ball, in dots: a cell is 2 by 4. Speeds a second.
BAT = (18.0, 34.0)  # what a bat gives it, along
# And up, one of: a hop, a jump, a leap: 4 rows, over 3 rows of text.
LIFT = ((24.0, 32.0), (36.0, 48.0), (68.0, 74.0))
GRAVITY = 150.0  # a second squared
FALL = 90.0  # its fastest, down
BOUNCE = 0.5  # share of its speed a floor gives back; under `SOFT`, none
SOFT = 14.0
WALL = 0.6  # share a side gives back
KICK = (1.2, 26.0)  # one time in three: this share, this speed at least
GRIP = 28.0  # a second squared lost as it rolls
CREEP = 4.0  # its slowest roll: to a whole cell, where it stops
LEAD = 2  # columns a cat keeps behind a ball that moves
THREAD = 1200  # dots of thread, at most
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
    # A cycle, a frame a column.
    "walk": (
        (("      /\\_/\\", " ~.___(o.o)", "   /|   |\\ "), 0),
        (("      /\\_/\\", " ~.___(o.o)", "   ||   || "), 0),
        (("      /\\_/\\", " -.___(o.o)", "   |\\   /| "), 0),
        (("      /\\_/\\", " -.___(o.o)", "   ||   || "), 0),
    ),
    "leap": ((("      /\\_/\\", " ~-=--(O.O)", "  ''     ``"), 0),),
    # The head down, over what is right of the box.
    "eat": (
        (("           ", " ~._/\\_/\\  ", " (__( o.o )"), 350),
        (("           ", " ~._/\\_/\\  ", " (__( -.- )"), 200),
        (("           ", " -._/\\_/\\  ", " (__( >.< )"), 200),
        (("           ", " ~._/\\_/\\  ", " (__( -.- )"), 200),
    ),
    # The pointer, a fish, a ball: it looks, its tail beats.
    "stare": (
        (("     /\\_/\\ ", "    ( O.O )", "    ~> ^ < "), 180),
        (("     /\\_/\\ ", "    ( O.O )", "    _> ^ < "), 180),
    ),
    "wiggle": (
        (("           ", "  ~_/\\_/\\  ", " (__( O.O )"), 130),
        (("           ", " ~_ /\\_/\\  ", "(___( O.O )"), 130),
    ),
    # A paw at what is right of the box.
    "bat": (
        (("    /\\_/\\  ", "   ( O.O )/", "   ~> ^ <  "), 140),
        (("    /\\_/\\  ", "   ( O.O ) ", "   _> ^ <-,"), 180),
    ),
    # Up on its hind legs, under what is out of reach.
    "beg": (
        (("    \\/\\_/\\/", "    ( O.O )", "     | ^ | "), 250),
        (("     /\\_/\\ ", "    ( O.O )", "    /| ^ |\\"), 250),
    ),
    "puzzled": ((("   ? /\\_/\\ ", "    ( o.O )", "    ~> ^ < "), 400),),
    "purr": (
        (("  ♥  /\\_/\\ ", "    ( ^.^ )", "    ~> ^ < "), 500),
        ((" ♥   /\\_/\\ ", "    ( -.- )", "    _> ^ < "), 500),
    ),
    # A gallop, a frame a column: stretched, the fore paws down, the
    # hind ones under it, gathered, the push, off.
    "run": (
        (("      /\\_/\\", " ~-=__(O.O)", "  //    \\\\ "), 0),
        (("      /\\_/\\", " ~-=__(O.O)", "   //   || "), 0),
        (("      /\\_/\\", " _-=-~(O.O)", "    || //  "), 0),
        (("      /\\_/\\", " _.-~~(O.O)", "    \\\\//   "), 0),
        (("      /\\_/\\", " _-=-~(O.O)", "    ||  \\\\ "), 0),
        (("      /\\_/\\", " ~-=__(O.O)", "   //   \\\\ "), 0),
    ),
    "hiss": (
        (("   _  /\\_/\\", " |/ \\_(>o<)", "  ||   ||  "), 160),
        (("   _  /\\_/\\", " \\/ \\_(>O<)", "  ||   ||  "), 160),
    ),
    # On its back, the thread round its paws.
    "roll": (
        (("           ", "    '' ''  ", " ~(___(^.^)"), 220),
        (("           ", "    `` ``  ", " _(___(^.^)"), 220),
    ),
}

# A plate, by portions left.
PLATE = ("\\.../", "\\.o./", "\\.o./", "\\oOo/", "\\oOo/")
PLATE_INK = {"": 75, "oO": 173, ".": 130}

# Facing right: at rest, then the two steps of its walk.
MOUSE = ('~-(,_,"°>', '~~(,_,"°>', '-~(_,_"°>')
MOUSE_INK = {"": 250, "~-": 217, ">": 211, "°": 231, ",": 245}

# A fish, its head to the left, by portions left; then as it flops.
FISH = (" .,'. ", "<+++><", "<//+><", "<///><")
FLOP = "<\\\\\\><"
FISH_INK = {"": 117, "<>": 75, "+": 252, ".,'": 244}

YARNS = (203, 213, 81, 220, 120, 141)  # colours of a ball

_FACE = re.compile(r"\((\s?)(\S)(\S)(\S)(\s?)\)")
#: The words it reads on the screen (`Frame.shape`): the rest is ink.
WORDS = ("food", "fish", "toy")
_WORD = re.compile(rf"\b(?:{'|'.join(WORDS)})\b", re.IGNORECASE)


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
        "?": 229,
        "♥": 204,
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


def fish_look(left: int, right: bool = True, flop: bool = False) -> dict[str, Any]:
    """A fish with `left` portions: by a cat that faces `right`, its
    head to it; `flop`: alive, on the floor."""

    def cells(row: str) -> list[list[dict[str, Any]]]:
        return paint((row,) if right else mirror((row,)), FISH_INK, bold=True)

    if not flop:
        return {"cells": cells(FISH[left])}
    beat = ((FISH[-1], 700), (FLOP, 180), (FISH[-1], 250), (FLOP, 180))
    return {
        "frames": [{"cells": cells(row), "ms": ms} for row, ms in beat],
        "loop": True,
    }


#: The dots of a ball; those within turn with it; played out, a ring.
_BALL = ((1, 0), (2, 0), (0, 1), (3, 1), (0, 2), (3, 2), (1, 3), (2, 3))
_CORE = ((1, 1), (2, 1), (2, 2), (1, 2))


def ball_look(
    color: int, plays: int = 1, dx: int = 0, dy: int = 0, turn: int = 0
) -> dict[str, Any]:
    """A ball of yarn: braille, 4 dots by 4 less its corners, `dx` and
    `dy` dots into its first cell. One dot within is off, by `turn`:
    it rolls. With no play left, a ring."""
    dots = Braille(BW + (dx > 0), 1 + (dy > 0))
    core = [d for i, d in enumerate(_CORE) if i != turn % 4] if plays else []
    for x, y in (*_BALL, *core):
        dots.set(dx + x, dy + y)
    return {"cells": paint(dots.rows(), {"": color}, bold=True)}


def wave(x: int) -> int:
    """How high over the floor the thread lies at the dot column `x`:
    0 to 2 dots, a wave."""
    return round(1.0 + math.sin(0.9 * x))


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


def mass(row: str) -> list[bool]:
    """The cells of a row that stop a ball: its glyphs, and the blanks
    between two of them, `BW` + 1 at most: a line of words is a floor."""
    out = [not ch.isspace() for ch in row]
    last = None
    for x, ink in enumerate(out):
        if ink:
            if last is not None and x - last - 1 <= BW + 1:
                out[last + 1 : x] = [True] * (x - last - 1)
            last = x
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
    n = max(6, min(24, round(6 + far / 4)))
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


class Fish:
    """A fish: its left column `x`, on the row `y`; frame cells.
    `word`, `until`: as a plate. `left`: portions; `holder`: the cat
    that has it; `steals`: the tries of the others at it."""

    def __init__(
        self, home: Home, x: int, g: int, word: tuple[int, int] | None, until: float
    ) -> None:
        self.home = home
        self.x, self.y = x, g - 1
        self.word = word
        self.until = until
        self.left = len(FISH) - 1
        self.holder: Cat | None = None
        self.steals = 0
        self.right = True
        self.sprite: Sprite | None = None

    @property
    def g(self) -> int:
        """The row under it, when it lies."""
        return self.y + 1

    def show(self, flop: bool = False) -> None:
        look = fish_look(self.left, self.right, flop)
        self.sprite = self.home.put(self.sprite, self.x, self.y, look, z=-1)

    def by(self, cat: Cat, mouth: bool = False) -> None:
        """Next to `cat`, on the side it faces: on the floor, or in its
        `mouth`."""
        self.x = cat.x + W if cat.right else cat.x - FW
        self.y = cat.g - (2 if mouth else 1)
        self.right = cat.right
        self.show()


class Yarn:
    """A ball of yarn, `BW` cells on one row. `x`: its left column;
    `g`: the row under it; frame cells. `word`, `until`: as a plate.
    `plays`: bats left; `player`: the cat that has it.

    A bat (`knock`) sends it along and up: a hop, a jump, or a leap
    over 3 rows of text. It moves in dots, 2 by 4 a cell (`px`, `py`:
    its top left one; `vx`, `vy`: dots a second), `TICK` a step: it
    falls, bounces on the text and on the floor of the view, off its
    sides and off the text (slower; one time in three, faster: back
    past the cat), rolls, slower and slower, and stops on a whole cell.
    No text under it: it falls.

    Its thread: the dots it went through (`points`: column, row, the
    row where it rests), neighbours joined when drawn. A dot in the air
    sinks a row of dots a step, down to the floor of its column, where
    it lies as a wave (`wave`); a ball that rolls lays it there. Thread
    on thread is one thread.
    """

    def __init__(
        self, home: Home, x: int, g: int, word: tuple[int, int] | None, until: float
    ) -> None:
        rng = home.rng
        self.home = home
        self.px, self.py = 2.0 * x, 4.0 * (g - 1)
        self.vx = self.vy = 0.0
        self.way = 1  # where it rolls, or last did
        self.moving = False
        self.rolled = 0.0  # dots, to the right
        self.word = word
        self.until = until
        self.color = rng.choice(YARNS)
        self.plays = rng.randint(*PLAYS)
        self.player: Cat | None = None
        self.points: list[list[int]] = []
        self.tip: tuple[int, int] | None = None  # where the thread ends
        self.loose: list[int] = []  # the points that sink
        self.down: set[tuple[int, int]] = set()  # the dots that lie
        self.stroke = 0  # bats taken
        self.due = math.inf
        self.ball: Sprite | None = None
        self.wire: Sprite | None = None
        self.shown: tuple[Any, ...] | None = None  # the ball as drawn
        self.fresh = False  # the thread changed since it was drawn

    @property
    def x(self) -> int:
        return round(self.px) >> 1

    @property
    def g(self) -> int:
        return (round(self.py) + 3) // 4 + 1

    # -- the ball -----------------------------------------------------

    def hits(self, px: float, py: float) -> bool:
        """Its box at the dot (`px`, `py`) has a solid cell in it."""
        solid = self.home.solid
        return any(
            solid(cx, cy)
            for cy in range(math.floor(py / 4), math.ceil((py + 4) / 4))
            for cx in range(math.floor(px / 2), math.ceil((px + 4) / 2))
        )

    def knock(self, way: int, now: float) -> None:
        """A bat to the right (`way` 1) or to the left (-1): up and
        away."""
        rng = self.home.rng
        self.plays -= 1
        self.stroke += 1
        self.way = way
        self.vx = way * rng.uniform(*BAT)
        self.vy = -rng.uniform(*rng.choice(LIFT))
        self.moving = True
        self.due = min(self.due, now)

    def fly(self, dt: float) -> None:
        """`dt` seconds: along, then down; an axis that would enter a
        solid cell is turned back instead."""
        on = self.vy >= 0 and self.hits(self.px, self.py + 0.5)
        if on:
            self.vy, self.py = 0.0, 4.0 * math.floor((self.py + 0.5) / 4)
            speed = abs(self.vx) - GRIP * dt
            if speed < CREEP:
                at = round(self.px)
                if at % 2 == 0:
                    return self.stop(at)
                speed = CREEP
            self.vx = self.way * speed
        else:
            self.vy = min(self.vy + GRAVITY * dt, FALL)
        nx = self.px + self.vx * dt
        if self.hits(nx, self.py):
            back = WALL * abs(self.vx)
            if abs(self.vx) > 2 * CREEP and self.home.rng.random() < 1 / 3:
                back = max(KICK[0] * abs(self.vx), KICK[1])  # back past the cat
            self.way = -self.way
            self.vx = self.way * back
        else:
            self.rolled += nx - self.px
            self.px = nx
        ny = self.py + self.vy * dt
        if not self.hits(self.px, ny):
            self.py = ny
        elif self.vy > 0:  # a floor: on it
            self.py = 4.0 * math.floor(ny / 4)
            self.vx *= 0.85
            self.vy = -self.vy * BOUNCE if self.vy > SOFT else 0.0
        else:
            self.vy *= -0.4

    def stop(self, at: int) -> None:
        self.px, self.vx, self.vy, self.moving = float(at), 0.0, 0.0, False
        if self.hits(self.px, self.py):  # in the text: a look around
            self.home.stale = True

    # -- the thread ---------------------------------------------------

    def floor(self, x: int, y: int) -> int:
        """The row of dots where a dot of the thread at (`x`, `y`)
        lies once fallen: over the first solid cell under it."""
        solid, cx, cy = self.home.solid, x >> 1, y >> 2
        if solid(cx, cy):
            return y
        while not solid(cx, cy + 1):
            cy += 1
        return 4 * cy + 3 - wave(x)

    def lay(self) -> None:
        """Thread from where it ends to the ball."""
        x = round(self.px) + (1 if self.way > 0 else 2)
        y = round(self.py) + 3
        points, down = self.points, self.down
        on = self.vy == 0.0 and self.hits(self.px, self.py + 0.5)
        ax, ay = self.tip or (x, y)
        # A ball that rolls lays a dot a column.
        n = abs(x - ax) if on else max(abs(x - ax), abs(y - ay))
        n = 1 if self.tip is None else n
        self.tip = (x, y)
        for i in range(1, n + 1):
            qx, qy = round(ax + (x - ax) * i / n), round(ay + (y - ay) * i / n)
            rest = self.floor(qx, qy)
            if on and rest <= qy:  # laid as it lies
                qy = max(rest, qy - 2)
            rest = max(rest, qy)
            if qy < rest:
                self.loose.append(len(points))
            elif (qx, qy) in down:
                continue
            else:
                down.add((qx, qy))
            points.append([qx, qy, rest])
            self.fresh = True
        if len(points) > THREAD:
            del points[: len(points) - THREAD]
            self.slack()

    def slack(self) -> None:
        """What sinks, what lies: counted again."""
        self.loose = [i for i, p in enumerate(self.points) if p[1] < p[2]]
        self.down = {(p[0], p[1]) for p in self.points if p[1] == p[2]}

    def sink(self) -> None:
        """A row of dots down, for the dots in the air; one that lands
        on thread is dropped."""
        if not self.loose:
            return
        points, down, landed = self.points, self.down, False
        for i in self.loose:
            p = points[i]
            p[1] += 1
            if p[1] == p[2]:
                landed = True
                if (p[0], p[1]) in down:
                    p[2] = -1
                else:
                    down.add((p[0], p[1]))
        if landed:
            self.points = [p for p in points if p[2] >= 0]
            self.slack()
        self.fresh = True

    def wake(self, now: float) -> None:
        """The text changed: a ball that was batted falls if nothing
        holds it, and its thread; one never batted stays by its word."""
        for p in self.points:
            p[2] = max(p[1], self.floor(p[0], p[1]))
        self.slack()
        if self.stroke and not self.moving:
            self.moving = not self.hits(self.px, self.py + 0.5)
        if self.moving or self.loose:
            self.due = min(self.due, now)

    def strand(self, keep: int = 1) -> tuple[int, int, list[str]] | None:
        """The thread: the cell of its top left corner, its rows.
        `keep`: one dot in so many. None: nothing of it in the view."""
        home = self.home
        top = 4 * home.view[1]
        points = [p for p in self.points if p[1] >= top]
        if not points:
            return None
        x0 = min(p[0] for p in points) >> 1
        y0 = min(p[1] for p in points) >> 2
        x1 = max(p[0] for p in points) >> 1
        y1 = max(p[1] for p in points) >> 2
        dots = Braille(x1 - x0 + 1, y1 - y0 + 1)
        last = None
        for x, y, _ in points:
            ax, ay = (x, y) if last is None or abs(x - last[0]) > 1 else last
            if abs(y - ay) > 2:
                # Two floors: it hangs straight, in the column of the
                # lower one.
                hx = x if y > ay else ax
                joint = [(hx, qy) for qy in range(min(y, ay), max(y, ay) + 1)]
            else:
                n = max(1, abs(x - ax), abs(y - ay))
                joint = [
                    (round(ax + (x - ax) * i / n), round(ay + (y - ay) * i / n))
                    for i in range(n + 1)
                ]
            for qx, qy in joint:
                if (qx + qy) % keep == 0:
                    dots.set(qx - 2 * x0, qy - 4 * y0)
            last = (x, y)
        for cx, cy in dots.cells():
            if home.solid(x0 + cx, y0 + cy):
                dots.bits[cy][cx] = 0
        return x0, y0, dots.rows()

    def fade(self) -> dict[str, Any] | None:
        """The thread, thinner and thinner, once: the driver removes it
        after."""
        looks = [self.strand(keep) for keep in (1, 2, 4)]
        frames = [
            {"cells": paint(look[2], {"": self.color}, dim=True), "ms": 300}
            for look in looks
            if look is not None
        ]
        return {"frames": frames, "loop": False} if frames else None

    # -- what is due --------------------------------------------------

    def show(self) -> None:
        home = self.home
        ix, iy = round(self.px), round(self.py)
        ball = (ix, iy, round(self.rolled / 2) % 4, self.plays > 0)
        wire = self.strand() if self.fresh else None
        self.fresh = False
        if wire is None and ball == self.shown:
            return
        with home.together():
            if wire is not None:
                x0, y0, rows = wire
                look = {"cells": paint(rows, {"": self.color})}
                self.wire = home.put(self.wire, x0, y0, look, z=-3)
            if ball != self.shown:
                look = ball_look(self.color, self.plays, ix & 1, iy & 3, ball[2])
                self.ball = home.put(self.ball, ix >> 1, iy >> 2, look, z=-2)
                self.shown = ball

    def step(self, now: float) -> None:
        """The steps due, three at most, and one picture."""
        steps = 1 + max(0, min(2, int((now - self.due) / TICK)))
        for _ in range(steps):
            if self.moving:
                # A dot at most at a time: nothing is gone through.
                n = max(1, math.ceil(max(abs(self.vx), abs(self.vy)) * TICK))
                for _ in range(n):
                    if self.moving:
                        self.fly(TICK / n)
                        self.lay()
            self.sink()
        self.show()
        if not self.moving and not self.loose:
            self.due = math.inf
        else:
            self.due += steps * TICK
            if self.due < now:
                self.due = now + TICK


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
        self.stride = 0  # and the columns done
        self.path: list[tuple[int, int]] = []  # the jump: cells left
        self.beat = HOP  # and the seconds each takes
        self.landing: tuple[int, int] | None = None
        self.meal: Plate | None = None  # the plate it eats at, or goes to
        self.fish: Fish | None = None  # the fish it has
        self.toy: Yarn | None = None  # the ball it plays with
        #: What the jump is at: `mouse`, `fish`, `steal`, `pointer`.
        self.prey: str | None = None
        self.tries = 0  # pounces at the mouse there is
        self.fun = 0.0  # its interest in the pointer, 0 to 1, at `mused`
        self.mused = 0.0
        self.ignored: tuple[int, int] | None = None  # the pointer it turned from
        self.spooked = False  # hissed at: it goes

    # -- what the cat knows -------------------------------------------

    def claims(self) -> list[tuple[int, int]]:
        """The places it holds: where it is, where it goes."""
        if self.act == "away":
            return []
        out = [(self.x, self.g)]
        if self.act == "leap" and self.landing is not None:
            out.append(self.landing)
        elif self.act in ("walk", "run"):
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
        home = self.home
        with home.together():
            self.sprite = home.put(self.sprite, self.x, self.g - H, content, z=self.z)
            if self.fish is not None and self.act == "leap":
                self.fish.by(self, mouth=True)

    def rest(self, pose: str, now: float, seconds: float | None) -> None:
        """A pose held `seconds`; None: until something happens."""
        self.act, self.due = pose, math.inf if seconds is None else now + seconds
        self.show(pose)

    def walk(self, goal: int, now: float) -> None:
        self.act, self.goal, self.due, self.stride = "walk", goal, now, 0
        self.right = goal >= self.x

    def run(self, goal: int, now: float) -> None:
        """A walk in a hurry, along its ledge."""
        self.act, self.goal, self.due, self.stride = "run", goal, now, 0
        self.right = goal >= self.x

    def pace(self) -> float:
        """Seconds to its next column: a run starts and ends slower."""
        if self.act == "walk":
            return STEP
        i = min(self.stride - 1, abs(self.goal - self.x))
        return EASE[i] if i < len(EASE) else RUN

    def after(self, now: float, seconds: float) -> None:
        """Its next step, `seconds` after this one was due: a late step
        does not make the next late."""
        late = now - self.due
        self.due = now + seconds - (late if 0 <= late < seconds / 2 else 0.0)

    def jump(self, to: tuple[int, int], now: float, start: tuple[int, int]) -> None:
        """Leap from `start` (left column, row under the paws) to `to`."""
        self.act, self.due, self.landing = "leap", now, to
        self.right = to[0] >= start[0]
        self.path, self.beat = arc(start, to), HOP

    def quit(self) -> None:
        """Leave the plate and the ball, forget the mouse. A fish it has
        stays with it."""
        if self.meal is not None:
            seats = self.meal.seats
            for side in [k for k, cat in seats.items() if cat is self]:
                del seats[side]
        if self.toy is not None and self.toy.player is self:
            self.toy.player = None
        self.meal, self.toy, self.prey = None, None, None

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
        self.beat = RISE

    def leave(self, now: float) -> None:
        """Out of the view: wait, then come back in it. Its fish is
        lost."""
        self.quit()
        if self.fish is not None:
            self.home.lose(self.fish, now)
        self.spooked = False
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
        self.prey = "mouse"
        if (mouse := self.home.mouse) is not None:
            mouse.scare(to[0], now)

    def land(self, now: float) -> None:
        """The jump is over."""
        home = self.home
        self.x, self.g = self.landing or (self.x, self.g)
        home.stale = True  # a look around, soon
        prey, self.prey = self.prey, None
        if self.fish is not None:
            self.settle(now)
        elif self.meal is not None:
            self.right = self.meal.x > self.x
            self.rest("eat", now, BITE)
        elif self.toy is not None:
            self.reach(now)
        elif prey == "mouse" and home.caught(self):
            home.shoo(now)
            self.rest("eat", now, 2.0)
        elif prey == "fish":
            self.grab(now)
        elif prey == "steal":
            self.snatch(now)
        elif prey == "pointer" and self.holds(home.pointer):
            self.swat(now)
        elif prey == "pointer" and self.under(home.pointer):
            self.rest("stare", now, 0.7)
        elif prey == "pointer":
            self.rest("puzzled", now, 0.8)
        else:
            self.rest("alert", now, 0.9)

    # -- the pointer --------------------------------------------------

    def amuse(self, now: float) -> None:
        """Its interest fades."""
        self.fun *= math.exp(-max(0.0, now - self.mused) / FUN)
        self.mused = now

    def holds(self, p: tuple[int, int] | None) -> bool:
        """The pointer at `p` is in its box, or next to it."""
        return (
            p is not None
            and self.x - 1 <= p[0] <= self.x + W
            and self.g - H <= p[1] < self.g
        )

    def under(self, p: tuple[int, int] | None) -> int:
        """The rows from its box up to the pointer at `p` over it, when
        it can leap that high; else 0."""
        if p is None or not self.x <= p[0] < self.x + W:
            return 0
        rise = self.g - H - p[1]
        return rise if 0 < rise <= REACH else 0

    def notice(self, was: tuple[int, int] | None, now: float) -> None:
        """The pointer moved from `was`, or went."""
        p = self.home.pointer
        self.amuse(now)
        if p is None:
            if self.toy is None and self.act in ("stare", "wiggle", "beg"):
                self.rest("puzzled", now, 0.8)
            return
        if was is None:
            self.fun = max(self.fun, 0.7)
        else:
            far = abs(p[0] - was[0]) + 2 * abs(p[1] - was[1])
            self.fun = min(1.0, self.fun + 0.1 + 0.03 * far)
        self.ignored = None
        right = p[0] >= self.x + W // 2
        if (
            self.act in ("stare", "wiggle")
            and self.toy is None
            and self.fish is None
            and right != self.right
        ):
            self.right = right  # it keeps its eyes on it
            self.show(self.act)
        if self.act in ("sleep", "yawn"):
            dx, dy = POKE
            close = abs(p[0] - self.x - W // 2) <= dx and abs(p[1] - self.g + 2) <= dy
            if close:  # poked awake
                self.rest("alert", now, 0.5)
        elif self.act in ("sit", "groom", "alert", "stare", "puzzled", "purr"):
            self.due = min(self.due, now + NOTICE)

    def swat(self, now: float) -> None:
        """It has the pointer: a few bats at it."""
        p = self.home.pointer
        if p is not None:
            self.right = p[0] >= self.x + W // 2
        self.rest("bat", now, 0.32 * self.rng.randint(2, 4))

    def stalk(self, now: float) -> bool:
        """What it does of the pointer. False: nothing, it has no
        interest in it."""
        home, rng = self.home, self.rng
        p = home.pointer
        self.amuse(now)
        if p is None or p == self.ignored:
            return False
        if self.fun < BORED:
            # A pointer that rests: under its paws, it purrs; else it
            # turns away. Both until the pointer moves.
            self.ignored = p
            if self.holds(p):
                self.rest("purr", now, rng.uniform(3.0, 5.0))
            elif self.act in ("stare", "bat", "puzzled"):
                self.right = p[0] < self.x + W // 2
                self.rest("groom", now, rng.uniform(2.5, 5.0))
            else:
                return False
            return True
        home.fresh(now)
        self.right = p[0] >= self.x + W // 2
        to = home.below(self, p)
        here = perch(home.spots, self.x, self.g)
        if self.holds(p):
            self.swat(now)
        elif self.under(p):
            self.rest("beg", now, rng.uniform(0.6, 1.2))
        elif to is None:  # out of reach: it watches
            self.rest("stare", now, 0.8)
        elif (
            here is not None
            and to[1] == self.g
            and here.has(to[0])
            and abs(to[0] - self.x) > POUNCE
        ):
            gap = rng.randint(6, 10)
            self.run(to[0] + (gap if to[0] < self.x else -gap), now)
        else:
            wait = (0.4 + 0.45 * rng.random()) * (1.25 - 0.5 * self.fun)
            self.rest("wiggle", now, wait)
        return True

    def pounce(self, now: float) -> None:
        """After the wiggle: at the pointer, where it is now."""
        home = self.home
        p = home.pointer
        to = home.below(self, p) if p is not None else None
        if to is None:
            self.rest("stare" if p is not None else "puzzled", now, 0.8)
        elif to == (self.x, self.g):
            self.rest("stare", now, 0.2)
        else:
            self.jump(to, now, (self.x, self.g))
            self.prey = "pointer"

    def hop(self, now: float) -> None:
        """After it begged: straight up at the pointer, and down."""
        rise = self.under(self.home.pointer)
        if not rise:
            return self.rest("stare", now, 0.5)
        ups = [(self.x, self.g - i) for i in range(1, rise + 1)]
        self.act, self.due, self.landing = "leap", now, (self.x, self.g)
        self.path = [*ups, ups[-1], *reversed(ups[:-1]), (self.x, self.g)]
        self.beat = RISE
        self.prey = "pointer"

    # -- the fish -----------------------------------------------------

    def beside(self, fish: Fish) -> bool:
        """It stands at an end of `fish`, which lies."""
        return fish.y == self.g - 1 and self.x in (fish.x - W, fish.x + FW)

    def grab(self, now: float) -> None:
        """Landed by the fish that lay there: a paw on it."""
        fish = self.home.fish
        if fish is None or fish.holder is not None or not fish.left:
            return self.rest("puzzled", now, 0.8)  # another has it
        if not self.beside(fish):
            return self.rest("alert", now, 0.9)
        self.right = fish.x > self.x
        fish.holder, self.fish, fish.until = self, fish, math.inf
        fish.right = self.right
        fish.show()
        self.rest("bat", now, 0.5)

    def snatch(self, now: float) -> None:
        """Landed by the fish another eats: it has it, one time in two;
        else it is hissed at, and goes."""
        fish = self.home.fish
        owner = fish.holder if fish is not None else None
        if (
            fish is None
            or owner is None
            or owner is self
            or owner.act != "eat"
            or not fish.left
            or not self.beside(fish)
        ):
            return self.rest("puzzled", now, 0.8)
        self.right = fish.x > self.x
        if self.rng.random() < 0.5:
            owner.fish = None
            owner.rest("hiss", now, 1.2)
            fish.holder, self.fish = self, fish
            self.rest("bat", now, 0.35)
        else:
            owner.rest("hiss", now, 1.0)
            self.spooked = True
            self.rest("alert", now, 0.5)

    def carry(self, now: float) -> None:
        """Away with its fish, to eat it; nowhere to go: here."""
        self.home.fresh(now)
        if (to := self.pick(far=True)) is None:
            return self.settle(now)
        self.jump((to[0], to[1].g), now, (self.x, self.g))

    def settle(self, now: float) -> None:
        """Its fish down before it, on the side with room, and a bite."""
        home, fish = self.home, self.fish
        assert fish is not None
        y = self.g - 1
        if not home.free(self.x + W, y, FW, 1) and home.free(self.x - FW, y, FW, 1):
            self.right = False
        elif home.free(self.x + W, y, FW, 1):
            self.right = True
        fish.by(self)
        self.rest("eat", now, BITE)

    # -- the ball of yarn ---------------------------------------------

    def play(self, yarn: Yarn, x: int, now: float, keen: bool = False) -> None:
        """To the column `x`, next to `yarn`: a run along its ledge
        (one time in two; `keen`: always), or a pounce."""
        yarn.player, self.toy = self, yarn
        here = perch(self.home.spots, self.x, self.g)
        if (x, yarn.g) == (self.x, self.g):
            self.reach(now)
        elif (
            here is not None
            and here.g == yarn.g
            and here.has(x)
            and (keen or self.rng.random() < 0.5)
        ):
            self.run(x, now)
        else:
            self.jump((x, yarn.g), now, (self.x, self.g))

    def reach(self, now: float) -> None:
        """By its ball: a bat at it; with no play left, on its back in
        the thread. The ball on its way: after it. Not by it: it gives
        up."""
        yarn = self.toy
        assert yarn is not None
        if yarn.moving:
            return self.watch(now)
        if self.g != yarn.g or self.x not in (yarn.x - W, yarn.x + BW):
            self.quit()
            return self.rest("puzzled", now, 0.8)
        self.right = yarn.x > self.x
        if yarn.plays > 0:
            self.rest("bat", now, 0.45)
        else:
            self.rest("roll", now, 1.8)

    def knock(self, now: float) -> None:
        """The bat lands: the ball is off; a look, and it is after
        it."""
        yarn = self.toy
        assert yarn is not None
        yarn.knock(1 if self.right else -1, now)
        self.rest("wiggle", now, self.rng.uniform(0.2, 0.45))

    def trail(self) -> int | None:
        """The column of its ledge it runs to, after a ball that moves:
        `LEAD` columns behind it, as far as the ledge goes. None: no
        ledge under it, or the ball is not ahead."""
        yarn = self.toy
        assert yarn is not None
        here = perch(self.home.spots, self.x, self.g)
        if here is None:
            return None
        ahead = 2 * yarn.x + BW >= 2 * self.x + W
        goal = yarn.x - W - LEAD if ahead else yarn.x + BW + LEAD
        goal = max(here.x0, min(goal, here.x1))
        return goal if goal != self.x and (goal > self.x) == ahead else None

    def follow(self) -> None:
        """As it runs after the ball: where the ball is now."""
        goal = self.trail()
        self.goal = self.x if goal is None else goal
        if self.goal != self.x:
            self.right = self.goal > self.x

    def watch(self, now: float) -> None:
        """Its eyes on a ball it cannot follow: at the end of its
        ledge, or under it."""
        yarn = self.toy
        assert yarn is not None
        right = 2 * yarn.x + BW >= 2 * self.x + W
        if self.act == "wiggle" and right == self.right:
            self.due = now + 0.15  # the same pose: nothing to send
        else:
            self.right = right
            self.rest("wiggle", now, 0.15)

    def chase(self, now: float) -> None:
        """After the ball. On its way: a run behind it along the ledge,
        then a watch. Stopped: to it, by a run or a pounce; no room by
        it: the play is over."""
        yarn = self.toy
        assert yarn is not None
        if yarn.moving:
            if (goal := self.trail()) is not None:
                self.run(goal, now)
            else:
                self.watch(now)
        elif (x := self.home.plaything(self)) is not None:
            self.play(yarn, x, now, keen=True)
        else:
            yarn.plays = 0
            self.quit()
            self.rest("puzzled", now, 0.8)

    # -- what is due --------------------------------------------------

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
                self.after(now, self.beat)
            else:
                self.land(now)
        elif self.act in ("walk", "run"):
            if self.act == "run" and self.toy is not None and self.toy.moving:
                self.follow()
            ahead = self.x + (1 if self.goal > self.x else -1)
            if self.x == self.goal:
                return self.halt(now)
            if home.taken(self, ahead, self.g):
                self.goal = self.x
                return self.halt(now, barred=True)
            self.x = ahead
            self.stride += 1
            self.show(self.act, self.stride - 1)
            self.after(now, self.pace())
        elif self.act == "eat" and self.meal is not None and self.meal.left:
            home.bite(self.meal, now)
            self.due = now + BITE
        elif self.act == "eat" and self.fish is not None:
            if home.nibble(self.fish, now):
                self.due = now + BITE
            else:
                self.fish = None
                self.rest("groom", now, rng.uniform(2.5, 5.0))
        elif self.act == "eat":  # the plate is empty, the mouse eaten
            self.quit()
            self.rest("groom", now, rng.uniform(2.5, 5.0))
        elif self.act == "bat" and self.toy is not None:
            self.knock(now)
        elif self.act == "bat" and self.fish is not None:
            self.carry(now)
        elif self.act == "bat":  # at the pointer: it looks at it
            self.rest("stare", now, rng.uniform(0.9, 2.5))
        elif self.act == "hiss" and self.fish is not None:
            self.rest("eat", now, BITE)
        elif self.act == "wiggle" and self.toy is not None:
            self.chase(now)
        elif self.act == "wiggle":
            self.pounce(now)
        elif self.act == "beg":
            self.hop(now)
        elif self.act == "roll" and self.toy is not None:
            self.toy.until = min(self.toy.until, now + UNDONE)
            self.quit()
            self.rest("groom", now, rng.uniform(2.5, 5.0))
        elif self.spooked:
            self.spooked = False
            self.flee(now)
            if self.act != "leap":
                self.rest("sit", now, rng.uniform(2.0, 5.0))
        elif self.stalk(now):
            pass
        elif (aim := home.scent(self)) is not None:
            self.hunt(aim, now)
        elif (aim := home.bait(self)) is not None:
            self.jump(aim, now, (self.x, self.g))
            self.prey = "fish"
        elif (aim := home.loot(self)) is not None:
            self.jump(aim, now, (self.x, self.g))
            self.prey = "steal"
        elif (seat := home.seat(self)) is not None:
            self.dine(*seat, now)
        elif (x := home.plaything(self)) is not None and home.yarn is not None:
            self.play(home.yarn, x, now)
        elif self.act == "yawn":
            self.rest("sleep", now, None)
        elif home.idle:
            self.rest("yawn", now, 1.4)
        else:
            self.wander(now)

    def halt(self, now: float, barred: bool = False) -> None:
        """The walk is over, or the run: after the ball, or within a
        pounce of the pointer. `barred`: by another cat."""
        if self.toy is not None and barred:
            self.quit()
            self.rest("puzzled", now, 0.8)
        elif self.toy is not None:
            self.chase(now)
        elif self.act == "run":
            self.rest("wiggle", now, self.rng.uniform(0.3, 0.7))
        else:
            self.rest("sit", now, self.rng.uniform(2.0, 6.0))

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
    cursor, whether the pane is idle; the plates, the mouse, the fish
    and the ball of yarn; the pointer.

    `count` cats, a coat each; `varied`: coats and skins drawn. `food`,
    `mice`, `fish`, `toys`: plates, mice, fishes and balls now and
    then, from `now` on. `hunt`: the host mouse is told (`mat`)."""

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
        fish: bool = False,
        toys: bool = False,
        hunt: bool = False,
        now: float = 0.0,
    ) -> None:
        self.pilot = pilot
        self.frame = frame
        self.rng = rng
        self.layer = "front" if front else "back"
        self.idle = False
        self.cursor: tuple[int, int] | None = None
        self.rows: list[str] = []  # the shape of the view, when read
        self.walls: list[list[bool]] = []  # `mass` of each row
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
        self.fish: Fish | None = None
        self.yarn: Yarn | None = None
        self.next_fish = now + rng.uniform(5.0, 12.0) if fish else math.inf
        self.next_toy = now + rng.uniform(6.0, 14.0) if toys else math.inf
        self.hunt = hunt
        self.pointer: tuple[int, int] | None = None  # frame cells
        #: A sprite of nothing over the view: the host mouse is told on
        #: sprites only. With its size.
        self.mat: tuple[Sprite, int, int] | None = None
        self.joint = False  # in `together`

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
            rows = pad([*self.frame.shape(WORDS), *[""] * height][:height], width)
            self.rows, self.view = rows, box
            self.walls = [mass(row) for row in rows]
            self.spots = ledges(rows, box)
            self.read, self.stale = now, False
            if self.hunt:
                self.carpet(width, height)
            self.lay(now)
            return True
        return False

    def fresh(self, now: float) -> None:
        """Read the view if it changed, or was read long ago."""
        if self.stale or now - self.read > 2.0:
            self.survey(now)

    def carpet(self, width: int, height: int) -> None:
        """The mat, as large as the view."""
        if self.mat is not None:
            if self.mat[1:] == (width, height):
                return
            self.mat[0].remove()
        mat = self.pilot.draw(
            frame=self.frame.id,
            at=(0, 0),
            space="view",
            layer=self.layer,
            z=-9,
            cells=[[{"t": " " * width}]] * height,
        )
        self.mat = (mat, width, height)

    @contextlib.contextmanager
    def together(self) -> Iterator[None]:
        """What is `put` inside: one request, one repaint."""
        if self.joint:
            yield
            return
        self.joint = True
        try:
            with self.pilot.batch():
                yield
        finally:
            self.joint = False

    def free(self, x: int, y: int, w: int, h: int) -> bool:
        """The box at (`x`, `y`) is in the view as read, with no text."""
        x0, y0, x1, y1 = self.view
        if x < x0 or y < y0 or x + w > x1 or y + h > y1:
            return False
        rows = self.rows[y - y0 : y - y0 + h]
        return all(row[x - x0 : x - x0 + w].isspace() for row in rows)

    def solid(self, x: int, y: int) -> bool:
        """The cell stops a ball: text, a side of the view as read, or
        under it. Over it: nothing does."""
        x0, y0, x1, y1 = self.view
        if not x0 <= x < x1 or y >= y1:
            return True
        return y >= y0 and self.walls[y - y0][x - x0]

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
        with self.together():
            sprite.update(anchor={"at": [x, y], "space": "frame"})
            sprite.set(**content)
        return sprite

    def call(self, now: float, asleep: float = 1.0) -> None:
        """Something new to see: the cats at rest look soon; of those
        asleep, a share."""
        for cat in self.cats:
            if cat.act in ("sit", "groom", "alert", "stare", "puzzled", "purr") or (
                cat.act in ("sleep", "yawn") and self.rng.random() < asleep
            ):
                cat.due = min(cat.due, now + self.rng.uniform(0.2, 1.2))

    # -- the plates ---------------------------------------------------

    def things(self) -> list[tuple[int, int, int]]:
        """What lies on a floor, a row high: its left column, the row
        under it, its width. Plates, a fish, a ball."""
        out = [(p.x, p.g, PW) for p in self.plates]
        fish, yarn = self.fish, self.yarn
        if fish is not None and (fish.holder is None or fish.holder.act != "leap"):
            out.append((fish.x, fish.g, FW))
        if yarn is not None:
            out.append((yarn.x, yarn.g, BW))
        return out

    def chairs(self, x: int, g: int, w: int = PW) -> list[tuple[bool, int]]:
        """The sides of a thing of `w` cells at `x` above `g` with room
        for a cat: whether it faces right there, and its column."""
        sides = ((True, x - W), (False, x + w))
        return [(r, cx) for r, cx in sides if self.free(cx, g - H, W, H)]

    def room(self, x: int, g: int, w: int = PW) -> bool:
        """A thing of `w` cells fits at `x` above `g`: no text, no
        other thing."""
        return self.free(x, g - 1, w, 1) and not any(
            tg == g and x < tx + tw + 1 and tx < x + w + 1
            for tx, tg, tw in self.things()
        )

    def laid(self, x: int, g: int) -> bool:
        """A thing is in the box of a cat at `x` above `g`."""
        return any(
            abs(g - tg) < H and x < tx + tw and tx < x + W
            for tx, tg, tw in self.things()
        )

    def beside(self, x: int, y: int, n: int, w: int = PW) -> tuple[int, int] | None:
        """Where the thing of `w` cells of the word at (`x`, `y`), `n`
        cells, goes: right of it, left of it, or on it; first where a
        cat can sit."""
        x0, _, x1, _ = self.view
        over = max(x0, min(x + (n - w) // 2, x1 - w))
        sites = [
            site
            for site in ((x + n + GAP, y + 1), (x - w - GAP, y + 1), (over, y))
            if self.room(*site, w)
        ]
        seated = (site for site in sites if self.chairs(*site, w))
        return next(seated, sites[0] if sites else None)

    def site(self, w: int) -> tuple[int, int] | None:
        """A place on a ledge for a thing of `w` cells: off the cats
        and the cursor, with room for a cat next to it."""
        rng = self.rng
        for s in rng.sample(self.spots, len(self.spots)):
            x = rng.randint(s.x0, s.x1 + W - w)
            busy = any(
                og == s.g and ox < x + w + 1 and x < ox + W + 1
                for cat in self.cats
                for ox, og in cat.claims()
            )
            if busy or near(x, s.g, self.cursor) or not self.room(x, s.g, w):
                continue
            if self.chairs(x, s.g, w):
                return (x, s.g)
        return None

    def dish(
        self, at: tuple[int, int], word: tuple[int, int] | None, until: float
    ) -> Plate:
        plate = Plate(*at, word, until)
        self.plates.append(plate)
        plate.sprite = self.put(None, plate.x, plate.g - PH, plate_look(plate.left), -1)
        return plate

    def lay(self, now: float) -> None:
        """After a read: a plate for each `food` in the view, a fish
        for a `fish`, a ball for a `toy`, one at a time; those whose
        word went, or with text over them, go. The mouse goes with its
        ledge."""
        x0, y0 = self.view[:2]
        words = {
            (x0 + m.start(), y0 + y): m[0].lower()
            for y, row in enumerate(self.rows)
            for m in _WORD.finditer(row)
        }

        def lost(word: tuple[int, int] | None, x: int, g: int, w: int) -> bool:
            return (word is not None and word not in words) or not self.free(
                x, g - 1, w, 1
            )

        for plate in [*self.plates]:
            if lost(plate.word, plate.x, plate.g, PW):
                self.clear(plate, now)
        fish, yarn = self.fish, self.yarn
        lying = fish is not None and fish.holder is None and fish.left
        if fish is not None and lying and lost(fish.word, fish.x, fish.g, FW):
            self.lose(fish, now)
        if yarn is not None:
            gone = yarn.word is not None and yarn.word not in words
            if gone or (not yarn.moving and lost(None, yarn.x, yarn.g, BW)):
                self.wind(now)
            else:
                yarn.wake(now)
        self.eaten &= words.keys()
        laid = {plate.word for plate in self.plates} | self.eaten
        laid |= {thing.word for thing in (self.fish, self.yarn) if thing is not None}
        for word, kind in words.items():
            if word in laid:
                continue
            w, full = PW, len(self.plates) >= PLATES
            if kind == "fish":
                w, full = FW, self.fish is not None
            elif kind == "toy":
                w, full = BW, self.yarn is not None
            if full or (at := self.beside(*word, len(kind), w)) is None:
                continue
            if kind == "food":
                self.dish(at, word, math.inf)
                self.call(now)
            elif kind == "fish":
                self.drop(at, word, math.inf)
            else:
                self.spool(at, word, math.inf)
        mouse = self.mouse
        if mouse is not None and not any(
            s.g == mouse.g and s.x0 <= mouse.lo and mouse.hi <= s.x1 + W - MW
            for s in self.spots
        ):
            self.shoo(now)

    def serve(self, now: float) -> None:
        """A plate of `--food`, on a ledge with room for a cat."""
        self.next_food = now + 5.0  # nowhere to put it: soon again
        if len(self.plates) >= PLATES or (self.stale and not self.survey(now)):
            return
        if (at := self.site(PW)) is not None:
            self.dish(at, None, now + self.rng.uniform(*SERVED))
            self.next_food = math.inf  # after this one
            self.call(now)

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

    # -- the fish -----------------------------------------------------

    def drop(
        self, at: tuple[int, int], word: tuple[int, int] | None, until: float
    ) -> None:
        """A fish at `at`, alive."""
        self.fish = Fish(self, *at, word, until)
        self.fish.show(flop=True)
        self.call(now=self.read, asleep=0.5)

    def toss(self, now: float) -> None:
        """A fish of `--fish`, on a ledge with room for a cat."""
        self.next_fish = now + 5.0  # nowhere to put it: soon again
        if self.fish is not None or (self.stale and not self.survey(now)):
            return
        if (at := self.site(FW)) is not None:
            self.drop(at, None, now + self.rng.uniform(*LYING))
            self.next_fish = math.inf  # after this one

    def lose(self, fish: Fish, now: float) -> None:
        """The fish goes: eaten, lost, or never taken."""
        if fish.sprite is not None:
            fish.sprite.remove()
        if fish.holder is not None:
            fish.holder.fish = None
        if fish is self.fish:
            self.fish = None
            if fish.word is None:
                self.next_fish = now + self.rng.uniform(*FISHES)

    def bait(self, cat: Cat) -> tuple[int, int] | None:
        """Where `cat` lands to take the fish that lies: its nearest
        free end."""
        fish = self.fish
        if fish is None or fish.holder is not None or not fish.left:
            return None
        ends = [
            (math.hypot(cx - cat.x, 2 * (fish.g - cat.g)), cx)
            for _, cx in self.chairs(fish.x, fish.g, FW)
            if not near(cx, fish.g, self.cursor) and not self.taken(cat, cx, fish.g)
        ]
        return (min(ends)[1], fish.g) if ends else None

    def loot(self, cat: Cat) -> tuple[int, int] | None:
        """Where `cat` lands to steal the fish another eats: its other
        end. One time in two, `STEALS` tries a fish."""
        fish = self.fish
        owner = fish.holder if fish is not None else None
        if (
            fish is None
            or owner is None
            or owner is cat
            or owner.act != "eat"
            or not fish.left
            or fish.steals >= STEALS
            or self.rng.random() < 0.5
        ):
            return None
        x = fish.x + FW if owner.right else fish.x - W
        free = self.free(x, fish.g - H, W, H) and not self.taken(cat, x, fish.g)
        if not free or near(x, fish.g, self.cursor):
            return None
        fish.steals += 1
        return (x, fish.g)

    def nibble(self, fish: Fish, now: float) -> bool:
        """A portion less. False at the last: what is left goes soon,
        and its word gets no other."""
        fish.left -= 1
        fish.show()
        if fish.left:
            return True
        fish.holder, fish.until = None, now + EMPTY
        if fish.word is not None:
            self.eaten.add(fish.word)
        return False

    # -- the ball of yarn ---------------------------------------------

    def spool(
        self, at: tuple[int, int], word: tuple[int, int] | None, until: float
    ) -> None:
        """A ball of yarn at `at`."""
        self.yarn = Yarn(self, *at, word, until)
        self.yarn.show()
        self.call(now=self.read, asleep=0.5)

    def throw(self, now: float) -> None:
        """A ball of `--toys`, on a ledge with room for a cat."""
        self.next_toy = now + 5.0  # nowhere to put it: soon again
        if self.yarn is not None or (self.stale and not self.survey(now)):
            return
        if (at := self.site(BW)) is not None:
            self.spool(at, None, now + self.rng.uniform(*LYING))
            self.next_toy = math.inf  # after this one

    def plaything(self, cat: Cat) -> int | None:
        """The column where `cat` stands to play with the ball: next to
        it, on its side first. None: no ball for it, or no room."""
        yarn = self.yarn
        if yarn is None or yarn.moving or yarn.player not in (None, cat):
            return None
        if yarn.player is None and not yarn.plays:
            return None
        sides = [yarn.x - W, yarn.x + BW]
        if 2 * cat.x + W > 2 * yarn.x:
            sides.reverse()
        for x in sides:
            free = self.free(x, yarn.g - H, W, H) and not self.taken(cat, x, yarn.g)
            if free and not near(x, yarn.g, self.cursor):
                return x
        return None

    def wind(self, now: float) -> None:
        """The ball goes, its thread after it; played out, its word
        gets no other."""
        yarn = self.yarn
        if yarn is None:
            return
        self.yarn = None
        if (cat := yarn.player) is not None:
            cat.quit()
            if cat.act in ("bat", "wiggle", "roll"):
                cat.rest("alert", now, 0.6)
        if yarn.ball is not None:
            yarn.ball.remove()
        if yarn.wire is not None and (look := yarn.fade()) is None:
            yarn.wire.remove()
        elif yarn.wire is not None:
            try:
                yarn.wire.set(**look)
            except PtyrcError as exc:  # the driver took it, with its rows
                if exc.code not in ("no-sprite", "gone"):
                    raise
        if yarn.word is None:
            self.next_toy = now + self.rng.uniform(*TOYS)
        elif not yarn.plays:
            self.eaten.add(yarn.word)

    # -- the pointer --------------------------------------------------

    def below(self, cat: Cat, p: tuple[int, int]) -> tuple[int, int] | None:
        """Where `cat` stands to have the pointer at `p` in its box, or
        over it within `REACH` rows: the place under it first, then the
        nearest."""
        px, py = p
        best = None
        for s in self.spots:
            x = max(s.x0, min(px - W // 2, s.x1))
            if not (s.g - H - REACH <= py < s.g and x <= px < x + W):
                continue
            if near(x, s.g, self.cursor) or self.laid(x, s.g):
                continue
            if self.taken(cat, x, s.g):
                continue
            rise = max(0, s.g - H - py)
            cost = (rise, math.hypot(x - cat.x, 2 * (s.g - cat.g)), x, s.g)
            best = cost if best is None else min(best, cost)
        return best[2:] if best is not None else None

    def point(self, data: dict[str, Any], now: float) -> None:
        """A move of the host mouse: where it is in the frame, None
        when it is out of it."""
        at = None
        if data.get("sprite") is not None and data.get("frame") == self.frame.id:
            at = tuple(data["pos"])
        was, self.pointer = self.pointer, at
        if at != was:
            for cat in self.cats:
                cat.notice(was, now)

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
        elif topic == "hover":
            self.point(data, now)
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
            held = cat.meal is not None or cat.fish is not None or cat.toy is not None
            off = not held and perch(self.spots, cat.x, cat.g) is None
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
        fish, yarn = self.fish, self.yarn
        if fish is not None and now >= fish.until:
            self.lose(fish, now)
        elif fish is None and now >= self.next_fish:
            self.toss(now)
        if yarn is not None and now >= yarn.due:
            yarn.step(now)
        if yarn is not None and not yarn.plays and yarn.player is None:
            yarn.until = min(yarn.until, now + UNDONE)  # played out, left
        if yarn is not None and now >= yarn.until:
            if yarn.plays and yarn.player is not None:
                yarn.until = now + 2 * BITE  # not under its paws
            else:
                self.wind(now)
        elif yarn is None and now >= self.next_toy:
            self.throw(now)

    def due(self, now: float) -> float:
        """When `step` has something to do."""
        times = [cat.due for cat in self.cats]
        times += [plate.until for plate in self.plates]
        times += [self.next_food, now + READ if self.stale else math.inf]
        times.append(self.next_mouse if self.mouse is None else self.mouse.due)
        fish, yarn = self.fish, self.yarn
        times.append(self.next_fish if fish is None else fish.until)
        times.append(self.next_toy if yarn is None else min(yarn.due, yarn.until))
        return min(times)

    def move(self, frame: Frame, now: float) -> None:
        """To `frame`, which took the place of this one: all leave, the
        cats come back."""
        if frame.id != self.frame.id:
            for cat in self.cats:
                if cat.sprite is not None:
                    cat.sprite.remove()
                    cat.sprite = None
        if self.mat is not None:
            self.mat[0].remove()
            self.mat = None
        self.frame = frame
        self.pointer = None
        for plate in [*self.plates]:
            self.clear(plate, now)
        if self.mouse is not None:
            self.shoo(now)
        if self.fish is not None:
            self.lose(self.fish, now)
        self.wind(now)
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
        fish=args.fish,
        toys=args.toys,
        hunt=args.hunt,
        now=start,
    )
    # With a snapshot the first events tell what already is: whether
    # the frame is idle, where the cursor is. Nothing is asked.
    pilot.subscribe("idle", "cursor", "screen", "frames", snapshot=True)
    if args.hunt:
        # The driver asks the host for the moves of its mouse.
        pilot.subscribe("hover")
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
    out.add_argument("--fish", action="store_true", help="a fish now and then")
    out.add_argument("--toys", action="store_true", help="a ball of yarn now and then")
    out.add_argument("--hunt", action="store_true", help="the cats hunt the host mouse")
    out.add_argument("-a", "--all", action="store_true", help="all of these, two cats")
    args = _cli.parse(out, argv)
    if args.all:
        args.random = args.food = args.mice = True
        args.fish = args.toys = args.hunt = True
    if args.count is None:
        args.count = 2 if args.all else 1
    if not 1 <= args.count <= MAX_CATS:
        out.error(f"--count must be between 1 and {MAX_CATS}")
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
