# SPDX-License-Identifier: GPL-3.0-or-later
"""`flock`: the dusk, the birds, the land, the app on a headless driver."""

from __future__ import annotations

import argparse
import math
import random
import threading
import time

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import _cli, flock
from ptyrc_apps.flock import REST, SET, Dusk, Flock, Land, Scene


def braille(glyph: str) -> int:
    """The dots of a braille glyph, as bits; 0 for another."""
    return ord(glyph) - 0x2800 if 0x2800 < ord(glyph) < 0x2900 else 0


def dots(grid: flock.Grid, inks: tuple[int, ...]) -> int:
    return sum(
        braille(glyph).bit_count()
        for row in grid
        for glyph, ink in row.values()
        if ink in inks
    )


def options(*argv: str, monkeypatch: pytest.MonkeyPatch) -> argparse.Namespace:
    """The options `main` would run the app with."""
    got: list[argparse.Namespace] = []
    monkeypatch.setattr(_cli, "run", lambda app, args: got.append(args) or 0)
    assert flock.main(list(argv)) == 0
    return got[0]


def test_stage_and_glow() -> None:
    assert flock.stage(0.0, 300.0) == flock.START
    assert flock.stage(300.0 * (1 + REST), 300.0) == pytest.approx(flock.START)
    assert flock.height(0.0) == 1.0 and flock.height(SET) == 0.0
    # The night and the start of the next dusk are alike: no light.
    assert flock.glow(0.0) == 0.0 and flock.glow(1.0 + REST) == 0.0
    assert flock.glow(SET) == 1.0
    lights = [flock.glow(f / 100) for f in range(int(100 * SET) + 1)]
    assert lights == sorted(lights)
    x0, y0 = flock.sun_at(80, 90.0, 7.0, 0.0)
    x1, y1 = flock.sun_at(80, 90.0, 7.0, SET)
    assert y0 == -7.0 and y1 == 97.0 and 0 < x0 < x1 < 160
    assert flock.sun_at(80, 90.0, 7.0, 1.0) == (x1, y1)


def test_dusk() -> None:
    dusk = Dusk(random.Random(1))
    sun = flock.sun_at(80, 92.0, 7.0, 0.7)
    low = dusk.picture(80, 23, 0.7, sun)
    assert dusk.picture(80, 23, 0.7, sun) == low
    assert Dusk(random.Random(1)).picture(80, 23, 0.7, sun) == low
    assert Dusk(random.Random(2)).picture(80, 23, 0.7, sun) != low
    assert len(low) == 23 and all(0 <= x < 80 for row in low for x in row)
    assert all(braille(g) and ink in flock.RAMP for row in low for g, ink in row.values())  # fmt: skip
    # Sparse, and most of it low, by the horizon.
    ink = [sum(braille(g).bit_count() for g, _ in row.values()) for row in low]
    assert 300 < sum(ink) < 0.25 * 160 * 92
    assert sum(ink[12:]) > 3 * sum(ink[:12])
    # No light: no sky.
    assert dusk.picture(80, 23, 0.0, sun) == [{}] * 23
    assert dusk.picture(80, 23, 1.0 + REST, sun) == [{}] * 23
    assert (
        dusk.picture(0, 5, 0.7, sun) == [{}] * 5 and dusk.picture(5, 0, 0.7, sun) == []
    )
    # Clouds drift.
    clouds = Dusk(random.Random(1), clouds=True)
    assert sum(len(row) for row in clouds.picture(80, 23, 0.7, sun)) > sum(len(row) for row in low)  # fmt: skip
    assert clouds.picture(80, 23, 0.7, sun, 0.0) != clouds.picture(
        80, 23, 0.7, sun, 60.0
    )
    assert dusk.picture(80, 23, 0.7, sun, 60.0) == low


def test_dance() -> None:
    one = Flock(random.Random(3))
    small = one.dance(40, 12, 5.0)
    assert len(small) == one.count(40, 12) == 65 and one.count(80, 24) == 260
    assert one.dance(40, 12, 5.0) == small
    assert Flock(random.Random(3)).dance(40, 12, 5.0) == small
    assert Flock(random.Random(4)).dance(40, 12, 5.0) != small
    assert one.dance(80, 24, 5.0) != one.dance(80, 24, 5.1)
    assert Flock(random.Random(3), birds=0).dance(80, 24, 5.0) == []
    # The first birds of one list at every size.
    assert len(one.birds) == 260 and one.dance(40, 12, 5.0) == small
    # They hold together, most in view, and fly without a jump.
    for w, h in ((80, 23), (200, 58)):
        reach = one.reach(2 * w, 4 * h)
        inside = 0
        for k in range(600):
            now = one.dance(w, h, k / 10)
            after = one.dance(w, h, (k + 1) / 10)
            mx, my = one.middle(2 * w, 4 * h, k / 10)
            assert all(math.hypot(x - mx, y - my) < 2.2 * reach for x, y in now)
            step = sum(math.hypot(a - x, b - y) for (x, y), (a, b) in zip(now, after, strict=True))  # fmt: skip
            assert step / len(now) < 0.05 * 2 * w
            inside += sum(0 <= x < 2 * w and 0 <= y < 4 * h for x, y in now)
        assert inside > 0.8 * 600 * len(now)


def test_land() -> None:
    land = Land(random.Random(5))
    assert (land.rows(24), land.rows(60), land.rows(0)) == (1, 2, 0)
    low = land.picture(80, 24)
    assert len(low) == 1 and len(low[0]) == 80
    assert all(braille(g) & 0xC0 for g, _ in low[0].values())
    assert all(91 <= land.top(px, 24) <= 95 for px in range(160))
    assert land.picture(80, 24, 9.0) == low and land.picture(0, 24) == [{}]
    # The sun warms the land under it.
    lit = land.picture(80, 24, sun=(80.0, 7.0, 208), f=0.7)
    assert {ink for _, ink in lit[0].values()} == set(flock.SOIL)
    # Reeds: they bend, a reed keeps its root.
    reeds = Land(random.Random(5), reeds=True)
    still = reeds.picture(80, 24, wind=0.0)
    assert (
        len(still) == reeds.rows(24) == 3 and reeds.picture(80, 24, 9.0, 0.0) == still
    )
    assert dots(still, flock.REEDS) > 100 and len(still[0]) < 40
    looks = {str(reeds.picture(80, 24, t / 5)) for t in range(40)}
    assert len(looks) > 20
    # A lake: a far shore, ripples, the path of the sun.
    lake = Land(random.Random(5), water=True)
    assert (lake.rows(24), lake.rows(60), lake.rows(2)) == (4, 9, 2)
    assert lake.horizon(24) == 4 * 21 - 2
    dark = lake.picture(80, 24, f=0.7)
    assert len(dark) == 4 and dots(dark[1:], flock.WATER) > 20
    assert dots(dark, (208,)) == 0
    shine = lake.picture(80, 24, sun=(80.0, 7.0, 208), f=0.7)
    path = [x for row in shine[1:] for x, (_, ink) in row.items() if ink == 208]
    assert len(path) > 8 and all(30 <= x <= 50 for x in path)
    assert lake.picture(80, 24, 0.4, sun=(80.0, 7.0, 208), f=0.7) != shine
    # Set for a while: no path.
    assert dots(lake.picture(80, 24, sun=(80.0, 7.0, 196), f=0.99), (196,)) == 0


def test_scene(monkeypatch: pytest.MonkeyPatch) -> None:
    args = options(
        "--seed", "2", "--at", "0.5", "--dusk", "100", monkeypatch=monkeypatch
    )
    scene = Scene(args, random.Random(2))
    assert scene.stage(0.0) == 0.5 and scene.rows(24) == 1
    assert scene.radius(80, 24) == 7.0 and scene.radius(200, 60) == 17.5
    fore = scene.fore(80, 24, 0.0)
    cx, cy, colour = scene.sun(80, 24, 0.0)
    assert 130 < dots(fore, (colour,)) <= 160  # pi * 7 * 7
    birds = dots(fore, flock.BIRDS)
    assert 150 < birds <= 260
    assert scene.fore(80, 24, 0.0) == fore and scene.fore(80, 24, 0.1) != fore
    assert scene.fore(0, 24, 0.0) == [{}] * 24
    # A bird before the sun: dots off in the disc.
    whole = dots(fore, (colour,))
    monkeypatch.setattr(scene, "birds", lambda *a: ([(cx, cy), (cx + 4, cy + 2)], []))
    assert dots(scene.fore(80, 24, 0.0), (colour,)) == whole - 4
    assert dots(scene.fore(80, 24, 0.0), flock.BIRDS) == 0
    monkeypatch.undo()
    # The sun sets behind the land; the sky under it is the land's.
    assert dots(scene.fore(80, 24, 100 * (SET - 0.5)), flock.SUN) == 0
    above = scene.above(80, 24, 20.0)
    assert len(above) == 24 and above[23] == {} and sum(len(row) for row in above) > 100


def test_roost_and_join(monkeypatch: pytest.MonkeyPatch) -> None:
    args = options("--seed", "2", "--at", "0", "--dusk", "100", monkeypatch=monkeypatch)
    scene = Scene(args, random.Random(2))

    def flying(f: float, roosts: list[tuple[int, int]] | None = None) -> list:
        return scene.birds(80, 24, 100 * f, roosts or [])[0]

    # Down at the start, all up after, down again for the night.
    counts = [len(flying(f)) for f in (0.0, 0.005, 0.06, 0.5, 0.9, 0.99, 1.05)]
    assert counts[0] == 0 and 0 < counts[1] < 249 and counts[2] == counts[3] == 249
    assert 0 < counts[4] <= 249 and counts[5] == counts[6] == 0
    # Late, they are low: they pour down.
    assert sum(y for _, y in flying(0.95)) / len(flying(0.95)) > 60
    # Perches: over the text, where the cell is free.
    rows = ["", "", "  roost here", "x"]
    spots = flock.perches(rows, 80, 24)
    assert spots and all(y == 7 and 4 <= x < 24 for x, y in spots)
    assert len(spots) < 20
    assert flock.perches(["top", "under"], 80, 24) == []
    perched = flying(1.0, spots)
    assert perched == [(float(x), float(y)) for x, y in spots]
    grid = scene.fore(80, 24, 100.0, spots)
    assert dots(grid[1:2], flock.BIRDS) == len(spots)
    # Groups come in from the sides, one after the other.
    args = options("--seed", "2", "--at", "0", "--dusk", "100", "-j", monkeypatch=monkeypatch)  # fmt: skip
    scene = Scene(args, random.Random(2))
    counts = [len(flying(f)) for f in (0.07, 0.2, 0.4, 0.7)]
    assert 40 < counts[0] < 120 and counts == sorted(set(counts)) and counts[3] == 249
    # Two flocks.
    args = options("--seed", "2", "-d", monkeypatch=monkeypatch)
    assert len(Scene(args, random.Random(2)).birds(80, 24, 0.0)[0]) == 249 + 87


def test_hawk(monkeypatch: pytest.MonkeyPatch) -> None:
    one = Flock(random.Random(3))
    times = [t / 10 for t in range(600) if flock.hawk(one, 160, 92, t / 10)]
    _, long = flock.HAWK
    assert long * 10 <= len(times) <= 2 * (long + 1) * 10 + 4
    begin = times[0]
    assert len(flock.hawk(one, 160, 92, begin + 2.0)) == 6
    # Through the middle of the flock, downwards.
    ys = [flock.hawk(one, 160, 92, begin + k * long / 4)[-1][1] for k in range(5)]
    assert ys == sorted(ys)
    mid = flock.hawk(one, 160, 92, begin + long / 2)[-1]
    assert mid == pytest.approx(one.middle(160, 92, begin + long / 2), abs=4)
    # The birds keep away from it.
    args = options("--seed", "3", "-k", "--dusk", "3000", "--at", "0.5", monkeypatch=monkeypatch)  # fmt: skip
    scene = Scene(args, random.Random(3))
    stoops = [t / 10 for t in range(600) if scene.birds(80, 24, t / 10)[1]]
    assert stoops
    for t in stoops:
        birds, [(hx, hy)] = scene.birds(80, 24, t)
        assert sum(math.hypot(x - hx, y - hy) < 4 for x, y in birds) <= 2
    grid = scene.fore(80, 24, stoops[len(stoops) // 2])
    assert flock.PREY in {ink for row in grid for _, ink in row.values()}


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert flock.main(["--print", "40x12", "--seed", "1"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert len(out) == 12 and "\x1b[38;5;" in "".join(out)
    assert flock.main(["--print", "-a", "--seed", "1", "--at", "0.7"]) == 0
    assert len(capsys.readouterr().out.splitlines()) == 24
    assert flock.main(["--print", "3x2", "-a"]) == 0
    assert len(capsys.readouterr().out.splitlines()) == 2
    for bad in (
        ["--dusk", "0"],
        ["--birds", "-1"],
        ["--sun", "-1"],
        ["--print", "0x4"],
    ):
        with pytest.raises(SystemExit):
            flock.main(bad)
    capsys.readouterr()


def played(pilot: Pilot, *options: str) -> tuple[int, int]:
    """Run the app 2 s on a pane with two lines of text. The pictures of
    the host seen, and the most sprites at once; the text must show
    whole all along, the land on the last row, and the host be clean
    after."""
    lines = ("words in the way", "   more words here")
    pilot.send("clear; printf '\\n\\n{}\\n\\n{}\\n'\n".format(*lines))
    pilot.wait_for(lines[1].strip(), frame="raw")
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    args = ["-s", pilot.session, "--seed", "4", "--dusk", "40", *options]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(flock.main(args, seconds=2.0))
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


def test_flock(session: Pilot) -> None:
    pictures, sprites = played(session)
    assert pictures > 5 and sprites == 3


def test_flock_all(session: Pilot) -> None:
    pictures, sprites = played(session, "-a", "--at", "0.9")
    assert pictures > 5 and sprites == 3
