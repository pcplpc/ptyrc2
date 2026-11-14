# SPDX-License-Identifier: GPL-3.0-or-later
"""`train`: the way, the scene without a driver, the widget on one."""

from __future__ import annotations

import itertools
from dataclasses import replace
from datetime import datetime

import pytest
from ptyrc import Pilot
from ptyrc_apps import train
from ptyrc_apps._canvas import Half, hexa
from ptyrc_apps._strip import Pointer
from ptyrc_apps.train import CYCLE, PACE, Trip

from .test_window import check, pixels, running

NOON = datetime(2026, 7, 20, 12, 30)
NIGHT = datetime(2026, 7, 20, 23, 30)
ALL = Trip(seed=4, land=True, poles=True, events=True, seasons=True, weather="day")


def cycle(trip: Trip, what: str | None) -> int:
    """The first cycle of `trip` with the event `what`."""
    return next(k for k in range(200) if train.event(trip, k) == what)


def test_events() -> None:
    assert all(train.event(Trip(seed=4), k) is None for k in range(100))
    drawn = [train.event(ALL, k) for k in range(300)]
    assert set(drawn) == {*train.EVENTS, None}
    assert drawn == [train.event(ALL, k) for k in range(300)]
    assert drawn != [train.event(replace(ALL, seed=5), k) for k in range(300)]
    assert 0.05 < drawn.count(None) / 300 < 0.3


def test_dist() -> None:
    bare = Trip(seed=4)
    assert train.dist(bare, 0) == 0 and train.dist(bare, -5) == 0
    assert train.dist(bare, 1000) == pytest.approx(1000 * PACE)
    # It never goes back, and never faster than its pace.
    steps = [train.dist(ALL, t / 4) for t in range(int(40 * CYCLE))]
    gaps = [b - a for a, b in itertools.pairwise(steps)]
    assert min(gaps) >= 0 and max(gaps) <= PACE / 4 + 1e-6
    # At a station it stops, for `STAND` seconds.
    start = cycle(ALL, "station") * CYCLE + train.WHEN
    halt = start + train.BRAKE
    assert train.dist(ALL, halt) == train.dist(ALL, halt + train.STAND)
    assert train.dist(ALL, halt - 0.5) < train.dist(ALL, halt)
    assert train.dist(ALL, halt + train.STAND + 0.5) > train.dist(ALL, halt)
    lost = train.BRAKE + train.STAND
    after = (cycle(ALL, "station") + 1) * CYCLE
    assert train.dist(ALL, after) - train.dist(ALL, start) == pytest.approx(
        PACE * (after - start - lost)
    )
    # A long trip costs no deep recursion.
    assert train.dist(ALL, 30000 * CYCLE) > 0


@pytest.mark.parametrize("line", train.LINES)
@pytest.mark.parametrize("trip", [Trip(seed=4), ALL], ids=["bare", "all"])
def test_fits(line: str, trip: Trip) -> None:
    trip = replace(trip, line=line)
    for w, h in (*train.SIZES, (1, 1), (5, 2), (70, 16)):
        for hour in range(0, 24, 4):
            when = NOON.replace(hour=hour)
            rows = train.scene(w, h, when, 41.3, trip)
            check(rows, w, h)
            assert rows == train.scene(w, h, when, 41.3, trip)  # pure


def test_it_goes() -> None:
    bare = Trip(seed=4)
    frames = [pixels(train.scene(48, 11, NOON, t / 10, bare)) for t in range(30)]
    assert len({tuple(f) for f in frames}) >= 25
    # The sky stands still; the verge runs.
    top = {tuple(f[:48]) for f in frames}
    low = {tuple(f[5 * 48 : 9 * 48]) for f in frames}
    assert len(top) == 1 and len(low) >= 15
    # The seconds of the time of the place change nothing.
    assert train.scene(48, 11, NOON, 3.0, bare) == train.scene(
        48, 11, NOON.replace(second=40), 3.0, bare
    )
    assert train.scene(48, 11, NOON, 3.0, bare) != train.scene(48, 11, NIGHT, 3.0, bare)


def test_options_add() -> None:
    bare = Trip(seed=4)
    seen = {tuple(pixels(train.scene(48, 11, NOON, 5.0, bare)))}
    for more in (
        {"land": True},
        {"poles": True},
        {"weather": "rain"},
        {"line": "alps"},
        {"deep": True},
    ):
        seen.add(tuple(pixels(train.scene(48, 11, NOON, 5.0, replace(bare, **more)))))
    assert len(seen) == 6
    winter = NOON.replace(month=1)
    assert train.scene(48, 11, winter, 0.0, bare) != train.scene(
        48, 11, winter, 0.0, replace(bare, seasons=True)
    )
    check(train.scene(48, 11, NOON, 0.0, replace(ALL, deep=True)), 48, 11, deep=True)
    wires = pixels(train.scene(48, 11, NOON, 5.0, replace(bare, poles=True)))
    assert sum(0x2800 < ord(c) < 0x2900 for c, _, _ in wires) > 30


def test_wires() -> None:
    trip = replace(ALL, weather="clouds", events=False, deep=True)
    wood = hexa(train._sky.at(NOON, 47.0, 0.25).shade((92, 66, 44)))
    for step in range(0, 240, 7):
        cells = pixels(train.scene(48, 11, NOON, step / train.FPS, trip))
        rows = [cells[i * 48 : (i + 1) * 48] for i in range(11)]
        for x in range(48):
            column = [rows[y][x] for y in range(11)]
            if any(bg == wood for _, _, bg in column[1:5]):
                continue  # a pole
            # A wire is seen in every column: braille on a cell of one
            # colour, a darker pixel elsewhere.
            dots = sum(0x2800 < ord(c) < 0x2900 for c, _, _ in column[:5])
            assert dots or any(c == "▀" for c, _, _ in column[:5]), (step, x)
    # A thing by the glass hides them.
    half = Half(4, 2)
    sky, leaf = (90, 120, 230), (0, 90, 0)
    half.fill(sky)
    strung = [(x, 0, "⠒", (0, 0, 0), sky, sky, sky) for x in range(4)]
    half.set(1, 0, leaf)
    half.set(2, 1, leaf)
    train._string(half, strung)
    assert sorted(half.over) == [(0, 0), (3, 0)]
    # The poles stand on the verge: they go by at its pace, a pixel a frame.
    bare = replace(trip, land=False)

    def poles(t: float) -> list[int]:
        row = pixels(train.scene(48, 11, NOON, t, bare))[3 * 48 : 4 * 48]
        return [x for x, (_, _, bg) in enumerate(row) if bg == wood]

    first = poles(2.0)
    assert first and poles(2.0 + 5 / train.FPS) == [x - 5 for x in first if x >= 5]


def test_planes() -> None:
    """A lit window is not seen through what is nearer."""
    trip = replace(ALL, weather=None, events=False, poles=False, deep=True)
    lamp = hexa(train.LAMP)
    lit = hidden = 0
    for step in range(0, 1500, 3):
        t = step / train.FPS
        cells = colours(train.scene(48, 11, NIGHT.replace(hour=21), t, trip))
        rows = [cells[i * 48 : (i + 1) * 48] for i in range(11)]
        for y in range(1, 10):
            for x in range(1, 47):
                if rows[y][x] != lamp:
                    continue
                lit += 1
                # A lamp alone in the dark crown of a tree by the glass.
                around = {rows[y + j][x + i] for j in (-1, 1) for i in (-1, 0, 1)}
                hidden += len(around) == 1
    assert lit > 50 and hidden == 0


def test_random(capsys: pytest.CaptureFixture[str]) -> None:
    args = ["--print", "34x8", "-a", "-r", "--seed", "11"]
    assert train.main(args) == 0
    first = capsys.readouterr().out
    assert train.main(args) == 0
    assert capsys.readouterr().out == first
    seen = {first}
    for seed in range(12, 20):
        assert train.main([*args[:-1], str(seed)]) == 0
        seen.add(capsys.readouterr().out)
    assert len(seen) == 9
    assert train.main(["--print", "34x8", "-r", "--line", "alps", "--after", "5"]) == 0
    capsys.readouterr()


def colours(rows: train.Rows) -> list[str]:
    """The background of each cell, `deep`, row by row."""
    return [run["bg"] for row in rows for run in row for _ in run["t"]]


def test_tunnel() -> None:
    trip = replace(ALL, weather=None, deep=True)
    start = cycle(trip, "tunnel") * CYCLE + train.WHEN
    dark = hexa(train.DARK)
    inside = colours(train.scene(48, 11, NOON, start + 4.0, trip))
    assert inside.count(dark) > 0.5 * 48 * 11
    assert len(set(inside)) > 3  # the car in the glass, a lamp
    before = colours(train.scene(48, 11, NOON, start - 1.0, trip))
    after = train.scene(48, 11, NOON, start + train.SPAN["tunnel"] + 3, trip)
    assert dark not in before and dark not in colours(after)
    # It comes from the right.
    mouth = colours(train.scene(48, 11, NOON, start + 0.9, trip))
    rows = [mouth[i * 48 : (i + 1) * 48] for i in range(11)]
    assert all(row[47] == dark for row in rows[2:6])
    assert all(row[0] != dark for row in rows)


def test_station() -> None:
    trip = replace(ALL, weather=None, deep=True)
    k = cycle(trip, "station")
    halt = k * CYCLE + train.WHEN + train.BRAKE
    stone = hexa(train.STONE)
    standing = train.scene(48, 11, NOON, halt + 1, trip)
    assert standing == train.scene(48, 11, NOON, halt + train.STAND - 1, trip)
    assert all(run["bg"] == stone for run in standing[-1])  # the platform
    assert standing != train.scene(48, 11, NOON, halt - 3, trip)
    far = train.scene(48, 11, NOON, k * CYCLE + 2.0, trip)
    assert not any(run["bg"] == stone for row in far for run in row)
    # No pole along the platform.
    dots = sum(0x2800 < ord(c) < 0x2900 for c, _, _ in pixels(standing))
    assert dots == 0


def test_other_events() -> None:
    trip = replace(ALL, weather=None, deep=True)
    k = cycle(trip, "train")
    start = k * CYCLE + train.WHEN
    coat = hexa(train.LIVERY[k % len(train.LIVERY)])
    passing = colours(train.scene(48, 11, NOON, start + train.SPAN["train"] / 2, trip))
    assert passing[-48:].count(coat) > 40  # its side fills the glass
    assert coat not in colours(train.scene(48, 11, NOON, start - 1, trip))
    assert coat not in colours(train.scene(48, 11, NOON, start + 5, trip))
    start = cycle(trip, "bridge") * CYCLE + train.WHEN
    iron = hexa(train.IRON)
    on = colours(train.scene(48, 11, NOON, start + train.SPAN["bridge"] / 2 + 1, trip))
    assert on.count(iron) > 48
    assert iron not in colours(train.scene(48, 11, NOON, start - 1, trip))
    assert iron not in colours(train.scene(48, 11, NOON, start + 9, trip))


def test_breath() -> None:
    now = [10.0]
    pointer = Pointer(lambda: now[0], train.KEEP)
    trip = Trip(seed=4, deep=True)
    clear = train.scene(48, 11, NOON, 10.0, trip)
    assert train.scene(48, 11, NOON, 10.0, trip, pointer) == clear
    pointer.move(20, 5)
    fogged = train.scene(48, 11, NOON, 10.0, trip, pointer)
    changed = [
        (x, y)
        for y in range(11)
        for x in range(48)
        if pixels(fogged)[y * 48 + x] != pixels(clear)[y * 48 + x]
    ]
    assert changed and all(abs(x - 20) <= 3 and abs(y - 5) <= 2 for x, y in changed)
    pointer.leave()
    # It fades, then is gone.
    half, faint = Half(48, 11), Half(48, 11)
    train.breath(half, pointer, 10.0)
    train.breath(faint, pointer, 10.0 + train.KEEP * 0.8)
    assert sum(half.px[11][20]) > sum(faint.px[11][20]) > 0
    late = 10.0 + train.KEEP + 0.1
    assert train.scene(48, 11, NOON, late, trip, pointer) == train.scene(
        48, 11, NOON, late, trip
    )
    train.breath(half, None, 0.0)


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert train.main(["--print", "--seed", "3", "--at", "12:00"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert len(out) == sum(h + 2 for _, h in train.SIZES)
    assert sum(line.count("╭") for line in out) == len(train.SIZES)
    args = ["--print", "34x8", "-a", "--seed", "3", "--at", "22:10", "--after", "30"]
    assert train.main(args) == 0
    first = capsys.readouterr().out
    assert first.count("\n") == 10 and "═" * 34 in first
    assert train.main(args) == 0
    assert capsys.readouterr().out == first
    assert train.main([*args, "--line", "coast", "--rgb"]) == 0
    assert "48;2;" in capsys.readouterr().out
    for bad in (["--line", "moon"], ["--print", "0x3"], ["--speed", "-1"]):
        with pytest.raises(SystemExit):
            train.main(bad)
    capsys.readouterr()


def test_app(session: Pilot) -> None:
    run = ["-s", session.session, "-a", "--seed", "3", "--after", "20"]
    info = running(
        session, "train", lambda: train.main(run, seconds=3), train.SIZES, "═"
    )
    assert info["title"] == ""
    assert session.widgets() == []
