# SPDX-License-Identifier: GPL-3.0-or-later
"""`stars`: the sky, the moon, the grass, the app on a headless driver."""

from __future__ import annotations

import datetime
import random
import threading
import time

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import stars
from ptyrc_apps.stars import Grass, Sky


def braille(glyph: str) -> int:
    """The dots of a braille glyph, as bits; 0 for another."""
    return ord(glyph) - 0x2800 if 0x2800 < ord(glyph) < 0x2900 else 0


def text(grid: stars.Grid, w: int) -> list[str]:
    return ["".join(run["t"] for run in line) for line in stars.runs(grid, w)]


def test_sky_is_a_map() -> None:
    sky = Sky(random.Random(1))
    small = sky.picture(40, 12)
    assert sky.picture(40, 12) == small
    assert Sky(random.Random(1)).picture(40, 12) == small
    assert Sky(random.Random(2)).picture(40, 12) != small
    # As dense at every size; a larger pane keeps the first stars.
    count = sum(len(row) for row in small)
    large = sky.picture(80, 24)
    assert 10 <= count <= 18 and 55 <= sum(len(row) for row in large) <= 70
    assert len(sky.stars) == 70 and len(large) == 24
    assert all(0 <= x < 80 for row in large for x in row)
    assert Sky(random.Random(1), density=0).picture(80, 24) == [{}] * 24
    assert sky.picture(0, 5) == [{}] * 5 and sky.picture(5, 0) == []
    # Time changes nothing unless asked.
    assert sky.picture(80, 24, 50.0) == large
    looks = [sky.picture(80, 24, t / 2, twinkle=True) for t in range(20)]
    assert len({str(look) for look in looks}) > 10
    assert sum(look == large for look in looks) < 20
    # A turn: the map again; half a turn: every star moved.
    assert sky.picture(80, 24, turn=1.0) == large
    half = sky.picture(80, 24, turn=0.5)
    assert half != large
    assert sum(len(row) for row in half) == sum(len(row) for row in large)


def test_sky_galaxy_and_figures() -> None:
    plain = Sky(random.Random(3)).picture(80, 24)
    galaxy = Sky(random.Random(3), galaxy=True).picture(80, 24)
    inks = {ink for row in galaxy for _, ink in row.values()}
    assert sum(len(row) for row in galaxy) > 4 * sum(len(row) for row in plain)
    assert set(stars.HAZE) <= inks
    sky = Sky(random.Random(3), figures=True)
    assert len({name for _, _, name in sky.figures}) == 3
    drawn = sky.picture(120, 40)
    assert stars.LINE in {ink for row in drawn for _, ink in row.values()}
    for points, path in stars.FIGURES.values():
        assert all(-1 <= i < len(points) for i in path)
        assert set(range(len(points))) <= set(path)
        assert all(1 <= level <= 3 for _, _, level in points)


def test_runs_and_clear() -> None:
    grid: stars.Grid = [{0: ("a", 1), 1: ("b", 1), 4: ("c", 2), 9: ("z", 3)}, {}]
    assert stars.runs(grid, 8) == [
        [{"t": "ab", "fg": 1}, {"t": "  c", "fg": 2}],
        [{"t": " "}],
    ]
    assert stars.runs(grid, 8, [0b10010, 0]) == [
        [{"t": " b", "fg": 1}, {"t": "  c", "fg": 2}],
        [{"t": " "}],
    ]
    assert stars.clear(["", "  ab"], 8, 2) == [0xFF, 0b11100001]
    assert stars.clear(["a世"], 6, 1) == [0b110000]
    assert stars.clear(["a", "b", "c"], 2, 2) == [0, 0]


def lit(grid: stars.Grid) -> int:
    """Dots of the moon."""
    return sum(
        braille(glyph).bit_count()
        for row in grid
        for glyph, ink in row.values()
        if ink == stars.MOON
    )


def test_moon() -> None:
    def drawn(phase: float, cx: float = 20.0) -> stars.Grid:
        grid: stars.Grid = [{} for _ in range(6)]
        stars.moon(grid, cx, 12.0, 6.0, phase)
        return grid

    disc = lit(drawn(0.5))
    assert 100 <= disc <= 125  # pi * 6 * 6
    assert drawn(0.0) == [{}] * 6
    sizes = [lit(drawn(phase)) for phase in (0.1, 0.25, 0.4, 0.5)]
    assert sizes == sorted(set(sizes)) and sizes[0] > 4
    assert abs(sizes[1] - disc / 2) <= 8
    assert abs(lit(drawn(0.25)) - lit(drawn(0.75))) <= 8
    # Waxing: the right side is lit; waning: the left.
    for phase, right in ((0.25, True), (0.75, False)):
        cells = [x for row in drawn(phase) for x in row]
        assert cells and all((x >= 10) == right for x in cells)
    # Half out of the view, and out of it.
    assert 0 < lit(drawn(0.5, 0.0)) < disc
    assert drawn(0.5, -9.0) == [{}] * 6
    # The dark side hides what is behind.
    grid: stars.Grid = [{8: ("✦", 231), 4: ("✦", 231)} for _ in range(6)]
    stars.moon(grid, 14.0, 12.0, 6.0, 0.25)
    assert 4 not in grid[3] and grid[3][8][1] == stars.MOON
    assert grid[0] == {8: ("✦", 231), 4: ("✦", 231)}


def test_moon_way_and_phase() -> None:
    x0, y0 = stars.moon_at(80, 24, 5.0, 0.0)
    x1, y1 = stars.moon_at(80, 24, 5.0, 0.5)
    x2, y2 = stars.moon_at(80, 24, 5.0, 1.0)
    assert (x0, x1, x2) == (-5.0, 80.0, 165.0)
    assert y0 == pytest.approx(101.0) and y2 == pytest.approx(101.0) and y1 == 7.0
    # New moons of 2024-04-08 (an eclipse) and 2026-11-09; a full one.
    for date, phase in (((2024, 4, 8, 18), 0.0), ((2026, 11, 9, 8), 0.0), ((2026, 11, 24, 15), 0.5)):  # fmt: skip
        got = stars.phase_of(datetime.datetime(*date, tzinfo=datetime.UTC))
        assert abs((got - phase + 0.5) % 1.0 - 0.5) < 0.03


def test_grass() -> None:
    grass = Grass(random.Random(5))
    still = grass.picture(80, 0.0, wind=0.0)
    assert len(still) == grass.rows == 2 and grass.picture(80, 9.0, wind=0.0) == still
    assert len(still[1]) >= 76 and 10 <= len(still[0]) <= 70
    assert all(braille(glyph) for row in still for glyph, _ in row.values())
    assert {ink for _, ink in still[1].values()} <= set(stars.GREENS[0])
    # The breeze: the grass changes, a blade keeps its root and its height.
    looks = [grass.picture(80, t / 6) for t in range(60)]
    assert len({str(look) for look in looks}) > 30
    dots = [sum(braille(g).bit_count() for row in look for g, _ in row.values()) for look in looks]  # fmt: skip
    assert max(dots) - min(dots) < 0.1 * max(dots)
    floor = 0xC0  # the two lowest dots of a cell
    for look in looks:
        assert sum(bool(braille(g) & floor) for g, _ in look[1].values()) >= 76
    assert stars.breeze(10, 3.0, 0.0) == 0.0
    assert stars.breeze(10, 3.0, -1.0) == -stars.breeze(10, -3.0, 1.0)
    assert all(-0.3 < stars.breeze(x, t / 3) < 1.7 for x in range(160) for t in range(90))  # fmt: skip
    # A meadow: taller, with flowers.
    wild = Grass(random.Random(5), wild=True).picture(80, 0.0)
    heads = [(g, ink) for row in wild for g, ink in row.values() if not braille(g)]
    assert len(wild) == 3 and len(wild[0]) > 10
    assert heads and all(g in "*•°" and ink in stars.PETALS for g, ink in heads)
    assert text(Grass(random.Random(5)).picture(3, 0.0), 3)[1].strip()


def test_meteor_and_bug() -> None:
    rng = random.Random(7)
    ways = set()
    for _ in range(20):
        one = stars.meteor(rng, 80, 24)
        x, y = one["at"]
        assert 0 <= x < 80 and 0 <= y < 8 and one["loop"] is False
        assert one["motion"]["vx"] == 2 * one["motion"]["vy"] * (1 if one["motion"]["vx"] > 0 else -1)  # fmt: skip
        assert one["ttl"] * 1000 > sum(f["ms"] for f in one["frames"])
        ways.add(one["motion"]["vx"] > 0)
    assert ways == {True, False}
    bug = stars.Bug(rng, 40, 20, 2)
    cells = {bug.step(1 / 6, 40, 20, 2) for _ in range(2000)}
    assert len(cells) > 20
    assert all(0 <= x < 40 and 14 <= y <= 18 for x, y in cells)
    assert bug.frames()[0]["cells"] == [[{"t": " "}]]


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert stars.main(["--print", "40x12", "--seed", "1", "--phase", "0.5"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert len(out) == 12 and "\x1b[38;5;230m" in "".join(out)
    assert stars.main(["--print", "-a", "--seed", "1"]) == 0
    assert len(capsys.readouterr().out.splitlines()) == 24
    for bad in (["--night", "0"], ["--stars", "-1"], ["--print", "0x4"]):
        with pytest.raises(SystemExit):
            stars.main(bad)
    capsys.readouterr()


def played(pilot: Pilot, *options: str) -> tuple[int, int]:
    """Run the app 2 s on a pane with two lines of text. The pictures of
    the host seen, and the most sprites at once; the text must show
    whole all along, the grass on the last row, and the host be clean
    after."""
    lines = ("words in the way", "   more words here")
    pilot.send("clear; printf '\\n\\n{}\\n\\n{}\\n'\n".format(*lines))
    pilot.wait_for(lines[1].strip(), frame="raw")
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    args = ["-s", pilot.session, "--seed", "4", "--night", "20", *options]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(stars.main(args, seconds=2.0))
    )
    thread.start()
    seen: set[str] = set()
    most = 0
    while thread.is_alive():
        rows = pilot.dump()["host"]
        host = "\n".join(rows)
        assert all(line.strip() in host for line in lines)
        most = max(most, len(pilot.sprites()))
        if sum(bool(braille(c)) for c in rows[-1]) > 40:
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


def test_stars(session: Pilot) -> None:
    pictures, sprites = played(session)
    assert pictures > 3 and sprites == 2


def test_stars_all(session: Pilot) -> None:
    pictures, sprites = played(session, "-a")
    assert pictures > 3 and sprites >= 2 + 80 // 14
