# SPDX-License-Identifier: GPL-3.0-or-later
"""`life`: the world, its picture, the app on a headless driver."""

from __future__ import annotations

import random
import threading
import time

from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import life
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
    assert set(life.WEIGHTS) <= set(life.GUESTS)


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
    assert played(session, "-c", "-t", "--front") > 100
