# SPDX-License-Identifier: GPL-3.0-or-later
"""`bricks`: the ball, the groups, the rackets and their shots, rules,
bonuses, the drawing."""

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
    assert not set(taken) & set(cells)  # nor on a brick; text is no obstacle
    assert all(x0 <= x < x1 and y0 <= y < y1 for x, y in taken)
    assert all(r.w in (bricks.SMALL, bricks.RACKET, bricks.WIDE) for r in rackets)
    assert len(game.balls) <= bricks.BALLS
    for ball in game.balls:
        assert x0 <= ball.at[0] < x1 and y0 <= ball.at[1] < y1
        assert ball.at not in walls and ball.at not in taken
        assert ball.fire or ball.at not in game.solid
        assert abs(ball.vy) == bricks.VY and abs(ball.vx) <= bricks.VX


def still(**kw: bool) -> bricks.Game:
    """A game that lays no group, serves no ball and never starts again."""
    game = bricks.Game(random.Random(1), **kw)
    game.spawn_due = game.serve_due = game.last = 10**9
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
            # A brick: 10, and up to 10 more in a row with no racket.
            won, n = game.score[0] - score, whats.count("break")
            assert bricks.POINTS * n <= won <= 2 * bricks.POINTS * n
        assert ("combo" in whats) <= ("break" in whats)
    assert seen.keys() >= {"group", "break", "serve", "bounce", "combo", "fade", "gone"}
    assert seen["break"] >= 40 and game.best[0] >= 100
    assert "reset" not in seen  # bricks are broken: no need to start again
    # Same seed, same game.
    a, b = (bricks.Game(random.Random(9)) for _ in range(2))
    for _ in range(200):
        a.step(BOX, WALLS)
        b.step(BOX, WALLS)
        assert [(o.x, o.y) for o in a.balls] == [(o.x, o.y) for o in b.balls]
        assert [(r.x, r.y) for r in a.rackets] == [(r.x, r.y) for r in b.rackets]
        assert a.solid.keys() == b.solid.keys()


def test_groups() -> None:
    """Small groups, one at a time, all over the view, where a ball
    gets: between the lines, after the short ones, in the blanks."""
    rows = [
        "" if i % 4 == 3 else "words " * (2 + i % 5) if i < 16 else ""
        for i in range(24)
    ]
    walls = walls_of(rows)
    box = (0, 0, 80, 24)
    game = bricks.Game(random.Random(5))
    laid = []
    for _ in range(900):
        game.step(box, walls)
        checks(game, box, walls)
        new = [e[1] for e in game.events if e[0] == "group"]
        assert len(new) <= 1  # one at a time
        laid += new
    assert len(laid) >= 30 and all(2 <= len(group) <= 3 for group in laid)
    assert all(
        abs(a.y - b.y) <= 1 and abs(a.x - b.x) <= 6 for g in laid for a in g for b in g
    )
    sectors = {(4 * g[0].x // 80, 3 * g[0].y // 24) for g in laid}
    assert len(sectors) == 12  # every twelfth of the view
    lines = {y for _, y in walls}
    after = [g for g in laid if all(b.y in lines for b in g)]
    between = [g for g in laid if g[0].y not in lines and g[0].y < 16]
    blank = [g for g in laid if g[0].y > 16]
    assert after and between and blank
    # The first ones are far from each other.
    first = [(g[0].x, g[0].y) for g in laid[:4]]
    assert all(
        abs(a[0] - b[0]) + 2 * abs(a[1] - b[1]) >= 12
        for i, a in enumerate(first)
        for b in first[:i]
    )
    # No room next to it: no group. A sealed row is no room.
    full = walls_of(["x" * 30, "", "x" * 30])
    game = bricks.Game(random.Random(1))
    for _ in range(40):
        game.step((0, 0, 30, 3), full)
    assert not game.solid and game._group((0, 0, 30, 4), full) == []


def test_terrain() -> None:
    walls = walls_of(["", "", "  a line of words, long enough  "])
    box = (0, 0, 60, 20)
    game = bricks.Game(random.Random(2))
    game.step(box, walls)
    assert len(game.bricks(box)) in (2, 3)  # a group at the first step
    for _ in range(40):
        game.step(box, walls)
    kept = {b.n: (b.x, b.y) for b in game.bricks(box)}
    assert len(game.groups) >= 3  # the first ones come fast
    # Scrolled away: what moves starts again there, groups are laid.
    far = (0, 500, 60, 520)
    for _ in range(60):
        game.step(far, set())
        checks(game, far, set())
    assert game.bricks(far) and game.balls and far[1] <= game.rackets[0].y < far[3]
    # And back: the bricks are where they were, until they are due.
    assert {b.n: (b.x, b.y) for b in game.bricks(box)} == kept
    # Text written on a brick: the brick is gone.
    one = game.bricks(box)[0]
    game.step(box, walls | {(one.x + 1, one.y)})
    assert one not in game.bricks(box) and (one.x, one.y) not in game.solid


def test_age() -> None:
    """A group goes 10 to 20 s after it is laid, dim a second before."""
    box = (0, 0, 60, 20)
    dues = set()
    for seed in range(6):
        game = bricks.Game(random.Random(seed), second=8)
        game.step(box, set())
        game.spawn_due = game.serve_due = game.last = 10**9
        (group,) = game.groups
        there = list(group.bricks)
        assert all(b.due == group.due for b in there)
        assert 10 * 8 < group.due <= 1 + 20 * 8
        dues.add(group.due)
        seen = {}
        while game.steps < group.due:
            game.step(box, set())
            for what, *data in game.events:
                seen[what] = (game.steps, data[0])
            if game.steps < group.due:
                assert game.bricks(box) == there
        assert seen == {"fade": (group.due - 8, there), "gone": (group.due, there)}
        assert not game.solid and not game.groups and not game.bricks(box)
    assert len(dues) == 6


def test_reset() -> None:
    """30 s with no brick broken: all starts again, but the scores."""
    box = (0, 0, 60, 20)
    game = bricks.Game(random.Random(4), rules=True, bonuses=True, second=4)
    for _ in range(30):
        game.step(box, set())
    assert game.solid and game.balls
    game.rackets.append(bricks.Racket(77, 0, left=10**6))
    game.step(box, set())
    racket = game.rackets[0]
    game.serve_due = 10**9
    game.last, game.score, game.best = game.steps, [130], [200]
    seen = []
    for i in range(30 * 4):
        game.balls = []  # no ball: no brick broken
        game.capsules = [bricks.Capsule(88, 3, 3, "fast", rest=-99)]
        game.fallers = [bricks.Faller(89, 50, 2)]
        at = (racket.x, racket.y)
        game.step(box, set())
        if i == 118:
            there = game.bricks(box)
        seen += [(i, e) for e in game.events if e[0] in ("reset", "gone") and i == 119]
    # The bricks that were in the view go, then the game starts again.
    assert there and seen[:2] == [(119, ("gone", there)), (119, ("reset", (30, 10)))]
    assert not (game.balls or game.capsules or game.fallers)
    assert len(game.groups) <= 1 and not {b.n for b in there} & {
        b.n for b in game.solid.values()
    }
    assert game.rackets == [racket] and not racket.dead and racket.w == bricks.RACKET
    assert (racket.x, racket.y) != at and game.score == [130] and game.best == [200]
    # A new map and a ball again; the count starts again.
    whats = set()
    for _ in range(40):
        game.step(box, set())
        whats.update(e[0] for e in game.events)
    assert whats >= {"group", "serve"} and "reset" not in whats and game.solid


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
    assert seen.keys() >= {"group", "break", "hit", "bump", "boom", "lost", "catch"}
    assert len(caught) >= 3 and caught <= set(bricks.BONUSES)
    assert seen["catch"] >= 12  # the rackets do go for them


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
    assert g.bricks(box) == [far] and g.score == [10 + 12 + 14]  # in a row
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
    # Two balls for one, `BALLS` at most; fire and speed for all.
    ball = shoot(g, 20.5, 2.5, 0.5)
    ball.vx, ball.combo = 0.3, 2
    for n in (2, 4, bricks.BALLS, bricks.BALLS):
        g._award(bricks.Capsule(51, 0, 0, "multi"), racket, box)
        assert len(g.balls) == n
    assert len({b.vx for b in g.balls[:4]}) == 4
    assert all(abs(b.vx) <= bricks.VX and b.vy == 0.5 for b in g.balls)
    assert all((b.x, b.y, b.combo) == (20.5, 2.5, 2) for b in g.balls)
    g._award(bricks.Capsule(52, 0, 0, "fire"), racket, box)
    g._award(bricks.Capsule(53, 0, 0, "fast"), racket, box)
    assert all(b.fire == 5 * g.second and b.fast for b in g.balls)
    # A spare ball out costs nothing; the last one, the score.
    g.balls = g.balls[:2]
    for ball, y in zip(g.balls, (11.5, 2.5), strict=True):
        ball.x, ball.y, ball.vx, ball.vy = 2.5, y, 0.0, 0.5
    g.score = [40]
    g.step(box, set())
    assert [e[0] for e in g.events] == ["out"] and g.score == [40]
    g.balls[0].y, g.balls[0].fire = 11.5, 0
    g.step(box, set())
    assert [e[0] for e in g.events] == ["lost"] and g.score == [0]
    # A twin racket for a while.
    g._award(bricks.Capsule(54, 0, 0, "twin"), racket, box)
    g.step(box, set())
    assert [r.dead for r in g.rackets] == [0, 0]
    for _ in range(bricks.LAST["twin"]):
        g.balls = []
        g.step(box, set())
    assert g.rackets == [racket]


def test_drops() -> None:
    """`multi` is by far the most common capsule; a game with `-b`
    has one caught within 20 s."""
    shares = bricks.BONUSES
    assert shares["multi"] >= 5 * max(
        n for kind, n in shares.items() if kind != "multi"
    )
    assert 2 * shares["multi"] > sum(shares.values())
    g = still(bonuses=True)
    box = (0, 0, 40, 12)
    kinds = []
    for i in range(400):
        one = lay(g, 3 * (i % 12), 2)
        g._smash(one, shoot(g, 1.5, 9.5), (one.x, 2))
        kinds += [c.kind for c in g.capsules]
        g.capsules = []
    assert 130 <= len(kinds) <= 190  # two bricks in five
    assert set(kinds) == set(shares) and kinds.count("multi") > len(kinds) / 2
    rows = ["", "  a few words", "", "", "more words in the way here", *[""] * 19]
    walls = walls_of(rows)
    box = (0, 0, 80, 24)
    late = 0
    for seed in range(8):
        game = bricks.Game(random.Random(seed), bonuses=True)
        caught = False
        for _ in range(20 * game.second):
            game.step(box, walls)
            checks(game, box, walls)
            caught = caught or ("catch", "multi") in {e[::2] for e in game.events}
        late += not caught
    assert late <= 1


def test_text() -> None:
    """Text stops a ball, not a racket."""
    box = (0, 0, 40, 24)
    line = {(x, 10) for x in range(4, 36)}
    g = still()
    g.home = (0.5, 0.6)
    assert (17, 10) in g._room(box, 5, set())
    g.step(box, line)
    racket = g.rackets[0]
    assert (racket.x, racket.y) == (17, 14)
    # A ball over the line, on its way out: the racket goes under the
    # line and comes out in its way.
    ball = shoot(g, 30.5, 9.5)
    ball.vx = -0.4
    under = hit = False
    for _ in range(30):
        g.step(box, line)
        checks(g, box, line)
        under = under or bool(line.intersection(racket.cells))
        hit = hit or any(e[0] == "bounce" for e in g.events)
    assert under and hit and g.balls == [ball] and not racket.dead
    # Under the text a racket is not there: the ball meets the text.
    g = still()
    g.step(box, line)
    racket = g.rackets[0]
    racket.x, racket.y = 17, 10
    ball = shoot(g, 19.5, 12.5)
    ball.owner = 7
    for _ in range(4):
        g._roll(ball, box, line)
    assert ball.vy > 0 and ball.owner == 7 and not g.events
    # With nothing to do it steps out, as near as it can, and stays.
    g.balls = []
    seen = []
    for _ in range(12):
        g.step(box, line)
        seen.append((racket.x, racket.y))
    assert seen[0] != (17, 10) and seen[1:] == [(17, 9)] * 11
    # It serves from the nearest free cell; a brick still kills it.
    racket.x, racket.y = 17, 10
    g.serve_due = 0
    for y in (9, 11):
        line |= {(x, y) for x in range(4, 36)}
    g.step(box, line)
    assert len(g.balls) == 1 and g.balls[0].at not in line
    racket.x, racket.y = 17, 10
    lay(g, 18, 10)
    g.step(box, line)
    assert racket.y != 10 and not racket.dead  # away, and back at once
    # It comes back off the text when it can.
    racket.dead = 1
    g.step(box, {(x, 12) for x in range(40)})
    assert not racket.dead and racket.y != 12
    # The only place to stop a ball is under the text: it goes there.
    roof = {(x, y) for x in range(40) for y in range(4) if x != 20}
    g = still()
    g.step(box, roof)
    racket = g.rackets[0]
    ball = shoot(g, 20.5, 7.5)
    hit = False
    for _ in range(12):
        g.step(box, roof)
        if any(e[0] == "bounce" for e in g.events):
            hit = True
            assert len(roof.intersection(racket.cells)) == 4
    assert hit and g.balls == [ball] and ball.vy > 0


def test_shots() -> None:
    """A shot is chosen for the bricks it breaks."""
    box = (0, 0, 40, 16)
    for side in (2, 32):
        # A ball on its way out, bricks on one side: sent there.
        g = still()
        g.step(box, set())
        for y in (2, 3, 4):
            lay(g, side, y)
            lay(g, side + 3, y)
        ball = shoot(g, 20.5, 6.5, 0.5)
        seen = []
        for _ in range(45):
            g.step(box, set())
            seen += [(e[0], ball.vx) for e in g.events if e[0] in ("bounce", "break")]
        assert seen[0][0] == "bounce" and (seen[0][1] < 0) == (side < 20)
        assert [what for what, _ in seen[:3]] == ["bounce", "break", "bounce"]
        # The worth of a shot, between two lines: the bricks it breaks;
        # a dent is less than a brick, a bomb more.
        shut = {(x, y) for x in range(40) for y in (0, 15)}
        g = still(rules=True)
        g.step(box, shut)
        racket = g.rackets[0]
        ball = shoot(g, 20.5, 6.5, 0.5)
        flight = g._foresee(ball, box, shut)

        def worth(lx: int, g=g, racket=racket, flight=flight, shut=shut) -> float:
            i = next(i for i in flight.entries if flight.path[i + 1][2] == 11.0)
            return g._shot(racket, flight, i, (lx, 11), box, shut)

        # Hit on its left end, the ball leaves to the left.
        towards, away = (20, 16) if side < 20 else (16, 20)
        bx = 8 if side < 20 else 30
        assert worth(towards) == worth(away) == 0
        it = lay(g, bx, 3, "hard")
        dent = worth(towards)
        assert 0 < dent < 0.5 and worth(away) == 0
        g._lift(it)
        it = lay(g, bx, 3)
        one = worth(towards)
        assert 2 * dent < one <= 1 and worth(away) == 0
        g._lift(it)
        lay(g, bx, 3, "bomb")
        lay(g, bx + 3, 3)
        lay(g, bx, 2)
        assert worth(towards) == 3 * one  # a bomb: its neighbours too
    # Two teams: bricks first; a ball out by the other's side, only
    # when nothing else tells two shots apart.
    for side in (2, 32):
        g = still(multiplayer=True)
        g.step(box, set())
        low = g.rackets[1]
        ball = shoot(g, 20.5, 6.5, 0.5)
        flight = g._foresee(ball, box, set())
        i = next(i for i in flight.entries if flight.path[i + 1][2] == 11.0)
        towards, away = (20, 16) if side < 20 else (16, 20)
        out = g._shot(low, flight, i, (away, 11), box, set())
        assert 0 < out < bricks.DENT  # out by the top: the tie-break
        lay(g, 8 if side < 20 else 30, 3)
        assert g._shot(low, flight, i, (towards, 11), box, set()) > 0.5 > out
        seen = []
        for _ in range(40):
            g.step(box, set())
            seen += [e for e in g.events if e[0] in ("bounce", "break", "lost", "out")]
        assert [e[0] for e in seen[:2]] == ["bounce", "break"] and seen[0][2] is low
        assert seen[1][3] == 1  # the brick is the low team's


def test_idle() -> None:
    """A racket with no ball to stop hits one on purpose when that
    breaks more bricks; else it waits near, and does not dither."""
    box = (0, 0, 40, 16)
    lines = {(x, y) for x in range(40) for y in (1, 12)}
    for wall in (True, False):
        g = still()
        g.step(box, lines)
        racket = g.rackets[0]
        if wall:
            for y in (4, 5):
                lay(g, 3, y)
                lay(g, 6, y)
        ball = shoot(g, 30.5, 6.5, 0.5)  # up and down between two lines
        assert g._foresee(ball, box, lines).side is None
        seen, moves, turns, was = [], 0, 0, (0, 0)
        for _ in range(120):
            at = (racket.x, racket.y)
            g.step(box, lines)
            checks(g, box, lines)
            seen += [e[0] for e in g.events if e[0] in ("bounce", "break")]
            way = (racket.x - at[0], racket.y - at[1])
            if way != (0, 0):
                moves += 1
                turns += way[0] * was[0] < 0 or way[1] * was[1] < 0
                was = way
        if wall:
            assert seen[0] == "bounce" and seen.count("break") >= 3
        else:
            assert "break" not in seen and g.balls == [ball]
            assert turns <= 2 and moves <= 40 and abs(racket.x + 2 - ball.x) <= 6


def test_duel() -> None:
    """Two rackets face to face: one of them keeps away for 2 to 3 s."""
    box = (0, 0, 40, 24)

    def rally(gap: int, second: int) -> tuple[bricks.Game, list, int]:
        """A ball between two rackets `gap` rows apart, held there."""
        g = still(multiplayer=True, second=second)
        g.step(box, set())
        top, low = g.rackets
        (top.x, top.y), (low.x, low.y) = (17, 12 - gap // 2), (17, 12 + gap // 2)
        ball = shoot(g, 19.5, 11.5, 0.5)
        seen: list = []
        for _ in range(3 * second):
            g.steps += 1
            g.events = []
            g._roll(ball, box, set())
            seen += g.events
            if g.events and g.events[-1][0] == "retreat":
                break
        return g, seen, g.steps

    picked = set()
    for seed in range(6):
        g, seen, start = rally(4, 8)
        g.rng.seed(seed)
        assert [e[0] for e in seen] == ["bounce"] * 4 + ["retreat"]
        assert [e[2].team for e in seen[:4]] in ([0, 1, 0, 1], [1, 0, 1, 0])
        one = seen[-1][2]
        (other,) = [r for r in g.rackets if r is not one]
        picked.add(one.team)
        assert 2 * 8 <= one.away - start <= 3 * 8 and not other.away
        # It goes to its side of the view, off the other, and stays;
        # there it still stops a ball on its way out.
        while g.steps < one.away:
            g.step(box, set())
            checks(g, box, set())
            assert "retreat" not in {e[0] for e in g.events}
            if g.steps > start + 4:
                rows = abs(one.y - other.y)
                assert rows > bricks.NEAR or one.y in (2, 21)
                assert (one.y < 12) == (one.team == 0)
        assert abs(one.y - other.y) > bricks.NEAR
        # Then it plays again.
        at, moved = (one.x, one.y), False
        for _ in range(60):
            g.step(box, set())
            moved = moved or (one.x, one.y) != at
        assert moved
    # Far from each other, or one team: no duel.
    assert "retreat" not in {e[0] for e in rally(8, 16)[1]}
    assert [e[0] for e in rally(8, 16)[1]].count("bounce") >= 4
    # Two balls, each hit by one team: no duel either.
    g = still(multiplayer=True)
    for i in range(8):
        g._duel(g.rackets[i % 2], bricks.Ball(i % 2, 1.5, 1.5, 0.0, 0.5))
    assert not g.events
    for i in range(4):
        g._duel(g.rackets[i % 2], bricks.Ball(5, 1.5, 1.5, 0.0, 0.5))
    assert [e[0] for e in g.events] == ["retreat"]
    g = still()
    for _ in range(6):
        g._duel(g.rackets[0], bricks.Ball(9, 1.5, 1.5, 0.0, 0.5))
    assert not g.events and not g.rackets[0].away


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
                assert text[0] + text[-1] in ("⟦⟧", "⟨⟩")
    for w in (bricks.SMALL, bricks.RACKET, bricks.WIDE):
        text = _bricks.racket(w)[0][0]["t"]
        assert len(text) == w and text[0] + text[-1] == "()"
    assert _bricks.racket(5)[0][0]["t"] == "(━━━)"
    assert _bricks.racket(5, 1)[0][0]["fg"] == _bricks.TEAM[1]
    assert set(_bricks.LABEL) == set(bricks.BONUSES)
    assert "".join(r["t"] for r in _bricks.board([120])[0]) == "◆ 120"
    row = _bricks.board([5, 70], [90, 70])[0]
    assert "".join(r["t"] for r in row) == "▲ 5  ▼ 70  best 90 70"


def test_bricks(session: Pilot) -> None:
    assert "bricks" in APPS
    page(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "40"]
    seen: set[str] = set()

    def look(host: str) -> bool:
        seen.update(host)
        return all(it in host for it in ("(━━━)", "⟦=⟧", "◆")) and bool(seen & {*"▀▄"})

    watch(session, lambda: bricks.main(args, seconds=1.5), look)


def test_bricks_all(session: Pilot) -> None:
    page(session)
    args = ["-s", session.session, "--seed", "4", "--speed", "60", "-a"]
    seen: set[str] = set()

    def look(host: str) -> bool:
        seen.update(host)
        # Two rackets and their scores, a ball, bricks of several kinds.
        return (
            host.count("(━━━)") >= 2
            and all(it in host for it in ("▲", "▼", "⟦=⟧"))
            and bool(seen & {*"▀▄"})
            and len(seen & {*"≡⟨oO*"}) >= 3
        )

    watch(session, lambda: bricks.main(args, seconds=4), look)
