# SPDX-License-Identifier: GPL-3.0-or-later
"""`shore`: the waves, the tide, what they leave, the app on a headless
driver."""

from __future__ import annotations

import math
import random
import threading
import time
from typing import TYPE_CHECKING

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import shore
from ptyrc_apps._strip import Pointer
from ptyrc_apps.shore import Shore

if TYPE_CHECKING:
    from conftest import Host


def inks(grid: shore.Grid) -> set[int]:
    return {ink for row in grid for _, ink in row.values()}


def glyphs(grid: shore.Grid) -> set[str]:
    return {glyph for row in grid for glyph, _ in row.values()}


def test_swash() -> None:
    assert shore.swash(0.0) == shore.swash(1.0) == shore.swash(-1.0) == 0.0
    assert shore.swash(shore.PEAK) == pytest.approx(1.0)
    up = [shore.swash(shore.PEAK * i / 10) for i in range(11)]
    back = [shore.swash(shore.PEAK + (1 - shore.PEAK) * i / 10) for i in range(11)]
    assert up == sorted(up) and back == sorted(back, reverse=True)


def test_tide() -> None:
    mid = shore.tide(0.0, 300.0, 24)
    assert mid == pytest.approx(96 * (shore.HIGH + shore.LOW) / 2)
    assert shore.tide(75.0, 300.0, 24) == pytest.approx(96 * shore.HIGH)
    assert shore.tide(225.0, 300.0, 24) == pytest.approx(96 * shore.LOW)
    assert shore.tide(300.0, 300.0, 24) == pytest.approx(mid)


def test_light() -> None:
    assert shore.light(0.0, 240.0) == (0, 0.0)
    assert shore.light(0.2 * 240, 240.0) == (1, 0.0)
    assert shore.light(0.4 * 240, 240.0) == (2, 0.0)
    palette, blend = shore.light(0.17 * 240, 240.0)
    assert palette == 0 and 0.0 < blend < 1.0
    assert shore.light(240.0, 240.0) == shore.light(0.0, 240.0)


def test_lace() -> None:
    net = shore.lace()
    assert net is shore.lace() and len(net) == len(net[0]) == shore.TILE
    flat = sorted(v for row in net for v in row)
    # As many dots under `q` as `q` says.
    assert flat[0] == 0.0 and flat[len(flat) // 4] == pytest.approx(0.25)


def test_picture_is_a_function_of_the_time() -> None:
    one, other = Shore(random.Random(1)), Shore(random.Random(1))
    late = one.picture(60, 20, 13.0)
    assert one.picture(60, 20, 2.0) != late
    assert other.picture(60, 20, 13.0) == late
    assert Shore(random.Random(2)).picture(60, 20, 13.0) != late
    assert one.picture(0, 0) == [] and one.picture(1, 1, 5.0) is not None


def test_sand_over_sea() -> None:
    grid = Shore(random.Random(1)).picture(80, 24, 3.0)
    dry = {ink[0] for ink in shore.SAND}
    sea = {ink[0] for ink in shore.SEA + shore.SHALLOW}
    assert inks(grid[:6]) <= dry
    assert inks(grid[-2:]) & sea and not inks(grid[-2:]) & dry


def test_a_wave_runs_up_and_back() -> None:
    one = Shore(random.Random(1), ebb=1e9)
    wave = one.wave(2, 160, 24)
    assert wave is one.wave(2, 160, 24)
    still = shore.tide(0.0, 1e9, 24)
    at = wave.at + wave.late[80]
    stay = shore.STAY * one.period
    assert one.edge(80, at - 0.1, 160, 24) <= still
    top = one.edge(80, at + shore.PEAK * stay, 160, 24)
    assert top == pytest.approx(still - wave.run[80])
    assert top < one.edge(80, at + 0.7 * stay, 160, 24) < still


def test_a_wave_leaves_a_line() -> None:
    one = Shore(random.Random(1), ebb=1e9)
    stay = shore.STAY * one.period
    wave = one.wave(2, 160, 24)
    lines = {ink[0] for ink in shore.LINE}
    before = one.picture(80, 24, wave.at - 0.5 * one.period)
    after = one.picture(80, 24, wave.at + 0.9 * stay)
    rows = {int(shore.tide(0.0, 1e9, 24) - run) >> 2 for run in wave.run}
    assert any(inks(after[y : y + 1]) & lines for y in rows)
    assert before != after
    # The foam of a wave on the sand.
    up = one.picture(80, 24, wave.at + 0.2 * stay)
    assert shore.FOAM[0][0] in inks(up)


def test_froth_on_the_rocks() -> None:
    rocks = shore.rocks_of(["", "", " ab"], 8, 4)
    assert rocks == [(x, y) for x in (0, 1, 2, 3) for y in (1, 3)]
    assert shore.rocks_of(["ab", "cd"], 2, 2) == []
    one = Shore(random.Random(1), grains=0)
    low = [(x, 22) for x in range(80)]
    bare = one.picture(80, 24, 3.0)
    assert one.picture(80, 24, 3.0, low) != bare
    # No froth on dry sand.
    assert one.picture(80, 24, 3.0, [(x, 2) for x in range(80)]) == bare


def test_extras() -> None:
    plain = Shore(random.Random(1)).picture(80, 24, 30.0)
    assert not glyphs(plain) & set("ж›‹,•")
    full = Shore(
        random.Random(1), drift=True, ripples=True, crabs=True, birds=True, walker=True
    )
    seen: set[str] = set()
    for t in range(0, 60, 3):
        seen |= glyphs(full.picture(80, 24, float(t)))
    assert "ж" in seen and seen & set("›‹") and seen & {g for g, _, _ in shore.FINDS}
    for kind in ("drift", "ripples", "walker"):
        one = Shore(random.Random(1), **{kind: True})
        assert one.picture(80, 24, 30.0) != plain


def found(grid: shore.Grid, wanted: str) -> list[tuple[int, int]]:
    return [
        (x, y) for y, row in enumerate(grid) for x, (g, _) in row.items() if g in wanted
    ]


def test_scare() -> None:
    now = [10.0]
    pointer = Pointer(lambda: now[0])
    assert shore.scare(None, 10.0) is None and shore.scare(pointer, 10.0) is None
    pointer.move(7, 3)
    assert shore.scare(pointer, 10.0) == (7, 3, 1.0)
    assert shore.scare(pointer, 10.0 + shore.STILL) == (7, 3, 1.0)
    x, y, much = shore.scare(pointer, 10.0 + shore.STILL + shore.FADE / 2)
    assert (x, y) == (7, 3) and much == pytest.approx(0.5)
    assert shore.scare(pointer, 10.0 + shore.STILL + shore.FADE) is None
    now[0] = 11.0
    pointer.leave()
    assert shore.scare(pointer, 11.0 + shore.FADE / 2)[2] == pytest.approx(0.5)


def test_the_animals_flee_the_pointer() -> None:
    now = [14.0]
    frame = 1 / 8  # seconds from a picture to the next, here
    pointer = Pointer(lambda: now[0])
    one = Shore(random.Random(3), birds=True, crabs=True)
    alone = Shore(random.Random(3), birds=True, crabs=True)

    def step(to: float) -> shore.Grid:
        grid = one.picture(80, 24, now[0], (), pointer)
        while now[0] < to:
            now[0] += frame
            grid = one.picture(80, 24, now[0], (), pointer)
        return grid

    # No pointer, or one that never came: the scene of the time.
    grid = step(14.0)
    assert grid == alone.picture(80, 24, 14.0)
    birds, crabs = found(grid, "›‹,"), found(grid, "ж")
    assert len(birds) >= 3 and crabs
    # On a bird: those about it fly, and land away from it.
    bx, by = birds[0]
    pointer.move(bx, by)
    seen: set[str] = set()
    for _ in range(12):
        seen |= glyphs(step(now[0] + frame))
    assert seen & set("v^")
    grid = step(now[0] + 1.0)
    assert not found(grid, "v^")
    for x, y in found(grid, "›‹,"):
        assert math.hypot(x - bx, 2 * (y - by)) >= shore.NEAR
    # On a crab: it runs, and stays away.
    cx, cy = found(grid, "ж")[0]
    pointer.move(cx, cy)
    for _ in range(8):
        grid = step(now[0] + frame)
        pointer.move(cx, cy)
        assert (cx, cy) not in found(grid, "ж")
    assert set(found(grid, "ж")) and not glyphs(grid) & set(".¨")
    for x, y in found(grid, "ж"):
        assert math.hypot(x - cx, 2 * (y - cy)) >= shore.SHY - 1
    # It left: they come back, and the scene is that of the time again.
    pointer.leave()
    step(now[0] + 60.0)
    assert step(now[0]) == alone.picture(80, 24, now[0])
    # A jump of the time: what it did to them is forgotten.
    pointer.move(*found(step(now[0]), "›‹,")[0])
    step(now[0] + 1.0)
    assert one.picture(80, 24, 500.0) == alone.picture(80, 24, 500.0)


def test_night() -> None:
    one = Shore(random.Random(1), day=240.0)
    day = inks(one.picture(80, 24, 0.0))
    night = inks(one.picture(80, 24, 0.45 * 240))
    assert shore.FOAM[0][0] in day and shore.MOON[0][2] not in day
    assert {ink[2] for ink in shore.MOON} & night
    assert not {ink[0] for ink in shore.SAND} - {ink[2] for ink in shore.SAND} & night


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert shore.main(["--print", "40x12", "--seed", "1"]) == 0
    out = capsys.readouterr().out
    assert len(out.splitlines()) == 12 and "\x1b[38;5;" in out
    assert shore.main(["--print", "-a", "--seed", "1", "--at", "100"]) == 0
    assert len(capsys.readouterr().out.splitlines()) == 24
    for bad in (
        ["--waves", "0"],
        ["--tide", "-1"],
        ["--sand", "-1"],
        ["--print", "0x4"],
    ):
        with pytest.raises(SystemExit):
            shore.main(bad)
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
    args = ["-s", pilot.session, "--seed", "4", "--waves", "2", *options]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(shore.main(args, seconds=2.0))
    )
    thread.start()
    seen: set[str] = set()
    most = 0
    while thread.is_alive():
        rows = pilot.dump()["host"]
        host = "\n".join(rows)
        assert all(line.strip() in host for line in lines)
        most = max(most, len(pilot.sprites()))
        if sum(0x2800 < ord(c) < 0x2900 for c in host) > 40:
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


def test_shore(session: Pilot) -> None:
    pictures, sprites = played(session)
    assert pictures > 3 and sprites == 1


def test_shore_all(session: Pilot) -> None:
    pictures, sprites = played(session, "-a")
    assert pictures > 3 and sprites == 1


def test_the_host_mouse_over_the_beach(hosted: tuple[Host, Pilot]) -> None:
    host, pilot = hosted
    args = ["-s", pilot.session, "--seed", "3", "--at", "14", "-b", "-c"]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(shore.main(args, seconds=4.0))
    )
    thread.start()

    def birds() -> list[tuple[int, int]]:
        rows = pilot.dump()["host"]
        return [(x, y) for y, row in enumerate(rows) for x, c in enumerate(row) if c in "›‹,"]  # fmt: skip

    deadline = time.monotonic() + 5
    while len(found := birds()) < 3:
        assert time.monotonic() < deadline, "no sandpiper"
        time.sleep(0.02)
    host.wait_seen(b"\x1b[?1003h")
    x, y = found[0]
    host.move(x, y)
    flown = False
    while thread.is_alive():
        flown |= any(c in row for row in pilot.dump()["host"] for c in "v^")
        time.sleep(0.02)
    thread.join()
    assert status == [0] and flown
