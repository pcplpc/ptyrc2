# SPDX-License-Identifier: GPL-3.0-or-later
"""`window`: the scene without a driver, then the widget on one."""

from __future__ import annotations

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
    # The cat looks at the pointer: an eye on its side.
    eye = "#c8e65a"
    assert eye not in {fg for _, fg, _ in pixels(away)} | {
        bg for *_, bg in pixels(away)
    }
    seen = {fg for _, fg, _ in pixels(here)} | {bg for *_, bg in pixels(here)}
    assert eye in seen
    pointer.leave()
    assert window.scene(40, 12, NOON, 5.0, look, pointer) != away  # it closes
    assert window.scene(40, 12, NOON, 9.0, look, pointer) == window.scene(
        40, 12, NOON, 9.0, look
    )


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
    assert session.widgets() == []


def test_app_border(session: Pilot) -> None:
    run = ["-s", session.session, "--border", "round", "--prefer", "top-right"]
    running(session, "window", lambda: window.main(run, seconds=2), window.SIZES, "─")
