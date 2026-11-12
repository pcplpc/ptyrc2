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
from pathlib import Path

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
    page(session, "", "", "", "", "", "", "", "", "a flower", "", "", "")
    args = ["-s", session.session, "--seed", "1"]
    bold: list[dict[str, object]] = []

    def seen(host: str) -> bool:
        rows = host.split("\n")
        # Planted off the word's row, the word bold over the text.
        tints = [s for s in session.sprites() if s.get("blend") == "tint"]
        bold.extend(tints)
        return rows[8] == "a flower" and any("|" in r for r in rows[:8] + rows[9:])

    watch(session, lambda: flower.main(args, seconds=2.0), seen)
    assert bold


def test_flower_place() -> None:
    rng = random.Random(1)
    small = flower.SHAPES["small"]
    cold = ["." * 40] * 20
    x0, y0 = flower.place(cold, (10, 10, 6), small, set(), rng)
    # The nearest root: 3 columns past an end of the word, a row off it.
    root = (x0 + small.root, y0 + small.h - 1)
    assert root in {(7, 9), (7, 11), (18, 9), (18, 11)}
    # Each cell must be cold and free.
    hot = [row[:17] + "h" * 23 for row in cold]
    x0, y0 = flower.place(hot, (10, 10, 6), small, set(), rng)
    assert x0 + small.root == 7
    taken = {(7, 9), (7, 11)}
    x0, y0 = flower.place(hot, (10, 10, 6), small, taken, rng)
    assert x0 + small.root < 7 and abs(y0 + small.h - 1 - 10) in (1, 2, 3)
    assert flower.place(["t" * 40] * 20, (10, 10, 6), small, set(), rng) is None
    # Out of the view: none.
    assert flower.place(cold[:2], (0, 0, 6), flower.SHAPES["large"], set(), rng) is None


def test_flower_anywhere(session: Pilot) -> None:
    page(session, "", "", "", "", "", "", "", "", "garden of stones", "", "", "")
    args = ["-s", session.session, "-a", "--seed", "1"]
    tints: list[dict[str, object]] = []

    def seen(host: str) -> bool:
        rows = host.split("\n")
        tints.extend(s for s in session.sprites() if s.get("blend") == "tint")
        return any("|" in r for r in rows[:8] + rows[9:])

    watch(session, lambda: flower.main(args, seconds=2.0), seen)
    assert not tints  # no bold with -a


def test_flower_pool() -> None:
    rng = random.Random(1)
    rows = ["the quick brown fox, Quick!", "a jumps 1234 over"]
    words = flower.pool(rows, [], 3, rng)
    assert len(words) == 3 and set(words) <= {"quick", "brown", "jumps", "over"}
    # Kept while in view, the rest drawn anew.
    assert flower.pool(rows, ["over", "gone"], 2, rng)[0] == "over"
    assert flower.pool(rows, ["over", "gone"], 2, rng)[1] != "gone"
    assert flower.pool([""], ["over"], 3, rng) == []


def test_flower_not_between_words() -> None:
    rng = random.Random(1)
    small = flower.SHAPES["small"]
    # Free cells one column wide between words: no room there.
    gaps = ["tt.tt.tt.tt.tt.tt.tt.tt.tt.tt"] * 12
    assert flower.place(gaps, (10, 6, 3), small, set(), rng) is None


def test_flower_shapes() -> None:
    for shape in flower.SHAPES.values():
        steps = (*shape.grow, *shape.sway)
        assert all(len(rows) == shape.h for rows in steps)
        assert all(len(row) == shape.w for rows in steps for row in rows)
        assert all(rows[-1][shape.root] != " " for rows in steps)  # the root


def test_flower_paint() -> None:
    colors = flower.Colors(petals=1, alt=2, heart=3, stem=4)
    rows = flower.paint(flower.SHAPES["large"].sway[1], colors, 5)
    assert len(rows) == 7 and all(len(row) == 7 for row in rows)
    assert rows[2][3] == {"t": "o", "fg": 3, "bold": True}
    assert rows[0][1]["fg"] == 1 and rows[0][2]["fg"] == 2
    assert rows[6][3] == {"t": "|", "fg": 4} and rows[0][0] == {"t": " "}
    assert all(
        r.get("dim") for row in flower.paint(["ab"], colors, 1, True) for r in row
    )


def until(ok: Callable[[], bool], seconds: float) -> bool:
    """Poll `ok` every 0.1 s, for at most `seconds`."""
    deadline = time.monotonic() + seconds
    while not ok():
        if time.monotonic() > deadline:
            return False
        time.sleep(0.1)
    return True


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


def test_fishes_tide() -> None:
    rng = random.Random(1)
    box = (0, 100, 80, 123)  # x in [0, 74], y in [100, 123]
    # Three in view, three left out by a scroll.
    cells = [(0, 100), (74, 123), (40, 110), (10, 99), (75, 110), (40, 0)]
    due, going = fishes.tide(cells, [None] * 6, box, 0.0, rng)
    assert going is None and due[:3] == [None] * 3
    assert all(fishes.WAIT[0] <= d <= fishes.WAIT[1] for d in due[3:])
    # They go one a call, in the order of their times, and come back.
    order = sorted(range(3, 6), key=lambda i: due[i])
    gone = []
    for tick in range(60):
        due, going = fishes.tide(cells, due, box, tick * 0.1, rng)
        if going is not None:
            gone.append(going)
            cells[going] = (20, 110)
    assert gone == order and due == [None] * 6
    # The same time for all: still one a call.
    due, going = fishes.tide([(0, 0)] * 3, [1.0] * 3, box, 2.0, rng)
    assert due.count(None) == 1 and due[going] is None
    # Back in view before its time: it carries on.
    due, _ = fishes.tide([(10, 99)], [None], box, 0.0, rng)
    assert fishes.tide([(10, 100)], due, box, 9.0, rng) == ([None], None)


def test_fishes_entry() -> None:
    rng = random.Random(2)
    box = (10, 100, 30, 102)
    # The only spot where the text hides a whole fish.
    rows = ["  ab cd", "  ab cdefgh  ab", "x y z"]
    assert {fishes.entry(rows, box, rng) for _ in range(20)} == {(15, 101)}
    # No text: a side of the view.
    spots = {fishes.entry(["", "", ""], box, rng) for _ in range(50)}
    assert {x for x, _ in spots} == {10, 24} and {y for _, y in spots} <= {
        100,
        101,
        102,
    }


def test_fishes_shy() -> None:
    rng = random.Random(3)
    box = (0, 100, 80, 123)
    # A block of text, rows 105 to 109, columns 40 to 69; the rest empty.
    rows = [" " * 40 + "x" * 30 if 5 <= y < 10 else "" for y in range(24)]
    spots = fishes.hideouts(rows, box)
    assert set(spots) == {(x, y) for x in range(39, 66) for y in range(105, 110)}

    def school(shy: bool) -> list[fishes.Fish]:
        return [
            fishes.Fish(x=x, y=y, vx=fishes.CALM, color="cyan", shy=shy)
            for x, y in [(20.0, 102.0), (74.0, 123.0), (0.0, 100.0)]
        ]

    def tick(fish: fishes.Fish, cursor: tuple[int, int] | None = None) -> None:
        fishes.lure(fish, spots, cursor, 0.1, rng)
        fishes.swim(fish, 0.1, box, cursor, False, rng)

    # Shy: over the text within 8 s, and still there after; it moves on.
    for fish in school(True):
        homes = set()
        for t in range(400):
            tick(fish)
            homes.add(fish.home)
            if t >= 80:
                assert fish.cell[:2] in spots, (t, fish)
        assert len(homes) > 2
    # Bold: no home, never slows down.
    for fish in school(False):
        for _ in range(400):
            tick(fish)
            assert fish.home is None and abs(fish.vx) >= fishes.CALM / 2
    # Scared: it flees first, even with a safe home ahead.
    fish = fishes.Fish(x=30.0, y=105.0, vx=fishes.CALM, color="cyan", shy=True)
    tick(fish, (38, 105))
    assert fish.home in spots
    assert fishes.reach(*fish.home, (38, 105)) >= fishes.NEAR_MOVING
    assert fish.vx == -fishes.FEAR
    # A home in the cursor's reach is given up.
    fish = fishes.Fish(x=40.0, y=105.0, vx=0.0, color="cyan", shy=True)
    fish.home = (40, 105)
    assert fishes.lure(fish, spots, (43, 106), 0.1, rng) != (40, 105)
    assert fishes.lure(fish, [(40, 105)], (43, 106), 0.1, rng) is None


def test_fishes_hide_behind_text(
    session: Pilot, monkeypatch: pytest.MonkeyPatch
) -> None:
    # Text on the right of rows 0 to 7; the rest of the screen empty.
    page(session, *[" " * 40 + "o" * 39] * 8)
    settle(session)
    born: list[fishes.Fish] = []
    real = fishes.Fish

    def fish(**kw: object) -> fishes.Fish:
        born.append(real(**kw))  # type: ignore[arg-type]
        return born[-1]

    monkeypatch.setattr(fishes, "Fish", fish)
    args = ["-s", session.session, "--seed", "6"]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(fishes.main(args, seconds=12))
    )
    thread.start()
    time.sleep(10)
    frame = session.frame("largest")
    cover = fishes.cover(frame.text(), fishes.view(frame))
    shy = [f for f in born if f.shy]
    hidden = [f for f in shy if f.shown and cover.get(f.shown[:2], 0) >= 5]
    assert len(born) == 8 and len(shy) == 4
    assert len(hidden) >= 3, [(f.shown, f.home) for f in shy]
    assert len(session.sprites()) == 8
    thread.join()
    assert status == [0]


def test_fishes_come_back_after_a_scroll(tmp_path: Path) -> None:
    text = tmp_path / "long.txt"
    text.write_text("".join(f"line {i}: the quick brown fox\n" for i in range(300)))
    vim = ["/usr/bin/vim", "-u", "NONE", "-N", str(text)]
    with Pilot.spawn(vim, size=(80, 24)) as pilot:
        pilot.wait_for("line 1:")
        settle(pilot)

        def in_view() -> int:
            # A sprite out of the view has a screen cell out of the pane.
            pane = pilot.frame("largest")
            return sum(
                1
                for s in pilot.sprites()
                if s["screen"] and pane.to_frame(*s["screen"]) is not None
            )

        args = ["-s", pilot.session, "--seed", "5"]
        status: list[int] = []
        thread = threading.Thread(
            target=lambda: status.append(fishes.main(args, seconds=15))
        )
        thread.start()
        assert until(lambda: in_view() == 8, 5)
        pilot.keys("ctrl+d", "ctrl+d")  # 22 rows down: most are left above
        assert until(lambda: pilot.frame("largest").origin[1] >= 22, 2)
        time.sleep(0.3)
        # Left behind, not removed: they go later, one by one.
        assert len(pilot.sprites()) == 8 and in_view() < 8
        counts = []
        assert until(lambda: counts.append(in_view()) or counts[-1] == 8, 9)
        assert len(set(counts)) > 2, counts
        sprites = pilot.sprites()
        assert len(sprites) == 8 and all(s["layer"] < 0 for s in sprites)
        thread.join()
        assert status == [0]


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
