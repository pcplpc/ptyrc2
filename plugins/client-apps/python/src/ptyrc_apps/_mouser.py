# SPDX-License-Identifier: GPL-3.0-or-later
"""Mouser: the cat of a window sill, and the pointer it hunts.

A cat is pixels on a sill: a `Pose` is a `Shape`, the column of its
middle, its height over the sill, the way it faces, its tail.

- `idle`: the cat alone, a function of the time: it sits, looks
  aside, grooms, stretches, lies down, sleeps at night and in naps;
  its tail sways; it watches what flies by (`prey`);
- `Cat`: the cat and the pointer, stepped `DT` by `DT`: it wakes,
  notices, runs, crouches and wiggles, pounces in an arc, lands, bats
  what it caught, looks around for what it missed, stands up and leaps
  under what is out of reach, loses interest, grooms, walks home.

`Cat.at(t, pointer, ...)` steps to `t`. A time before the last one,
or another sill: the cat starts again from the trail of the pointer;
a `Cat` just made is a function of `t` and of the trail.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

from ._strip import Pointer, noise

__all__ = [
    "DT",
    "SHAPES",
    "TAILS",
    "Cat",
    "Pose",
    "Shape",
    "Sill",
    "cells",
    "idle",
]

DT = 1 / 30  # seconds a step
G = 110.0  # pixels a second a second, down
ACC = 70.0  # of a run
SLOT = 8.0  # seconds an act of the cat alone
NOTICE = 0.3  # seconds it stares before it goes
REACH = 5.5  # pixels over its feet it can catch at
#: What it does sat: on no mullion.
SAT = ("alert", "miss", "hold", "watch", "beg", "seek")


@dataclass(frozen=True)
class Shape:
    """Pixels of a cat, `#`, the feet on the last row. `anchor`: the
    column of its middle. `back`: seen from behind, the tail apart, on
    the sill; else from the side, facing right."""

    rows: tuple[str, ...]
    anchor: int = 2
    back: bool = True


def _side(*rows: str) -> Shape:
    return Shape(rows, 3, False)


SIT = Shape((".#.#.", ".###.", ".###.", "#####", "#####"))
SHAPES: dict[str, Shape] = {
    "sit": SIT,
    "twitch": Shape(("...#.", ".###.", ".###.", "#####", "#####")),
    "look": Shape(("..#.#", "..###", ".###.", "#####", "#####")),
    "alert": Shape((".#.#.", ".###.", ".###.", ".###.", "#####", "#####")),
    "stare": Shape(("..#.#", "..###", ".###.", ".###.", "#####", "#####")),
    "lick": Shape(("..#.#", ".####", "#####", "#####")),
    "raise": Shape((".#.#.#", ".###.#", ".####.", "#####.", "#####.")),
    "swat": Shape((".#.#..", ".###..", ".#####", "#####.", "#####.")),
    "beg": Shape(
        (".....#", ".#.#.#", ".####.", ".###..", ".###..", ".###..", ".###..")
    ),
    "leap": Shape((".....#", ".#.#.#", ".####.", ".###..", ".###..", ".#.#..")),
    "squat": Shape((".#.#.", "#####", "#####")),
    "loaf": Shape(("..#.#..", ".#####.", "#######"), 3),
    "curl": Shape((".#####.", "#######"), 3),
    "breath": Shape(("..###..", ".#####.", "#######"), 3),
    "ball": Shape((".###.", "#####")),
    "swell": Shape((".###.", "#####", "#####")),
    "walk": _side("#....#.", "#...###", ".######", ".#####.", ".#...#."),
    "step": _side("#....#.", "#...###", ".######", ".#####.", "..#.#.."),
    "bound": _side(".....#.", "#...###", ".######", ".#####.", "#.....#"),
    "gather": _side("#....#.", ".#..###", ".######", ".#####.", "..#.#.."),
    "crouch": _side(".....#.", ".######", "#######"),
    "wiggle": _side(".#...#.", ".######", "#######"),
    "rise": _side(".....#.", "....###", "..####.", ".###...", "#......"),
    "fly": _side("#....#.", ".######", ".#####.", "......#"),
    "fall": _side("#......", ".###.#.", "..#####", "...####", ".....##"),
    "bow": _side("#......", ".##....", ".####.#", "..#####"),
}
#: The tail of a cat seen from behind, on the sill: columns from its
#: middle, pixels up.
TAILS: dict[str, tuple[tuple[int, int], ...]] = {
    "rest": ((3, 0), (4, 0), (5, 1)),
    "flat": ((3, 0), (4, 0), (5, 0)),
    "flick": ((3, 0), (4, 1)),
    "none": (),
}


@dataclass(frozen=True)
class Pose:
    """A cat: its shape, the column of its middle, its feet over the
    sill, the way it faces (1 right), its tail and the side it is on."""

    shape: Shape
    x: float
    z: float = 0.0
    face: int = 1
    tail: str = "none"
    way: int = 1


def cells(pose: Pose, rows: int) -> list[tuple[int, int]]:
    """The pixels of `pose` on a sill at the bottom of `rows` rows."""
    cx, base = round(pose.x), rows - 1 - round(pose.z)
    shape = pose.shape
    out = [
        (cx + pose.face * (i - shape.anchor), base - j)
        for j, line in enumerate(reversed(shape.rows))
        for i, char in enumerate(line)
        if char == "#"
    ]
    if shape.back and round(pose.z) == 0:
        out += [(cx + pose.way * k, rows - 1 - up) for k, up in TAILS[pose.tail]]
    return out


@dataclass(frozen=True)
class Sill:
    """Where a cat lives: `w` by `rows` pixels, mullions at `cols`;
    `home`, the column of its middle at rest, its tail to `way`; `lo`
    to `hi`, the columns its middle can take; `left` to `right`, those
    where it shows when the pointer is gone."""

    seed: int
    w: int
    rows: int
    home: int
    way: int
    cols: tuple[int, ...]
    lo: int
    hi: int
    left: int
    right: int

    def clear(self, x: float, half: int = 2) -> bool:
        """A cat at `x`, `half` pixels to each side, is on no mullion."""
        cx = round(x)
        return not any(cx - half <= c <= cx + half for c in self.cols)

    @property
    def wide(self) -> bool:
        """Room at home to lie down."""
        return self.clear(self.home, 3) and 3 <= self.home < self.w - 3

    @property
    def high(self) -> float:
        """The most its feet leave the sill."""
        return float(max(2, min(8, self.rows - 8)))

    @property
    def range(self) -> float:
        """The longest pounce."""
        return max(3.0, min(7.0, (self.hi - self.lo) / 3))

    @property
    def run(self) -> float:
        """Pixels a second, at most."""
        return 7.0 + 0.2 * self.w

    def stop(self, x: float, shown: bool = False) -> int:
        """The column nearest `x` to sit at: within reach, on no
        mullion; `shown`: where it shows when the pointer is gone."""
        lo, hi = (self.left, self.right) if shown else (self.lo, self.hi)
        cx = round(min(hi, max(lo, x)))
        for d in range(self.w):
            for c in (cx - d, cx + d):
                if lo <= c <= hi and self.clear(c):
                    return c
        return cx

    def tail(self, x: float, prefer: int) -> int:
        """The side of the tail of a cat sat at `x`: `prefer`, if it
        fits there."""
        cx = round(x)
        for way in (prefer, -prefer):
            cols = [cx + way * k for k in (3, 4, 5)]
            if all(0 <= c < self.w and c not in self.cols for c in cols):
                return way
        return prefer


# --- the cat alone ---------------------------------------------------


def _act(seed: int, k: int, light: float) -> str:
    """What the cat alone does in the slot `k`."""
    drowsy = noise(seed + 121, k // 6) < (0.85 if light < 0.25 else 0.18)
    r = noise(seed + 122, k)
    if drowsy:
        return "sleep" if r < 0.8 else "loaf"
    for name, share in (
        ("sit", 0.4),
        ("look", 0.16),
        ("groom", 0.14),
        ("up", 0.1),
        ("stretch", 0.08),
    ):
        r -= share
        if r < 0:
            return name
    return "loaf"


def _sway(t: float, lash: bool = False) -> str:
    """The tail of a cat that sits: it sways, flicks now and then;
    `lash`: it beats."""
    if lash:
        return ("flick", "flat")[int(t * 5) % 2]
    if int(t) % 13 < 2:
        return ("rest", "flick")[int(t * 3) % 2]
    return "flat" if int(t / 5) % 3 == 1 else "rest"


def idle(sill: Sill, t: float, light: float, prey: float | None = None) -> Pose:
    """The cat alone at `t`, at home. `light`: of the day, `0..1`;
    `prey`: the column of what flies outside."""
    seed, x, way = sill.seed, float(sill.home), sill.way
    k = int(t // SLOT)
    u = t - k * SLOT
    act, last = _act(seed, k, light), _act(seed, k - 1, light) if k else "sit"
    down = ("sleep", "loaf")
    side = 1 if noise(seed + 123, k) < 0.5 else -1
    loaf = SHAPES["loaf" if sill.wide else "squat"]

    def sat(name: str, face: int = 1, tail: str | None = None) -> Pose:
        return Pose(SHAPES[name], x, 0.0, face, tail or _sway(t), way)

    if act == "sleep":
        if last not in down and u < 0.75:
            return Pose(loaf, x)
        breath = int(t / 1.75) % 2
        names = ("curl", "breath") if sill.wide else ("ball", "swell")
        return Pose(SHAPES[names[breath]], x)
    if act == "loaf":
        if prey is not None:
            return Pose(loaf, x)
        ear = sill.wide and 3.0 <= u < 3.25
        return Pose(SHAPES["curl"] if ear else loaf, x)
    if last in down and u < 0.5:  # it gets up
        return Pose(loaf, x)
    if prey is not None:
        near = abs(prey - x) < 3
        face = 1 if prey > x else -1
        return sat("alert" if near else "stare", face, _sway(t, True))
    if act == "look" and 1.5 <= u < 5.5:
        return sat("look", side)
    if act == "groom" and 1.0 <= u < 6.5:
        return sat(("lick", "look")[int(t * 2.5) % 2], side, "flat")
    if act == "up" and 2.0 <= u < 4.5:
        return sat("alert", 1, _sway(t, True))
    if act == "stretch" and 2.0 <= u < 3.75 and sill.wide:
        return Pose(SHAPES["bow"], x, 0.0, side)
    if act == "sit" and 5.0 <= u < 5.25:
        return sat("twitch", side)
    return sat("sit")


# --- the cat and the pointer -----------------------------------------


def _sign(v: float) -> int:
    return 1 if v > 0 else -1 if v < 0 else 0


class Cat:
    """A cat that hunts the pointer over its sill."""

    def __init__(self) -> None:
        self.sill: Sill | None = None
        self.pointer: Pointer | None = None
        self.mode = "idle"
        self.light = 1.0
        self.t0, self.n = math.inf, 0

    @property
    def busy(self) -> bool:
        """It does more than sit at home."""
        return self.mode != "idle"

    @property
    def now(self) -> float:
        return self.t0 + self.n * DT

    def at(
        self,
        t: float,
        pointer: Pointer | None,
        sill: Sill,
        light: float = 1.0,
        prey: float | None = None,
    ) -> Pose:
        """The cat at `t`."""
        self.light = light
        if pointer is None or (pointer.at is None and pointer.since == -math.inf):
            self.mode, self.sill, self.t0 = "idle", None, math.inf
            return idle(sill, t, light, prey)
        if sill != self.sill or pointer is not self.pointer or t < self.now - DT:
            self._start(sill, pointer, t)
        while self.now + DT <= t + 1e-9:
            gone = pointer.at is None and self.now >= pointer.since
            if self.mode == "idle" and gone:
                self.n += int((t - self.now) / DT)
                break
            self._step()
        return self._pose(t, prey)

    def _start(self, sill: Sill, pointer: Pointer, t: float) -> None:
        self.sill, self.pointer = sill, pointer
        start = pointer.since
        if pointer.trail:
            start = min(start, pointer.trail[0][2])
        start = max(start, t - 40.0)
        self.t0, self.n = math.floor(start / DT) * DT, 0
        self.mode, self.since, self.wait, self.k = "idle", self.t0, 0.0, 0
        self.x, self.z, self.vx, self.vz = float(sill.home), 0.0, 0.0, 0.0
        self.face, self.paw, self.dist = 1, 1, 0.0
        self.fun = 0.0
        self.cell: tuple[int, int] | None = None  # of the pointer, as last seen
        self.ignored: tuple[int, int] | None = None
        self.spot: tuple[float, float] | None = None  # where it saw it last
        self.lost = -math.inf
        self.caught = self.steep = self.dash = self.begging = False
        self.swats, self.gap, self.top = 0, 3.0, 1.0
        self.shuffle = False

    # What it sees.

    def _see(self, now: float) -> tuple[int, int] | None:
        """The cell of the pointer at `now`."""
        p = self.pointer
        assert p is not None
        if p.at is None:
            if now >= p.since:
                return None
        elif now < p.since:
            return None
        seen = None
        for x, y, at in p.trail:
            if at > now:
                break
            seen = (x, y)
        if seen is None:
            seen = p.trail[0][:2] if p.trail else p.at
        return seen

    def _noise(self, salt: int) -> float:
        assert self.sill is not None
        return noise(self.sill.seed + 130 + salt, self.k)

    def _go(self, mode: str, wait: float = 0.0) -> None:
        self.mode, self.since, self.wait = mode, self.now, wait
        self.k += 1
        self.shuffle = False
        if mode in SAT:
            self._aside(self.cell is None)

    def _step(self) -> None:
        sill = self.sill
        assert sill is not None
        now = self.now
        seen = self._see(now)
        if seen is not None:
            if seen != self.cell:
                if self.cell is None:
                    self.fun = max(self.fun, 0.7)
                else:
                    far = abs(seen[0] - self.cell[0]) + 2 * abs(seen[1] - self.cell[1])
                    self.fun = min(1.0, self.fun + 0.1 + 0.03 * far)
                self.cell = seen
            self.spot = (float(seen[0]), sill.rows - 1.5 - 2 * seen[1])
        elif self.cell is not None:
            self.cell, self.ignored, self.lost = None, None, now
        self.fun *= math.exp(-DT / 8.0)
        fresh = seen is not None and seen != self.ignored
        getattr(self, "_" + self.mode)(now - self.since, seen is not None, fresh)
        self.n += 1

    # How it moves.

    def _move(self, goal: float, top: float) -> bool:
        """A step of a run to `goal`, `top` pixels a second at most.
        True: it is there."""
        sill = self.sill
        assert sill is not None
        dx = goal - self.x
        want = math.copysign(min(top, math.sqrt(2 * ACC * abs(dx))), dx)
        if abs(dx) < 0.1:
            want = 0.0
        dv = want - self.vx
        lim = ACC * DT * (2.0 if dv * self.vx < 0 else 1.0)
        self.vx += min(lim, max(-lim, dv))
        x = min(float(sill.hi), max(float(sill.lo), self.x + self.vx * DT))
        self.dist += abs(x - self.x)
        self.x = x
        if abs(self.vx) > 0.5:
            self.face = _sign(self.vx)
        if abs(goal - self.x) < 0.35 and abs(self.vx) < 3.0:
            self.vx = 0.0
            return True
        return False

    def _aside(self, shown: bool = False) -> bool:
        """Off a mullion before it sits; `shown`: and from behind a
        curtain. True: on its way."""
        sill = self.sill
        assert sill is not None
        goal = sill.stop(self.x, shown)
        if abs(goal - self.x) < 0.5 and not self.shuffle:
            return False
        self.shuffle = not self._move(float(goal), 5.0)
        return self.shuffle

    def _aim(self) -> tuple[float, float, bool]:
        """How far the pointer is to the right, how high, and whether
        a leap gets there."""
        sill = self.sill
        assert sill is not None and self.spot is not None
        px, pz = self.spot
        px = min(float(sill.hi), max(float(sill.lo), px))
        return px - self.x, pz, pz <= sill.high + REACH

    def _under(self) -> float:
        """How far to the right it stands up under the pointer."""
        sill = self.sill
        assert sill is not None and self.spot is not None
        return sill.stop(self.spot[0]) - self.x

    def _think(self, here: bool) -> None:
        sill = self.sill
        assert sill is not None
        if self.spot is None or (not here and self.now - self.lost > 1.0):
            self._go("seek")
            return
        d, _, reach = self._aim()
        if here and self.fun < 0.15:
            self._go("watch", 1.0)
        elif not reach:
            self._go("run" if abs(self._under()) > 1.5 else "beg", 1.0 + self._noise(1))
        elif abs(d) > sill.range:
            self.gap = 2.5 + self._noise(2) * max(0.0, sill.range - 3.5)
            self.dash = self._noise(3) < 0.35 + 0.5 * self.fun
            self._go("run")
        else:
            self._crouch()

    def _crouch(self) -> None:
        d = self._aim()[0]
        if abs(d) > 0.75:
            self.face = _sign(d)
        self.vx = 0.0
        self._go("wiggle", (0.4 + 0.45 * self._noise(4)) * (1.25 - 0.5 * self.fun))

    def _launch(self, up: bool = False) -> None:
        sill = self.sill
        assert sill is not None and self.spot is not None
        px, pz = self.spot
        land = min(float(sill.hi), max(float(sill.lo), px))
        d = min(sill.range + 1.0, max(-sill.range - 1.0, land - self.x))
        high = sill.high if up else min(sill.high, max(1.5, pz - 2.5))
        self.vz = self.top = math.sqrt(2 * G * high)
        self.vx = 0.0 if up else d * G / (2 * self.vz)
        self.steep = up or abs(d) < 1.5
        if not self.steep:
            self.face = _sign(d)
        self.paw = _sign(px - self.x) or self.paw
        self.z, self.caught = 0.01, False
        self.fun = max(0.0, self.fun - 0.05)
        self._go("pounce")

    # What it does: `u` seconds in the mode; the pointer is `here`;
    # `fresh`: here, and not one it turned away from.

    def _idle(self, u: float, here: bool, fresh: bool) -> None:
        sill = self.sill
        assert sill is not None
        if not fresh:
            return
        act = _act(sill.seed, int(self.now // SLOT), self.light)
        self._go("wake" if act in ("sleep", "loaf") else "alert", NOTICE)

    def _wake(self, u: float, here: bool, fresh: bool) -> None:
        if u >= 0.7:
            self._go("alert", NOTICE)

    def _alert(self, u: float, here: bool, fresh: bool) -> None:
        if self._aside():
            return
        if self.spot is not None:
            self.paw = _sign(self.spot[0] - self.x) or self.paw
        if u >= self.wait:
            self._think(here)

    def _run(self, u: float, here: bool, fresh: bool) -> None:
        sill = self.sill
        assert sill is not None
        if not here and self.now - self.lost > 0.6:
            self._go("seek")
            return
        d, _, reach = self._aim()
        goal = self.x + self._under()
        if reach:
            goal = self.x + d - _sign(d) * self.gap if abs(d) > self.gap else self.x
        there = self._move(goal, sill.run)
        if not reach:
            if there:
                self._go("beg", 1.0 + self._noise(1))
        elif abs(d) <= sill.range and self.dash and abs(self.vx) > 0.5 * sill.run:
            self._launch()
        elif there:
            self._crouch()

    def _wiggle(self, u: float, here: bool, fresh: bool) -> None:
        sill = self.sill
        assert sill is not None
        d, _, reach = self._aim()
        if here and (abs(d) > sill.range + 1.5 or not reach) and u > 0.15:
            self._think(here)
        elif u >= self.wait:
            self._launch()

    def _pounce(self, u: float, here: bool, fresh: bool) -> None:
        sill = self.sill
        assert sill is not None
        self.vz -= G * DT
        self.z += self.vz * DT
        self.x = min(float(sill.hi), max(float(sill.lo), self.x + self.vx * DT))
        if here:
            d, pz, _ = self._aim()
            if abs(d) <= 2.5 and self.z - 1.0 <= pz <= self.z + 6.5:
                self.caught = True
        if self.z <= 0.0:
            self.z, self.vx, self.vz = 0.0, 0.0, 0.0
            self._go("land")

    def _land(self, u: float, here: bool, fresh: bool) -> None:
        if u < 0.16:
            return
        if not here:  # no second leap at what is gone
            self.lost = -math.inf
            self._go("miss")
            return
        d, _, reach = self._aim()
        if not reach:
            self._go("run" if abs(self._under()) > 1.5 else "beg", 1.0 + self._noise(1))
        elif self.caught and abs(d) <= 3.0:
            self.fun = min(1.0, self.fun + 0.1)
            self.swats = 2 + int(self._noise(5) * 3)
            self._go("hold")
        else:
            self._go("miss")

    def _miss(self, u: float, here: bool, fresh: bool) -> None:
        if self._aside(not here):
            return
        sill = self.sill
        assert sill is not None
        far = here and abs(self._aim()[0]) > sill.range
        if u >= (0.25 if far else 0.55):
            self._think(here)

    def _hold(self, u: float, here: bool, fresh: bool) -> None:
        if not here:
            if self.now - self.lost > 0.5:
                self._go("seek")
            return
        d = self._aim()[0]
        if abs(d) > 3.5:
            self._go("alert", 0.12)
            return
        if self._aside():
            return
        self.paw = _sign(d) or self.paw
        if u >= 0.32 * self.swats:
            self._go("watch", 0.9 + 1.6 * self._noise(6))

    def _watch(self, u: float, here: bool, fresh: bool) -> None:
        sill = self.sill
        assert sill is not None
        if not here:
            if self.now - self.lost > 0.5:
                self._go("seek")
            return
        if self._aside():
            return
        d, pz, reach = self._aim()
        self.paw = _sign(d) or self.paw
        if abs(d) > sill.range and self.fun >= 0.15:
            self._go("alert", 0.12)
        elif u < self.wait:
            return
        elif self.fun < 0.15:
            self.ignored = self.cell
            self._go("bored")
        elif not reach:
            self._go("beg", 1.0 + self._noise(1))
        elif abs(d) <= 3.0 and pz <= 6.0:
            self.swats = 1 + int(self._noise(5) * 2)
            self._go("hold")
        else:
            self._crouch()

    def _beg(self, u: float, here: bool, fresh: bool) -> None:
        if not here:
            if self.now - self.lost > 0.5:
                self._go("seek")
            return
        if self._aim()[2] or abs(self._under()) > 1.5:
            self._go("alert", 0.12)
        elif self._aside():
            return
        elif self.fun < 0.15:
            self.ignored = self.cell
            self._go("bored")
        elif u >= self.wait:
            self._launch(up=True)

    def _bored(self, u: float, here: bool, fresh: bool) -> None:
        if fresh:
            self._go("alert", 0.2)
        elif u >= 2.6:
            self._go("home")

    def _seek(self, u: float, here: bool, fresh: bool) -> None:
        if fresh:
            self._go("alert", 0.15)
        elif self._aside(True):
            return
        elif u >= 1.3 + 1.2 + 2.0 * self._noise(7):
            self._go("home")

    def _home(self, u: float, here: bool, fresh: bool) -> None:
        sill = self.sill
        assert sill is not None
        if fresh:
            self.vx = 0.0
            self._go("alert", 0.2)
        elif self._move(float(sill.home), 4.5):
            self.x = float(sill.home)
            self._go("settle")

    def _settle(self, u: float, here: bool, fresh: bool) -> None:
        if fresh:
            self._go("alert", NOTICE)
        elif u >= 0.4:
            self._go("idle")

    # What it looks like.

    def _pose(self, t: float, prey: float | None) -> Pose:
        sill = self.sill
        assert sill is not None
        mode, u = self.mode, t - self.since
        if mode == "idle":
            return idle(sill, t, self.light, prey)
        x, face = self.x, self.face
        toward = self.paw or 1
        if self.spot is not None:
            d, pz = self.spot[0] - x, self.spot[1]
        else:
            d, pz = 0.0, 0.0

        def side(name: str, z: float = 0.0, way: int | None = None) -> Pose:
            return Pose(SHAPES[name], x, z, way or face)

        def sat(name: str, way: int, tail: str = "rest") -> Pose:
            return Pose(SHAPES[name], x, 0.0, way, tail, sill.tail(x, -toward))

        def gait(names: tuple[str, str], stride: float, hop: int = 0) -> Pose:
            i = int(self.dist / stride) % 2
            return side(names[i], float(hop * (1 - i)))

        if self.shuffle or mode == "home":
            return gait(("walk", "step"), 1.5)
        if mode == "wake":
            wide = sill.wide and sill.home == round(x)
            return (
                Pose(SHAPES["loaf" if wide else "squat"], x)
                if u < 0.35
                else Pose(SIT, x, 0.0, 1, "flick", sill.way)
            )
        if mode == "alert":
            return sat("alert" if abs(d) < 2 else "stare", toward, "flick")
        if mode == "run":
            if abs(self.vx) < 4.0:
                return gait(("walk", "step"), 1.5)
            return gait(("bound", "gather"), 2.5, 1)
        if mode == "wiggle":
            return side(("crouch", "wiggle")[int(t * 7) % 2])
        if mode == "pounce":
            if self.steep:
                return Pose(SHAPES["leap"], x, self.z, toward)
            k = self.vz / self.top
            return side("rise" if k > 0.4 else "fall" if k < -0.4 else "fly", self.z)
        if mode == "land":
            return Pose(SHAPES["squat"], x) if self.steep else side("crouch")
        if mode == "miss":
            return sat("stare", toward if int(u / 0.18) % 2 else -toward, "flick")
        if mode == "hold":
            beat = (u % 0.32) < 0.14
            return sat("raise" if beat or pz > 6.0 else "swat", toward, "flick")
        if mode == "watch":
            name = "alert" if abs(d) < 2 else "look" if pz < 8 else "stare"
            return sat(name, toward, _sway(t, self.fun > 0.3))
        if mode == "beg":
            return sat("beg", 1 if int(u / 0.3) % 2 else -1, "flat")
        if mode == "bored":
            return sat(("lick", "look")[int(t * 2.5) % 2], -toward, "flat")
        if mode == "seek":
            if u < 1.3:
                way = -toward if 0.5 <= u < 0.9 else toward
                return sat("stare", way, "flick")
            return sat("look" if int(u / 1.1) % 2 else "sit", toward, _sway(t, True))
        # settle
        return Pose(SHAPES["look"], x, 0.0, face, "rest", sill.way)
