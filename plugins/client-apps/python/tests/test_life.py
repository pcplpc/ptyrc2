# SPDX-License-Identifier: GPL-3.0-or-later
"""`life`: the world, its picture, the app on a headless driver."""

from __future__ import annotations

import random
import threading
import time

from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import _patterns, life
from ptyrc_apps.life import Life


def naive(cells: set[tuple[int, int]], w: int, h: int) -> set[tuple[int, int]]:
    out = set()
    for y in range(h):
        for x in range(w):
            n = sum(
                ((x + dx) % w, (y + dy) % h) in cells
                for dx in (-1, 0, 1)
                for dy in (-1, 0, 1)
                if (dx, dy) != (0, 0)
            )
            if n == 3 or (n == 2 and (x, y) in cells):
                out.add((x, y))
    return out


def test_life_step() -> None:
    rng = random.Random(1)
    for w, h in ((3, 5), (16, 8), (37, 23)):
        world = Life(w, h, ages=4, gone=3)
        world.soup(rng)
        age = dict.fromkeys(world.cells(), 1)
        dead: dict[tuple[int, int], int] = {}
        for _ in range(20):
            was, new = world.cells(), naive(world.cells(), w, h)
            world.step()
            assert world.cells() == new
            assert world.population == len(new)
            age = {c: age.get(c, 0) + 1 for c in new}
            dead = {c: n + 1 for c, n in dead.items() if c not in new}
            dead |= dict.fromkeys(was - new, 1)
            for k, plane in enumerate(world.ages):
                got = {
                    (x, y)
                    for y, row in enumerate(plane)
                    for x in range(w)
                    if row >> x & 1
                }
                assert got == {c for c, a in age.items() if a > k}
            for k, plane in enumerate(world.gone):
                got = {
                    (x, y)
                    for y, row in enumerate(plane)
                    for x in range(w)
                    if row >> x & 1
                }
                assert got == {c for c, n in dead.items() if n == k + 1}


def test_life_patterns() -> None:
    world = Life(12, 12)
    world.put(life.GUESTS["glider"], 1, 1)
    start = world.cells()
    for _ in range(4 * 12):  # once around the torus
        world.step()
    assert world.cells() == start and world.generation == 48
    # A wall: the glider does not get through.
    world = Life(12, 12)
    world.put(life.GUESTS["glider"], 1, 1)
    open_ = [0xFFF if y < 8 else 0 for y in range(12)]
    for _ in range(60):
        world.step(open_)
    assert all(y < 8 for _, y in world.cells())
    assert all(len({len(row) for row in art}) == 1 for art in life.GUESTS.values())
    assert set(life.SHARES) == set(_patterns.KINDS)
    assert sum(life.SHARES.values()) == 100


def shape(world: Life) -> tuple[int, int, frozenset[tuple[int, int]]]:
    """Where the cells are, and their shape from there."""
    cells = world.cells()
    x0, y0 = min(x for x, _ in cells), min(y for _, y in cells)
    return x0, y0, frozenset((x - x0, y - y0) for x, y in cells)


def test_life_library() -> None:
    """Each pattern is what the table says: run, on a torus too large
    for it to meet itself."""
    assert len(_patterns.LIBRARY) == 240
    assert _patterns.decode("bo$2bo$3o!") == life.GUESTS["glider"]
    assert _patterns.decode("o2$2o") == ("O.", "..", "OO")
    for p in _patterns.LIBRARY.values():
        assert p.kind in _patterns.KINDS and p.name == p.name.lower()
        assert len({len(row) for row in p.art}) == 1 and max(p.w, p.h) <= 64
        assert "O" in p.art[0] and "O" in p.art[-1], p.name
        assert any(row[0] == "O" for row in p.art), p.name
        assert any(row[-1] == "O" for row in p.art), p.name
        periodic = p.kind in (_patterns.STILL, _patterns.OSCILLATOR, _patterns.SHIP)
        gens = p.period if periodic else 100 if p.kind == _patterns.SEED else 240
        pad = 16 + (gens if periodic else gens // 2)
        world = Life(p.w + 2 * pad, p.h + 2 * pad)
        world.put(p.art, pad, pad)
        assert world.population == p.cells
        x0, y0, start = shape(world)
        for g in range(1, gens):
            world.step()
            assert world.population or g > 100, p.name
            if periodic and world.population == p.cells:
                assert shape(world)[2] != start, p.name  # no shorter period
            elif g == 8:
                assert shape(world)[2] != start, p.name
        world.step()
        x, y, now = shape(world) if periodic else (0, 0, start)
        if periodic:
            assert now == start and (x - x0, y - y0) == p.move, p.name
            assert (p.move != (0, 0)) == (p.kind == _patterns.SHIP), p.name
            assert (p.period == 1) == (p.kind == _patterns.STILL), p.name
        elif p.kind == _patterns.FILLER:
            assert world.population > 20 * p.cells, p.name
        elif p.kind != _patterns.SEED:  # a gun, a puffer: it grows
            assert world.population > p.cells + 4, p.name
            assert p.period == 0 and p.move == (0, 0)


def test_life_turn() -> None:
    world = Life(40, 40)
    for name in ("glider", "lwss", "weekender", "big-glider"):
        p = _patterns.LIBRARY[name]
        seen = set()
        for k in range(8):
            art = _patterns.turn(p.art, k)
            seen.add(tuple(art))
            world = Life(60, 60)
            world.put(art, 25, 25)
            x0, y0, start = shape(world)
            for _ in range(p.period):
                world.step()
            x, y, now = shape(world)
            assert now == start
            assert (x - x0, y - y0) == _patterns.heading(p.move, k), (name, k)
        assert len(seen) == (4 if name in ("weekender", "big-glider") else 8)
    assert [p.name for p in _patterns.of(_patterns.FILLER)] == ["max", "spacefiller-1"]


def test_life_room() -> None:
    world = Life(40, 24)  # 10 by 6 blocks
    assert life.room(world) == [[10] * 10] * 6
    world.put(["O"], 5, 9)  # the block (1, 2)
    far = life.room(world)
    assert far[2][1] == 0 and far[2][2] == far[1][0] == far[3][2] == 1
    assert far[2][6] == 5 and far[2][7] == 4  # around the torus
    assert far[5][1] == 3 and far[0][1] == 2
    assert max(max(row) for row in far) == 5
    # A closed cell is as a cell alive.
    open_ = [(1 << 40) - 1] * 24
    open_[20] &= ~(1 << 38)
    far = life.room(world, open_)
    assert far[5][9] == 0 and far[5][0] == 1 and far[2][1] == 0
    # A width that is no multiple of the block.
    world = Life(10, 8)
    world.put(["O"], 9, 0)
    assert life.room(world) == [[1, 1, 0], [1, 1, 1]]


def box(world: Life) -> tuple[int, int, int, int]:
    cells = world.cells()
    xs, ys = [x for x, _ in cells], [y for _, y in cells]
    return min(xs), min(ys), max(xs), max(ys)


def test_life_host() -> None:
    # A hole in a world of blocks: the pattern goes there, whole.
    for seed in range(12):
        rng = random.Random(seed)
        world = Life(160, 96)
        for y in range(0, 96, 8):
            for x in range(0, 160, 8):
                if not (64 <= x < 120 and 24 <= y < 72):
                    world.put(["OO", "OO"], x, y)
        blocks = world.cells()
        host = life.Host(rng, ("oscillator", "ship"))
        landed = host.drop(world)
        assert landed is not None and list(host.recent) == [landed.name]
        new = world.cells() - blocks
        assert blocks <= world.cells()
        assert len(new) == _patterns.LIBRARY[landed.name].cells
        assert all(60 <= x < 118 and 20 <= y < 70 for x, y in new), landed
        assert 0 < host.void < 0.5 and 0 < host.calm <= 1
    # An empty world: all of it a hole; large patterns first.
    rng = random.Random(5)
    world = Life(160, 96)
    host = life.Host(rng)
    assert host.wait(2.0) > 1.0  # nothing known of the world: as if full
    n = host.open(world)
    assert 3 < n <= life.OPENING and len(set(host.recent)) == n
    sizes = [max(p.w, p.h) for p in map(_patterns.LIBRARY.get, host.recent)]
    assert sum(size > 12 for size in sizes) > n / 2
    assert 0.25 <= host.wait(2.0) <= 2.0 and host.wait(0) == float("inf")
    host.void = 1.0
    assert 0.25 <= host.wait(2.0) <= 0.75
    host.void = 0.0
    assert 1.0 <= host.wait(2.0) <= 3.0
    # The kinds asked, no other; the same pattern not twice in a row.
    host = life.Host(random.Random(1), ("gun",))
    names = []
    for _ in range(8):
        world = Life(200, 120)
        landed = host.drop(world)
        assert landed is not None
        names.append(landed.name)
    assert {_patterns.LIBRARY[name].kind for name in names} == {"gun"}
    assert len(set(names)) > 4
    assert life.Host(rng, ()).drop(Life(60, 40)) is None
    assert life.Host(rng).drop(Life(0, 0)) is None


def test_life_host_aim_and_seen() -> None:
    # A ship is sent where the way is free: blocks to the right of
    # the hole, above and under it, none to the left.
    for seed in range(8):
        world = Life(160, 96)
        for y in range(0, 96, 4):
            world.put(["OO", "OO"], 100, y)
        for x in range(0, 160, 8):
            world.put(["OO", "OO"], x, 20)
            world.put(["OO", "OO"], x, 76)
        host = life.Host(random.Random(seed), ("ship",))
        far = life.room(world)
        for name in ("lwss", "glider", "copperhead"):
            p = _patterns.LIBRARY[name]
            dx, _ = _patterns.heading(p.move, host.aim(p, far, 22, 12))
            assert dx < 0, (name, seed)
    # Where it is seen rather than under the text; under it when
    # there is no room elsewhere; never where it is closed.
    full = (1 << 160) - 1
    left = [full >> 80] * 96  # the cells x < 80
    for seed in range(8):
        world = Life(160, 96)
        landed = life.Host(random.Random(seed)).drop(world, None, left)
        assert landed is not None and box(world)[2] < 80
        world = Life(160, 96)
        host = life.Host(random.Random(seed))
        assert host.drop(world, left, [0] * 96) is not None and box(world)[2] < 80
        assert host.drop(world, [0] * 96, left) is None


def test_life_host_squeeze() -> None:
    # Sparse ashes, no hole for a gun: it takes the place of a few.
    rng = random.Random(3)
    world = Life(160, 96)
    for y in range(0, 96, 16):
        for x in range(0, 160, 16):
            world.put(["OO", "OO"], x, y)
    n = world.population
    host = life.Host(rng, ("gun",))
    landed = host.drop(world)
    assert landed is not None and host.calm > 0
    p = _patterns.LIBRARY[landed.name]
    assert n + p.cells - 4 * 30 < world.population < n + p.cells
    # No hole at all, few cells: a small one over them. Too many: none.
    world = Life(64, 64)
    for y in range(0, 64, 4):
        for x in range(0, 64, 4):
            world.put(["O"], x + 1, y + 1)
    assert max(max(row) for row in life.room(world)) == 0
    host = life.Host(rng)
    landed = host.drop(world)
    assert landed is not None
    p = _patterns.LIBRARY[landed.name]
    assert max(p.w, p.h) <= life.SMALL and p.kind in ("ship", "seed")
    assert host.void == 0 and host.calm == 0
    # A world not calm: the lively kinds wait.
    assert host.weight(_patterns.LIBRARY["acorn"], 1.0) == 0
    assert host.weight(_patterns.LIBRARY["pulsar"], 1.0) == 1.0
    host.recent.append("pulsar")
    assert host.weight(_patterns.LIBRARY["pulsar"], 1.0) == life.STALE


def test_life_options() -> None:
    assert life._kinds("ship, gun") == ("ship", "gun")
    assert set(life._kinds("all")) == set(_patterns.KINDS)
    assert "filler" not in life.KINDS
    for bad in (["--kinds", "fish"], ["--kinds", ","], ["--rate", "-1"]):
        try:
            life.main(["-g", *bad])
        except SystemExit as stop:
            assert stop.code == 2
        else:
            raise AssertionError(bad)


def test_life_resize_and_room() -> None:
    world = Life(8, 8, ages=2)
    world.put(["OO", "OO"], 6, 6)
    assert world.empty(0, 0, 6, 6) and not world.empty(5, 5, 2, 2)
    assert not world.empty(7, 0, 2, 8)  # it wraps
    world.resize(7, 12)
    assert world.cells() == {(6, 6), (6, 7)} and len(world.ages[1]) == 12
    rng = random.Random(2)
    world = Life(60, 40)
    at = life.drop(world, rng)
    assert at is not None and world.population >= 5
    # No room in a full world, none in a closed one.
    world.soup(rng)
    n = world.population
    assert life.drop(world, rng) is None and world.population == n
    assert life.drop(Life(60, 40), rng, [0] * 40) is None
    art = life.GUESTS["lwss"]
    turns = {tuple(life.turned(art, random.Random(s))) for s in range(60)}
    assert len(turns) == 8 and all(sum(r.count("O") for r in t) == 9 for t in turns)


def test_life_watch() -> None:
    world = Life(40, 40)
    world.put(life.GUESTS["pulsar"], 3, 3)
    world.put(["OOO"], 30, 30)
    world.put(life.GUESTS["glider"], 22, 2)
    watch_ = life.Watch()
    stills = [world.step() or watch_.still(world.population) for _ in range(300)]
    assert stills[life.STILL + life.QUIET] and not any(stills[: life.QUIET])
    world = Life(64, 64)
    world.put(life.GUESTS["r-pentomino"], 30, 30)
    watch_.reset()
    assert not any(world.step() or watch_.still(world.population) for _ in range(150))
    assert life.Watch().still(0)


def test_life_picture() -> None:
    world = Life(8, 8, ages=len(life.AGES), gone=len(life.TRAIL))
    world.put(["OO", "OO"], 0, 0)  # a block: still
    world.put(["O"], 5, 5)  # dies alone
    assert life.picture(world) == [
        [{"t": "⠛", "fg": "green"}],
        [{"t": "  ⠐", "fg": "green"}],
    ]
    world.step()
    assert life.picture(world) == [[{"t": "⠛", "fg": "green"}], [{"t": " "}]]
    assert life.picture(world, True, True) == [
        [{"t": "⠛", "fg": life.AGES[1]}],
        [{"t": "  ⠐", "fg": life.TRAIL[0]}],
    ]
    for _ in range(3):
        world.step()
    assert life.picture(world, True, True)[1] == [{"t": "  ⠐", "fg": life.TRAIL[3]}]
    for _ in range(10):
        world.step()
    assert life.picture(world, True, True) == [
        [{"t": "⠛", "fg": life.AGES[-1]}],
        [{"t": " "}],
    ]
    assert life.BRAILLE[0b11111111] == "⣿" and life.BRAILLE[0b10000000] == "⢀"


def test_life_wall_and_blasts() -> None:
    open_ = life.wall(["", "  ab"], 6, 2)
    assert open_ == [0xFFF] * 4 + [0b110000000011] * 4
    assert life.wall(["a世"], 5, 1)[0] == 0b1100000000
    assert life.wall(["a  b"], 4, 1)[0] == 0
    world = Life(8, 8)
    world.put(["OO", "OO", "OO", "OO"], 2, 4)
    world.put(["OO", "OO"], 6, 0)
    assert life.picture(world, shown=[0xF0] * 8) == [
        [{"t": "   ⠛", "fg": "green"}],
        [{"t": " "}],
    ]
    world.step([0xFF] * 4 + [0] * 4)
    assert life.blasts(world) == [(1, 1)]


def dots(host: str) -> int:
    return sum(0x2800 < ord(c) < 0x2900 for c in host)


def played(pilot: Pilot, *options: str) -> int:
    """Run the app 1.5 s on a pane with two lines of text. The most
    braille glyphs seen at once; the text must show whole all along and
    the host be clean after."""
    lines = ("words in the way", "   more words here")
    pilot.send("clear; printf '\\n\\n{}\\n\\n{}\\n'\n".format(*lines))
    pilot.wait_for(lines[1].strip(), frame="raw")
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    args = ["-s", pilot.session, "--seed", "4", "--speed", "30", *options]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(life.main(args, seconds=1.5))
    )
    thread.start()
    most = 0
    while thread.is_alive():
        host = "\n".join(pilot.dump()["host"])
        assert all(line.strip() in host for line in lines)
        most = max(most, dots(host))
        session_clock.sleep(0.02)
    thread.join()
    assert status == [0]
    deadline = time.monotonic() + 3
    while pilot.sprites() and time.monotonic() < deadline:
        session_clock.sleep(0.02)
    assert pilot.sprites() == []
    assert pilot.dump()["host"] == pilot.text()
    return most


def test_life(session: Pilot) -> None:
    assert played(session) > 100


def test_life_all(session: Pilot) -> None:
    assert played(session, "-a") > 100
    assert played(session, "-g", "--rate", "8", "--kinds", "all") > 20
    assert played(session, "-g", "-w", "--rate", "0") > 20
    assert played(session, "-c", "-t", "--front") > 100
