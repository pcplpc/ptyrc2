# SPDX-License-Identifier: GPL-3.0-or-later
"""`fishes --tank`: the tank without a driver, the widget on one."""

from __future__ import annotations

import itertools
import random
import threading

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import fishes

MORE = {"decorate": True, "bubbles": True, "species": True}


def plain(tank: fishes.Tank) -> list[str]:
    return ["".join(run["t"] for run in row) for row in tank.rows()]


def swum(tank: fishes.Tank, steps: int = 100) -> fishes.Tank:
    for _ in range(steps):
        tank.step(fishes.TICK)
    return tank


@pytest.mark.parametrize("more", [{}, MORE])
def test_bounds(more: dict[str, bool]) -> None:
    for (w, h), seed in ((size, seed) for size in fishes.TANKS for seed in range(4)):
        tank = fishes.Tank(w, h, random.Random(seed), **more)
        for _ in range(150):
            tank.step(fishes.TICK)
            assert [len(text) for text in plain(tank)] == [w] * h
            low = h - 2 if more else h - 1
            for fish in tank.school:
                x, y, glyph = fish.cell
                assert 0 <= x <= w - len(glyph) and 1 <= y <= low, (w, h, fish)


def test_school() -> None:
    # One fish in a bowl; as many as the water holds, `count` at most.
    sizes = [len(fishes.Tank(w, h, random.Random(1)).school) for w, h in fishes.TANKS]
    assert sizes[0] == 1 and sizes == sorted(sizes) and sizes[-1] == 8
    assert len(fishes.Tank(40, 10, random.Random(1), count=3).school) == 3
    bare = fishes.Tank(40, 10, random.Random(1))
    assert {fish.kind for fish in bare.school} == {fishes.CLASSIC}
    kinds = set()
    for seed in range(10):
        tank = fishes.Tank(40, 10, random.Random(seed), species=True)
        kinds |= {fish.kind.name for fish in tank.school}
    assert len(kinds) >= 5
    bowl = fishes.Tank(9, 4, random.Random(1), species=True)
    assert all(fish.length <= 6 for fish in bowl.school)


def test_swims() -> None:
    tank = fishes.Tank(28, 8, random.Random(2))
    before = [fish.cell for fish in tank.school]
    looks = {str(plain(swum(tank, 5))) for _ in range(30)}
    assert len(looks) > 20
    assert [fish.cell for fish in tank.school] != before
    # The same seed, the same tank.
    a = swum(fishes.Tank(28, 8, random.Random(7), **MORE))
    b = swum(fishes.Tank(28, 8, random.Random(7), **MORE))
    assert a.rows() == b.rows()


def test_water() -> None:
    bare = swum(fishes.Tank(28, 8, random.Random(3)))
    top = plain(bare)[0]
    assert set(top) <= set("~≈ ") and top.strip()
    assert set("".join(plain(bare)[1:])) <= set(fishes.LEFT + fishes.RIGHT + " ")
    # Too low for a surface.
    assert fishes.Tank(12, 3, random.Random(3)).top == 0


def test_decor() -> None:
    tank = fishes.Tank(40, 10, random.Random(4), decorate=True)
    assert len(tank.decor) >= 2
    assert sum(shape.h >= 4 for shape, *_ in tank.decor) <= 1
    spans = [(x, x + shape.w) for shape, x, *_ in tank.decor]
    assert all(a[1] < b[0] for a, b in itertools.pairwise(spans))
    assert all(0 <= a and b <= 40 for a, b in spans)
    sand = plain(tank)[-1]
    assert set(sand) <= set(".,:_ ") and sand.count(" ") < 10
    assert len(fishes.Tank(9, 4, random.Random(4), decorate=True).decor) == 1
    # A decoration in front hides the fish behind it.
    nth = next(i for i, d in enumerate(tank.decor) if len(d[0].frames) == 1)
    shape, x, *_ = tank.decor[nth]
    tank.decor[nth] = (*tank.decor[nth][:3], True, 0)
    feet = [frame[-1] for frame in shape.frames]
    dx = next(i for i in range(shape.w) if feet[0][i] != " ")
    fish = tank.school[0]
    fish.x, fish.y, fish.vx = float(x + dx), float(tank.floor - 1), 0.0
    assert plain(tank)[tank.floor - 1][x + dx] in {row[dx] for row in feet}
    tank.decor[nth] = (*tank.decor[nth][:3], False, 0)
    assert plain(tank)[tank.floor - 1][x + dx] == fish.cell[2][0]


def test_bubbles() -> None:
    tank = fishes.Tank(40, 10, random.Random(5), bubbles=True)
    assert len(tank.vents) == 2
    seen = 0
    for _ in range(300):
        tank.step(fishes.TICK)
        seen += len(tank.air)
        assert all(tank.top - 0.5 <= b.y <= tank.floor for b in tank.air)
    assert seen > 50
    assert swum(fishes.Tank(40, 10, random.Random(5))).air == []


def test_finger() -> None:
    tank = swum(fishes.Tank(40, 10, random.Random(6)), 20)

    def far() -> float:
        return sum(
            abs(f.x + f.length / 2 - 30) + 2 * abs(f.y - 5) for f in tank.school
        ) / len(tank.school)

    roaming = far()
    tank.on_move(30, 5)
    swum(tank, 80)
    assert far() < roaming and far() < 8  # they came
    spots = [fish.cell[:2] for fish in tank.school]
    assert len(set(spots)) == len(spots)  # each at its place
    for fish in tank.school:  # nose to the pointer
        right = fish.cell[2] == fish.kind.glyph(True, fish.cell[0])
        assert right == (fish.x + fish.length / 2 < 30)
    still = [fish.cell for fish in tank.school]
    assert [fish.cell for fish in swum(tank, 10).school] == still
    tank.on_leave()
    swum(tank, 60)
    assert [fish.cell for fish in tank.school] != still  # and went


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert fishes.main(["--tank", "--print", "--seed", "1"]) == 0
    out = capsys.readouterr().out
    assert out.count("┌") == len(fishes.TANKS) and "\x1b" not in out
    assert fishes.main(["--tank", "-a", "--print", "30x9", "--seed", "1"]) == 0
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == 11 and all(len(line) == 32 for line in lines)
    for argv in (["--print"], ["--tank", "--print", "3x3"]):
        with pytest.raises(SystemExit):
            fishes.main(argv)
        capsys.readouterr()


@pytest.mark.parametrize("argv", [[], ["-a"]])
def test_tank(session: Pilot, argv: list[str]) -> None:
    seen: list[dict] = []
    host: list[str] = []
    run = ["-s", session.session, "--tank", "--seed", "1", *argv]
    thread = threading.Thread(target=lambda: fishes.main(run, seconds=3))
    thread.start()
    while thread.is_alive():
        seen += [w for w in session.widgets() if w["state"] == "placed"]
        host += session.dump()["host"]
        session_clock.sleep(0.05)
    thread.join()
    assert seen and {w["tag"] for w in seen} == {"fishes"}
    assert all(tuple(w["size"]) in fishes.TANKS for w in seen)
    assert any("║" in row and "~" in row for row in host)  # glass, water
    assert any(fishes.LEFT in row or fishes.RIGHT in row for row in host) or argv
    assert session.sprites() == [] and session.widgets() == []
