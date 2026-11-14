# SPDX-License-Identifier: GPL-3.0-or-later
"""`pond`: the drops, the shadows of the words, the scene, the app on a
headless driver."""

from __future__ import annotations

import itertools
import math
import random
import threading
import time

import pytest
from ptyrc import Pilot
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
    at = list(pond.drops(3, 50.0, 80, 24, 24.0))
    assert at and list(pond.drops(3, 50.0, 80, 24, 24.0)) == at
    assert list(pond.drops(4, 50.0, 80, 24, 24.0)) != at
    assert list(pond.drops(3, 50.0, 80, 24, 0.0)) == []
    for one in at:
        assert one.t0 <= 50.0 < one.t0 + one.life and 4.0 <= one.life <= 9.0
        assert 0 <= one.x < 160 and 0 <= one.y < 96 and 1 <= one.rings <= 3
    # A drop is the same all its life, and falls once.
    keys: dict[int, Ripple] = {}
    for tick in range(2400):
        for one in pond.drops(3, tick / 8, 80, 24, 24.0):
            assert keys.setdefault(one.key, one) == one
    assert 90 <= len(keys) <= 150  # 24 a minute, 5 minutes
    wide = {r.key for t in range(2400) for r in pond.drops(3, t / 8, 160, 48, 24.0)}
    assert 3.2 * len(keys) < len(wide) < 4.8 * len(keys)
    # A rate that is a function of the second; short lives.
    rain = list(pond.drops(3, 9.0, 80, 24, lambda s: 600.0 * (s >= 8), (1.2, 2.4)))
    assert rain and all(r.t0 >= 8.0 and r.life <= 2.4 for r in rain)


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
    assert len(water.pads) == 9 and len(water.leaves(40, 12)) == 2
    assert all(braille(glyph) for row in large for glyph, _ in row.values())
    greens = inked(large, pond.GREENS)
    assert 30 <= len(greens) <= 140
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
    assert all(2.4 <= pad.r <= 6.6 for _, _, pad in leaves)


def test_rings() -> None:
    water = Pond(random.Random(1), pads=0, drops=0)
    water.kisses = [Ripple(7, 80.0, 60.0, 0.0, 9.0, 3)]
    assert inked(water.picture(80, 24, 0.05), pond.RING) == [(40, 15)]
    spans = []
    for t in (1.0, 3.0, 5.0, 8.0):
        cells = inked(water.picture(80, 24, t), pond.RING)
        far = max(math.hypot(2 * x + 1 - 80, 4 * y + 2 - 60) for x, y in cells)
        assert abs(far - pond.SPEED * t) < 4.5
        spans.append(far)
    assert spans == sorted(spans)
    # It fades: fewer dots a length, darker.
    young, old = water.picture(80, 24, 2.0), water.picture(80, 24, 8.5)
    assert {ink for _, ink in young[15].values()} & set(pond.RING[:3])
    assert not {ink for row in old for _, ink in row.values()} & set(pond.RING[:4])
    assert water.picture(80, 24, 9.5) == Pond(random.Random(1), pads=0, drops=0).picture(80, 24, 9.5)  # fmt: skip
    # A line of words over it: no ring beyond.
    walls = pond.index(["", "", "", "", "", "", "", "", "", "x" * 80], 80, 24)
    for t in (3.0, 6.0, 8.5):
        cells = inked(water.picture(80, 24, t, walls), pond.RING)
        assert cells and all(y >= 9 for _, y in cells)
    # A drop on a word, on a pad: no ring.
    water.kisses = [Ripple(8, 80.0, 38.0, 0.0, 9.0, 3)]
    assert not inked(water.picture(80, 24, 3.0, walls), pond.RING)
    padded = Pond(random.Random(1), drops=600)
    leaves = padded.leaves(80, 24, 4.0)
    for one in pond.drops(padded.seed, 4.0, 80, 24, 600.0):
        hit = any(math.hypot(one.x - x, one.y - y) < p.r for x, y, p in leaves)
        padded.kisses = [one]
        padded.rate = 0.0
        assert not (hit and inked(padded.picture(80, 24, 4.0), pond.RING[:2]))


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
    for bad in (["--night", "0"], ["--pads", "-1"], ["--drops", "-1"], ["--print", "0x4"], ["--season", "monsoon"]):  # fmt: skip
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
