# SPDX-License-Identifier: GPL-3.0-or-later
"""`_bricks`: the glyphs, the ball by half cells, the events drawn."""

from __future__ import annotations

import random
from types import SimpleNamespace as Thing
from typing import Any

from ptyrc_apps import _bricks

KINDS = {"plain": 1, "hard": 3, "loose": 1, "bumper": 2, "bomb": 1}


class Sprite:
    def __init__(self, log: list[Any], spec: dict[str, Any]) -> None:
        self.log, self.spec, self.gone = log, spec, False

    def set(self, **kw: Any) -> None:
        self.spec.update(kw)
        self.log.append(("set", self))

    def move(self, x: int, y: int, space: str = "view") -> None:
        self.spec["at"] = (x, y)
        self.log.append(("move", self))

    def update(self, **kw: Any) -> None:
        self.spec.update(kw)

    def remove(self) -> None:
        self.gone = True


class Pilot:
    """Stands for the driver: keeps what is drawn."""

    def __init__(self) -> None:
        self.log: list[Any] = []

    def draw(self, **kw: Any) -> Sprite:
        self.log.append(("draw", Sprite(self.log, kw)))
        return self.log[-1][1]

    def drawn(self) -> list[dict[str, Any]]:
        out = [s.spec for what, s in self.log if what == "draw"]
        self.log.clear()
        return out


def text(look: dict[str, Any]) -> list[str]:
    """The text of each frame of a look."""
    frames = look.get("frames") or [look]
    return ["".join(run["t"] for run in f["cells"][0]) for f in frames]


def stage(effects: bool = False) -> tuple[_bricks.Stage, Pilot]:
    pilot = Pilot()
    frame = Thing(id=1)
    return _bricks.Stage(pilot, frame, random.Random(1), effects), pilot  # type: ignore[arg-type]


def game(**kw: Any) -> Thing:
    base = {
        "teams": 1,
        "bricks": lambda box: [],
        "fallers": [],
        "capsules": [],
        "rackets": [],
        "balls": [],
        "events": [],
        "score": [0],
        "best": [0],
    }
    return Thing(**{**base, **kw})


def ball(x: float, y: float, **kw: Any) -> Thing:
    return Thing(**{"n": 7, "x": x, "y": y, "owner": 0, "fire": 0, **kw})


BOX = (0, 0, 40, 20)


def test_glyphs() -> None:
    seen = set()
    for kind, lives in KINDS.items():
        for left in range(1, lives + 1):
            for one in text(_bricks.brick(kind, left, 2)):
                assert len(one) == 3 and one[0] + one[2] in ("⟦⟧", "⟨⟩")
                seen.add((kind, one))
    # A kind is told by its glyphs alone, hard by its ink too.
    assert text(_bricks.brick("plain", 1, 0)) == ["⟦=⟧"]
    assert text(_bricks.brick("loose", 1, 0)) == ["⟨=⟩"]
    assert [text(_bricks.brick("hard", n, 0))[0][1] for n in (3, 2, 1)] == [*"≡=-"]
    assert _bricks.brick("hard", 2, 0)["cells"][0][0]["fg"] not in _bricks.PALETTE
    assert len({one for _, one in seen}) == 8
    assert all(len(t) == 3 and t[0] + t[2] == "⟨⟩" for t in text(_bricks.faller(1)))
    for w in (3, 5, 7):
        calm, hit = (_bricks.racket(w, 0, h)[0][0] for h in (False, True))
        assert calm["t"] == "(" + "━" * (w - 2) + ")" and calm["fg"] == _bricks.TEAM[0]
        assert hit["t"] == "(" + "═" * (w - 2) + ")" and hit["fg"] != calm["fg"]
    assert text(_bricks.ball()) == ["▀"] and text(_bricks.ball(low=True)) == ["▄"]
    inks = {_bricks.ball(fire=True, step=i)["cells"][0][0]["fg"] for i in range(3)}
    assert inks == set(_bricks.FLAME)
    for look in (_bricks.pop("plain", 0), _bricks.lock(), _bricks.splash()):
        assert look["loop"] is False and len(set(map(len, text(look)))) == 1
    assert [g["frames"][0]["cells"][0][0]["t"] for g in
            _bricks.shards(random.Random(1), "loose", 0)] == [*"⟨=⟩"]  # fmt: skip


def test_ball_by_halves() -> None:
    """Half a row a step: the ball changes at each step."""
    st, pilot = stage()
    it = ball(5.5, 3.5)
    g = game(balls=[it])
    shown = []
    for _ in range(4):
        st.show(g, BOX)
        sprite, (at, _) = st.moving[it.n]
        shown.append((at, text(sprite.spec)[0]))
        assert not st.late
        it.x, it.y = it.x + 0.9, it.y + 0.5
    assert shown == [((5, 3), "▄"), ((6, 4), "▀"), ((7, 4), "▄"), ((8, 5), "▀")]
    # No effects: a sprite for the ball, one for the score, no trail.
    assert len(pilot.drawn()) == 2
    g.balls = []
    st.show(g, BOX)
    assert not st.moving


def test_glide() -> None:
    """A row in a step: half way, then in place."""
    st, _ = stage()
    it = ball(5.5, 3.0)
    g = game(balls=[it])
    st.show(g, BOX)
    it.x, it.y = 7.3, 4.0
    st.show(g, BOX)
    assert st.moving[it.n][1][0] == (6, 3) and text(st.moving[it.n][0].spec) == ["▄"]
    assert st.late
    st.glide()
    assert st.moving[it.n][1][0] == (7, 4) and text(st.moving[it.n][0].spec) == ["▀"]
    assert not st.late
    st.glide()  # nothing left to do


def test_trail_and_fire() -> None:
    st, pilot = stage(effects=True)
    it = ball(5.5, 3.5)
    g = game(balls=[it], teams=2, score=[0, 0], best=[0, 0])
    st.show(g, BOX)
    trail = [s for s in pilot.drawn() if s.get("z") == 1]
    assert len(trail) == 1 and trail[0]["at"] == (5, 3) and trail[0]["loop"] is False
    assert trail[0]["frames"][0]["cells"][0][0]["fg"] == _bricks.TEAM[0]
    it.fire = 9
    inks = set()
    for _ in range(3):
        st.show(g, BOX)
        inks.add(st.moving[it.n][0].spec["cells"][0][0]["fg"])
        assert not [s for s in pilot.drawn() if s.get("z") == 1]  # flames, no trail
    assert inks == set(_bricks.FLAME)


def test_fade() -> None:
    """A group about to go is dim; gone, a mark in its place. The game
    starts again: three dots."""
    st, pilot = stage()
    one = Thing(n=3, x=10, y=2, kind="plain", lives=1, color=1)
    two = Thing(n=4, x=13, y=2, kind="hard", lives=2, color=1)
    g = game(bricks=lambda box: [one, two])
    st.show(g, BOX)
    assert ["dim" in s["cells"][0][0] for s in pilot.drawn()[:2]] == [False, False]
    g.events = [("fade", [one, two])]
    st.show(g, BOX)
    looks = [st.bricks[n][0].spec["cells"][0][0] for n in (3, 4)]
    assert [(k["t"], k.get("dim"), "bold" in k) for k in looks] == [
        ("⟦=⟧", True, False),
        ("⟦=⟧", True, False),
    ]
    two.lives = 1  # hit while dim: still dim
    g.events = []
    st.show(g, BOX)
    assert st.bricks[4][0].spec["cells"][0][0] == {"t": "⟦-⟧", "fg": 245, "dim": True}
    pilot.drawn()
    g = game(events=[("gone", [one, two]), ("reset", (20, 10))])
    st.show(g, BOX)
    assert not st.bricks and not st.dim
    marks = pilot.drawn()
    assert [(s["at"], s["z"], text(s)) for s in marks] == [
        ((10, 2), 0, ["⟦·⟧", " · "]),
        ((13, 2), 0, ["⟦·⟧", " · "]),
        ((18, 10), 4, ["· · ·", " · · ", "  ·  "]),
    ]
    for look in marks:
        assert look["loop"] is False and look["ttl"] > 0
        assert all(f["cells"][0][0].get("dim") for f in look["frames"])
    for kind in KINDS:
        assert text(_bricks.brick(kind, 1, 0, True)) == text(_bricks.brick(kind, 1, 0))
        assert len(set(map(len, text(_bricks.fade(kind, 0))))) == 1


def test_events() -> None:
    st, pilot = stage()
    racket = Thing(n=1, team=0, x=4, y=9, w=5, dead=0)
    g = game(rackets=[racket])
    st.show(g, BOX)
    assert text(st.moving[1][0].spec) == ["(━━━)"]
    # A bounce: the racket flashes for FLASH steps.
    g.events = [("bounce", (6, 8), racket)]
    for _ in range(_bricks.FLASH):
        st.show(g, BOX)
        assert text(st.moving[1][0].spec) == ["(═══)"]
        g.events = []
    st.show(g, BOX)
    assert text(st.moving[1][0].spec) == ["(━━━)"]
    pilot.drawn()
    # No effects: a mark for a brick, a serve, a ball out; no particle.
    brick = Thing(n=3, x=10, y=2, kind="plain", lives=0, color=1)
    g.events = [
        ("break", (11, 2), brick, 0),
        ("serve", (6, 8)),
        ("lost", (6, 19), 0),
        ("out", (30, 0), 0),
        ("retreat", (6, 9), racket),
        ("combo", (11, 2), 3),
        ("bounce", (6, 8), racket),
        ("unheard", (1, 1), 2),
        ("bare",),
    ]
    st.show(g, BOX)
    marks = [s for s in pilot.drawn() if s.get("z") == 4]
    assert [(s["at"], text(s)[0]) for s in marks] == [
        ((10, 2), "⟦·⟧"),
        ((4, 8), "(   )"),
        ((4, 19), "  x  "),
        ((28, 0), "  x  "),
    ]
    assert all("motion" not in s for s in marks)

    # With effects: shards behind the bricks, sparks, a label for a combo.
    st, pilot = stage(effects=True)
    st.show(g, BOX)
    drawn = pilot.drawn()
    shards = [s for s in drawn if s.get("z") == -1]
    assert [s["at"] for s in shards] == [(10, 2), (11, 2), (12, 2)]
    assert [s["frames"][0]["cells"][0][0]["t"] for s in shards] == [*"⟦=⟧"]
    assert shards[0]["motion"]["vx"] < 0 < shards[2]["motion"]["vx"]
    labels = [text(s)[0] for s in drawn if "motion" in s and s["motion"]["vy"] == -3.0]
    assert labels == ["0", "x3"]
    assert len(drawn) > len(marks) + 5
