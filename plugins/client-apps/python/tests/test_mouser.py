# SPDX-License-Identifier: GPL-3.0-or-later
"""`_mouser`: the cat alone, then the cat and the pointer."""

from __future__ import annotations

import itertools

import pytest
from ptyrc_apps import _mouser
from ptyrc_apps._mouser import DT, SHAPES, Cat, Pose, Sill, cells, idle
from ptyrc_apps._strip import Pointer

#: A bay of 40 by 12: two mullions, the cat in the middle pane.
SILL = Sill(5, 40, 24, 20, 1, (13, 26), 3, 36, 7, 32)
SMALL = Sill(5, 18, 12, 5, -1, (9,), 3, 14, 4, 13)
REST = ("hold", "watch", "beg", "bored", "seek", "settle", "alert", "miss")


class Play:
    """A pointer, a cat, a clock: what the cat does, 12 times a second."""

    def __init__(self, sill: Sill = SILL, light: float = 1.0) -> None:
        self.sill, self.light, self.t = sill, light, 0.0
        self.pointer = Pointer(lambda: self.t)
        self.cat = Cat()
        self.seen: list[tuple[float, str, Pose]] = []

    def run(self, seconds: float) -> list[tuple[float, str, Pose]]:
        start = len(self.seen)
        for _ in range(round(seconds * 12)):
            self.t += 1 / 12
            pose = self.cat.at(self.t, self.pointer, self.sill, self.light)
            self.seen.append((self.t, self.cat.mode, pose))
        return self.seen[start:]

    def move(self, x: int, y: int, seconds: float = 0.0) -> list:
        self.pointer.move(x, y)
        return self.run(seconds)


def test_shapes() -> None:
    for name, shape in SHAPES.items():
        assert len({len(row) for row in shape.rows}) == 1, name
        assert set("".join(shape.rows)) <= {"#", "."}, name
        assert "#" in shape.rows[-1] and "#" in shape.rows[0], name
        assert 0 <= shape.anchor < len(shape.rows[0]), name
    # Mirrored about its middle; the feet on the sill; a tail on the sill only.
    right = cells(Pose(SHAPES["walk"], 10.0, 0.0, 1), 24)
    left = cells(Pose(SHAPES["walk"], 10.0, 0.0, -1), 24)
    assert {(20 - x, y) for x, y in right} == set(left)
    assert max(y for _, y in right) == 23
    sat = cells(Pose(_mouser.SIT, 10.0, 0.0, 1, "rest", -1), 24)
    assert min(x for x, _ in sat) == 5 and len(sat) == 18 + 3
    assert len(cells(Pose(_mouser.SIT, 10.0, 3.0, 1, "rest", -1), 24)) == 18


@pytest.mark.parametrize("sill", [SILL, SMALL], ids=["bay", "small"])
def test_alone(sill: Sill) -> None:
    """Alone the cat stays home, on the sill, on no mullion, and does
    more than sit; it sleeps at night."""
    day = [idle(sill, n / 4, 1.0) for n in range(4 * 1200)]
    night = [idle(sill, n / 4, 0.0) for n in range(4 * 1200)]
    for pose in (*day, *night):
        assert pose.x == sill.home and pose.z == 0
        assert not {x for x, _ in cells(pose, sill.rows)} & set(sill.cols)
    assert day == [idle(sill, n / 4, 1.0) for n in range(4 * 1200)]
    names = {name for name, s in SHAPES.items() if any(p.shape is s for p in day)}
    assert {"sit", "look", "lick", "alert", "twitch"} <= names
    assert len({p.tail for p in day}) >= 3  # the tail sways

    def asleep(poses: list[Pose]) -> int:
        return sum(len(p.shape.rows) <= 3 for p in poses)

    assert asleep(night) > 0.6 * len(night)
    assert 0 < asleep(day) < 0.4 * len(day)
    # What flies by: it looks that way.
    assert idle(sill, 3.0, 1.0, sill.home + 9.0).face == 1
    assert idle(sill, 3.0, 1.0, sill.home - 9.0).face == -1


def test_no_pointer() -> None:
    cat = Cat()
    assert cat.at(3.0, None, SILL) == idle(SILL, 3.0, 1.0)
    assert cat.at(3.0, Pointer(lambda: 0.0), SILL) == idle(SILL, 3.0, 1.0)
    assert not cat.busy


def test_hunt() -> None:
    """It notices, runs to the pointer, crouches, pounces and lands on
    it, bats it; the pointer gone, it looks for it and walks home."""
    play = Play()
    play.run(1.0)
    seen = play.move(33, 9, 1.4)
    for x in (34, 33, 34, 33, 34, 33, 34):  # it stirs
        seen += play.move(x, 9, 0.5)
    modes = [mode for mode, _ in itertools.groupby(m for _, m, _ in seen)]
    assert modes[:2] == ["alert", "run"]
    for a, b in itertools.pairwise(["alert", "run", "pounce", "land", "hold"]):
        assert modes.index(a) < modes.index(b), modes
    xs = [pose.x for _, mode, pose in seen if mode in ("alert", "run")]
    assert xs[0] == SILL.home and xs[-1] > SILL.home + 5
    assert all(a <= b for a, b in itertools.pairwise(xs))  # no step back
    flight = [pose for _, mode, pose in seen if mode == "pounce"]
    assert max(p.z for p in flight) >= 1.5 and not flight[0].shape.back
    held = [pose for _, mode, pose in seen if mode == "hold"]
    assert all(abs(p.x - 33.5) <= 3.5 for p in held)
    assert {SHAPES["raise"], SHAPES["swat"]} <= {p.shape for p in held}
    assert play.cat.busy
    play.pointer.leave()
    after = play.run(12.0)
    modes = [mode for mode, _ in itertools.groupby(m for _, m, _ in after)]
    assert modes[-4:] == ["seek", "home", "settle", "idle"], modes
    walk = [pose.x for _, mode, pose in after if mode == "home"]
    assert all(a >= b for a, b in itertools.pairwise(walk))
    assert after[-1][2] == idle(SILL, play.t, 1.0) and not play.cat.busy


@pytest.mark.parametrize("sill", [SILL, SMALL], ids=["bay", "small"])
def test_bounds(sill: Sill) -> None:
    """Whatever the pointer does: the cat within its sill, its feet on
    it or over it; sat, it is on no mullion."""
    play = Play(sill)
    for n in range(260):
        k = n * 2654435761 % 2**32
        if k % 11 == 0:
            play.pointer.leave()
            play.run(0.3 + k % 5)
            continue
        play.move(k % sill.w, (k >> 8) % (sill.rows // 2), 0.1 + (k >> 16) % 9 / 6)
    assert len({mode for _, mode, _ in play.seen}) >= 10
    for t, mode, pose in play.seen:
        assert sill.lo <= pose.x <= sill.hi, (t, mode)
        assert 0 <= pose.z <= sill.high + 0.5, (t, mode)
        top = min(y for _, y in cells(pose, sill.rows))
        assert top >= 0, (t, mode)
        if pose.shape.back and not pose.z and mode in REST:
            assert sill.clear(pose.x), (t, mode, pose.x)
        if mode == "seek" and pose.shape.back:
            assert sill.left <= round(pose.x) <= sill.right, (t, pose.x)


def test_out_of_reach() -> None:
    """Under a pointer too high: it stands up, and leaps."""
    play = Play()
    seen = play.move(28, 0, 1.0)
    for x in (29, 28, 29, 28, 29, 28):
        seen += play.move(x, 0, 1.0)
    begs = [pose for _, mode, pose in seen if mode == "beg"]
    assert begs and all(p.shape is SHAPES["beg"] for p in begs)
    assert all(abs(p.x - 28.5) <= 2.5 for p in begs)
    leaps = [pose for _, mode, pose in seen if mode == "pounce"]
    assert leaps and max(p.z for p in leaps) > SILL.high - 1
    assert all(p.shape is SHAPES["leap"] for p in leaps)


def test_bored() -> None:
    """A pointer that does not move: the cat turns away, grooms, goes
    home; it comes back when the pointer stirs."""
    play = Play()
    seen = play.move(30, 9, 60.0)
    modes = [mode for mode, _ in itertools.groupby(m for _, m, _ in seen)]
    assert modes[-4:] == ["bored", "home", "settle", "idle"], modes
    assert play.pointer.at is not None and not play.cat.busy
    assert play.move(31, 9, 0.5)[-1][1] != "idle"


def test_asleep() -> None:
    """Asleep, it takes longer to notice."""
    night = next(
        t for t in range(0, 4000, 8) if _mouser._act(5, t // 8, 0.0) == "sleep"
    )
    play = Play(light=0.0)
    play.t = night + 2.0
    seen = play.move(30, 9, 1.2)
    modes = [mode for mode, _ in itertools.groupby(m for _, m, _ in seen)]
    assert modes[:2] == ["wake", "alert"]


def test_replay() -> None:
    """A cat just made says what a cat stepped all along says: it is a
    function of the time and of the trail."""
    play = Play()
    play.run(0.5)
    for n, x in enumerate((30, 31, 8, 9, 22, 22, 35)):
        play.move(x, 9 - n % 3, 0.4)
        for t in (play.t, play.t - 0.25):
            fresh = Cat().at(t, play.pointer, SILL)
            again = Cat()
            again.at(play.t, play.pointer, SILL)  # back in time: from the start
            assert fresh == again.at(t, play.pointer, SILL)
        assert play.seen[-1][2] == Cat().at(play.t, play.pointer, SILL)
    assert play.cat.now <= play.t < play.cat.now + DT + 1e-9
