# SPDX-License-Identifier: GPL-3.0-or-later
"""`fire`: the scene without a driver, the widget on one."""

from __future__ import annotations

import itertools
import threading
import unicodedata

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import fire

FLAGS = ("blocks", "logs", "cycle", "sparks", "glow")
LOOKS = [
    fire.Look(**dict.fromkeys(names, True))
    for n in (0, 1, len(FLAGS))
    for names in itertools.combinations(FLAGS, n)
]
ALL = fire.Look(**dict.fromkeys(FLAGS, True))


def plain(rows: fire.Rows) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


def ink(rows: fire.Rows, glyphs: str | None = None) -> int:
    """Cells drawn; of `glyphs` only when given."""
    return sum(
        c != " " and (glyphs is None or c in glyphs) for row in plain(rows) for c in row
    )


@pytest.mark.parametrize("look", LOOKS)
def test_bounds(look: fire.Look) -> None:
    for (w, h), t in itertools.product(fire.SIZES, (0.0, 1.3, 200.0, 3500.0)):
        rows = fire.scene(t, w, h, look, poke=0.5)
        assert len(rows) == h
        for text in plain(rows):
            assert len(text) == w, (w, h, text)
            assert all(unicodedata.east_asian_width(c) not in "WF" for c in text)
        for run in itertools.chain.from_iterable(rows):
            assert set(run) <= {"t", "fg", "bg", "bold", "dim"}
            assert all(isinstance(run.get(k, 0), int) for k in ("fg", "bg"))


def test_pure() -> None:
    for look in (fire.Look(), ALL):
        assert fire.scene(7.3, 23, 9, look) == fire.scene(7.3, 23, 9, look)
        assert fire.scene(7.3, 23, 9, look) != fire.scene(7.4, 23, 9, look)
    other = fire.Look(seed=2)
    assert fire.scene(7.3, 23, 9) != fire.scene(7.3, 23, 9, other)


def test_bare_is_glyphs() -> None:
    for w, h in fire.SIZES:
        rows = fire.scene(3.0, w, h)
        assert set("".join(plain(rows))) <= set(fire.GLYPHS)
        assert ink(rows) > 0
        assert all("bg" not in run for row in rows for run in row)
        # It stands on the bottom of the box and thins upwards.
        assert ink(rows[-1:]) >= ink(rows[:1])


def test_blocks() -> None:
    rows = fire.scene(3.0, 23, 9, fire.Look(blocks=True))
    assert set("".join(plain(rows))) <= set(" ▀▄")
    assert any("bg" in run for row in rows for run in row)


def test_heat() -> None:
    # None beside the bed or over the flames; the most at the foot.
    assert fire.heat(-1, 0.5, 1.0, 23, 18.0, 1.0, 1) == 0.0
    assert fire.heat(11, 40.0, 1.0, 23, 18.0, 1.0, 1) == 0.0
    foot = sum(fire.heat(11, 0.5, t / 7, 23, 18.0, 1.0, 1) for t in range(50))
    high = sum(fire.heat(11, 9.0, t / 7, 23, 18.0, 1.0, 1) for t in range(50))
    assert foot > high > 0
    for t in range(50):
        for x, y in itertools.product(range(23), range(18)):
            assert 0.0 <= fire.heat(x, y + 0.5, t / 7, 23, 18.0, 1.0, 1) <= 1.0


def test_fuel() -> None:
    assert fire.fuel(123.0, fire.Look()) == fire.FULL
    look = fire.Look(cycle=True, burn=100.0)
    levels = [fire.fuel(t, look) for t in range(2, 100)]
    assert levels == sorted(levels, reverse=True)  # it burns down
    assert levels[0] == 1.0 and levels[-1] < 0.1
    assert fire.fuel(102.0, look) == fire.fuel(2.0, look)  # a log is put on
    assert all(0 < fire.fuel(t / 10, look) <= 1 for t in range(2000))

    def flames(t0: float) -> int:
        return sum(ink(fire.scene(t0 + i / 3, 33, 11, look)) for i in range(30))

    assert flames(5.0) > 3 * flames(92.0)


def test_logs() -> None:
    look = fire.Look(logs=True)
    candle, brazier, hearth = (
        plain(fire.scene(1.0, w, h, look)) for w, h in ((5, 4), (9, 5), (33, 11))
    )
    assert candle[-2:] == [" ▐█▌ ", " ▀▀▀ "]
    assert brazier[-2][0] == "◥" and brazier[-2][-1] == "◤"
    assert hearth[-1].count("╨") == 2 and hearth[-2].count("╥") == 2
    assert sum(c in "▓▒" for c in hearth[-1]) > sum(c in "▓▒" for c in hearth[-2]) > 4
    # With the light, bricks behind the logs, brighter near the flames.
    lit = fire.scene(1.0, 33, 11, fire.Look(logs=True, glow=True))
    assert ink(lit, "┬┴─") > 100
    shades = {
        run["fg"] for row in lit for run in row if set(str(run["t"])) <= set("┬┴─")
    }
    assert len(shades) >= 3 and shades <= set(fire.LIT)


def test_embers() -> None:
    # Burnt down: no flame, the logs glow more.
    look = fire.Look(logs=True, cycle=True, burn=100.0)

    def coals(t: float) -> int:
        rows = fire.scene(t, 33, 11, look)
        return sum("bg" in run for row in rows[-2:] for run in row)

    assert sum(coals(95.0 + i / 5) for i in range(10)) > 0
    assert ink(fire.scene(95.0, 33, 11, look)[:-2]) < 12


def test_sparks() -> None:
    def count(poke: float | None) -> int:
        return sum(
            len(fire.sparks(t / 10, 33, 9, fire.FULL, 1, poke)) for t in range(300)
        )

    assert count(None) > 30
    assert count(0.6) > 2 * count(None)  # a poke throws a burst
    for t in range(300):
        for x, y in fire.sparks(t / 10, 33, 9, fire.FULL, 1, 0.6):
            assert 0 <= x < 33 and 0 <= y < 9
    assert fire.sparks(1.0, 2, 2, 1.0, 1) == {}
    rows = [fire.scene(t / 10, 33, 11, fire.Look(sparks=True)) for t in range(100)]
    assert sum(ink(r, "*·") for r in rows) > 0


def test_poke() -> None:
    poke = fire.Poke(again=4.0)
    assert poke.age(0.0) is None
    poke.on_move(3, 2, now=10.0)
    assert poke.age(10.5) == 0.5
    poke.on_move(4, 2, now=11.0)  # still over it: the same poke
    assert poke.age(11.5) == 1.5
    poke.on_move(4, 3, now=14.5)  # and again, later
    assert poke.age(15.0) == 0.5
    poke.on_leave()
    poke.on_move(1, 1, now=15.5)  # back over it
    assert poke.age(15.5) == 0.0
    assert poke.age(15.5 + fire.POKED) is None
    # The flames rise.
    calm = sum(ink(fire.scene(t / 5, 33, 11)) for t in range(40))
    poked = sum(ink(fire.scene(t / 5, 33, 11, poke=0.3)) for t in range(40))
    assert poked > calm


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert fire.main(["--print"]) == 0
    out = capsys.readouterr().out
    assert all(f" {w}x{h} " in out for w, h in fire.SIZES[2:])
    assert "\x1b" not in out
    assert fire.main(["-a", "--print", "33x11", "--at", "5"]) == 0
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == 13 and all(len(line) == 35 for line in lines)
    for argv in (["--print", "0x3"], ["--print", "wide"], ["--burn", "0"]):
        with pytest.raises(SystemExit):
            fire.main(argv)
        capsys.readouterr()


@pytest.mark.parametrize("argv", [[], ["-a", "--prefer", "top-right"]])
def test_fire(session: Pilot, argv: list[str]) -> None:
    seen: list[dict] = []
    run = ["-s", session.session, *argv]
    thread = threading.Thread(target=lambda: fire.main(run, seconds=3))
    thread.start()
    while thread.is_alive():
        seen += [w for w in session.widgets() if w["state"] == "placed"]
        session_clock.sleep(0.05)
    thread.join()
    assert seen and {w["tag"] for w in seen} == {"fire"}
    assert all(tuple(w["size"]) in fire.SIZES for w in seen)
    assert session.widgets() == []
