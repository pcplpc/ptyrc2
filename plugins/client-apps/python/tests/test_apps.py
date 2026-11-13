# SPDX-License-Identifier: GPL-3.0-or-later
"""Each app runs against a headless driver and cleans up after itself."""

from __future__ import annotations

import itertools
import os
import random
import re
import shutil
import subprocess
import threading
import time
from collections import deque
from collections.abc import Callable
from datetime import datetime
from pathlib import Path
from typing import Any

import pytest
from ptyrc import Frame, Match, Pilot, Sprite
from ptyrc_apps import (
    APPS,
    _arcade,
    _cli,
    cat,
    clock,
    confetti,
    fishes,
    flower,
    marks,
    rain,
    sand,
    snake,
)
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
    args = ["-s", session.session, "--one-shot", "--bursts", "2", "--sparkle"]
    watch(
        session,
        lambda: confetti.main(args, random.Random(3)),
        lambda host: any(
            a[x : x + 3] == "· ·" and b[x : x + 3] == " o " and c[x : x + 3] == "· ·"
            for a, b, c in zip(*(host.split("\n")[i:] for i in range(3)), strict=False)
            for x in range(len(a))
        ),
    )
    args += ["--fancy", "--duration", "0.6"]
    watch(
        session,
        lambda: confetti.main(args, random.Random(3)),
        lambda host: sum(host.count(c) for c in "❋✦*·") >= 10,
    )


def test_confetti_bursts(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    log = record(monkeypatch, Clock())
    args = [
        "-s",
        session.session,
        "--one-shot",
        "--bursts",
        "3",
        "--duration",
        "0.3",
        "--fancy",
    ]
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
    args = [
        "-s",
        session.session,
        "--one-shot",
        "--duration",
        "0.8",
        "--size",
        "200",
        "--sparkle",
    ]
    args.append("--fancy")
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
    args = ["-s", session.session, "--bursts", "3", "--sparkle", "--fancy"]
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


def calls(
    monkeypatch: pytest.MonkeyPatch, clock: Clock, stop: int | None = None
) -> list[tuple[float, str, dict[str, Any]]]:
    """Patch what `confetti` draws with to log `(time, name, arguments)`;
    ^C after `stop` calls."""
    log: list[tuple[float, str, dict[str, Any]]] = []

    def logged(name: str) -> Callable[..., object]:
        real = getattr(confetti, name)

        def call(pilot: Pilot, **kw: Any) -> object:
            log.append((clock.now, name, kw))
            if stop is not None and len(log) > stop:
                raise KeyboardInterrupt
            return real(pilot, **kw)

        return call

    monkeypatch.setattr(confetti, "time", clock)
    for name in ("pop", "burst", "rocket"):
        monkeypatch.setattr(confetti, name, logged(name))
    return log


def test_confetti_pops(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    # Plain: a burst is the four pops of the first ptyrc, in a second.
    log = calls(monkeypatch, Clock())
    args = ["-s", session.session, "--one-shot", "--bursts", "2"]
    assert confetti.main(args, random.Random(4)) == 0
    assert [name for _, name, _ in log] == ["pop"] * 8
    assert sorted(kw["color"] for _, _, kw in log) == sorted(confetti.COLORS * 2)
    assert all(kw["art"] is confetti.POP and kw["persist"] for _, _, kw in log)
    assert apart([kw["origin"] for _, _, kw in log]), log
    times = [t for t, _, _ in log]
    assert times == sorted(times) and times[-1] < confetti.PERIOD + 1.01
    # The driver plays them out and removes them.
    deadline = time.monotonic() + 3
    while session.sprites() and time.monotonic() < deadline:
        time.sleep(0.02)
    assert session.sprites() == []
    # `--random`: of several kinds.
    log.clear()
    args = ["-s", session.session, "--one-shot", "--bursts", "5", "--random"]
    assert confetti.main(args, random.Random(1)) == 0
    assert len({kw["art"] for _, _, kw in log}) > 2
    for art in confetti.POPS:
        assert len({(len(rows), *map(len, rows)) for rows in art}) == 1, art


def test_confetti_fireworks(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    log = calls(monkeypatch, Clock())
    args = [
        "-s",
        session.session,
        "--one-shot",
        "--bursts",
        "3",
        "--fireworks",
        "--fancy",
    ]
    assert confetti.main(args, random.Random(6)) == 0
    rockets = [(t, kw) for t, name, kw in log if name == "rocket"]
    bursts = [(t, kw) for t, name, kw in log if name == "burst"]
    assert len(rockets) == len(bursts) == 3 and len(log) == 6
    for at, shot in rockets:
        # From the last row of the view to a cell of its upper part,
        # where the explosion starts when the rocket stops.
        x, y = shot["origin"]
        assert y < 16 and y + shot["rise"] == 23
        (then, bang) = next(b for b in bursts if b[1]["origin"] == (x, y))
        assert then == pytest.approx(at + shot["seconds"])
        assert bang["shape"] == "ball"
    # Plain: a rocket for each pop.
    log.clear()
    args = ["-s", session.session, "--one-shot", "--fireworks"]
    assert confetti.main(args, random.Random(6)) == 0
    assert sorted(name for _, name, _ in log) == ["pop"] * 4 + ["rocket"] * 4


def test_confetti_all(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    log = calls(monkeypatch, Clock(), stop=40)
    args = ["-s", session.session, "-a", "--bursts", "4"]
    assert confetti.main(args, random.Random(8)) == 0
    assert {name for _, name, _ in log} == {"rocket", "burst"}
    bursts = [kw for _, name, kw in log if name == "burst"]
    assert {kw["shape"] for kw in bursts} == set(confetti.SHAPES)
    assert len({kw["size"] for kw in bursts}) > 5
    assert any("colors" in kw for kw in bursts)
    assert not all("colors" in kw for kw in bursts)
    assert len({kw["look"] for _, name, kw in log if name == "rocket"}) > 2


def test_confetti_rocket(session: Pilot) -> None:
    # The driver alone flies it: up a column, slower and slower, to its
    # origin; then it is gone.
    session.send("clear\n")
    time.sleep(0.3)
    confetti.rocket(
        session, origin=(30, 4), rise=18, seconds=0.9, look=("^", "!:'"), persist=True
    )
    rows: list[tuple[float, int]] = []
    start = time.monotonic()
    while session.sprites():
        heads = [y for y, row in enumerate(session.dump()["host"]) if row[30:31] == "^"]
        if heads:
            rows.append((time.monotonic() - start, heads[0]))
        time.sleep(0.01)
    ys = [y for _, y in rows]
    assert ys == sorted(ys, reverse=True) and ys[0] > 14 and ys[-1] <= 6, rows
    # More rows in the first half of the flight than in the second.
    mid = next(y for t, y in rows if t >= 0.45)
    assert ys[0] - mid > mid - ys[-1], rows


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


def test_confetti_spot_weights() -> None:
    """The pick is the one of the plain double loop, cell for cell."""

    def plain(
        classes: list[str], rng: random.Random, avoid: list[tuple[int, int]], reach: int
    ) -> tuple[int, int]:
        h, w = len(classes), max(map(len, classes))
        cold = [[c != "h" for c in row.ljust(w, ".")] for row in classes]
        cells, weights = [], []
        for y in range(h):
            for x in range(w):
                if any(abs(x - i) <= reach and abs(y - j) <= reach for i, j in avoid):
                    continue
                near = [
                    cold[j][i]
                    for j in range(max(0, y - reach), min(h, y + reach + 1))
                    for i in range(max(0, x - reach), min(w, x + reach + 1))
                ]
                cells.append((x, y))
                weights.append(sum(near) / len(near) if cold[y][x] else 0.0)
        if not any(weights):
            return rng.choice(cells)
        return rng.choices(cells, weights)[0]

    draw = random.Random(11)
    for n in range(60):
        w, h = draw.randint(8, 30), draw.randint(8, 14)
        classes = [
            "".join(draw.choice("hht.p") for _ in range(draw.randint(1, w)))
            for _ in range(h)
        ]
        classes[0] = classes[0].ljust(w, "h")
        avoid = [(draw.randrange(w), draw.randrange(h)) for _ in range(n % 3)]
        reach = draw.randint(0, 3)
        seed = draw.getrandbits(32)
        got = confetti.spot(classes, random.Random(seed), avoid, reach)
        assert got == plain(classes, random.Random(seed), avoid, reach), classes


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
    args = ["-s", session.session, "--bursts", "2", "--duration", "0.3"]
    args.append("--fancy")
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
    # Cut to `size`, the oldest kept; `avoid` not drawn.
    assert flower.pool(rows, ["over", "quick", "fox"], 1, rng) == ["over"]
    for _ in range(10):
        assert "over" not in flower.pool(rows, [], 4, rng, ["over"])


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


def grains(row: str) -> int:
    """Cells of `row` holding sand: braille."""
    return sum("\u2801" <= c <= "\u28ff" for c in row)


def test_sand(session: Pilot) -> None:
    page(session, "", "", "", "", "", "", "", "", "      a shelf of words")
    args = ["-s", session.session, "--now", "--rate", "400"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(sand.main(args, seconds=4)))
    thread.start()
    piled = stayed = False
    typed = 0.0
    while thread.is_alive():
        dump = session.dump()
        for app, host in zip(dump["app"], dump["host"], strict=True):
            for a, h in zip(app, host.ljust(len(app)), strict=False):
                assert a == " " or a == h, (app, host)  # never on the text
        on_shelf = grains(dump["host"][7][6:22])
        if not piled and on_shelf > 3:
            piled = True  # grains rest on the words
            assert all(grains(row) <= 3 for row in dump["host"][:5])  # a stream
            session.send("echo busy\n")
            typed = time.monotonic()
        if piled and time.monotonic() - typed > 0.5 and "busy" in dump["host"][10]:
            # The pane is used: the pile stays, the stream goes on.
            stayed = stayed or (on_shelf > 3 and len(session.sprites()) == 1)
        time.sleep(0.02)
    thread.join()
    assert status == [0] and piled and stayed
    # The driver takes the sand away when it sees the connection closed.
    assert until(lambda: session.sprites() == [], 2)
    assert session.dump()["host"] == session.text()


def test_sand_waits_for_idle(session: Pilot) -> None:
    page(session, "top")
    # Busy: no sand, and the wait ends with the app.
    assert sand.still(session, "idle", 0.3) is None
    assert sand.main(["-s", session.session], seconds=1) == 0
    assert session.sprites() == []
    # Left alone, the driver answers when the pane is idle.
    frame = sand.still(session, "idle", 15)
    assert frame is not None and frame.is_idle


def test_sand_survives_a_resize(session: Pilot) -> None:
    page(session, "top")
    args = ["-s", session.session, "--now", "--rate", "400"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(sand.main(args, seconds=8)))
    thread.start()
    resized = kept = False
    back = 0.0
    while thread.is_alive():
        floor = session.dump()["host"][-1]
        if not resized and grains(floor[34:46]) >= 10:
            resized = True  # a pile under the spout, mid-screen
            session.resize(50, 24)
        elif resized and not back:
            # A pane this empty is a new frame after a resize: the app
            # follows it.
            sprites = session.sprites()
            if sprites and sprites[0]["screen"] and sprites[0]["size"] == [50, 24]:
                back = time.monotonic()
        elif back and time.monotonic() - back < 1.0:
            # The pile is there, but for what was cut off; the new
            # stream has not reached the floor yet.
            kept = kept or (len(floor) <= 50 and grains(floor[34:46]) >= 10)
        time.sleep(0.02)
    thread.join()
    assert status == [0] and resized and kept


def test_sand_cursor(session: Pilot) -> None:
    page(session, "top", "", "", "")
    session.send("printf 'here: '\n")
    session.wait_for(r"^here: \$", regex=True)
    column = session.cursor().x
    assert column == 8
    args = ["-s", session.session, "--now", "--cursor"]
    watch(
        session,
        lambda: sand.main(args, seconds=1.5),
        lambda host: (
            (rows := host.split("\n"))
            and all(grains(r) == grains(r[column : column + 1]) for r in rows)
            and sum(grains(r) for r in rows[:4]) >= 3
        ),
    )


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
    # Scrolled out of sight: back in from the edge, whole.
    size = len(game.body)
    game.step((10, 200, 30, 205), set())
    assert not game.ghost and game.body[0][1] == 200
    assert size - 1 <= len(game.body) <= size


def test_snake_steer() -> None:
    """A player: waits for an arrow, goes straight, never back over its
    neck, stops at a wall, bites itself off."""
    game = snake.Game(random.Random(3), player=True)
    box = (0, 0, 10, 5)
    walls = {(9, 2)}
    game.step(box, walls)
    game.body, game.grow, game.apple = deque([(5, 2)]), 0, (0, 0)
    game.step(box, walls)
    assert list(game.body) == [(5, 2)]  # no arrow yet
    game.press("right")
    game.grow = 3
    for _ in range(3):
        game.step(box, walls)
    assert list(game.body) == [(8, 2), (7, 2), (6, 2), (5, 2)]
    game.press("left")  # back over the neck: ignored
    game.step(box, walls)
    assert game.body[0] == (8, 2)  # against the wall: waits
    game.grow = 2
    for key in ("down", "left", "up"):
        game.press(key)
        game.step(box, walls)
    # Into its own body: bitten off there.
    assert list(game.body) == [(7, 2), (7, 3), (8, 3)]
    assert ("lost", [(7, 2), (6, 2), (5, 2)]) in game.events


def test_snake_grab(session: Pilot) -> None:
    """`--grab`: the arrows are taken while it runs, given back after."""
    page(session, "", "", "words in the way", "", "   more words here")
    settle(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "30", "--grab"]
    grabbed = []

    def look(host: str) -> bool:
        grabbed.append(session.info()["grabbed"])
        return "←↑↓→  ctrl+t  ◆" in host and "@" in host

    watch(session, lambda: snake.main(args, seconds=1.5), look)
    assert any(grabbed) and not session.info()["grabbed"]


def test_snake_front(session: Pilot) -> None:
    """`--front`: the snake, the apple, foes and beams over the text."""
    page(session, "", "", "words in the way", "", "   more words here")
    settle(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "30", "-x", "-l"]
    layers: set[int] = set()

    def look(host: str) -> bool:
        layers.update(s["layer"] for s in session.sprites())
        return "@" in host and "●" in host

    watch(session, lambda: snake.main([*args, "--front"], seconds=1.5), look)
    assert layers and all(n > 0 for n in layers), layers


def test_snake_ghost() -> None:
    """Out of sight or under text: through anything, unhurt, to a safe cell."""
    rows = ["", "", "####################", "####################", "", "", ""]
    walls = snake.walls_of(rows)
    box = (0, 0, 20, 7)
    game = snake.Game(random.Random(2), enemies=True, lasers=True)
    game.body = deque([(5, -40 - i) for i in range(6)])
    game.beams = [snake.Beam(1, tuple((5, y) for y in range(2)), False, fuse=0)]
    safe = set(snake.room(box, walls, None, 0))
    seen = []
    for _ in range(10):
        game.step(box, walls)
        seen.append(game.ghost)
        assert len(game.body) == 6
        if not game.ghost:
            break
    # In from the edge, through the beam and the text, into the
    # largest region; solid on its first cell.
    assert seen[0] and not seen[-1] and game.body[0] == (5, 4) in safe
    # Text written on its head: it goes through the text.
    game = snake.Game(random.Random(2))
    game.body = deque([(5, 2), (5, 1), (5, 0)])
    game.step(box, walls)
    assert game.ghost
    runs = [run for row in game.picture()[1] for run in row if run["t"] != " "]
    assert all(run.get("dim") for run in runs) and len(runs) == 3
    game.step(box, walls)
    assert not game.ghost and list(game.body) == [(5, 4), (5, 3), (5, 2)]


def test_snake_room() -> None:
    """Apples go where the head can reach, never in a dead end."""
    rows = [
        "          #         ",
        "########  #  ###### ",
        "       #  #  #    # ",
        "       #  #  #### # ",
        "       #  #       # ",
        "########  ######### ",
        "                    ",
    ]
    walls = snake.walls_of(rows)
    box = (0, 0, 20, 7)
    ok = set(snake.room(box, walls, (0, 6), 10))
    assert {(8, 0), (9, 5), (11, 1), (19, 3), (15, 6)} <= ok
    assert (0, 6) not in ok  # the head
    assert not ok & {(0, 2), (6, 4)}  # shut in
    # One cell wide to nowhere: no way to turn back.
    assert not ok & {(0, 0), (7, 6), (14, 2), (13, 4)}
    # A long snake: out of a two wide pocket behind a bridge, too.
    long = set(snake.room(box, walls, (0, 6), 30))
    assert (11, 1) not in long and (8, 0) not in long and (19, 3) in long
    # No start: the largest region. Shut in: all of it.
    assert set(snake.room(box, walls, None, 10)) == ok
    shut = set(snake.room(box, walls, (0, 2), 10))
    assert len(shut) == 20 and (6, 4) in shut and (8, 0) not in shut
    assert snake.room(box, walls, (0, 1), 10) == []
    game = snake.Game(random.Random(1))
    for _ in range(300):
        game.step(box, walls)
        if game.apple is not None:
            assert game.apple in snake.room(box, walls, None, 1)
    assert game.eaten >= 5
    # The text shuts the apple in: it goes where the head can reach.
    x, y = game.apple
    ring = {(x + dx, y + dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1)} - {(x, y)}
    walls = walls - {(x, y)} | ring
    cut = walls - set(game.body)
    game.step(box, cut)
    reach = snake.room(box, cut, game.body[0], 1) + list(game.body)
    assert game.apple != (x, y) and game.apple in reach
    # Unchanged map: the apple stays.
    apple, eaten = game.apple, game.eaten
    game.step(box, cut)
    assert game.apple == apple or game.eaten > eaten


def test_snake_arcade(session: Pilot) -> None:
    page(session, "", "", "words in the way", "", "   more words here")
    settle(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "60", "-a"]
    seen: set[str] = set()

    def look(host: str) -> bool:
        seen.update(host)
        # The snake, an apple, a foe, a beam announced, a beam firing,
        # the score.
        return (
            "@" in seen
            and "◆" in seen
            and "●" in seen
            and bool(seen & {"x", "+"})
            and (bool(seen & {"╌", "╎"}) and bool(seen & {"━", "┃"}))
        )

    watch(session, lambda: snake.main(args, seconds=4), look)


def test_snake_over_its_particles(session: Pilot) -> None:
    page(session, "", "", "words in the way")
    settle(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "60", "-e"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(snake.main(args, seconds=3)))
    thread.start()
    sparks = best = 0
    while thread.is_alive():
        sprites = session.sprites()
        long = [s for s in sprites if s["z"] == snake.SNAKE_Z]
        sparks += sum(s["kind"] == "animation" and s["z"] == 3 for s in sprites)
        for row in session.dump()["host"][:1]:
            if found := re.search(r"◆ (\d+)  best (\d+)", row):
                assert int(found[1]) <= int(found[2])
                best = max(best, int(found[2]))
        assert len(long) <= 1
        time.sleep(0.05)
    thread.join()
    assert status == [0] and best >= 5
    # Particles were seen: they are under the snake.
    assert sparks and snake.SNAKE_Z > 3
    assert until(lambda: session.sprites() == [], 3)
    assert session.dump()["host"] == session.text()


def test_board_hides_what_passes_under(session: Pilot) -> None:
    """A score has gaps; a racket under them does not show through."""
    frame = session.frame("raw")
    session.draw(["[=================]"], at=(-20, 0), layer="back", z=2)
    board = _arcade.Board(session, frame)
    with session.batch():
        board.show(_arcade.tally(7, 12))

    def top() -> str:
        return session.dump()["host"][0]

    assert until(lambda: top().endswith("=◆ 7  best 12"), 2), top()
    sent = board.shown
    with session.batch():
        board.show(_arcade.tally(7, 12))  # the same: nothing sent
        assert board.shown is sent
        board.show(_arcade.tally(8, 12))
    assert until(lambda: top().endswith("=◆ 8  best 12"), 2), top()
    assert len(session.sprites()) == 2


def test_snake_rules() -> None:
    rows = ["", "  wall of words  ", "", "   another   one", "", "", ""]
    walls = snake.walls_of(rows, 10, 100)
    box = (10, 100, 40, 107)

    def inside(c: tuple[int, int]) -> bool:
        return 10 <= c[0] < 40 and 100 <= c[1] < 107

    game = snake.Game(random.Random(5), golden=True, enemies=True, lasers=True)
    seen: dict[str, int] = {}
    golden = 0
    for _ in range(3000):
        size = len(game.body)
        game.step(box, walls)
        cells = list(game.body)
        assert len(set(cells)) == len(cells)
        assert not walls & set(cells) and all(map(inside, cells))
        steps = itertools.pairwise(cells)
        assert all(abs(a[0] - b[0]) + abs(a[1] - b[1]) == 1 for a, b in steps)
        foes = [f.at for f in game.foes]
        assert len(set(foes)) == len(foes) <= snake.FOES
        assert not set(foes) & (walls | set(cells)) and all(map(inside, foes))
        assert len(game.beams) <= snake.BEAMS
        for beam in game.beams:
            assert len(beam.cells) >= 5 and not walls & set(beam.cells)
            line = {c[1] if beam.across else c[0] for c in beam.cells}
            assert len(line) == 1 and all(map(inside, beam.cells))
            if beam.firing:  # nothing stands in a beam that fires
                assert not set(beam.cells) & (set(cells) | set(foes))
        assert all(0 <= i < max(1, len(cells)) for i in game.lumps)
        for what, *data in game.events:
            seen[what] = seen.get(what, 0) + 1
            if what == "eat":
                assert data[0] == cells[0]
                golden += data[1]
            elif what == "lost":
                assert data[0] and len(cells) < size + 1
            elif what == "zap":
                assert data[0].firing and data[0].left == snake.FIRE
    assert seen.keys() == {"eat", "lost", "pop", "zap"}
    assert game.eaten == seen["eat"] >= 20 and 0 < golden < seen["eat"]
    # Same seed, same game; the flags off, the plain game.
    a, b = (snake.Game(random.Random(9), lasers=True) for _ in range(2))
    plain, old = snake.Game(random.Random(9)), snake.Game(random.Random(9))
    for _ in range(300):
        for one in (a, b, plain, old):
            one.step(box, walls)
        assert a.body == b.body and [x.cells for x in a.beams] == [
            x.cells for x in b.beams
        ]
        assert plain.body == old.body and not plain.foes and not plain.beams
        assert {e[0] for e in plain.events} <= {"eat", "lost"}


def test_snake_beam_and_bite() -> None:
    box = (0, 0, 20, 6)
    game = snake.Game(random.Random(1), lasers=True)
    game.body = deque([(x, 2) for x in range(8, 2, -1)])
    game.apple, game.beam_due = (19, 5), 10**9
    game.beams = [snake.Beam(1, tuple((5, y) for y in range(6)), False, fuse=1)]
    game.step(box, set())
    # The beam fires on the fourth segment: the tail is lost from there.
    assert [e[0] for e in game.events] == ["zap", "lost"]
    assert game.events[1][1][0] == (5, 2) and (5, 2) not in game.body
    assert list(game.body) == [(9, 2), (8, 2), (7, 2), (6, 2)]
    # Announced beams are walls to the snake when there is a way around.
    game = snake.Game(random.Random(1), lasers=True)
    game.body, game.apple, game.beam_due = deque([(2, 2)]), (8, 2), 10**9
    game.beams = [snake.Beam(1, tuple((5, y) for y in range(1, 6)), False)]
    for _ in range(snake.FUSE - 1):
        game.step(box, set())
        assert game.body[0][0] != 5 or game.body[0][1] == 0
    # A foe reaches the middle of the body and bites it off.
    game = snake.Game(random.Random(1), enemies=True)
    game.body = deque([(x, 2) for x in range(12, 4, -1)])
    game.apple, game.foe_due, game.steps = (19, 5), 10**9, 1
    game.foes = [snake.Foe(1, (9, 3))]
    game.step(box, set())
    assert [e[0] for e in game.events] == ["lost", "pop"] and not game.foes
    assert len(game.body) < 8


def test_snake_arcade_art() -> None:
    rng = random.Random(2)
    one = _arcade.spark(rng, (1, 2))
    assert one["loop"] is False and one["motion"]["ay"] > 0
    assert one["ttl"] * 1000 > sum(f["ms"] for f in one["frames"])
    assert len(_arcade.burst(7, rng, (1,))) == 7
    for across, firing in itertools.product((True, False), repeat=2):
        look = _arcade.beam(6, across, firing)
        for frame in look["frames"]:
            rows = ["".join(run["t"] for run in row) for row in frame["cells"]]
            assert (len(rows), len(rows[0])) == ((1, 6) if across else (6, 1))
    game = snake.Game(random.Random(1), golden=True)
    game.body, game.lumps = deque([(x, 0) for x in range(9, -1, -1)]), [4]
    _, cells = game.picture(fancy=True)
    row = cells[0][::-1]  # head first
    assert "".join(run["t"] for run in row) == "@oooOOoooo"
    assert row[1]["fg"] == _arcade.BODY[0] and row[-1]["fg"] == _arcade.BODY[-1]


TOWN = [
    "",
    "",
    "",
    "a long line of text, long enough for a cat to walk on it",
    "",
    "",
    "",
    "",
    "another line, as long as the first, for the cat to jump to",
]


def test_cat_art() -> None:
    for pose, frames in cat.POSES.items():
        for rows, _ in frames:
            assert [len(row) for row in rows] == [cat.W] * cat.H, pose
    # Seen from the other side, it looks the other way.
    left = cat.look("walk", right=False, step=0)["cells"]
    assert "".join(run["t"] for run in left[1]) == "(o.o)___.~ "
    assert len(cat.look("sleep")["frames"]) == len(cat.POSES["sleep"])
    # A skin changes glyphs, not the box; the plain one changes nothing.
    for ears, eyes, nose in itertools.product(cat.EARS, cat.EYES, cat.NOSES):
        skin = cat.Skin(ears, eyes, nose)
        for pose, frames in cat.POSES.items():
            for rows, _ in frames:
                assert [len(row) for row in cat.dress(rows, skin)] == [cat.W] * 3, pose
    assert cat.dress(cat.POSES["walk"][0][0], cat.PLAIN) == cat.POSES["walk"][0][0]
    odd = cat.Skin("|\\_/|", "*O", "w")
    assert cat.dress(cat.POSES["sit"][0][0], odd)[:2] == ("     |\\_/| ", "    ( *w* )")
    assert cat.dress(cat.POSES["yawn"][2][0], odd)[1] == "    ( -O- )"
    assert len({(c.fur, c.shade) for c in cat.COATS}) == len(cat.COATS)
    grey = cat.look("alert", coat=cat.COATS[1])["cells"][1]
    assert {run.get("fg") for run in grey} == {None, 250, 117, 244}
    # The plate and the mouse.
    assert {len(row) for row in cat.PLATE} == {cat.PW}
    assert {len(row) for row in cat.MOUSE} == {cat.MW}
    left = cat.mouse_look(right=False, step=0)["cells"][0]
    assert "".join(run["t"] for run in left) == '<°",_,)~~'


def test_cat_ledges() -> None:
    box = (0, 100, 60, 112)
    spots = cat.ledges(TOWN, box)
    # On each line, and on the floor of the view; frame cells.
    assert [s.g for s in spots] == [103, 108, 112]
    first = spots[0]
    assert (first.x0, first.x1) == (0, 60 - cat.W)
    # It may hang over the end of a line, not stand on nothing.
    assert cat.ledges(["", "", "", "0123456789"], (0, 0, 40, 6))[0].x1 == 5
    assert cat.perch(spots, 5, 103) == first and cat.perch(spots, 5, 104) is None
    # Text in the box of the cat: no ledge there.
    rows = [*TOWN]
    rows[2] = " " * 20 + "words"
    cut = [s for s in cat.ledges(rows, box) if s.g == 103]
    assert [(s.x0, s.x1) for s in cut] == [(0, 20 - cat.W - 1), (26, 60 - cat.W)]


def test_cat_arc_and_near() -> None:
    path = cat.arc((0, 10), (30, 14))
    assert path[-1] == (30, 14)
    assert min(y for _, y in path) < 10, "a jump goes up first"
    assert [x for x, _ in path] == sorted(x for x, _ in path)
    assert cat.near(10, 20, (12, 18)) and cat.near(10, 20, (4, 21))
    assert not cat.near(10, 20, (40, 18)) and not cat.near(10, 20, None)


def test_cat(session: Pilot) -> None:
    settle(session)
    watch(
        session,
        lambda: cat.main(["-s", session.session, "--seed", "1"], seconds=2.0),
        lambda host: "/\\_/\\" in host,
    )


def test_cat_life(session: Pilot) -> None:
    session.send("clear; printf '\\n\\n\\n%060d\\n\\n\\n\\n\\n%060d\\n' 0 0\n")
    session.wait_for("0" * 60, frame="raw")
    time.sleep(0.3)
    frame = session.frame("raw")
    home = cat.Home(session, frame, random.Random(4))
    (it,) = home.cats
    now = 0.0

    def run(until: Callable[[], bool], limit: float = 20.0) -> None:
        # The app's loop, with a clock of our own.
        nonlocal now
        end = now + limit
        while not until():
            assert now < end, it.act
            now = max(now + 0.01, min(it.due, end))
            it.step(now)

    run(lambda: it.act == "alert")  # it dropped in
    assert it.sprite is not None and it.sprite.visible
    assert cat.perch(home.spots, it.x, it.g) is not None
    # The pane goes idle: a yawn, then sleep, until something happens.
    home.on("idle", {"frame": frame.id, "idle": True}, now)
    run(lambda: it.act == "sleep")
    assert it.due == float("inf")
    assert "-.-" in shown(session)
    home.on("idle", {"frame": frame.id, "idle": False}, now)
    assert it.act == "alert"
    # The cursor comes near: it jumps, and lands away from it.
    col, row = frame.to_screen(it.x + 2, it.g - 1)
    home.on("cursor", {"x": col, "y": row, "visible": True}, now)
    assert it.act == "leap"
    run(lambda: it.act == "alert")
    assert not cat.near(it.x, it.g, home.cursor)
    # Text written over it: `covered` says so, and it moves.
    was = (it.x, it.g)
    home.cursor = None
    col, row = frame.to_screen(it.x, it.g - 2)
    session.send(f"printf '\\033[{row + 1};{col + 1}H%s\\033[20;1H' ::::::::::\n")
    session.wait_for("::::::::::", frame="raw")
    time.sleep(0.2)
    assert it.sprite.info()["covered"] > cat.COVERED
    home.check(now)
    assert it.act == "leap"
    run(lambda: it.act == "alert")
    assert (it.x, it.g) != was and it.sprite.info()["covered"] == 0
    it.sprite.remove()


def cat_house(session: Pilot, text: str, **kw: Any) -> tuple[Any, Callable[..., None]]:
    """A home on `text`, and its loop with a clock of our own."""
    session.send(f"clear; printf '{text}'\n")
    session.wait_for("0" * 60, frame="raw")
    time.sleep(0.3)
    home = cat.Home(session, session.frame("raw"), random.Random(7), **kw)
    now = 0.0

    def run(until: Callable[[], bool], limit: float = 60.0) -> None:
        nonlocal now
        end = now + limit
        while not until():
            assert now < end, [it.act for it in home.cats]
            now = max(now + 0.01, min(home.due(now), end))
            home.step(now)

    return home, run


def test_cat_count(session: Pilot) -> None:
    text = "\\n\\n\\n%060d\\n\\n\\n\\n\\n%060d\\n"
    home, run = cat_house(session, f"{text}' 0 '0", count=3, varied=True)
    assert len({it.coat for it in home.cats}) == 3
    resting = ("alert", "sit", "groom", "walk")
    run(lambda: all(it.act in resting for it in home.cats))
    for _ in range(40):  # and a while more: they move, never onto each other
        for a, b in itertools.combinations(home.cats, 2):
            assert abs(a.x - b.x) >= cat.W or abs(a.g - b.g) >= cat.H, (a.x, b.x)
        run(lambda: True)
    assert len(session.sprites()) == 3


def test_cat_food(session: Pilot) -> None:
    text = "\\n\\n\\n%060d\\n\\n\\n\\n\\n%060d\\n"
    home, run = cat_house(session, f"{text}' 0 '0", count=2)
    run(lambda: all(it.act in ("alert", "sit") for it in home.cats))
    assert not home.plates
    # The word, in the middle of a free row: a plate right of it.
    session.send("printf '\\033[14;30Hfood\\033[20;1H'\n")
    session.wait_for("    food ", frame="raw")
    time.sleep(0.2)
    home.stale = True
    run(lambda: bool(home.plates))
    (plate,) = home.plates
    (word,) = session.find(r"\bfood\b", regex=True)
    assert (plate.x, plate.g) == (word.x + 4 + cat.GAP, word.y + 1)
    assert "\\oOo/" in shown(session)
    # They come, one on each side that is free, and face it.
    run(lambda: any(it.act == "eat" for it in home.cats))
    eater = next(it for it in home.cats if it.act == "eat")
    assert eater.meal is plate and eater.g == plate.g
    assert eater.x == plate.x + cat.PW and not eater.right
    # Emptied, the plate goes, and the word gets no other.
    run(lambda: not home.plates)
    assert plate.left == 0 and home.eaten == {(word.x, word.y)}
    home.stale = True
    run(lambda: all(it.meal is None and it.act != "eat" for it in home.cats))
    assert not home.plates and "\\.../" not in shown(session)


def test_cat_served_and_mice(session: Pilot) -> None:
    text = "\\n\\n\\n%060d\\n\\n\\n\\n\\n%060d\\n"
    home, run = cat_house(session, f"{text}' 0 '0", count=2, food=True, mice=True)
    # A plate comes by itself, on a ledge, with a seat.
    run(lambda: bool(home.plates))
    (plate,) = home.plates
    assert plate.word is None and home.chairs(plate.x, plate.g)
    # A mouse comes, walks, and goes: caught, or out by an end.
    home.next_mouse = 0.0
    run(lambda: home.mouse is not None)
    mouse = home.mouse
    assert mouse.lo <= mouse.x <= mouse.hi
    run(lambda: mouse.sprite is not None)
    assert '"' in shown(session)
    run(lambda: any(it.tries for it in home.cats))  # a cat pounced
    run(lambda: home.mouse is not mouse)
    assert '"' not in shown(session)


def test_cat_options(session: Pilot) -> None:
    settle(session)
    watch(
        session,
        lambda: cat.main(["-s", session.session, "--count", "3", "-a"], seconds=2.0),
        lambda host: "_/" in host,
    )
    # `-a` alone: two cats.
    most = 0

    def two(host: str) -> bool:
        nonlocal most
        most = max(most, sum(s["size"] == [cat.W, cat.H] for s in session.sprites()))
        return most == 2

    watch(session, lambda: cat.main(["-s", session.session, "-a"], seconds=3.0), two)
    assert most == 2
    with pytest.raises(SystemExit):
        cat.main(["-s", session.session, "--count", "0"])


def test_marks_wanted() -> None:
    def match(y: int, shown: bool = True, text: str = "ab") -> Match:
        return Match(1, 3, y, len(text), text, (3, y - 100) if shown else None)

    found = [
        (1, 0, [match(90, False), match(100), match(109), match(140, False)]),
        (1, 1, [match(105, text="xyz"), match(106, text="")]),
    ]
    keys, counts = marks.wanted(found, {1: (100, 10)})
    # In view first, then the nearest rows; an empty match is no mark.
    assert [k[2] for k in keys] == [100, 109, 105, 90, 140]
    assert keys[2] == (1, 3, 105, 1, "xyz")
    assert counts == [[2, 4], [1, 1]]
    assert [k[2] for k in marks.wanted(found, {1: (100, 10)}, most=3)[0]] == [
        100,
        109,
        105,
    ]


def test_marks_hud() -> None:
    rows = marks.hud(["TODO", "a very long pattern indeed"], [[2, 2], [1, 12]])
    text = ["".join(run["t"] for run in row) for row in rows]
    assert text == ["  TODO                 2 ", "  a very long pat…  1/12 "]
    assert rows[0][1]["bg"] == marks.INKS[0][0] != rows[1][1]["bg"]


def test_marks(session: Pilot) -> None:
    session.send("clear; echo 'an apple, a pear, an Apple'\n")
    session.wait_for("Apple", frame="raw")
    time.sleep(0.3)
    args = ["-s", session.session, "apple", "-i", "-e", "p[ea]+r"]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(marks.main(args, seconds=4)))
    thread.start()

    def tints() -> list[tuple[int, int]]:
        return sorted(
            (s["size"][0], s["anchor"]["at"][1])
            for s in session.sprites()
            if s["blend"] == "tint" and s["visible"]
        )

    def displayed(note: str) -> bool:
        return any(note in row for row in session.dump()["host"])

    # Three matches on the row of the output, and the display.
    assert until(lambda: [w for w, _ in tints()] == [4, 5, 5], 3), session.sprites()
    assert len({y for _, y in tints()}) == 1
    assert until(lambda: displayed(" apple    2") and displayed(" p[ea]+r  1"), 2)
    # New text: marked when it shows, without asking.
    session.send("echo APPLE\n")
    assert until(lambda: len(tints()) >= 5, 3), tints()  # typed, then printed
    assert until(lambda: displayed(" apple    4"), 2), shown(session)
    thread.join()
    assert status == [0]
    assert until(lambda: session.sprites() == [], 3)
    assert marks.main(["-s", session.session, "-e", "("]) == 1


def test_rain(session: Pilot) -> None:
    session.send("clear; printf '\\n\\n\\n\\n%070d\\n' 0\n")
    session.wait_for("0" * 70, frame="raw")
    time.sleep(0.3)
    splashes: set[int] = set()
    drops: set[str] = set()

    def seen(host: str) -> bool:
        rows = host.split("\n")
        splashes.update(y for y, row in enumerate(rows) if "'" in row or "." in row)
        drops.update(c for c in "│╎" if c in host)
        return len(drops) == 2 and {3, 23} <= splashes

    args = ["-s", session.session, "--rate", "300"]
    watch(session, lambda: rain.main(args, seconds=2.5), seen)
    # On the line of text (the row above it) and on the floor; behind
    # the text, nothing shows.
    assert 4 not in splashes, splashes
    assert rain.main(["-s", session.session, "--rate", "-1"]) == 1


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
    assert main(["confetti", "-s", session.session, "--one-shot"]) == 0
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


def pane_info(ident: int, rect: list[int], state: str = "stable") -> dict[str, object]:
    usable = state == "stable"
    return {"id": ident, "kind": "pane", "state": state, "usable": usable} | {
        "rect": rect,
        "origin": [0, 0],
        "key": "tmux:0",
    }


class Wire:
    """A pilot that only tells events and notes the sprite updates."""

    def __init__(self, events: list[tuple[str, object]]) -> None:
        self.todo = deque(events)
        self.calls: list[tuple[str, dict[str, object]]] = []

    def subscribe(self, *topics: str, snapshot: bool = False) -> None:
        self.calls.append(("subscribe", {"topics": list(topics)}))

    def event(self, timeout: float | None = None) -> tuple[str, object] | None:
        return self.todo.popleft() if self.todo else None

    def call(self, method: str, **params: object) -> None:
        self.calls.append((method, params))


def test_sand_heir() -> None:
    frame = Frame(None, pane_info(3, [0, 0, 40, 24], "inactive"))
    left = Frame(None, pane_info(7, [0, 0, 30, 24]))
    right = Frame(None, pane_info(8, [31, 0, 49, 24]))
    assert sand.heir(frame, [frame, left, right]) is left
    # Not while it settles, not another window, not a sliver.
    settling = Frame(None, pane_info(7, [0, 0, 30, 24], "candidate"))
    assert sand.heir(frame, [settling, right]) is None
    other = Frame(None, pane_info(7, [0, 0, 30, 24]) | {"key": "tmux:1"})
    assert sand.heir(frame, [other]) is None
    # The lower of two panes, the screen cut from 30 rows to 20.
    low = Frame(None, pane_info(4, [0, 15, 100, 14], "inactive"))
    new = [Frame(None, pane_info(i, [0, y, 100, 9])) for i, y in ((7, 0), (8, 10))]
    assert sand.heir(low, new) is None
    assert sand.heir(low, new, (1.0, 20 / 30)) is new[1]


def test_sand_follows_its_frame() -> None:
    # Two empty panes resized: new frames. Told by `frames`, the app
    # moves its sprite once the heir is usable; nothing is asked.
    old = pane_info(3, [0, 0, 40, 24])
    gone = pane_info(3, [0, 0, 40, 24], "inactive")
    wire = Wire(
        [
            ("frames", {"frames": [old]}),
            ("frames", {"frames": [gone, pane_info(7, [0, 0, 30, 24], "candidate")]}),
            ("frames", {"frames": [gone, pane_info(7, [0, 0, 30, 24])]}),
            ("frames", {"frames": [gone, pane_info(7, [0, 0, 30, 24])]}),
            ("exit", {"status": 0}),
            ("frames", {"frames": [pane_info(9, [0, 0, 30, 24])]}),
        ]
    )
    sand.follow(wire, Frame(wire, old), Sprite(wire, 5))
    assert wire.calls == [
        ("subscribe", {"topics": ["frames", "exit"]}),
        ("draw.update", {"id": 5, "frame": 7}),
    ]


@pytest.mark.skipif(shutil.which("tmux") is None, reason="needs tmux")
def test_sand_follows_two_panes_resized() -> None:
    """Why `heir` exists. Two stacked tmux panes with little text, the
    host resized from 30 rows to 20: the driver cannot tell which pane
    is which and allocates new frames. A sprite left alone on an old
    frame is out of sight for good; the sand follows."""
    sock = f"ptyrc-apps-{os.getpid()}"
    tmux = ["tmux", "-L", sock, "-f", "/dev/null"]
    shell = ["env", "PS1=$ ", "sh"]

    def panes() -> list[Frame]:
        return [f for f in pilot.frames() if f.kind == "pane" and f.usable]

    with Pilot.spawn([*tmux, "new-session", *shell], size=(100, 30)) as pilot:
        try:
            pilot.wait_for("$ ")
            subprocess.run([*tmux, "split-window", "-v", *shell], check=True)
            assert until(lambda: len(panes()) == 2, 6)
            top, bottom = sorted(panes(), key=lambda f: f.rect[1])
            alone = pilot.effect("sparkle", frame=top.id, layer="back")
            args = ["-s", pilot.session, "-f", str(bottom.id), "--now"]
            status: list[int] = []
            thread = threading.Thread(
                target=lambda: status.append(sand.main(args, seconds=9))
            )
            thread.start()

            def poured() -> dict[str, object] | None:
                mine = [s for s in pilot.sprites() if s["id"] != alone.id]
                return mine[0] if mine and mine[0]["visible"] else None

            assert until(lambda: poured() is not None, 3)
            assert poured()["frame"] == bottom.id
            pilot.resize(100, 20)
            assert until(lambda: len(panes()) == 2, 6)
            new = {f.id: f for f in panes()}
            assert not {top.id, bottom.id} & set(new), "the frames were kept"
            # The sand is on the pane now where its own was.
            assert until(lambda: poured() is not None, 3)
            heir = new[poured()["frame"]]
            assert heir.rect == (0, 10, 100, 9) and poured()["size"] == [100, 9]
            # Nothing brings the other back.
            assert alone.info()["visible"] is False
            thread.join()
            assert status == [0]
        finally:
            subprocess.run([*tmux, "kill-server"], check=False)


def test_sight(session: Pilot) -> None:
    """`_cli.Sight`: the text is read when the driver tells, the cursor
    is told, and nothing is asked on a quiet screen."""
    page(session, "one")
    asked: list[str] = []
    with Pilot(session.session) as pilot:
        frame = pilot.frame("raw")
        sight = _cli.Sight(pilot, frame, cursor=True)
        request = pilot._request
        pilot._request = lambda method, *a, **kw: (
            asked.append(method) or request(method, *a, **kw)
        )
        assert sight.look() and sight.changed
        assert sight.rows[0] == "one" and asked[-1] == "screen.text"
        asked.clear()
        # Quiet: the looks cost nothing.
        time.sleep(0.3)
        assert sight.look() and not sight.changed and asked == []
        assert until(lambda: sight.look() and sight.cursor == (2, 1), 2)
        # Text written: read again, once the driver has told.
        session.send("echo t''wo\n")
        session.wait_for("two")
        assert until(lambda: sight.look() and sight.changed, 2)
        assert "two" in sight.rows[2] and sight.cursor == (2, 3)
        assert set(asked) == {"screen.text"} and len(asked) <= 3
        # A resize: the frame settles; the sight asks while it waits,
        # then reads.
        session.resize(60, 20)
        assert until(lambda: sight.look() and frame.size == (60, 20), 4)
        assert len(sight.rows) == 20
        with sight.drawing():
            pilot.draw(["x"], frame=frame.id)
        assert sight.look()


def test_options_between_the_words() -> None:
    out = _cli.parser("marks", "")
    out.add_argument("patterns", nargs="+")
    out.add_argument("-i", "--ignore-case", action="store_true")
    args = _cli.parse(out, ["ERROR", "-i", "WARN", "-f", "3"])
    assert args.patterns == ["ERROR", "WARN"] and args.ignore_case
    assert args.frame == 3
