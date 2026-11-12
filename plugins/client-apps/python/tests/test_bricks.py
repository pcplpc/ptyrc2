# SPDX-License-Identifier: GPL-3.0-or-later
"""`bricks`: the ball, the terrain, the rackets, rules, bonuses, the drawing."""

from __future__ import annotations

import random
import threading
import time
from collections.abc import Callable

from ptyrc import Pilot
from ptyrc_apps import APPS, _bricks, bricks
from ptyrc_apps.snake import walls_of

Box = tuple[int, int, int, int]
ROWS = ["", "  wall of words  ", "", "", "", "", "   another   one", *[""] * 13]
BOX = (10, 100, 70, 120)
WALLS = walls_of(ROWS, 10, 100)


def watch(pilot: Pilot, run: Callable[[], int], seen: Callable[[str], bool]) -> None:
    """Run the app in a thread; `seen` must hold on the host at some
    point, and the host must be clean once its sprites are gone."""
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(run()))
    thread.start()
    ok = False
    while thread.is_alive():
        ok = ok or seen("\n".join(pilot.dump()["host"]))
        time.sleep(0.02)
    thread.join()
    assert status == [0]
    deadline = time.monotonic() + 3
    while pilot.sprites() and time.monotonic() < deadline:
        time.sleep(0.02)
    assert ok, "the app never showed"
    assert pilot.sprites() == []
    assert pilot.dump()["host"] == pilot.text()


def page(pilot: Pilot) -> None:
    """Words on the screen of the shell, and a pane to draw on."""
    lines = ("", "", "words in the way", "", "", "", "   more words here")
    text = "\\n".join(lines)
    pilot.send(f"clear; printf '{text}\\n'\n")
    pilot.wait_for("more words here", frame="raw")
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert pilot.panes()


def checks(game: bricks.Game, box: Box, walls: set) -> None:
    """What holds after every step."""
    x0, y0, x1, y1 = box
    cells = [c for b in game.bricks(box) for c in b.cells]
    assert len(set(cells)) == len(cells) and not walls & set(cells)
    assert all(game.solid[c].y == c[1] for c in cells)
    rackets = [r for r in game.rackets if not r.dead]
    taken = [c for r in rackets for c in r.cells]
    assert len(set(taken)) == len(taken)  # never one on another
    assert not set(taken) & (walls | set(cells))
    assert all(x0 <= x < x1 and y0 <= y < y1 for x, y in taken)
    assert all(r.w in (bricks.SMALL, bricks.RACKET, bricks.WIDE) for r in rackets)
    assert len(game.balls) <= bricks.BALLS
    for ball in game.balls:
        assert x0 <= ball.at[0] < x1 and y0 <= ball.at[1] < y1
        assert ball.at not in walls and ball.at not in taken
        assert ball.fire or ball.at not in game.solid
        assert abs(ball.vy) == bricks.VY and abs(ball.vx) <= bricks.VX


def still(**kw: bool) -> bricks.Game:
    """A game that lays no area and serves no ball."""
    game = bricks.Game(random.Random(1), **kw)
    game.spawn_due = game.serve_due = 10**9
    return game


def lay(game: bricks.Game, x: int, y: int, kind: str = "plain") -> bricks.Brick:
    game.serial += 1
    one = bricks.Brick(game.serial, x, y, kind, bricks.KINDS[kind][0])
    game._lay(one)
    return one


def shoot(game: bricks.Game, x: float, y: float, vy: float = -0.5) -> bricks.Ball:
    ball = bricks.Ball(99, x, y, 0.0, vy)
    game.balls = [ball]
    return ball


def test_fly() -> None:
    def hard(c: tuple[int, int]) -> bool:
        return c == (3, 0) or c[1] == 2

    # A free cell: it moves. A wall on the side: x is reversed, y goes on.
    assert bricks.fly(0.5, 0.5, 0.5, 0.5, hard)[:4] == (1.0, 1.0, 0.5, 0.5)
    x, y, vx, vy, hits = bricks.fly(2.5, 0.25, 0.9, 0.5, hard)
    assert (x, y, vx, vy) == (2.5, 0.75, -0.9, 0.5) and hits == [((3, 0), False)]
    # A floor: y is reversed, x goes on; the hit is flat.
    x, y, vx, vy, hits = bricks.fly(0.5, 1.75, 0.5, 0.5, hard)
    assert (x, y, vx, vy) == (1.0, 1.75, 0.5, -0.5) and hits == [((1, 2), True)]


def test_game() -> None:
    game = bricks.Game(random.Random(3))
    seen: dict[str, int] = {}
    for _ in range(1500):
        score = game.score[0]
        game.step(BOX, WALLS)
        checks(game, BOX, WALLS)
        assert len(game.rackets) == 1 and not game.fallers and not game.capsules
        assert all(b.kind == "plain" and b.lives == 1 for b in game.bricks(BOX))
        whats = [e[0] for e in game.events]
        for what in whats:
            seen[what] = seen.get(what, 0) + 1
        if "lost" in whats:
            assert game.score == [0]  # a ball out: back to 0
        else:
            assert game.score[0] == score + bricks.POINTS * whats.count("break")
    assert seen.keys() >= {"area", "break", "serve"}
    assert seen["break"] >= 20 and game.best[0] >= 100
    # Same seed, same game.
    a, b = (bricks.Game(random.Random(9)) for _ in range(2))
    for _ in range(200):
        a.step(BOX, WALLS)
        b.step(BOX, WALLS)
        assert [(o.x, o.y) for o in a.balls] == [(o.x, o.y) for o in b.balls]
        assert [(r.x, r.y) for r in a.rackets] == [(r.x, r.y) for r in b.rackets]
        assert a.solid.keys() == b.solid.keys()


def test_terrain() -> None:
    walls = walls_of(["", "", "  a line of words, long enough  "])
    box = (0, 0, 60, 20)
    game = bricks.Game(random.Random(2))
    game.step(box, walls)
    assert len(game.bricks(box)) >= bricks.LOW  # the first view: at once
    for _ in range(40):
        game.step(box, walls)
    kept = {b.n: (b.x, b.y) for b in game.bricks(box)}
    # Scrolled away: what moves starts again there, a new area is laid.
    far = (0, 500, 60, 520)
    for _ in range(60):
        game.step(far, set())
        checks(game, far, set())
    assert game.bricks(far) and game.balls and far[1] <= game.rackets[0].y < far[3]
    # And back: the bricks are where they were.
    assert {b.n: (b.x, b.y) for b in game.bricks(box)} == kept
    # Text written on a brick: the brick is gone.
    one = game.bricks(box)[0]
    game.step(box, walls | {(one.x + 1, one.y)})
    assert one not in game.bricks(box) and (one.x, one.y) not in game.solid
    # Areas hang under the text more often than they stand on it.
    under = over = 0
    for seed in range(40):
        game = bricks.Game(random.Random(seed))
        game.step(box, walls)
        for area in (e[1] for e in game.events if e[0] == "area"):
            top, low = min(b.y for b in area), max(b.y for b in area)
            span = {x for b in area for x, _ in b.cells}
            under += any((x, top - 1) in walls for x in span)
            over += any((x, low + 1) in walls for x in span)
    assert under > over > 0


def test_rules() -> None:
    seen: dict[str, int] = {}
    kinds: set[str] = set()
    caught: set[str] = set()
    for seed in range(4):
        game = bricks.Game(
            random.Random(seed), rules=True, bonuses=True, multiplayer=True
        )
        for _ in range(1500):
            game.step(BOX, WALLS)
            checks(game, BOX, WALLS)
            near = game.bricks(BOX)
            kinds.update(b.kind for b in near)
            assert all(1 <= b.lives <= bricks.KINDS[b.kind][0] for b in near)
            assert {r.team for r in game.rackets} == {0, 1}
            assert len(game.rackets) <= 4  # a twin each, at most
            for what, *data in game.events:
                seen[what] = seen.get(what, 0) + 1
                if what == "catch":
                    caught.add(data[1])
                elif what == "lost":
                    assert game.score[data[1]] == 0
        assert len(game.score) == len(game.best) == 2
    assert kinds == set(bricks.KINDS)
    assert seen.keys() >= {"area", "break", "hit", "bump", "boom", "lost", "catch"}
    assert len(caught) >= 3 and caught <= set(bricks.BONUSES)


def test_kinds() -> None:
    box = (0, 0, 40, 12)
    # A hard brick takes three hits.
    g = still(rules=True)
    hard = lay(g, 8, 2, "hard")
    floor = {(x, 8) for x in range(40)}  # the ball comes back
    shoot(g, 9.5, 3.0)
    lives = []
    for _ in range(60):
        g.rackets[0].dead = 2  # no racket in the way
        g.step(box, floor)
        lives.append(hard.lives)
    assert lives[0] == 2 and 1 in lives and 0 in lives and (8, 2) not in g.solid
    # A bomb takes its neighbours, bombs among them, not the far ones.
    g = still(rules=True)
    lay(g, 8, 2, "bomb")
    lay(g, 12, 2, "bomb")
    lay(g, 16, 1)
    far = lay(g, 28, 2)
    shoot(g, 9.5, 3.0)
    g.step(box, set())
    assert [e[0] for e in g.events].count("boom") == 2
    assert g.bricks(box) == [far] and g.score == [30]
    # A bumper sends the ball away, faster, and takes two hits.
    g = still(rules=True)
    bumper = lay(g, 8, 2, "bumper")
    ball = shoot(g, 9.5, 3.0)
    g.step(box, set())
    assert bumper.lives == 1 and ball.vy > 0 and abs(ball.vx) >= 0.5 and ball.fast
    # A loose brick falls and kills the racket under it.
    g = still(rules=True)
    lay(g, 8, 2, "loose")
    g.step(box, set())
    racket = g.rackets[0]
    shoot(g, 9.5, 3.0)
    g.step(box, set())
    assert not g.bricks(box) and [(f.x, f.y) for f in g.fallers] in ([(8, 2)], [(8, 3)])
    g.balls = []
    killed = False
    for _ in range(12):
        racket.x, racket.y = 7, 7  # held there
        g.step(box, set())
        killed = killed or "kill" in {e[0] for e in g.events}
    assert killed and not g.fallers and racket.dead
    for _ in range(bricks.DEAD):
        g.step(box, set())
    assert not racket.dead  # and comes back
    # On fire, a ball goes through the bricks.
    g = still(bonuses=True)
    for y in (2, 3, 4):
        lay(g, 8, y)
    ball = shoot(g, 9.5, 5.5)
    ball.fire = 20
    for _ in range(8):
        g.rackets[0].dead = 2
        g.step(box, set())
    assert not g.bricks(box) and ball.vy < 0


def test_bonuses() -> None:
    box = (0, 0, 40, 12)
    g = still(bonuses=True)
    g.step(box, set())
    racket = g.rackets[0]
    # A capsule sinks to the racket; small and wide change its size.
    for kind, w in (("small", bricks.SMALL), ("wide", bricks.WIDE)):
        g.capsules = [bricks.Capsule(50, racket.x + 2, racket.y - 2, kind)]
        for _ in range(8):
            g.step(box, set())
        assert not g.capsules and racket.w == w and racket.span
        mid = racket.x + racket.w // 2
        for _ in range(bricks.LAST[kind]):
            g.step(box, set())
        assert racket.w == bricks.RACKET and racket.x + racket.w // 2 == mid
    # A racket goes for a good capsule, not for a bad one.
    for kind, goes in (("fire", True), ("small", False)):
        at = (racket.x, racket.y)
        g.capsules = [bricks.Capsule(60, racket.x + 12, racket.y, kind, rest=-99)]
        g.step(box, set())
        assert ((racket.x, racket.y) != at) == goes
    g.capsules = []
    # Three balls for one; fire and speed for all.
    shoot(g, 20.5, 2.5, 0.5)
    g._award(bricks.Capsule(51, 0, 0, "multi"), racket, box, set())
    assert len(g.balls) == 3 and len({b.vx for b in g.balls}) == 3
    g._award(bricks.Capsule(52, 0, 0, "fire"), racket, box, set())
    g._award(bricks.Capsule(53, 0, 0, "fast"), racket, box, set())
    assert all(b.fire == 5 * g.second and b.fast for b in g.balls)
    # A twin racket for a while.
    g._award(bricks.Capsule(54, 0, 0, "twin"), racket, box, set())
    g.step(box, set())
    assert [r.dead for r in g.rackets] == [0, 0]
    for _ in range(bricks.LAST["twin"]):
        g.balls = []
        g.step(box, set())
    assert g.rackets == [racket]


def test_racket() -> None:
    box = (0, 0, 40, 16)
    g = still()
    g.step(box, set())
    racket = g.rackets[0]
    assert (racket.x, racket.y, racket.w, racket.dead) == (17, 8, 5, 0)
    # A ball on its way out by the top: the racket gets over it and is
    # hit from below. Then under it: both sides.
    ball = shoot(g, 30.5, 10.5)
    ball.vx = -0.3
    sides = set()
    for _ in range(120):
        vy = ball.vy
        g.step(box, set())
        if ball.vy != vy:
            assert (racket.y < ball.at[1]) == (ball.vy > 0)
            sides.add(ball.vy > 0)
    assert sides == {True, False} and g.balls == [ball]
    # Two rackets: each keeps its side, and loses its score there.
    g = still(multiplayer=True)
    g.step(box, set())
    top, low = g.rackets
    assert top.y < low.y and (top.team, low.team) == (0, 1)
    g.score = [50, 70]
    g.step(box, set())
    g.balls = [bricks.Ball(9, 3.5, 15.5, 0.0, 0.5)]
    g.step(box, set())
    assert g.score == [50, 0] and g.events[-1] == ("lost", (3, 15), 1)
    g.balls = [bricks.Ball(10, 36.5, 0.25, 0.0, -0.5)]
    g.step(box, set())
    assert g.score == [0, 0] and g.best == [50, 70] and not g.balls


def test_art() -> None:
    for kind, (lives, _) in bricks.KINDS.items():
        for left in range(1, lives + 1):
            look = _bricks.brick(kind, left, 3)
            for frame in look.get("frames") or [look]:
                text = "".join(run["t"] for run in frame["cells"][0])
                assert len(text) == bricks.BW
                assert all(0x2801 <= ord(ch) < 0x2900 for ch in text)  # braille
    for w in (bricks.SMALL, bricks.RACKET, bricks.WIDE):
        text = _bricks.racket(w)[0][0]["t"]
        assert len(text) == w and text[0] + text[-1] == "[]"
    assert _bricks.racket(5)[0][0]["t"] == "[===]"
    assert _bricks.racket(5, 1)[0][0]["fg"] == _bricks.TEAM[1]
    assert set(_bricks.LABEL) == set(bricks.BONUSES)
    assert len(_bricks.ball(fire=True)["frames"]) == 3
    assert "".join(r["t"] for r in _bricks.board([120])[0]) == "◆ 120"
    row = _bricks.board([5, 70], [90, 70])[0]
    assert "".join(r["t"] for r in row) == "▲ 5  ▼ 70  best 90 70"


def test_bricks(session: Pilot) -> None:
    assert "bricks" in APPS
    page(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "40"]
    watch(
        session,
        lambda: bricks.main(args, seconds=1.5),
        lambda host: all(it in host for it in ("[===]", "o", "⢾⣿⣿⡷", "◆")),
    )


def test_bricks_all(session: Pilot) -> None:
    page(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "60", "-a"]
    seen: set[str] = set()

    def look(host: str) -> bool:
        seen.update(host)
        # Two rackets and their scores, a ball, bricks of several kinds.
        return (
            host.count("[===]") >= 2
            and all(it in host for it in ("▲", "▼", "o"))
            and len(seen & {"⣏", "⣹", "⡱", "⠶", "⣻"}) >= 3
        )

    watch(session, lambda: bricks.main(args, seconds=4), look)
