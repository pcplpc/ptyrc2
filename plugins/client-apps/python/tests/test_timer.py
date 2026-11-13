# SPDX-License-Identifier: GPL-3.0-or-later
"""`timer`: the time, the sand and the faces without a driver; one run
on a session."""

from __future__ import annotations

import math
import threading
import unicodedata
from dataclasses import replace
from datetime import datetime

import pytest
from ptyrc import Pilot
from ptyrc import clock as session_clock
from ptyrc_apps import clock, timer

PLAIN = timer.Look()
FANCY = timer.Look(glass=True, colour=True)
LOOKS = [timer.Look(glass=g, colour=c) for g in (False, True) for c in (False, True)]


def plain(rows: timer.Rows, w: int = 0) -> list[str]:
    return ["".join(str(run["t"]) for run in row).ljust(w) for row in rows]


def dots(text: str) -> int:
    """Braille dots of a string."""
    return sum((ord(c) - 0x2800).bit_count() for c in text if "⠀" <= c <= "⣿")


@pytest.mark.parametrize(
    ("text", "seconds"),
    [
        ("25m", 1500),
        ("90s", 90),
        ("1h30m", 5400),
        ("1h", 3600),
        ("1.5h", 5400),
        ("2m30s", 150),
        ("1:30", 90),
        ("1:30:00", 5400),
        ("25", 1500),
        ("0.5", 30),
        (" 5M ", 300),
    ],
)
def test_duration(text: str, seconds: float) -> None:
    assert timer.duration(text) == seconds


@pytest.mark.parametrize("text", ["", "m", "0", "0s", "-5m", "5x", "1:3", "m5", "1h1h"])
def test_not_a_duration(text: str) -> None:
    with pytest.raises(ValueError, match="not a duration"):
        timer.duration(text)


def test_until() -> None:
    now = datetime(2026, 11, 13, 17, 0, 30)
    assert timer.until("17:30", now) == 29 * 60 + 30
    assert timer.until("17:00", now) == 24 * 3600 - 30
    assert timer.until("9:05", now) == (16 * 60 + 5) * 60 - 30
    with pytest.raises(ValueError, match="HH:MM"):
        timer.until("25:00", now)


def test_plan() -> None:
    assert timer.pomodoro("25/5") == (1500, 300)
    assert timer.pomodoro("50m/10m") == (3000, 600)
    with pytest.raises(ValueError):
        timer.pomodoro("25")
    assert timer.plan(90.0) == [timer.Phase("timer", 90.0)]
    names = [p.name for p in timer.plan(None, 1500, 300, rounds=3)]
    assert names == ["work", "break", "work", "break", "work"]
    assert [p.name for p in timer.plan(None, 1500, 300, rounds=1)] == ["work"]
    assert [p.name for p in timer.plan(None, 1500, 300, rounds=0)] == ["work", "break"]


def test_stamp() -> None:
    assert timer.stamp(1500) == "25:00"
    assert timer.stamp(1499.2) == "25:00"  # a second begun counts
    assert timer.stamp(0.01) == "00:01"
    assert timer.stamp(0) == timer.stamp(-3) == "00:00"
    assert timer.stamp(5999) == "99:59"
    assert timer.stamp(6000) == "1:40:00"
    assert [timer.short(s) for s in (0, 45, 60, 61, 1500, 3600, 5400)] == [
        "0s",
        "45s",
        "1m",
        "2m",
        "25m",
        "1h",
        "1.5h",
    ]


def test_run() -> None:
    run = timer.Run(timer.plan(100.0), now=10.0)
    state = run.state(10.0)
    assert (state.left, state.share, state.done, state.rounds) == (100.0, 0.0, False, 0)
    assert run.state(35.0).share == 0.25
    assert run.toggle(35.0) is True
    assert run.state(60.0).left == 75.0 and run.state(60.0).paused
    assert run.toggle(60.0) is False
    assert run.state(134.9).done is False
    done = run.state(140.0)
    assert done.done and done.left == 0.0 and done.share == 1.0
    assert done.over == pytest.approx(5.0)


def test_run_speed() -> None:
    run = timer.Run(timer.plan(1500.0), speed=60.0)
    assert run.state(10.0).left == 900.0
    assert run.state(25.0).done


def test_run_pomodoro() -> None:
    run = timer.Run(timer.plan(None, 50.0, 10.0, rounds=2))
    first = run.state(0.0)
    assert (first.name, first.round, first.rounds, first.turn) == ("work", 1, 2, None)
    rest = run.state(50.0 + timer.TURN / 2)
    assert (rest.name, rest.index, rest.round) == ("break", 1, 1)
    assert rest.turn == pytest.approx(0.5)
    assert run.state(55.0).turn is None
    last = run.state(65.0)
    assert (last.name, last.index, last.round, last.left) == ("work", 2, 2, 45.0)
    assert run.state(111.0).done


def test_run_loop() -> None:
    run = timer.Run(timer.plan(None, 50.0, 10.0, rounds=0), loop=True)
    state = run.state(60.0 * 7 + 55.0)
    assert (state.name, state.index, state.round, state.rounds) == ("break", 15, 8, 0)
    assert not state.done


@pytest.mark.parametrize("k", range(2, 7))
def test_bulbs(k: int) -> None:
    upper, lower = timer.bulbs(k)
    n = timer.grains(k)
    assert n == len(upper) == len(set(upper)) and len(lower) == len(set(lower)) >= n
    w, h = 2 * k + 1, 2 * k + 2
    for x, y in upper:  # inside the upper bulb, off the walls
        i = y // 4 - 1
        assert 0 <= i < k and i + 1 <= x // 2 <= 2 * k - 1 - i
    for x, y in lower:
        j = y // 4 - k - 1
        assert 0 <= j < k and k - j <= x // 2 <= k + j
    assert all(0 <= x < 2 * w and 0 <= y < 4 * h for x, y in upper + lower)
    # The last grain to leave is at the neck; the first to land, on the floor.
    assert upper[-1][1] == 4 * (k + 1) - 1
    assert lower[0][1] == 4 * (2 * k + 1) - 1
    # Mirror images: as much sand on each side of the middle.
    for part in (upper, lower):
        assert sorted(part) == sorted((4 * k + 1 - x, y) for x, y in part)


@pytest.mark.parametrize("k", range(2, 7))
def test_sand_is_counted(k: int) -> None:
    n = timer.grains(k)
    before: tuple[frozenset, frozenset] | None = None
    for fell in range(n + 1):
        upper, lower = timer.sand(k, fell)
        assert (len(upper), len(lower)) == (n - fell, fell)
        if before is not None:  # one grain left, one landed, none moved
            assert upper < before[0] and lower > before[1]
        before = (upper, lower)


@pytest.mark.parametrize("k", range(2, 7))
@pytest.mark.parametrize(
    ("length", "speed"), [(60.0, 1.0), (1500.0, 1.0), (1500.0, 60.0), (7.0, 0.5)]
)
def test_the_glass_empties_on_time(k: int, length: float, speed: float) -> None:
    n = timer.grains(k)
    run = timer.Run(timer.plan(length), speed=speed)
    end = length / speed
    seen = []
    for step in range(401):
        state = run.state(end * step / 400)
        fell = timer.fallen(k, state.share)
        seen.append(fell)
        # A grain every `length / n` seconds.
        assert fell == min(n, math.floor(n * state.share + 1e-9)) or (
            fell == n - 1 and not state.done
        )
    assert seen == sorted(seen) and seen[0] == 0
    assert seen[-1] == n and seen[-2] < n  # the last one as the time ends
    run = timer.Run(timer.plan(length), speed=speed)
    assert timer.fallen(k, run.state(end * (1 - 1e-6)).share) == n - 1
    assert timer.fallen(k, run.state(end).share) == n


@pytest.mark.parametrize("k", range(2, 7))
def test_glass_drawn(k: int) -> None:
    w, h, n = 2 * k + 1, 2 * k + 2, timer.grains(k)
    for share in (0.0, 0.3, 0.5, 0.99, 1.0):
        state = timer.State(length=100.0, left=100.0 * (1 - share), paused=True)
        rows = plain(timer.glass(k, state, PLAIN), w)
        assert len(rows) == h and all(len(row) == w for row in rows)
        assert rows[0] == "▄" * w and rows[-1] == "▀" * w
        assert rows[1][0] == "╲" and rows[1][-1] == "╱"
        assert rows[k].strip()[0] == "╲" and rows[k + 1].strip()[0] == "╱"
        fell = timer.fallen(k, share)
        assert dots("".join(rows[: k + 1])) == n - fell
        assert dots("".join(rows[k + 1 :])) == fell
    # Running: grains in the air, between the neck and the heap only.
    state = timer.State(length=100.0, left=60.0, beat=0.3)
    rows = plain(timer.glass(k, state, PLAIN), w)
    fell = timer.fallen(k, 0.4)
    assert dots("".join(rows[: k + 1])) == n - fell
    assert dots("".join(rows[k + 1 :])) > fell
    other = plain(timer.glass(k, replace(state, beat=0.4), PLAIN), w)
    assert other != rows and other[: k + 1] == rows[: k + 1]


@pytest.mark.parametrize("k", range(2, 7))
def test_spin(k: int) -> None:
    w, h = 2 * k + 1, 2 * k + 2
    frames = [plain(timer.spin(k, t / 10, PLAIN), w) for t in range(11)]
    for rows in frames:
        assert len(rows) == h and all(len(row) == w for row in rows)
    start, end = frames[0], frames[-1]
    # Half a turn: the sand that lay in the lower bulb is in the upper.
    assert dots("".join(start[h // 2 :])) > 1.5 * dots("".join(start[: h // 2]))
    assert dots("".join(end[: h // 2])) > 1.5 * dots("".join(end[h // 2 :]))
    assert len({tuple(rows) for rows in frames}) == 11
    state = timer.State(length=10.0, left=10.0, index=1, turn=0.5)
    assert plain(timer.glass(k, state, PLAIN), w) == frames[5]


STATES = [
    timer.State(length=1500.0, left=1500.0),
    timer.State(length=1500.0, left=612.4, beat=3.7),
    timer.State(length=1500.0, left=0.4, beat=8.2),
    timer.State(length=1500.0, left=700.0, paused=True, beat=1.6),
    timer.State(length=1500.0, left=0.0, done=True, over=75.0, beat=2.6),
    timer.State("break", 300.0, 290.0, index=3, round=2, rounds=4, turn=0.4),
    timer.State("work", 20000.0, 17000.0, index=2, round=2),
]


@pytest.mark.parametrize("look", LOOKS)
@pytest.mark.parametrize("state", STATES)
def test_bounds(state: timer.State, look: timer.Look) -> None:
    faces = timer.ladder(look)
    assert len(faces) <= 16
    assert list(faces) == sorted(faces, key=lambda s: (s[0] * s[1], s))
    for w, h in faces:
        for hover in (False, True):
            rows = timer.face(state, w, h, look, hover)
            assert 0 < len(rows) <= h, (w, h)
            for text in plain(rows):
                assert len(text) <= w, (w, h, text)
                assert all(unicodedata.east_asian_width(c) not in "WF" for c in text)
                assert all(unicodedata.category(c)[0] != "C" for c in text)
            if not look.colour:
                assert all(
                    run.get("fg")
                    in (None, "white", "yellow", "bright-yellow", clock.DIM)
                    for row in rows
                    for run in row
                )


def test_faces() -> None:
    state = timer.State(length=1500.0, left=612.4)
    assert plain(timer.face(state, 5, 1, PLAIN)) == ["10:13"]
    assert plain(timer.face(state, 12, 1, PLAIN)) == ["10:13 ━━╸───"]
    assert plain(timer.face(state, 20, 2, PLAIN)) == [
        "10:13       of 25:00",
        "━━━━━━━━────────────",
    ]
    for size, font in (((17, 3), clock.SMALL), ((32, 5), clock.LARGE)):
        rows = plain(timer.face(state, *size, PLAIN), size[0])
        assert clock.decode(rows, font) == "10:13"
    rows = plain(timer.face(state, 32, 8, PLAIN), 32)
    assert clock.decode(rows[:5], clock.LARGE) == "10:13"
    assert rows[6].startswith("━" * 13 + "─") and "of 25:00" in rows[7]
    long = timer.State(length=20000.0, left=17000.0)
    assert plain(timer.face(long, 5, 1, PLAIN)) == ["4h44m"]
    assert plain(timer.face(long, 12, 1, PLAIN))[0].startswith("4:43:20 ")
    assert (
        clock.decode(plain(timer.face(long, 17, 3, PLAIN), 17), clock.SMALL) == "04:44"
    )


def test_labels() -> None:
    def said(state: timer.State) -> str:
        return plain([timer.label(state)])[0]

    assert said(timer.State("work", 10.0, 5.0, round=2, rounds=4)) == "work 2/4"
    assert said(timer.State("break", 10.0, 5.0, round=7)) == "break 7"
    assert said(timer.State(length=90.0, left=5.0, paused=True)) == "paused of 01:30"
    assert said(timer.State(left=0.0, done=True)) == "done"
    assert said(timer.State(left=0.0, done=True, over=75.0)) == "done +1:15"


def test_bar() -> None:
    assert plain([timer.bar(1.0, 6)]) == ["━━━━━━"]
    assert plain([timer.bar(0.0, 6)]) == ["──────"]
    assert plain([timer.bar(0.5, 6)]) == ["━━━───"]
    assert plain([timer.bar(0.25, 6)]) == ["━╸────"]
    assert plain([timer.bar(7.0, 3)]) == ["━━━"]


def test_tone() -> None:
    def at(left: float, **more: object) -> object:
        return timer.tone(timer.State(length=100.0, left=left, **more), FANCY)

    assert timer.tone(timer.State(), PLAIN) is None
    assert [at(90.0), at(30.0), at(15.0), at(5.0)] == [
        "bright-green",
        "bright-yellow",
        208,
        "bright-red",
    ]
    assert at(90.0, name="break") == "bright-cyan"


def test_hover_and_title() -> None:
    look = timer.Look(glass=True)
    state = timer.State(length=1500.0, left=612.4, paused=True)
    quiet = plain(timer.face(state, 9, 10, look), 9)
    over = plain(timer.face(state, 9, 10, look, True), 9)
    assert "10:13" not in "".join(quiet)
    assert over[4] == "  10:13  " and over[:4] == quiet[:4] and over[5:] == quiet[5:]
    assert "11m" in plain(timer.face(state, 5, 6, look, True), 5)[2]
    assert timer.title(state, (9, 10), look) == "10:13"
    assert timer.title(state, (5, 6), look) == "11m"
    assert timer.title(state, (27, 8), look) == "timer"
    assert timer.title(state, (5, 1), look) == ""
    assert timer.title(replace(state, done=True), (9, 10), look) == "done"
    assert timer.title(state, (17, 3), timer.Look(name="tea")) == "tea"
    hover = timer.Hover()
    assert hover.on_move(3, 4) is False and hover.at == (3, 4)
    assert hover.on_leave() is False and hover.at is None


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert timer.main(["25m", "--print", "--at", "0.4"]) == 0
    out = capsys.readouterr().out
    assert "5x1\n" in out and "32x8\n" in out and "│15:00│" in out
    assert timer.main(["--pomodoro", "25/5", "-a", "--print", "9x10", "--at", "1"]) == 0
    out = capsys.readouterr().out
    assert out.startswith("9x10\n") and " done " in out and "\x1b" not in out
    assert timer.main(["--print", "30x9", "-g"]) == 0
    assert capsys.readouterr().out.startswith("27x8\n")
    assert timer.main(["--print", "2x1"]) == 1


@pytest.mark.parametrize(
    "argv",
    [["5m", "--until", "17:30"], ["--pomodoro", "25"], ["--rounds", "-1"], ["0m"]],
)
def test_bad_options(argv: list[str], capsys: pytest.CaptureFixture[str]) -> None:
    with pytest.raises(SystemExit) as exc:
        timer.main([*argv, "--print"])
    assert exc.value.code == 2
    capsys.readouterr()


def test_all() -> None:
    args = timer.options(["-a"])
    assert args.glass and args.colour and args.ring and args.burst
    assert not args.modal and not args.click


def test_live(session: Pilot) -> None:
    """A short run with all: a glass with the time as its title or
    digits next to it, then the end: the border `alert`, confetti, and
    the app leaves by itself."""
    argv = ["-s", session.session, "6s", "-a", "--hold", "4", "--seed", "1"]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(timer.main(argv, seconds=60))
    )
    thread.start()
    running = ended = burst = False
    sizes = set(timer.ladder(FANCY))
    while thread.is_alive():
        widgets = [w for w in session.widgets() if w["tag"] == "timer"]
        if widgets and widgets[0]["state"] == "placed":
            info = widgets[0]
            assert tuple(info["size"]) in sizes
            x, y, w, h = info["rect"]
            inside = [
                line[x + 1 : x + w - 1] for line in session.shown()[y + 1 : y + h - 1]
            ]
            if info["border"] == "alert":
                ended = ended or any("done" in line for line in inside)
                burst = burst or len(session.sprites()) > 1
            else:
                running = running or any("╲" in line for line in inside)
        session_clock.sleep(0.02)
    thread.join()
    assert status == [0]
    assert running and ended and burst
