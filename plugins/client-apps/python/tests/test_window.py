# SPDX-License-Identifier: GPL-3.0-or-later
"""`window`: the scene without a driver, then the widget on one."""

from __future__ import annotations

import argparse
import itertools
import threading
import unicodedata
from dataclasses import replace
from datetime import datetime

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import _sky, window
from ptyrc_apps._strip import LAMP, Pointer
from ptyrc_apps.window import Look

NOON = datetime(2026, 7, 20, 12, 30)
NIGHT = datetime(2026, 7, 20, 23, 30)
ALL = Look(seed=5, land=True, seasons=True, weather="day", passing=True, cosy=True)


def check(rows: window.Rows, w: int, h: int, deep: bool = False) -> None:
    """`rows` fill a box of `w` by `h`: narrow glyphs, colours of the 256."""
    assert len(rows) == h
    for row in rows:
        text = "".join(run["t"] for run in row)
        assert len(text) == w, text
        assert all(unicodedata.east_asian_width(c) not in "WF" for c in text)
        assert all(unicodedata.category(c)[0] != "C" for c in text)
        for run in row:
            assert set(run) <= {"t", "fg", "bg"} and "bg" in run
            for key in ("fg", "bg"):
                if key in run and deep:
                    assert isinstance(run[key], str) and len(run[key]) == 7
                elif key in run:
                    assert 16 <= run[key] <= 255


def pixels(rows: window.Rows) -> list[tuple]:
    """A cell a tuple `(glyph, fg, bg)`, row by row."""
    return [
        (c, run.get("fg") if c != " " else None, run["bg"])
        for row in rows
        for run in row
        for c in run["t"]
    ]


@pytest.mark.parametrize("place", window.PLACES)
@pytest.mark.parametrize("look", [Look(seed=5), ALL], ids=["bare", "all"])
def test_fits(place: str, look: Look) -> None:
    look = replace(look, place=place)
    for w, h in (*window.SIZES, (1, 1), (3, 2), (61, 17)):
        for hour in range(0, 24, 3):
            when = NOON.replace(hour=hour)
            rows = window.scene(w, h, when, 12.5, look)
            check(rows, w, h)
            assert rows == window.scene(w, h, when, 12.5, look)  # pure


@pytest.mark.parametrize("kind", ["clear", "clouds", "rain", "fog", "snow"])
def test_weathers(kind: str) -> None:
    look = replace(ALL, weather=kind)
    plain = pixels(window.scene(40, 12, NOON, 3.0, replace(ALL, weather=None)))
    rows = window.scene(40, 12, NOON, 3.0, look)
    check(rows, 40, 12)
    assert kind == "clear" or pixels(rows) != plain  # one cloud, or none
    falls = kind in ("rain", "snow")
    assert any(0x2800 < ord(c) < 0x2900 for c, _, _ in pixels(rows)) == falls
    if falls:  # it falls
        assert pixels(rows) != pixels(window.scene(40, 12, NOON, 3.5, look))


def test_bare_is_still() -> None:
    """Bare, the scene depends on the time of the place only."""
    look = Look(seed=5)
    assert not look.moving and ALL.moving
    assert window.scene(40, 12, NOON, 0.0, look) == window.scene(40, 12, NOON, 99, look)
    assert window.scene(40, 12, NOON, 0.0, look) != window.scene(40, 12, NIGHT, 0, look)
    day = {bg for _, _, bg in pixels(window.scene(40, 12, NOON, 0.0, look))}
    night = pixels(window.scene(40, 12, NIGHT, 0.0, look))
    assert len(day) >= 5
    assert any(c in "·✦⠁⠂⠄⠈⠐⠠" for c, _, _ in night)  # stars
    assert not any(c in "·✦" for c, _, _ in pixels(window.scene(40, 12, NOON, 0, look)))
    assert pixels(window.scene(40, 12, NOON, 0.0, look)) != pixels(
        window.scene(40, 12, NOON, 0.0, Look(seed=6))
    )


def test_options_add() -> None:
    """Each option changes the picture."""
    bare = Look(seed=5)
    dusk = NOON.replace(hour=21, minute=40)
    seen = {tuple(pixels(window.scene(40, 12, dusk, 2.0, bare)))}
    winter = NOON.replace(month=1)
    for more in (
        {"land": True},
        {"weather": "clouds"},
        {"cosy": True},
        {"land": True, "passing": True},
        {"deep": True},
    ):
        seen.add(tuple(pixels(window.scene(40, 12, dusk, 2.0, replace(bare, **more)))))
    assert len(seen) == 6
    # The season shows with `seasons` only.
    assert window.scene(40, 12, winter, 0.0, bare) != window.scene(
        40, 12, winter, 0.0, replace(bare, seasons=True)
    )
    check(window.scene(40, 12, NOON, 0.0, replace(ALL, deep=True)), 40, 12, deep=True)


def test_passing() -> None:
    look = Look(seed=5, land=True, passing=True)
    frames = {tuple(pixels(window.scene(40, 12, NOON, t, look))) for t in range(200)}
    assert len(frames) > 20
    birds = [
        t
        for t in range(400)
        if any(c in "⠑⠊⠒" for c, _, _ in pixels(window.scene(40, 12, NOON, t, look)))
    ]
    assert birds
    night = [
        t / 5
        for t in range(600)
        if any(
            fg == 230 and 0x2800 < ord(c) < 0x2900
            for c, fg, _ in pixels(window.scene(40, 12, NIGHT, t / 5, look))
        )
    ]
    assert night, "no meteor in two minutes"


def test_hover() -> None:
    now = [0.0]
    pointer = Pointer(lambda: now[0])
    look = Look(seed=5, land=True, cosy=True, deep=True)
    away = window.scene(40, 12, NOON, 0.0, look, pointer)
    assert away == window.scene(40, 12, NOON, 0.0, look)
    assert window.curtain(40, 0.0) > window.curtain(40, 1.0) >= 1
    assert window.curtain(7, 0.0) == window.curtain(7, 1.0) == 1
    pointer.move(30, 8)
    now[0] = 5.0
    here = window.scene(40, 12, NOON, 5.0, look, pointer)
    assert here != away
    assert pixels(here)[4] != pixels(away)[4]  # the curtain went aside
    pointer.leave()
    assert window.scene(40, 12, NOON, 5.0, look, pointer) != away  # it closes
    assert window.scene(40, 12, NOON, 9.0, look, pointer) == window.scene(
        40, 12, NOON, 9.0, look
    )


def _fur(look: Look, t: float, pointer: Pointer | None) -> set[tuple[int, int]]:
    half = window.Half(40, 12)
    room = window._room(look, 40, 12, "summer", None)
    window._cat(half, _sky.at(NOON), t, pointer, room)
    return {
        (x, y)
        for y, row in enumerate(half.px)
        for x, c in enumerate(row)
        if c != (0, 0, 0)
    }


def test_cat_hunts() -> None:
    """The cat runs along its pane to the pointer, leaps at it, and
    goes home when it leaves; never on a mullion."""
    look = Look(seed=5, cosy=True)
    room = window._room(look, 40, 12, "summer", None)
    home, _ = room.perch
    lo, hi = window._run(40, 12, room.perch)
    assert lo < home < hi
    cols = set(window._bars(40, 12)[0])
    now = [0.0]
    pointer = Pointer(lambda: now[0])
    rest = _fur(look, 0.0, pointer)
    pointer.move(lo + 2, 5)
    xs, tops = [], []
    for n in range(40):
        now[0] = t = n / 4
        x, _, _ = window.chase(pointer, t, home, lo, hi)
        xs.append(x)
        fur = _fur(look, t, pointer)
        assert not {x for x, _ in fur} & cols, t
        tops.append(min(y for _, y in fur))
    assert xs[0] == home and xs[-1] == lo
    assert all(a >= b for a, b in itertools.pairwise(xs))  # no step back
    assert min(tops) < min(tops[:4]) - 2  # it leaps
    pointer.move(hi + 30, 5)  # out of its pane: it waits at the edge
    now[0] = 20.0
    assert window.chase(pointer, 20.0, home, lo, hi)[0] == hi
    pointer.leave()
    now[0] = 40.0
    assert window.chase(pointer, 40.0, home, lo, hi)[0] == home
    body = {(x, y) for x, y in rest if y < 23}
    assert body <= _fur(look, 40.0, pointer)  # at rest again


@pytest.mark.parametrize("kind", ["rain", "snow"])
def test_cat_is_indoors(monkeypatch: pytest.MonkeyPatch, kind: str) -> None:
    """What falls outside never shows on the cat: its cells are the
    same as with nothing outside but the land."""
    cat, cells, under = window._cat, set[tuple[int, int]](), [0]

    def spy(half: window.Half, *args: object) -> None:
        before, over = [list(row) for row in half.px], set(half.over)
        cat(half, *args)
        cells.clear()
        for y, row in enumerate(half.px):
            cells.update((x, y // 2) for x, c in enumerate(row) if c != before[y][x])
        under[0] += len(cells & over)

    monkeypatch.setattr(window, "_cat", spy)
    look = Look(seed=5, land=True, passing=True, cosy=True, weather=kind)
    pointer = Pointer(lambda: 0.0)
    pointer.move(20, 11)  # the eyes
    frames = []
    for t in (n / 4 for n in range(240)):
        rows = window.scene(40, 12, NOON, t, look, pointer)
        frames.append((t, pixels(rows), sorted(cells)))
    assert under[0] > 20  # it falls on the cat
    for name in ("rain", "snow", "stars", "_birds", "_meteor"):
        monkeypatch.setattr(window, name, lambda *args, **kwargs: None)
    for t, seen, painted in frames:
        assert painted
        plain = pixels(window.scene(40, 12, NOON, t, look, pointer))
        for x, y in painted:
            assert seen[40 * y + x] == plain[40 * y + x], (t, x, y)


CATS = [s for s in window.SIZES if s[0] >= 18 and s[1] >= 6]
ODD = [(18, 6), (19, 7), (25, 9), (33, 8), (34, 6), (35, 11), (61, 17), (200, 60)]


@pytest.mark.parametrize("size", [*CATS, *ODD])
def test_cat_clear_of_the_bars(size: tuple[int, int]) -> None:
    """The cat, tail and all, on no mullion; its body in the open,
    its ears under the rail with a row between."""
    w, h = size
    cols, rail = window._bars(w, h)
    side = window.curtain(w, 0.0)
    seen = set()
    for seed in range(201):
        perch = window._perch(w, h, seed)
        assert perch is not None
        left, way = perch
        body = range(left, left + 5)
        assert side <= left and left + 4 < w - side
        assert not set(cols) & {*body, *window._tail(left, way)}
        assert all(0 <= x < w for x in window._tail(left, way))
        assert rail is None or rail < 2 * h - len(window.CAT) - 1
        seen.add(perch)
    assert len(seen) >= 2 or w < 19  # drawn from the seed; one place at 18


def test_cat_clear_of_the_bars_drawn() -> None:
    """In the picture: no pixel of the cat on a mullion column."""
    look = Look(seed=0, cosy=True, deep=True)
    for w, h in [*CATS, (19, 7), (35, 11)]:
        cols, _ = window._bars(w, h)
        for seed in range(0, 200, 7):
            half = window.Half(w, h)
            room = window._room(replace(look, seed=seed), w, h, "summer", None)
            window._cat(half, _sky.at(NOON), 0.0, None, room)
            fur = {x for row in half.px for x, c in enumerate(row) if c != (0, 0, 0)}
            assert fur and not fur & set(cols), (w, h, seed)


SEASONS = {"spring": 4, "summer": 7, "autumn": 10, "winter": 1}


@pytest.mark.parametrize("place", window.PLACES)
def test_cat_stands_out(place: str) -> None:
    """The cat against what is behind it and against the mullions, by
    day and by night, as the 256 show them."""
    low = []
    for season, month in SEASONS.items():
        for kind in (None, "clouds", "rain", "fog"):
            for seed in range(8):
                for w, h in ((40, 12), (26, 8), (18, 6)):
                    look = Look(seed, place, land=True, seasons=True, cosy=True)
                    look = replace(look, weather=kind)
                    room = window._room(look, w, h, season, kind)
                    left, _ = room.perch
                    for hour in (12, 0):
                        when = datetime(2026, month, 20, hour, 30)
                        sky = _sky.at(when, look.lat, window.COVER.get(kind or "", 0))
                        out = window._outside(w, h, look, sky, season, kind, 0, None)
                        rows = 2 * h
                        cells = [
                            (x, y)
                            for x in range(left - 1, left + 6)
                            for y in range(rows - 6, rows)
                        ]
                        behind = ((sky.light, tuple(out.px[y][x] for x, y in cells)),)
                        fit = window._against(room.coat, behind, rim=True)
                        coat = window._lit(room.coat, sky.light)
                        frame = window._lit(room.frame, sky.light)
                        apart = window._apart(coat, frame)
                        low.append(min(fit, apart))
    assert min(low) >= 18
    assert sum(v < 25 for v in low) <= len(low) // 50


@pytest.mark.parametrize("place", window.PLACES)
def test_indoors_vary(place: str) -> None:
    """The seed still chooses: several coats and cloths a place."""
    coats, cloths = set(), set()
    for seed in range(40):
        look = Look(seed, place, land=True, seasons=True, cosy=True)
        room = window._room(look, 40, 12, "summer", None)
        coats.add(room.coat)
        cloths.add(room.cloth)
    assert len(coats) >= 4 and len(cloths) >= 4


def test_indoors_still() -> None:
    """The things indoors keep their colours as the time runs and the
    clouds pass."""
    for seed in range(8):
        look = replace(ALL, seed=seed, weather="clouds", deep=True)
        left, _ = window._room(look, 40, 12, "summer", "clouds").perch
        bar = window._bars(40, 12)[0][0]
        # A cell of the curtain, of a mullion, of the back of the cat.
        cells = [0, bar, 40 * 10 + left + 2, 40 * 11 + left + 2]
        frames = [pixels(window.scene(40, 12, NOON, t, look)) for t in (10, 13, 40)]
        assert frames[0] != frames[2]  # the clouds move
        for seen in frames:
            assert [seen[i] for i in cells] == [frames[0][i] for i in cells]


def test_house_lit_under_the_pointer() -> None:
    hexa = "#{:02x}{:02x}{:02x}".format(*LAMP)
    evening = NOON.replace(hour=22, minute=0)

    def lit(rows: window.Rows) -> int:
        return sum(hexa in (run.get("fg"), run["bg"]) for row in rows for run in row)

    # A land with a house: its window is lit at night, by itself.
    look = next(
        look
        for look in (Look(seed=n, land=True, deep=True) for n in range(40))
        if lit(window.scene(40, 12, evening, 0.0, look))
    )
    assert lit(window.scene(40, 12, NOON, 0.0, look)) == 0
    found = 0
    for x in range(40):
        for y in range(12):
            pointer = Pointer(lambda: 0.0)
            pointer.move(x, y)
            found = max(found, lit(window.scene(40, 12, NOON, 0.0, look, pointer)))
    assert found >= 1


def test_era() -> None:
    start = datetime(2026, 11, 13, 9, 5, 7, 123)
    assert window.Era(start).when(60) == datetime(2026, 11, 13, 9, 6, 7)
    assert window.Era(start, 1440.0).when(60) == datetime(2026, 11, 14, 9, 5, 7)
    era = window.Era(start, 1.0, (21, 30), "winter")
    assert era.start == datetime(2026, 1, 20, 21, 30)
    for season, (month, day) in window.MIDDLE.items():
        assert _sky.season_of(datetime(2026, month, day).timetuple().tm_yday) == season


def test_random(capsys: pytest.CaptureFixture[str]) -> None:
    def draw(*more: str) -> argparse.Namespace:
        out = argparse.ArgumentParser()
        out.add_argument("--place")
        out.add_argument("--weather")
        out.add_argument("-y", "--seasons", action="store_true")
        window.times(out)
        args = out.parse_args(more)
        window.drawn(args, "place", window.PLACES)
        return args

    plain = draw("--seed", "3")
    assert (plain.place, plain.season, plain.at, plain.weather) == (None,) * 4
    assert plain.days == 0 and not plain.seasons
    seen = [draw("-r", "--seed", str(n)) for n in range(120)]
    assert seen[7] == draw("-r", "--seed", "7")  # a seed: the same draws
    assert {a.place for a in seen} == set(window.PLACES)
    assert {a.season for a in seen} == set(_sky.SEASONS)
    assert {a.weather for a in seen} == set(_sky.WEATHERS)
    assert all(a.season == "winter" for a in seen if a.weather == "snow")
    assert all(a.seasons and -28 <= a.days <= 28 for a in seen)
    assert len({a.at for a in seen}) > 60
    assert sum(not 7 <= a.at[0] < 18 for a in seen) > 30  # low suns, evenings
    # What is given stays.
    kept = draw("-r", "--place", "sea", "--at", "07:10", "--season", "spring")
    assert (kept.place, kept.at, kept.season, kept.days) == (
        "sea",
        (7, 10),
        "spring",
        0,
    )
    assert draw("-r", "--weather", "fog").weather == "fog"
    assert draw("-r").seed != draw("-r").seed  # no seed: another at each run
    # The moments, by the sun.
    assert 4.5 < window.moment("dawn", 5.0, 21.0, 0.5) < 5.5
    assert (
        19.9
        < window.moment("golden", 5.0, 21.0, 0.0)
        < window.moment("golden", 5.0, 21.0, 1.0)
        < 21.0
    )
    assert window.moment("night", 8.0, 16.5, 0.9) < 8.0
    start = window.Era(NOON, 1.0, (6, 0), "winter", days=-3).start
    assert (start.month, start.day, start.hour) == (1, 17, 6)
    args = ["--print", "26x8", "-a", "-r", "--seed", "9"]
    assert window.main(args) == 0
    first = capsys.readouterr().out
    assert window.main(args) == 0
    assert capsys.readouterr().out == first
    assert window.main([*args[:-1], "10"]) == 0
    assert capsys.readouterr().out != first


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert window.main(["--print", "--seed", "3", "--at", "12:00"]) == 0
    out = capsys.readouterr().out.splitlines()
    assert len(out) == sum(h + 2 for _, h in window.SIZES)
    assert sum(line.count("╘") for line in out) == len(window.SIZES)
    args = [
        "--print",
        "26x8",
        "-a",
        "--seed",
        "3",
        "--at",
        "22:10",
        "--season",
        "winter",
    ]
    assert window.main(args) == 0
    first = capsys.readouterr().out
    assert first.count("\n") == 10 and "═" * 26 in first
    assert window.main(args) == 0
    assert capsys.readouterr().out == first
    assert window.main([*args, "--place", "sea", "--weather", "fog", "--rgb"]) == 0
    assert "48;2;" in capsys.readouterr().out
    for bad in (
        ["--at", "25:00"],
        ["--print", "0x3"],
        ["--speed", "0"],
        ["--place", "x"],
    ):
        with pytest.raises(SystemExit):
            window.main(bad)
    capsys.readouterr()


def running(pilot: Pilot, tag: str, run, sizes, sill: str) -> dict:
    """Run a widget app in a thread: its widget is placed, in a variant
    of `sizes`, its frame drawn. The info of the widget."""
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(run()))
    thread.start()
    seen: dict = {}
    while thread.is_alive():
        for info in pilot.widgets():
            if info["tag"] != tag or info["state"] != "placed":
                continue
            assert info["variant"] in {f"{w}x{h}" for w, h in sizes}, info
            (w, h), (x, y, _, _) = info["size"], info["rect"]
            lines = pilot.shown()
            if sill * w in lines[y + h + 1][x : x + w + 2]:
                seen = info
        session_clock.sleep(0.05)
    thread.join()
    assert status == [0]
    assert seen, "the widget never showed"
    return seen


def test_app(session: Pilot) -> None:
    run = ["-s", session.session, "-a", "--seed", "3", "--speed", "600"]
    info = running(
        session, "window", lambda: window.main(run, seconds=3), window.SIZES, "═"
    )
    assert info["title"] == ""
    for _ in range(50):  # the driver drops them once it reads the close
        if not session.widgets():
            break
        session_clock.sleep(0.1)
    assert session.widgets() == []


def test_app_border(session: Pilot) -> None:
    run = ["-s", session.session, "--border", "round", "--prefer", "top-right"]
    running(session, "window", lambda: window.main(run, seconds=2), window.SIZES, "─")
