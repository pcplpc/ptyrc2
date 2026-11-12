# SPDX-License-Identifier: GPL-3.0-or-later
"""Each app runs against a headless driver and cleans up after itself."""

from __future__ import annotations

import itertools
import random
import re
import threading
import time
from collections.abc import Callable
from datetime import datetime

import pytest
from ptyrc import Pilot
from ptyrc_apps import APPS, clock, confetti, fishes, flower, sand, snake
from ptyrc_apps.__main__ import main


def shown(pilot: Pilot) -> str:
    return "\n".join(pilot.dump()["host"])


def watch(pilot: Pilot, run: Callable[[], int], seen: Callable[[str], bool]) -> None:
    """Run an app in a thread; `seen` must hold on the host at some point,
    while the app runs or its persistent sprites play, and the host must
    be clean once they are gone."""
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(run()))
    thread.start()
    ok = False
    while thread.is_alive():
        ok = ok or seen(shown(pilot))
        time.sleep(0.02)
    thread.join()
    assert status == [0]
    deadline = time.monotonic() + 3
    while pilot.sprites() and time.monotonic() < deadline:
        ok = ok or seen(shown(pilot))
        time.sleep(0.02)
    assert ok, "the app never showed"
    assert pilot.sprites() == []
    assert pilot.dump()["host"] == pilot.text()


class Clock:
    """Stands for `time` in `confetti`: a sleep moves the clock at once."""

    def __init__(self) -> None:
        self.now = 0.0

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.now += max(0.0, seconds)


def record(
    monkeypatch: pytest.MonkeyPatch, clock: Clock, stop: int | None = None
) -> list[tuple[float, tuple[int, int]]]:
    """Patch `confetti.burst` to log `(time, origin)`; ^C after `stop`."""
    log: list[tuple[float, tuple[int, int]]] = []
    real = confetti.burst

    def burst(*args: object, origin: tuple[int, int], **kw: object) -> object:
        log.append((clock.now, origin))
        if stop is not None and len(log) > stop:
            raise KeyboardInterrupt
        return real(*args, origin=origin, **kw)

    monkeypatch.setattr(confetti, "time", clock)
    monkeypatch.setattr(confetti, "burst", burst)
    return log


def apart(origins: list[tuple[int, int]]) -> bool:
    return all(
        max(abs(a[0] - b[0]), abs(a[1] - b[1])) > 3
        for a, b in itertools.combinations(origins, 2)
    )


def test_confetti(session: Pilot) -> None:
    args = ["-s", session.session, "--duration", "0.6", "--bursts", "2", "--sparkle"]
    watch(
        session,
        lambda: confetti.main(args, random.Random(3)),
        lambda host: sum(host.count(c) for c in "❋✦*·") >= 10,
    )


def test_confetti_bursts(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    log = record(monkeypatch, Clock())
    args = ["-s", session.session, "--bursts", "3", "--duration", "0.3"]
    assert confetti.main(args, random.Random(1)) == 0
    times = [t for t, _ in log]
    assert len(set(times)) == 3 and all(0 <= t < confetti.PERIOD for t in times)
    assert apart([o for _, o in log]), log
    with pytest.raises(SystemExit):
        confetti.main(["--bursts", "11"])


def test_confetti_stays_behind_busy_text(session: Pilot) -> None:
    # A busy screen: every other cell holds text.
    session.send(
        "clear; for i in $(seq 20); do echo 'ab cd ef gh ij kl mn op qr st'; done\n"
    )
    session.wait_for("st", frame="raw")
    time.sleep(0.3)
    args = ["-s", session.session, "--duration", "0.8", "--size", "200", "--sparkle"]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(confetti.main(args, random.Random(5)))
    )
    thread.start()
    pieces = samples = 0
    while thread.is_alive() or session.sprites():
        # One call: the app screen and what the host shows, together.
        dump = session.dump()
        for app, host in zip(dump["app"], dump["host"], strict=True):
            for a, h in zip(app, host.ljust(len(app)), strict=False):
                assert a == " " or a == h, (app, host)
        pieces += sum(row.count(c) for row in dump["host"] for c in "❋✦*·")
        samples += 1
        time.sleep(0.01)
    thread.join()
    assert status == [0] and samples > 5
    assert pieces > 0, "the confetti never showed between the words"


def test_confetti_party(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    log = record(monkeypatch, Clock(), stop=9)
    args = ["-s", session.session, "--party", "--bursts", "3", "--sparkle"]
    assert confetti.main(args, random.Random(2)) == 0
    # Three rounds of three, one round per period; then ^C.
    rounds = [int(t // confetti.PERIOD) for t, _ in log[:9]]
    assert rounds == [0, 0, 0, 1, 1, 1, 2, 2, 2], log
    assert all(apart([o for _, o in log[i : i + 3]]) for i in range(7)), log
    # Party bursts die with the connection, at once: none has played out.
    deadline = time.monotonic() + 1
    while session.sprites() and time.monotonic() < deadline:
        time.sleep(0.02)
    assert session.sprites() == []


def test_confetti_spot() -> None:
    rng = random.Random(7)
    # Left half hot, right half static text and free cells.
    classes = ["h" * 20 + "t." * 10] * 10
    avoid: list[tuple[int, int]] = []
    for _ in range(2000):
        x, y = confetti.spot(classes, rng, avoid)
        assert 20 <= x < 40 and 0 <= y < 10  # never on the hot side
        assert apart([*avoid, (x, y)])  # away from the last two
        avoid = [*avoid, (x, y)][-2:]
    # All hot: still a cell. Smaller than the gaps: still a cell.
    x, y = confetti.spot(["hhh"] * 2, rng, [(0, 0), (1, 1)])
    assert 0 <= x < 3 and 0 <= y < 2
    assert confetti.spot([], rng) == (0, 0)


def test_confetti_party_avoids_hot(
    session: Pilot, monkeypatch: pytest.MonkeyPatch
) -> None:
    # Rows 0 to 11, columns 0 to 78, rewritten five times a second; the
    # rest stays blank.
    session.send(
        "a=$(printf '%079d' 0); b=$(echo $a | tr 0 o); while :; do"
        " for l in $a $b; do printf '\\033[H'; for r in 1 2 3 4 5 6 7 8 9 10 11 12;"
        " do echo $l; done; sleep 0.2; done; done\n"
    )
    session.wait_for("ooooo", frame="raw")
    time.sleep(0.5)
    hot = session.frame("raw").heat_classes()
    assert all(set(row[:79]) == {"h"} for row in hot[:12]), hot
    log = record(monkeypatch, Clock(), stop=12)
    args = ["-s", session.session, "--party", "--bursts", "2", "--duration", "0.3"]
    assert confetti.main(args, random.Random(2)) == 0
    assert len(log) == 13
    assert all(y >= 12 for _, (_, y) in log), log


def page(pilot: Pilot, *lines: str) -> None:
    """Fill the screen of the shell with `lines`."""
    text = "\\n".join(lines)
    pilot.send(f"clear; printf '{text}\\n'\n")
    pilot.wait_for(max(lines, key=len).strip(), frame="raw")
    time.sleep(0.3)


def test_flower(session: Pilot) -> None:
    page(session, "", "", "", "", "", "a flower", "", "", "")
    args = ["-s", session.session]
    stages: set[str] = set()

    def seen(host: str) -> bool:
        rows = host.split("\n")
        # Right of the word, one cell away, centred on its row.
        if rows[5].startswith("a flower  ( o )"):
            stages.add("bud")
        if rows[4].startswith("          ) o (") and rows[5].startswith("a flower ( /"):
            stages.add("open")
        return stages == {"bud", "open"}

    watch(session, lambda: flower.main(args, seconds=1.6), seen)


def test_flower_paint() -> None:
    rows = flower.paint(flower.SWAY[1], "red")
    assert len(rows) == 7 and all(len(row) == 7 for row in rows)
    assert rows[2][3] == {"t": "o", "fg": "bright-yellow", "bold": True}
    assert rows[0][1]["fg"] == "red" and rows[6][3]["fg"] == "green"
    assert all(len(set(map(len, frame))) == 1 for frame in flower.SWAY)


def settle(pilot: Pilot) -> None:
    """Wait for a pane: a new layout is a candidate for 1.5 s, and
    nothing is drawn on it before."""
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert pilot.panes()


def test_fishes(session: Pilot) -> None:
    settle(session)
    args = ["-s", session.session, "--seed", "2", "--count", "4"]
    watch(
        session,
        lambda: fishes.main(args, seconds=1.0),
        lambda host: host.count(fishes.LEFT) + host.count(fishes.RIGHT) >= 2,
    )


def test_fish_flees_the_cursor() -> None:
    rng = random.Random(1)
    box = (0, 0, 80, 24)
    fish = fishes.Fish(x=30.0, y=10.0, vx=-fishes.CALM, color="cyan")
    # Swimming left, towards a cursor at its nose: it turns and bolts.
    fishes.swim(fish, 0.1, box, (28, 10), False, rng)
    assert fish.vx == fishes.FEAR and fish.cell[2] == fishes.RIGHT
    for _ in range(20):
        fishes.swim(fish, 0.1, box, (28, 10), False, rng)
    assert fish.scared <= 0 and 0 < fish.vx <= fishes.CALM  # calm again, away
    # A far cursor frightens only when it moves.
    far = fishes.Fish(x=40.0, y=10.0, vx=-1.0, color="cyan")
    fishes.swim(far, 0.1, box, (28, 10), False, rng)
    assert far.vx == -1.0
    fishes.swim(far, 0.1, box, (28, 10), True, rng)
    assert far.vx == fishes.FEAR
    # Walls turn it around.
    edge = fishes.Fish(x=0.2, y=0.0, vx=-5.0, color="cyan")
    fishes.swim(edge, 0.1, box, None, False, rng)
    assert edge.x == 0 and edge.vx == 5.0


def test_sand(session: Pilot) -> None:
    page(session, "", "", "", "", "", "", "", "", "      a shelf of words")
    args = ["-s", session.session, "--now", "--rate", "400"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(sand.main(args, seconds=4)))
    thread.start()
    piled = swept = False
    typed = 0.0
    while thread.is_alive():
        dump = session.dump()
        for app, host in zip(dump["app"], dump["host"], strict=True):
            for a, h in zip(app, host.ljust(len(app)), strict=False):
                assert a == " " or a == h, (app, host)  # never on the text
        if not piled and "▒" in dump["host"][7][6:22]:
            piled = True  # grains rest on the words
            session.send("echo busy\n")
            typed = time.monotonic()
        if piled and time.monotonic() - typed > 0.2 and not session.sprites():
            swept = True  # the pane is used: the sand is gone
        time.sleep(0.02)
    thread.join()
    assert status == [0] and piled and swept
    assert session.dump()["host"] == session.text()


def test_snake(session: Pilot) -> None:
    page(session, "", "", "words in the way", "", "   more words here")
    settle(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "30"]
    watch(
        session,
        lambda: snake.main(args, seconds=1.5),
        lambda host: "@" in host and "●" in host,
    )


def test_snake_game() -> None:
    rows = ["", "  wall of words  ", "", "   another   one", ""]
    walls = snake.walls_of(rows, 10, 100)
    assert (12, 101) in walls and (10, 101) not in walls and (16, 101) not in walls
    assert snake.walls_of(["a世b"]) == {(0, 0), (1, 0), (2, 0), (3, 0)}
    # Around a wall: the first step of a shortest path.
    blocked = {(1, 0), (1, 1)}
    step = snake.route((0, 0), (2, 0), lambda c: c not in blocked and 0 <= c[1] < 3)
    assert step == (0, 1)
    assert snake.route((0, 0), (2, 0), lambda c: False) is None
    game = snake.Game(random.Random(3))
    box = (10, 100, 30, 105)
    for _ in range(400):
        game.step(box, walls)
        cells = list(game.body)
        assert len(set(cells)) == len(cells)  # never over itself
        assert not walls & set(cells)  # never over a word
        assert all(10 <= x < 30 and 100 <= y < 105 for x, y in cells)
        steps = itertools.pairwise(cells)
        assert all(abs(a[0] - b[0]) + abs(a[1] - b[1]) == 1 for a, b in steps)
    assert game.eaten >= 5
    at, cells = game.picture()
    assert at == (min(x for x, _ in game.body), min(y for _, y in game.body))
    # Scrolled out of sight: it starts again in the new view.
    game.step((10, 200, 30, 205), set())
    assert len(game.body) <= 2 and 200 <= game.body[0][1] < 205


def test_clock(session: Pilot) -> None:
    def seen(host: str) -> bool:
        top = host.split("\n")[0]
        return bool(re.search(r" \d\d:\d\d:\d\d$", top)) and len(top) == 79

    watch(session, lambda: clock.main(["-s", session.session], seconds=1.2), seen)


def test_clock_big_bottom_left(session: Pilot) -> None:
    def seen(host: str) -> bool:
        bottom = host.split("\n")[-3:]
        return any(c in "".join(bottom) for c in "█▀▄") and bottom[-1][:1] in "█▀▄"

    args = ["-s", session.session, "--big", "--prefer", "bottom-left"]
    watch(session, lambda: clock.main(args, seconds=1.2), seen)


def test_clock_face() -> None:
    row = clock.face(datetime(2026, 11, 12, 9, 5, 7))[0]
    assert "".join(run["t"] for run in row) == " 09:05:07 "


def test_dispatch(
    session: Pilot,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(confetti, "time", Clock())
    assert main([]) == 2
    out = capsys.readouterr().out
    assert all(name in out for name in APPS)
    assert main(["nope"]) == 2
    assert main(["confetti", "-s", session.session, "--duration", "0.1"]) == 0
    assert main(["confetti", "-s", "no-such-session"]) == 1
    assert "ptyrc-app: gone" in capsys.readouterr().err


@pytest.mark.parametrize("name", sorted(APPS))
def test_no_time_limit(name: str, capsys: pytest.CaptureFixture[str]) -> None:
    # An app runs until ^C or the end of the session: no option for it.
    with pytest.raises(SystemExit):
        main([name, "--help"])
    assert "--sec" not in capsys.readouterr().out
    with pytest.raises(SystemExit) as exc:
        main([name, "--sec", "1"])
    assert exc.value.code == 2
