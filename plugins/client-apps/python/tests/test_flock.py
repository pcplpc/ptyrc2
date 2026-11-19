# SPDX-License-Identifier: GPL-3.0-or-later
"""`flock`: the dusk, the birds, the land, the app on a headless driver."""

from __future__ import annotations

import argparse
import itertools
import math
import random
import threading
import time

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import _cli, flock
from ptyrc_apps.flock import REST, SET, Air, Dusk, Flock, Land, Scene


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


def sky(
    w: int = 80, over: int = 23, at: float = 0.4, dusk: float = 3000.0
) -> flock.Air:
    """A sky over flat land, at the stage `at`, a long dusk."""
    return Air(w, over, lambda t: flock.stage(t, dusk, at), dusk, lambda px: 4 * over)


def test_flight() -> None:
    one = Flock(random.Random(3))
    small = one.fly(sky(40, 12), 5.0)
    assert len(small) == one.count(40, 12) == 25 and one.count(80, 24) == 100
    assert one.fly(sky(40, 12), 5.0) == small
    # A flock just made is a function of the time.
    assert Flock(random.Random(3)).fly(sky(40, 12), 5.0) == small
    assert Flock(random.Random(4)).fly(sky(40, 12), 5.0) != small
    assert one.fly(sky(40, 12), 5.1) != small
    assert Flock(random.Random(3), birds=0).fly(sky(), 5.0) == []
    # The first birds of one list at every size.
    # More, in a larger view: up from the land at the next step.
    assert len(one.fly(sky(), 5.1)) == 25 and len(one.fly(sky(), 5.2)) == 95
    assert len(one.fly(sky(40, 12), 5.2)) == 25 and len(one.birds) == 95
    for w, over in ((80, 23), (200, 58)):
        air = sky(w, over)
        one = Flock(random.Random(3))
        u = one.reach(2 * w, 4 * over)
        now = one.fly(air, 0.0)
        inside = cells = far = 0
        for k in range(1, 400):
            after = one.fly(air, k / 10)
            steps = [math.hypot(a - x, b - y) for (x, y), (a, b) in zip(now, after, strict=True)]  # fmt: skip
            # A bird flies on, without a jump; none stops.
            assert max(steps) < 2.5 * flock.PACE * u * flock.DT
            assert sum(steps) / len(steps) > 0.3 * flock.PACE * u * flock.DT
            now = after
            inside += sum(0 <= x < 2 * w and 0 <= y < 4 * over for x, y in now)
            cells += len({(int(x) >> 1, int(y) >> 2) for x, y in now})
            mx = sorted(x for x, _ in now)[len(now) // 2]
            my = sorted(y for _, y in now)[len(now) // 2]
            far += sum(math.hypot(x - mx, y - my) > 1.5 * u for x, y in now)
        total = 399 * len(now)
        # Most in view; loose: few share a cell; together, less a few.
        assert inside > 0.85 * total
        assert cells > 0.5 * total
        assert 0.002 * total < far < 0.15 * total
    # They fly as one: the same way.
    up = range(len(one.x))
    vx, vy = sum(one.vx) / len(up), sum(one.vy) / len(up)
    speed = sum(math.hypot(one.vx[i], one.vy[i]) for i in up) / len(up)
    assert math.hypot(vx, vy) > 0.4 * speed
    # A view of another size: the same birds, where they were in it.
    wide = one.fly(sky(100, 58), 39.9)
    assert len(wide) == one.count(100, 58)
    assert [x for x, _ in wide] == pytest.approx([x / 2 for x, _ in now][: len(wide)])


def test_split_and_scare() -> None:
    one = Flock(random.Random(3))
    air = sky()
    u = one.reach(160, 92)
    apart = 0.0
    seen = set()
    for k in range(2000):
        one.fly(air, k / 10)
        seen.add(one.rift)
        tribes = set(one.tribe)
        assert (len(tribes) > 1) == (one.rift is not None)
        if one.rift is not None:
            mine = [i for i, tribe in enumerate(one.tribe) if tribe]
            rest = [i for i, tribe in enumerate(one.tribe) if not tribe]
            assert 0.15 * 95 < len(mine) < 0.55 * 95
            mid = [
                (sum(one.x[i] for i in part) / len(part), sum(one.y[i] for i in part) / len(part))
                for part in (mine, rest)
            ]  # fmt: skip
            apart = max(apart, math.dist(*mid))
    # Now and then a part goes its own way, and comes back.
    assert None in seen and 2 <= len(seen) <= 5 and apart > 1.5 * u
    # What scares them: they leave the place.
    calm, wary = Flock(random.Random(3)), Flock(random.Random(3))
    at = calm.fly(air, 20.0)
    assert wary.fly(air, 20.0) == at
    x = sorted(x for x, _ in at)[len(at) // 2]
    y = sorted(y for _, y in at)[len(at) // 2]

    def near(birds: list[tuple[float, float]]) -> int:
        return sum(math.hypot(a - x, b - y) < 0.4 * u for a, b in birds)

    assert near(at) > 23
    for k in range(201, 213):  # drawn a step late
        here, there = calm.fly(air, k / 10), wary.fly(air, k / 10, (x, y, 1.0))
    assert near(there) <= 3 < near(here)
    # A pointer that rests scares less far.
    for k in range(213, 300):
        there = wary.fly(air, k / 10, (x, y, flock.STILL))
        assert sum(math.hypot(a - x, b - y) < 0.2 * u for a, b in there) <= 6


def test_glide() -> None:
    # Drawn between two steps: a smooth curve, a frame a short way on.
    assert flock._glide(0.0, 1.0, 2.0, 0.0) == 0.5
    assert flock._glide(0.0, 1.0, 2.0, 1.0) == 1.5
    assert flock._glide(3.0, 3.0, 3.0, 0.4) == 3.0
    one, air = Flock(random.Random(3)), sky()
    u = one.reach(160, 92)
    frames = [one.fly(air, 10.0 + j * flock.PERIOD) for j in range(300)]
    assert one.k == 199
    hops = [
        math.dist(a, b)
        for now, after in itertools.pairwise(frames)
        for a, b in zip(now, after, strict=True)
    ]
    pace = flock.PACE * u * flock.PERIOD
    assert max(hops) < 2.5 * pace < 1.5 and sum(hops) / len(hops) > 0.5 * pace
    # No turn at a step: the speed of a bird changes little in a frame.
    for a, b, c in zip(frames, frames[1:], frames[2:], strict=False):
        for (ax, ay), (bx, by), (cx, cy) in zip(a, b, c, strict=True):
            assert math.hypot(cx - 2 * bx + ax, cy - 2 * by + ay) < 0.25 * pace


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
    assert 62 < birds <= 100
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
    assert counts[0] == 0 and 0 < counts[1] < 95 and counts[2] == counts[3] == 95
    assert 0 < counts[4] <= 95 and counts[5] == counts[6] == 0
    # They take off from all along the land, low, and gather.
    early = flying(0.02)
    xs = sorted(x for x, _ in early)
    assert xs[-1] - xs[0] > 80 and len(early) > 23

    def spread(f: float) -> float:
        """Of the middle four birds in five, across."""
        xs = sorted(x for x, _ in flying(f))
        return xs[9 * len(xs) // 10] - xs[len(xs) // 10]

    assert spread(0.03) > 1.5 * min(spread(f) for f in (0.3, 0.5, 0.7))
    # At dusk they go down apart: over the whole land, not in a column.
    landed: dict[int, float] = {}
    state = scene.flock.state
    for k in range(850, 990):
        flying(k / 1000)
        for i, s in enumerate(state):
            if s == flock.DOWN and i not in landed:
                landed[i] = scene.flock.x[i]
    assert len(landed) == 95
    sites = {int(x * flock.SITES / 160) for x in landed.values()}
    assert len(sites) >= flock.SITES - 2
    assert max(landed.values()) - min(landed.values()) > 120
    # A bird glides down, when the dusk gives it the time.
    one = Flock(random.Random(3))
    sink = []
    for k in range(300):
        one.fly(sky(at=0.855, dusk=300.0), k / 10)
        sink += [
            one.vy[i] / math.hypot(one.vx[i], one.vy[i])
            for i, s in enumerate(one.state)
            if s == flock.IN
        ]
    assert len(sink) > 1500 and 0.3 < sum(sink) / len(sink) < 0.55
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
    assert 15 < counts[0] < 47 and counts == sorted(set(counts)) and counts[3] == 95
    # Those not in yet have no perch; all have one at night.
    assert len(flying(0.07, spots)) == counts[0]
    assert len(flying(1.0, spots)) == len(spots)
    # Two flocks: each after its own lead.
    args = options("--seed", "2", "-d", monkeypatch=monkeypatch)
    scene = Scene(args, random.Random(2))
    assert len(scene.birds(80, 24, 0.0)[0]) == 95 + 34
    second = sum(scene.flock.tribe)
    assert 18 < second < 50


def test_hawk(monkeypatch: pytest.MonkeyPatch) -> None:
    args = options("--seed", "3", "-k", "--dusk", "3000", "--at", "0.4", monkeypatch=monkeypatch)  # fmt: skip
    scene = Scene(args, random.Random(3))
    u = scene.flock.reach(160, 92)
    hunts: list[list[tuple[float, float]]] = []
    was: list[tuple[float, float]] = []
    close = 0
    for k in range(3000):
        birds, hawk = scene.birds(80, 24, k / 10)
        if hawk and not was:
            hunts.append([])
            still = birds
        if hawk:
            hunts[-1] += hawk
            (hx, hy) = hawk[0]
            close = max(close, sum(math.hypot(x - hx, y - hy) < 4 for x, y in birds))
            if was:
                # It flies on, faster than they do.
                step = math.dist(hawk[0], was[0])
                assert 1.5 * flock.PACE * u * flock.DT < step < 2.3 * flock.PACE * u * flock.DT  # fmt: skip
            if len(hunts[-1]) == 12:
                grid = scene.fore(80, 24, k / 10)
                assert flock.PREY in {ink for row in grid for _, ink in row.values()}
        was = hawk
    # Now and then, not for long; it comes from over the view.
    every, long = flock.HAWK
    assert 2 <= len(hunts) <= 300 / every + 1
    assert all(10 < len(hunt) <= 10 * long + 1 for hunt in hunts)
    assert all(hunt[0][1] < 0 for hunt in hunts)
    # It goes for the thick of the flock, and they keep away from it.
    mx = sum(x for x, _ in still) / len(still)
    my = sum(y for _, y in still) / len(still)
    assert min(math.hypot(x - mx, y - my) for x, y in hunts[-1]) < u
    assert close <= 5
    # No hawk without the option, nor as they go to roost.
    args = options("--seed", "3", "-k", "--dusk", "3000", "--at", "0.9", monkeypatch=monkeypatch)  # fmt: skip
    scene = Scene(args, random.Random(3))
    assert not any(scene.birds(80, 24, k / 10)[1] for k in range(0, 2000, 2))


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
    # The lines of a former play cleared first: the typed command shows
    # them too, a `clear` after it would hide them during the play.
    pilot.send("clear\n")
    pilot.wait_for(lines[1].strip(), gone=True, frame="raw")
    pilot.send("printf '\\n\\n{}\\n\\n{}\\n'\n".format(*lines))
    pilot.wait_for(lines[1].strip(), frame="raw")
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline and not (
        pilot.panes()
        and all(s.strip() in "\n".join(pilot.dump()["host"]) for s in lines)
    ):
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


# Pictures counted by polling: real time gives the poll its share.
@pytest.mark.realtime
def test_flock(session: Pilot) -> None:
    pictures, sprites = played(session)
    assert pictures > 5 and sprites == 3


# Pictures counted by polling: real time gives the poll its share.
@pytest.mark.realtime
def test_flock_all(session: Pilot) -> None:
    pictures, sprites = played(session, "-a", "--at", "0.9")
    assert pictures > 5 and sprites == 3
