# SPDX-License-Identifier: GPL-3.0-or-later
"""The reef of `fishes`: art, places, memory, bubbles, events."""

from __future__ import annotations

import math
import random
import threading
import time
from typing import Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import _reef, fishes
from ptyrc_apps._reef import CROSSERS, DECOR, SUNK, TALL, Decor, Events, Reef

BOX = (0, 100, 60, 123)  # 60 x 24 cells, scrolled to row 100


class Drawn:
    """Stands for a `Sprite`: what it was drawn with, and its end."""

    def __init__(self, spec: dict[str, Any]) -> None:
        self.spec = spec
        self.gone = False

    def remove(self) -> None:
        self.gone = True

    def move(self, x: int, y: int, space: str = "view") -> None:
        self.spec["at"] = (x, y)

    def set(self, **kw: Any) -> None:
        self.spec.update(kw)


class Canvas:
    """Stands for a `Pilot` and for its `Frame`."""

    id = 1

    def __init__(self) -> None:
        self.drawn: list[Drawn] = []

    def draw(self, rows: Any = None, **kw: Any) -> Drawn:
        self.drawn.append(Drawn(kw))
        return self.drawn[-1]

    @property
    def live(self) -> list[Drawn]:
        return [d for d in self.drawn if not d.gone]


def water(*text: tuple[int, str], h: int = 24, w: int = 60) -> list[str]:
    """A view of `h` empty rows with `text` at the given rows."""
    rows = [""] * h
    for y, line in text:
        rows[y] = line
    return _reef.pad(rows, w)


def test_art() -> None:
    for art in (*DECOR, *CROSSERS, *(s.fall for s in SUNK), *(s.rest for s in SUNK)):
        sizes = {(len(row), len(rows)) for rows in art.frames for row in rows}
        assert sizes == {(art.w, art.h)}, art.name
        assert art.inks and all("" in ink for ink in art.inks), art.name
        # It stands on its last row, in every frame.
        assert all(rows[-1].strip() for rows in art.frames), art.name
        assert all(_reef.mirror(_reef.mirror(rows)) == rows for rows in art.frames)
    assert len({shape.name for shape in DECOR}) == len(DECOR)
    assert _reef.mirror([fishes.RIGHT]) == (fishes.LEFT,)
    assert fishes.CLASSIC.glyph(False, 3) == fishes.LEFT
    for kind in fishes.SPECIES:
        assert len({len(g) for g in kind.right}) == 1 and kind.colors
        assert len(kind.glyph(False, 0)) == kind.length
    # A space is transparent; one run per colour.
    assert _reef.paint(["a  bc"], {"": 1, "c": 2}, dim=True) == [
        [
            {"t": "a", "fg": 1, "dim": True},
            {"t": "  "},
            {"t": "b", "fg": 1, "dim": True},
            {"t": "c", "fg": 2, "dim": True},
        ]
    ]


def test_stand() -> None:
    rows = water((2, "    xxxx xxxx"), (5, "x"), (6, "  xxxxxxxxxx   xx"), w=20)
    # On row 2: ink under 2 cells of 3; nothing above.
    assert _reef.stand(rows, 2, 20, 3, 2) == list(range(3, 12))
    assert _reef.stand(rows, 2, 20, 1, 1) == [4, 5, 6, 7, 9, 10, 11, 12]
    assert _reef.stand(rows, 2, 20, 3, 3) == []  # out of the view
    # On row 6: the x of row 5 and a column each side are in the way.
    assert _reef.stand(rows, 6, 20, 3, 1) == [*range(2, 11), 14, 15]
    assert _reef.stand(rows, 6, 20, 3, 4) == [14, 15]  # row 2 is in the way too
    assert _reef.stand(rows, 7, 20, 1, 1) == []  # no text to stand on
    assert _reef.depths(rows, 6) == [5, 24, 6, 6, 2, 2]


def test_reef_remembers(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(_reef, "DENSITY", 1.0)
    rng = random.Random(1)
    line = "the quick brown fox jumps over the lazy dog"
    lines = [(3, line), (9, line[:9]), (10, line), (20, line)]
    rows = water(*lines)
    reef = Reef()
    # A view on the move: the rows near its top wait.
    planted = reef.survey(rows, BOX, False, rng)
    assert {d.ground for d in planted} == {109, 110, 120}
    assert reef.seen == set(range(100 + TALL, 123))
    planted += reef.survey(rows, BOX, True, rng)
    assert {d.ground for d in planted} == {103, 109, 110, 120}
    assert reef.seen == set(range(101, 123))
    for d in planted:
        x, y = d.at
        assert all(
            not rows[r - 100][x : x + d.shape.w].strip()
            for r in range(y, y + d.shape.h)
        )
        assert rows[d.ground - 100][x : x + d.shape.w].strip()
    cells = [(x, y) for d in planted for x, y in d.cells]
    assert len(cells) == len(set(cells))
    # Room above row 120: something tall.
    assert {d.ground: d.shape.h for d in planted}[120] > 1
    # Populated once: nothing new there, whatever the text and the view.
    before = [(d.shape.name, d.at) for d in reef.decor]
    assert reef.survey(rows, BOX, True, rng) == []
    up = _reef.pad([""] * 5 + rows[:-5], 60)  # the view 5 rows up
    assert reef.survey(up, (0, 95, 60, 118), True, rng) == []
    below = reef.survey(rows, (0, 124, 60, 147), True, rng)  # a page down
    assert below and all(d.ground > 124 for d in below)
    assert [(d.shape.name, d.at) for d in reef.decor][: len(before)] == before
    # The text under one goes: it is forgotten, its row is surveyed again.
    gone = reef.prune(water(*lines[1:]), BOX)
    assert [d.ground for d in gone] == [103] and 103 not in reef.seen
    assert reef.prune(water(*lines[1:]), BOX) == []


def test_reef_shows_what_is_near(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(_reef, "DENSITY", 1.0)
    rng = random.Random(2)
    rows = water(*[(y, "xxxx xxxx xxxx xxxx") for y in range(6, 23, 4)])
    reef, canvas = Reef(), Canvas()
    reef.survey(rows, BOX, True, rng)
    reef.show(canvas, canvas, BOX)  # type: ignore[arg-type]
    first = [d.spec for d in canvas.live]
    assert len(first) == len(reef.decor) == 5
    assert all(s["space"] == "frame" and s["layer"] < 0 for s in first)
    reef.show(canvas, canvas, (0, 124, 60, 147))  # a page down: still near
    assert len(canvas.live) == 5 and len(canvas.drawn) == 5
    reef.show(canvas, canvas, (0, 400, 60, 423))  # far: no sprite, all kept
    assert canvas.live == [] and len(reef.decor) == 5
    reef.show(canvas, canvas, BOX)  # back: the same, redrawn
    assert [d.spec for d in canvas.live] == first


def test_bubbles() -> None:
    rng = random.Random(3)
    reef, canvas = Reef(), Canvas()
    vents = reef.area(BOX, 0.0, rng)
    assert len(vents) == 3 and all(0 < v.x < 59 for v in vents)
    # An area keeps its vents; another has its own; back, the same.
    assert reef.area((0, 103, 60, 126), 0.0, rng) is vents
    assert reef.area((0, 112, 60, 135), 0.0, rng) is not vents
    assert reef.area(BOX, 9.0, rng) is vents
    for tick in range(600):
        reef.bubbles(canvas, canvas, BOX, tick * 0.1, rng)  # type: ignore[arg-type]
    assert 20 < len(canvas.drawn) < 90  # a minute, three vents
    assert {d.spec["at"] for d in canvas.drawn} == {(v.x - 1, 123) for v in vents}
    for spec in (d.spec for d in canvas.drawn):
        assert spec["motion"]["vy"] < 0 and spec["loop"] is False
        # It reaches the top of the view, 24 rows up, as it ends.
        life = sum(f["ms"] for f in spec["frames"]) / 1000
        assert 24 <= -spec["motion"]["vy"] * life < 25 and spec["ttl"] > life
        glyphs = "".join(
            run["t"].strip() for f in spec["frames"] for run in f["cells"][0]
        )
        assert set(glyphs) == {"."}
        assert all(
            run.get("dim", True) for f in spec["frames"] for run in f["cells"][0]
        )


def test_species_swim() -> None:
    rng = random.Random(4)
    box = (0, 0, 80, 24)
    sword = next(k for k in fishes.SPECIES if k.name == "sword")
    eel = next(k for k in fishes.SPECIES if k.name == "eel")
    assert sword.glyph(True, 0) == "><(((°>--" and sword.glyph(False, 0) == "--<°)))><"
    # The eel slithers: another body at each column.
    assert {eel.glyph(True, x) for x in (4, 5)} == set(eel.right)
    assert eel.glyph(False, 4) == "<:~-~-~-~"
    # A long fish turns earlier at the right wall.
    fish = fishes.Fish(x=70.9, y=3.0, vx=5.0, color="cyan", kind=sword)
    fishes.swim(fish, 0.1, box, None, False, rng)
    assert fish.x == 71 and fish.vx == -5.0
    assert fishes.inside((71, 3), box, 9) and not fishes.inside((72, 3), box, 9)
    assert fishes.tide([(76, 3)] * 2, [None] * 2, box, 0.0, rng, [3, 9])[0][0] is None
    # One that leaves goes through the wall; the tide takes it.
    fish.vx, fish.leaving = 5.0, True
    for _ in range(40):
        fishes.swim(fish, 0.1, box, None, False, rng)
    assert fish.x > 80 and fishes.lure(fish, [(1, 1)], None, 0.1, rng) is None


def test_menace_and_depart() -> None:
    rng = random.Random(5)
    box = (0, 0, 80, 24)
    fish = fishes.Fish(x=30.0, y=10.0, vx=fishes.CALM, color="cyan")
    assert fishes.menace(fish, (60, 10), True, []) == ((60, 10), True)
    assert fishes.menace(fish, (60, 10), True, [(50.0, 10.0)]) == ((60, 10), True)
    assert fishes.menace(fish, None, False, [(50.0, 10.0), (38.0, 10.0)]) == (
        (38.0, 10.0),
        False,
    )
    fishes.swim(fish, 0.1, box, *fishes.menace(fish, None, False, [(38.0, 10.0)]), rng)
    assert fish.vx == -fishes.FEAR
    # A bold, calm fish of the view leaves by the nearest side.
    shy = fishes.Fish(x=10.0, y=1.0, vx=1.0, color="cyan", shy=True)
    out = fishes.Fish(x=90.0, y=1.0, vx=1.0, color="cyan")
    bold = fishes.Fish(x=60.0, y=1.0, vx=-1.0, color="cyan")
    assert fishes.depart([shy, out, fish], box, rng) is None  # `fish` is scared
    assert fishes.depart([shy, out, bold], box, rng) is bold
    assert bold.leaving and bold.vx == fishes.CALM


def test_sunk_object_lands_and_stays() -> None:
    rng = random.Random(6)
    rows = water((0, "x" * 20), (12, "x" * 50), (20, "x" * 60))
    reef, canvas = Reef(), Canvas()
    events = Events(reef, 0.0, rng)
    events._drop(canvas, canvas, rows, BOX)  # type: ignore[arg-type]
    fall = events.fall
    assert fall is not None and events.busy
    w, h = fall.kind.fall.w, fall.kind.fall.h
    # Down a free column, onto the first text below.
    assert 21 <= fall.x and fall.rest == 100 + (12 if fall.x < 50 else 20) - h
    assert canvas.drawn[0].spec["at"] == (fall.x, 100)
    for _ in range(100):
        events.step(canvas, canvas, rows, BOX, 0.0, 0.1)  # type: ignore[arg-type]
    assert events.fall is None and len(canvas.drawn) == 1
    (landed,) = reef.decor
    assert landed.sprite is canvas.drawn[0] and not canvas.drawn[0].gone
    assert canvas.drawn[0].spec["at"] == landed.at == (fall.x, fall.rest)
    assert landed.shape is fall.kind.rest and landed.shape.w == w
    assert reef.prune(rows, BOX) == []
    # No free column 3 rows deep: nothing falls.
    events._drop(canvas, canvas, water((2, "x" * 60)), BOX)  # type: ignore[arg-type]
    assert events.fall is None


def test_hook() -> None:
    rng = random.Random(7)
    rows = water((0, "x" * 30), (8, "x" * 60))
    reef, canvas = Reef(), Canvas()
    events = Events(reef, 0.0, rng)
    events._cast(canvas, canvas, rows, BOX)  # type: ignore[arg-type]
    hook = events.hook
    assert hook is not None and hook.x >= 30 and 3 <= hook.most <= 8
    deepest, feared = 0, []
    for _ in range(400):
        threats, _ = events.step(canvas, canvas, rows, BOX, 0.0, 0.1)  # type: ignore[arg-type]
        if events.hook is None:
            break
        cells = canvas.drawn[0].spec["cells"]
        assert [r[0]["t"] for r in cells] == ["|"] * (len(cells) - 1) + ["J"]
        deepest = max(deepest, len(cells))
        feared += threats
    # Down to its depth, then up and gone; fishes fear its tip.
    assert events.hook is None and canvas.drawn[0].gone and deepest == hook.most
    assert (hook.x, 100 + hook.most - 1) in feared and not threats


def test_crossers() -> None:
    rng = random.Random(8)
    reef, canvas = Reef(), Canvas()
    events = Events(reef, 0.0, rng)
    seen = set()
    for _ in range(60):
        events._cross(canvas, canvas, BOX, 0.0)  # type: ignore[arg-type]
        cross, spec = events.cross, canvas.drawn[-1].spec
        assert cross is not None
        seen.add(cross.kind.name)
        vx, vy = spec["motion"]["vx"], spec["motion"]["vy"]
        x, y = spec["at"]
        # It starts out of the view and is gone once across.
        if cross.kind.rises:
            assert vy < 0 == vx and y == 124 and 0 <= x <= 60 - cross.kind.w
            assert abs(-vy * spec["ttl"] - (24 + cross.kind.h)) < 1e-6
        else:
            assert x == (-cross.kind.w if vx > 0 else 60) and vy == 0
            assert 100 <= y <= 124 - cross.kind.h
            assert abs(abs(vx) * spec["ttl"] - (60 + cross.kind.w)) < 1e-6
        # Its mouth leads.
        mx, _ = cross.mouth(1.0)
        assert mx == pytest.approx(x + vx + (cross.kind.w if vx > 0 else 0))
        threats, _ = events.step(canvas, canvas, [], BOX, 1.0, 0.1)  # type: ignore[arg-type]
        assert bool(threats) == cross.kind.scary
        events.due = math.inf  # nothing else starts
        events.step(canvas, canvas, [], BOX, cross.ends, 0.1)  # type: ignore[arg-type]
        assert events.cross is None
    assert seen == {kind.name for kind in CROSSERS}


def test_decor_spec() -> None:
    frame = Canvas()
    castle = next(s for s in DECOR if s.name == "castle")
    shell = next(s for s in DECOR if s.name == "shell")
    spec = Decor(castle, (4, 9), castle.inks[0], phase=1, ms=500).spec(frame)  # type: ignore[arg-type]
    assert spec["layer"] == _reef.BEHIND and spec["at"] == (4, 9)
    assert [f["ms"] for f in spec["frames"]] == [500] * 3
    flags = [
        [r["t"] for r in f["cells"][0] if r.get("fg") == 196] for f in spec["frames"]
    ]
    assert flags == [[">"], ["~"], [">"]]
    spec = Decor(shell, (0, 0), shell.inks[0], front=True).spec(frame)  # type: ignore[arg-type]
    assert spec["layer"] == _reef.FRONT and spec["z"] == 1 and "frames" not in spec


def test_fishes_all(session: Pilot, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(_reef, "DENSITY", 1.0)
    monkeypatch.setattr(_reef, "EVERY", (0.2, 0.4))
    lines = ["", "", "", "", "", "the quick brown fox jumps over the lazy dog"] * 3
    text = "\\n".join(lines)
    session.send(f"clear; printf '{text}\\n'\n")
    session.wait_for("lazy dog", frame="raw")
    deadline = time.monotonic() + 4
    while not session.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert session.panes()
    args = ["-s", session.session, "--seed", "4", "--count", "4", "-a"]
    status: list[int] = []
    thread = threading.Thread(
        target=lambda: status.append(fishes.main(args, seconds=6))
    )
    thread.start()
    most, layers = 0, set()
    while thread.is_alive():
        sprites = session.sprites()
        most = max(most, len(sprites))
        layers |= {s["layer"] for s in sprites}
        time.sleep(0.1)
    thread.join()
    assert status == [0]
    # Fishes, decorations on three lines, bubbles; all behind the text.
    assert most >= 4 + 3 + 2, most
    assert {_reef.BUBBLES, -1} <= layers and max(layers) < 0, layers
    deadline = time.monotonic() + 3
    while session.sprites() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert session.sprites() == []
    assert session.dump()["host"] == session.text()
