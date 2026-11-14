# SPDX-License-Identifier: GPL-3.0-or-later
"""`pond`: the drops, the shadows of the words, the scene, the app on a
headless driver."""

from __future__ import annotations

import fcntl
import itertools
import math
import os
import pty
import random
import struct
import subprocess
import termios
import threading
import time

import pytest
from ptyrc import Pilot, PtyrcError
from ptyrc import clock as session_clock
from ptyrc_apps import pond, stars
from ptyrc_apps.pond import Koi, Pond, Ripple


def braille(glyph: str) -> int:
    """The dots of a braille glyph, as bits; 0 for another."""
    return ord(glyph) - 0x2800 if 0x2800 < ord(glyph) < 0x2900 else 0


def inked(grid: stars.Grid, inks: tuple[int, ...]) -> list[tuple[int, int]]:
    """The cells of `grid` in one of `inks`."""
    return [(x, y) for y, row in enumerate(grid) for x, (_, ink) in row.items() if ink in inks]  # fmt: skip


def test_drops() -> None:
    at = list(pond.drops(3, 50.0, 80, 24, 240.0))
    assert at and list(pond.drops(3, 50.0, 80, 24, 240.0)) == at
    assert list(pond.drops(4, 50.0, 80, 24, 240.0)) != at
    assert list(pond.drops(3, 50.0, 80, 24, 0.0)) == []
    for one in at:
        assert one.t0 <= 50.0 < one.t0 + one.life and 1.4 <= one.life <= 3.2
        assert 0 <= one.x < 160 and 0 <= one.y < 96 and 1 <= one.rings <= 2
        assert 6.0 <= one.reach <= 20.0
    # A drop is the same all its life, and falls once.
    keys: dict[int, Ripple] = {}
    for tick in range(2400):
        for one in pond.drops(3, tick / 8, 80, 24, 24.0):
            assert keys.setdefault(one.key, one) == one
    assert 90 <= len(keys) <= 150  # 24 a minute, 5 minutes
    wide = {r.key for t in range(2400) for r in pond.drops(3, t / 8, 160, 48, 24.0)}
    assert 3.2 * len(keys) < len(wide) < 4.8 * len(keys)
    # A rate that is a function of the second; short lives.
    rain = list(pond.drops(3, 9.0, 80, 24, lambda s: 600.0 * (s >= 8), *pond.SHOWERS))
    assert rain and all(r.t0 >= 8.0 and r.life <= 1.3 for r in rain)
    assert all(r.rings == 1 and r.reach <= 6.0 for r in rain)


def test_ring() -> None:
    one = Ripple(7, 0.0, 0.0, 0.0, 2.0, 2, 12.0)
    assert one.ring(0.0) == (0.0, 1.0)
    assert one.ring(-0.1) is None and one.ring(2.0) is None
    # It leaves fast and slows down: half its way in a fifth of its time.
    wide = [one.ring(t / 10)[0] for t in range(20)]
    assert wide == sorted(wide) and wide[4] > 5.8 and wide[-1] > 11.9
    faint = [one.ring(t / 10)[1] for t in range(20)]
    assert faint == sorted(faint, reverse=True) and faint[-1] < 0.06
    # The second: later, smaller, fainter, and over with the first.
    assert one.ring(0.4, 1) is None
    for t in (0.5, 1.0, 1.9):
        (r0, s0), (r1, s1) = one.ring(t), one.ring(t, 1)
        assert r1 < r0 - 3 and s1 < s0
    assert one.ring(1.99, 1)[0] == pytest.approx(8.4, abs=0.01)


def test_shadow() -> None:
    assert pond.index(["", "  ab", "c"], 3, 2) == {1: [2]}
    walls = pond.index(["", "", "   " + "x" * 30, "", "", "", "      o"], 40, 8)
    assert sorted(walls) == [2, 6] and len(walls[2]) == 30
    free = pond.shadow(40.0, 20.0, 30.0, {})
    assert free is not None and set(free[0]) == {math.inf}
    assert pond.shadow(13.0, 26.0, 30.0, walls) is None  # on the `o`
    assert pond.shadow(20.0, 10.0, 30.0, walls) is None  # on the line
    shade = pond.shadow(36.0, 26.0, 40.0, walls)
    assert shade is not None
    near, far = shade
    up = int(0.75 * pond.BINS)  # the line, above
    assert 12 <= near[up] <= 16 and far[up] > near[up] + 100
    left = int(0.5 * pond.BINS)  # the `o`, a stone: the ring closes behind
    assert 18 <= near[left] <= 22 and near[left] + 6 <= far[left] < near[left] + 20
    assert near[int(0.25 * pond.BINS)] == math.inf  # below: nothing
    # Out of reach: not a shadow.
    short = pond.shadow(36.0, 26.0, 8.0, walls)
    assert short is not None and set(short[0]) == {math.inf}


def test_pond_is_a_map() -> None:
    water = Pond(random.Random(1))
    small = water.picture(40, 12)
    assert water.picture(40, 12) == small
    assert Pond(random.Random(1)).picture(40, 12) == small
    assert Pond(random.Random(2)).picture(40, 12) != small
    large = water.picture(80, 24)
    assert len(large) == 24 and all(0 <= x < 80 for row in large for x in row)
    assert len(water.pads) == 6 and len(water.leaves(40, 12)) == 1
    assert all(braille(glyph) for row in large for glyph, _ in row.values())
    greens = inked(large, pond.GREENS)
    assert 15 <= len(greens) <= 90
    assert water.picture(0, 5) == [{}] * 5 and water.picture(5, 0) == []
    bare = Pond(random.Random(1), pads=0, drops=0)
    assert not inked(bare.picture(80, 24, 3.0), (*pond.GREENS, *pond.RING))
    # The time: it moves, and a moment is the same at every call.
    looks = [water.picture(80, 24, t / 8) for t in range(80)]
    assert len({str(look) for look in looks}) > 60
    assert water.picture(80, 24, 5.0) == looks[40]
    # Pads of a cluster do not overlap.
    many = Pond(random.Random(1), pads=40)
    leaves = many.leaves(80, 24)
    for i, (_, _, a) in enumerate(leaves):
        for _, _, b in leaves[:i]:
            if (a.u, a.v) == (b.u, b.v):
                assert math.hypot(a.ox - b.ox, a.oy - b.oy) >= a.r + b.r
    assert all(2.4 <= pad.r <= 5.6 for _, _, pad in leaves)


def test_rings() -> None:
    water = Pond(random.Random(1), pads=0, drops=0)
    one = Ripple(7, 80.0, 60.0, 0.0, 3.0, 2, 30.0)
    water.kisses = [one]
    assert inked(water.picture(80, 24, 0.005), pond.RING) == [(40, 15)]
    spans = []
    for t in (0.2, 0.6, 1.2, 2.4):
        cells = inked(water.picture(80, 24, t), pond.RING)
        far = max(math.hypot(2 * x + 1 - 80, 4 * y + 2 - 60) for x, y in cells)
        assert abs(far - one.ring(t)[0]) < 4.5
        spans.append(far)
    assert spans == sorted(spans)

    def dots(grid: stars.Grid) -> int:
        return sum(braille(glyph).bit_count() for row in grid for glyph, _ in row.values())  # fmt: skip

    # It fades: fewer dots a length, darker.
    young, old = water.picture(80, 24, 0.3), water.picture(80, 24, 2.8)
    assert {ink for row in young for _, ink in row.values()} & set(pond.RING[:3])
    assert not {ink for row in old for _, ink in row.values()} & set(pond.RING[:4])
    assert dots(young) / one.ring(0.3)[0] > 2 * dots(old) / one.ring(2.8)[0]
    assert water.picture(80, 24, 3.1) == Pond(random.Random(1), pads=0, drops=0).picture(80, 24, 3.1)  # fmt: skip
    # A line of words over it: no ring beyond.
    walls = pond.index(["", "", "", "", "", "", "", "", "", "x" * 80], 80, 24)
    for t in (0.6, 1.2, 2.4):
        cells = inked(water.picture(80, 24, t, walls), pond.RING)
        assert cells and all(y >= 9 for _, y in cells)
    # A drop on a word, on a pad: no ring.
    water.kisses = [Ripple(8, 80.0, 38.0, 0.0, 3.0, 2, 30.0)]
    assert not inked(water.picture(80, 24, 1.0, walls), pond.RING)
    padded = Pond(random.Random(1), drops=600)
    leaves = padded.leaves(80, 24, 4.0)
    fell = list(pond.drops(padded.seed, 4.0, 80, 24, 600.0))
    drawn: list[Ripple] = []
    rings = padded._rings
    padded._rings = lambda water, ripples, t, walls: (drawn.extend(ripples), rings(water, ripples, t, walls))[1]  # fmt: skip
    padded.picture(80, 24, 4.0)
    assert drawn and len(drawn) < len(fell)
    for one in fell:
        hit = any(math.hypot(one.x - x, one.y - y) < p.r for x, y, p in leaves)
        assert not (hit and one in drawn)


def test_koi() -> None:
    rng = random.Random(5)
    walls = pond.index([""] * 8 + [" " * 20 + "x" * 30] * 3, 80, 24)

    def stone(x: float, y: float) -> bool:
        return int(x) >> 1 in walls.get(int(y) >> 2, ())

    under = steps = 0
    for _ in range(6):
        fish = Koi(rng, 80, 24)
        for _ in range(2400):
            fish.step(1 / 8, 80, 24, stone)
            assert -14 <= fish.x < 174 and -14 <= fish.y < 110
            assert abs(fish.turn) <= 1.6
            under += stone(fish.x, fish.y)
            steps += 1
        assert len(fish.trail) == 24
        gaps = [math.dist(a, b) for a, b in itertools.pairwise(fish.trail)]  # fmt: skip
        assert all(0.9 < gap < 1.1 for gap in gaps)
    assert under < 0.03 * steps
    # Out after a resize: it comes back.
    fish.x, fish.y = 300.0, 200.0
    for _ in range(800):
        fish.step(1 / 8, 40, 12, stone)
    assert 0 <= fish.x < 80 and 0 <= fish.y < 48
    water = Pond(random.Random(5), pads=0, drops=0, koi=True)
    kisses: dict[int, Ripple] = {}
    for i in range(400):
        water.step(1 / 8, 80, 24, i / 8)
        # A ring now and then; gone when it is over.
        assert all(i / 8 - one.life < one.t0 <= i / 8 for one in water.kisses)
        kisses.update((one.key, one) for one in water.kisses)
    assert 3 <= len(kisses) <= 16
    assert all(0 <= one.x < 160 and 0 <= one.y < 96 for one in kisses.values())
    assert water.koi is not None and len(water.koi) == 2
    coats = {ink for coat in pond.COATS for ink in coat}
    drawn = inked(water.picture(80, 24, 50.0), tuple(coats))
    assert 4 <= len(drawn) <= 40
    water.step(1 / 8, 200, 60, 50.0)
    assert len(water.koi) == 5
    water.step(1 / 8, 80, 24, 500.0)
    assert len(water.koi) == 2 and len(water.kisses) <= 2
    still = Pond(random.Random(5))
    still.step(1 / 8, 80, 24, 0.0)
    assert still.koi is None


def test_fear() -> None:
    water = Pond(random.Random(5), pads=0, drops=0, koi=True, striders=True)
    assert water.fear(0.0) is None
    water.touch(81, 50, 10.0)
    assert water.fear(10.0) == (81, 50, 1.0) and water.fear(9.0) is None
    felt = [water.fear(10.0 + t / 4)[2] for t in range(12)]
    assert felt == sorted(felt, reverse=True) and felt[-1] < 0.03
    assert water.fear(10.0 + pond.CALM) is None
    # A koi near the pointer turns its tail to it, darts, and leaves a ring.
    for i in range(200):
        water.step(1 / 20, 80, 24, i / 20)
    fish = water.koi[0]
    fish.x, fish.y, fish.heading, fish.turn = 70.0, 50.0, 0.0, 0.0
    far = water.koi[1]
    far.x, far.y = 150.0, 10.0
    water.kisses = []
    fish.kiss = far.kiss = 99.0
    water.touch(81, 50, 10.0)
    gone = []
    for i in range(40):
        before = (fish.x, fish.y)
        water.step(1 / 20, 80, 24, 10.0 + i / 20)
        gone.append(math.dist(before, (fish.x, fish.y)) * 20)
    assert fish.x < 60 and math.hypot(fish.x - 81, fish.y - 50) > pond.SHY
    assert max(gone) > 2.5 * fish.speed and far.dash == 0.0
    assert [(one.x, one.y) for one in water.kisses] == [(pytest.approx(70, abs=1), pytest.approx(50, abs=1))]  # fmt: skip
    # Calm again: its pace, its turns.
    for i in range(120):
        water.step(1 / 20, 80, 24, 14.0 + i / 20)
    assert fish.dash < 0.01 and abs(fish.turn) <= 1.6
    # A strider slides off the pointer, and comes back.
    water = Pond(random.Random(5), pads=0, drops=0, striders=True)
    (x, y, _), *_ = water.skaters(80, 24, 2.0, lambda x, y: False)
    home = inked(water.picture(80, 24, 2.0), (pond.STRIDER,))
    water.touch(x + 3, y, 2.0)
    away = inked(water.picture(80, 24, 2.0), (pond.STRIDER,))
    assert away != home and min(a for a, _ in away) < min(a for a, _ in home) - 3
    assert inked(water.picture(80, 24, 2.0 + pond.CALM), (pond.STRIDER,)) == inked(
        Pond(random.Random(5), pads=0, drops=0, striders=True).picture(
            80, 24, 2.0 + pond.CALM
        ),
        (pond.STRIDER,),
    )


def test_current() -> None:
    def lit(water: Pond, t: float) -> list[tuple[int, int]]:
        return inked(water.picture(80, 24, t), pond.SHEEN)

    still = Pond(random.Random(1), pads=0, drops=0, current=0.0)
    assert still.flow == (0.0, 0.0)
    water = Pond(random.Random(1), pads=0, drops=0)
    assert math.hypot(*water.flow) == pytest.approx(pond.FLOW)
    assert lit(water, 0.0) == lit(still, 0.0) and lit(water, 0.0)
    assert lit(water, 30.0) != lit(still, 30.0)
    # A strider is carried while it rests.
    carried = Pond(random.Random(1), pads=0, drops=0, striders=True, current=2.0)
    fx, fy = carried.flow
    for a, b in itertools.pairwise(range(400)):
        (x0, y0, p0), *_ = carried.skaters(80, 24, a / 20, lambda x, y: False)
        (x1, y1, p1), *_ = carried.skaters(80, 24, b / 20, lambda x, y: False)
        assert math.hypot(x1 - x0, y1 - y0) < 14  # no jump
        if p0.key == p1.key and a / 20 - p0.t0 > 0.3:
            assert (x1 - x0, y1 - y0) == pytest.approx((fx / 20, fy / 20), abs=1e-6)


def test_moon() -> None:
    def lit(phase: float, t: float = 90.0) -> list[tuple[int, int]]:
        water = Pond(random.Random(1), pads=0, drops=0, moon=True, phase=phase)
        return inked(water.picture(80, 24, t), pond.MOON)

    full = lit(0.5)
    assert 25 <= len(full) <= 60
    assert lit(0.0) == []
    assert len(lit(0.12)) < len(lit(0.25)) < len(full)
    # In the middle of its way at half the night; out for a fifth.
    assert all(33 <= x <= 47 for x, _ in full)
    assert lit(0.5, 270.0) == [] and lit(0.5, 330.0)
    assert lit(0.5, 10.0)[0][0] < 40 < lit(0.5, 200.0)[0][0]
    assert pond.moon_at(80, 24, 6.0, 0.0)[0] == -6.0
    assert pond.moon_at(80, 24, 6.0, 1.0)[0] == 166.0
    assert pond.moon_at(80, 24, 6.0, 0.5) == (80.0, pytest.approx(28.8))
    # The swell shakes it.
    assert len({str(lit(0.5, 90.0 + t / 8)) for t in range(16)}) > 12


def test_adrift_striders_lotus_rain() -> None:
    def made(**options: object) -> Pond:
        return Pond(random.Random(1), drops=0, **options)  # type: ignore[arg-type]

    for season, (inks, _, _) in pond.ADRIFT.items():
        water = made(pads=0, adrift=True, season=season)
        assert inked(water.picture(80, 24, 2.0), inks)
        others = {ink for name, (i, _, _) in pond.ADRIFT.items() if name != season for ink in i}  # fmt: skip
        assert not inked(water.picture(80, 24, 2.0), tuple(others - set(inks)))
    assert set(pond.ADRIFT) == set(pond.SEASONS)
    # They drift: a minute later, elsewhere.
    water = made(pads=0, adrift=True, season="autumn")
    inks = pond.ADRIFT["autumn"][0]
    assert inked(water.picture(80, 24, 0.0), inks) != inked(water.picture(80, 24, 60.0), inks)  # fmt: skip
    water = made(striders=True)
    places = set()
    for t in range(240):
        cells = inked(water.picture(80, 24, t / 8), (pond.STRIDER,))
        assert 1 <= len(cells) <= 12
        places.add(cells[0])
        # Never on a pad.
        leaves = water.leaves(80, 24, t / 8)

        def dry(x: float, y: float, leaves: list = leaves) -> bool:
            return any(math.hypot(x - a, y - b) < p.r + 1.5 for a, b, p in leaves)

        for x, y, push in water.skaters(80, 24, t / 8, dry):
            assert not dry(x, y) or not dry(push.x, push.y)
            assert push.t0 <= t / 8 and math.hypot(x - push.x, y - push.y) < 30
    assert len(places) > 6
    assert any(inked(water.picture(80, 24, t / 8), pond.RING) for t in range(40))
    plain, bloom = made(pads=30), made(pads=30, lotus=True)
    glyphs = [g for row in bloom.picture(80, 24) for g, _ in row.values() if not braille(g)]  # fmt: skip
    assert 3 <= len(glyphs) <= 20 and set(glyphs) <= set(pond.BLOOMS)
    assert len(inked(bloom.picture(80, 24), pond.LOTUS)) == len(glyphs)
    assert not inked(plain.picture(80, 24), pond.LOTUS)
    # Showers: many small rings at the height of one, few between two.
    wet = made(pads=0, rain=True)
    rates = [wet.shower(s) for s in range(100)]
    assert max(rates) > 0.95 * pond.RAIN and min(rates) < 0.14 * pond.RAIN
    high = max(range(100), key=wet.shower)
    low = min(range(100), key=wet.shower)
    assert len(inked(wet.picture(80, 24, high), pond.RING)) > 4 * len(inked(wet.picture(80, 24, low), pond.RING))  # fmt: skip


def test_fly() -> None:
    rng = random.Random(7)
    fly = pond.Fly(rng, 80, 24)
    cells = [fly.step(1 / 8, 80, 24) for _ in range(4000)]
    assert all(0 <= x < 80 and 0 <= y < 24 for x, y in cells)
    assert len(set(cells)) > 60
    # It hovers most of the time, and darts.
    moves = [a != b for a, b in itertools.pairwise(cells)]
    assert 0.05 < sum(moves) / len(moves) < 0.5
    assert max(abs(a[0] - b[0]) for a, b in itertools.pairwise(cells)) >= 3
    # The pointer near: it leaves, and stays away.
    for _ in range(40):
        x, y = fly.step(1 / 20, 80, 24)
    for _ in range(60):
        cell = fly.step(1 / 20, 80, 24, (x, y, 1.0))
    assert math.hypot(cell[0] - x, 2 * (cell[1] - y)) >= pond.WARY
    assert fly.step(1 / 20, 80, 24, (x, y, 0.2)) == cell
    frames = fly.frames()
    assert [f["cells"][0][0]["t"] for f in frames] == ["‡", "╪"]
    assert (frames[0]["cells"][0][0]["fg"], frames[1]["cells"][0][0]["fg"]) in pond.FLIES  # fmt: skip


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert pond.main(["--print", "40x12", "--seed", "1"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert len(out) == 12 and "\x1b[38;5;" in "".join(out)
    assert pond.main(["--print", "-a", "--seed", "1", "--phase", "0.5"]) == 0
    full = capsys.readouterr().out
    assert len(full.splitlines()) == 24 and "\x1b[38;5;230m" in full
    assert pond.main(["--print", "-a", "--seed", "1", "--phase", "0.5"]) == 0
    assert capsys.readouterr().out == full
    for bad in (["--night", "0"], ["--pads", "-1"], ["--drops", "-1"], ["--current", "-1"], ["--print", "0x4"], ["--season", "monsoon"]):  # fmt: skip
        with pytest.raises(SystemExit):
            pond.main(bad)
    capsys.readouterr()


def played(pilot: Pilot, *options: str) -> tuple[int, int]:
    """Run the app 2 s on a pane with two lines of text. The pictures of
    the host seen, and the most sprites at once; the text must show
    whole all along, and the host be clean after."""
    lines = ("words in the way", "   more words here")
    pilot.send("clear; printf '\\n\\n{}\\n\\n{}\\n'\n".format(*lines))
    pilot.wait_for(lines[1].strip(), frame="raw")
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    args = ["-s", pilot.session, "--seed", "4", "--pads", "30", "--drops", "200", *options]  # fmt: skip
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(pond.main(args, seconds=2.0))
    )
    thread.start()
    seen: set[str] = set()
    most = 0
    while thread.is_alive():
        rows = pilot.dump()["host"]
        host = "\n".join(rows)
        assert all(line.strip() in host for line in lines)
        most = max(most, len(pilot.sprites()))
        if sum(bool(braille(c)) for row in rows for c in row) > 40:
            seen.add(host)
        session_clock.sleep(0.02)
    thread.join()
    assert status == [0]
    deadline = time.monotonic() + 3
    while pilot.sprites() and time.monotonic() < deadline:
        session_clock.sleep(0.02)
    assert pilot.sprites() == []
    assert pilot.dump()["host"] == pilot.text()
    return len(seen), most


def test_pond(session: Pilot) -> None:
    pictures, sprites = played(session)
    assert pictures > 3 and sprites == 1


def test_pond_all(session: Pilot) -> None:
    pictures, sprites = played(session, "-a")
    assert pictures > 3 and sprites >= 2


class Host:
    """A terminal that is not there: `ptyrc run` on a pty, what it
    writes kept, the moves of a mouse typed in."""

    def __init__(self, name: str) -> None:
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        argv = [os.environ["PTYRC_BIN"], "run", "--name", name, "--"]
        argv += ["env", "PS1=$ ", "sh"]
        self.driver = subprocess.Popen(
            argv, stdin=slave, stdout=slave, stderr=slave, start_new_session=True
        )
        os.close(slave)
        self.seen = b""
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self) -> None:
        try:
            while data := os.read(self.master, 65536):
                self.seen += data
        except OSError:
            pass

    def move(self, x: int, y: int) -> None:
        """A move with no button down on a screen cell, SGR form."""
        os.write(self.master, f"\x1b[<35;{x + 1};{y + 1}M".encode())

    def close(self) -> None:
        self.driver.kill()
        self.driver.wait()
        os.close(self.master)


def test_pointer(monkeypatch: pytest.MonkeyPatch) -> None:
    name = f"pond-{os.getpid()}-{time.monotonic_ns() % 10**9}"
    host = Host(name)
    touched: list[tuple[float, float]] = []
    touch = Pond.touch

    def note(self: Pond, x: float, y: float, t: float) -> None:
        touched.append((x, y))
        touch(self, x, y, t)

    monkeypatch.setattr(Pond, "touch", note)
    try:
        deadline = time.monotonic() + 5
        while True:
            try:
                pilot = Pilot(name)
                break
            except (PtyrcError, OSError):
                assert time.monotonic() < deadline, "no session"
                time.sleep(0.02)
        with pilot:
            pilot.wait_for("$ ")
            while not pilot.frame("raw").usable:
                assert time.monotonic() < deadline, "no usable frame"
                time.sleep(0.02)
            args = ["-s", name, "--seed", "4", "--pads", "0", "--drops", "0", "-k"]
            status: list[int] = []
            thread = threading.Thread(
                target=lambda: status.append(pond.main(args, seconds=6.0))
            )
            thread.start()
            # The moves are asked of the host; one anywhere on the view is told.
            while b"\x1b[?1003h" not in host.seen or not pilot.sprites():
                assert time.monotonic() < deadline, host.seen[-200:]
                time.sleep(0.02)
            for cell in ((70, 20), (3, 1), (40, 12)):
                host.move(*cell)
                while (2 * cell[0] + 1, 4 * cell[1] + 2) not in touched:
                    assert thread.is_alive() and time.monotonic() < deadline, touched
                    time.sleep(0.02)
            thread.join()
            assert status == [0]
    finally:
        host.close()
