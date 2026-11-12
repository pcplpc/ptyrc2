# SPDX-License-Identifier: GPL-3.0-or-later
"""`blocks`: the pieces, the lines, chains, the cursor, the drawing."""

from __future__ import annotations

import random
import threading
import time
from collections.abc import Callable

import pytest
from ptyrc import Pilot
from ptyrc_apps import _blocks, blocks
from ptyrc_apps.snake import walls_of

ROWS = ["", "  wall of words  ", "", "   another   one", "", "", "", "", "", ""]
BOX = (10, 100, 34, 110)
WALLS = walls_of(ROWS, 10, 100)
BRAILLE = {chr(c) for c in range(0x2801, 0x2900)}


def play(game: blocks.Game, steps: int, box=BOX, walls=WALLS, cursor=None) -> list:
    """Run `steps`; the rules hold at each. The events."""
    x0, y0, x1, y1 = box
    events = []
    for _ in range(steps):
        game.step(box, walls, cursor)
        events += game.events
        heap = game.blocks
        assert not walls & set(heap)
        assert all(x0 <= x < x1 and y0 <= y < y1 for x, y in heap)
        assert len(game.pieces) <= game.count
        for piece in game.pieces:
            cells = game.cells(piece)
            assert len(cells) == 4 and not set(cells) & (walls | set(heap))
            assert 0 <= piece.phase < game.period
    return events


def test_shapes_and_braille() -> None:
    assert {k: len(v) for k, v in blocks.SHAPES.items()} == {
        "I": 2,
        "O": 1,
        "T": 4,
        "S": 2,
        "Z": 2,
        "J": 4,
        "L": 4,
    }
    assert all(len(set(s)) == 4 for v in blocks.SHAPES.values() for s in v)
    cell = [(x, y) for x in range(2) for y in range(4)]
    assert blocks.braille(cell) == ((0, 0), ["⣿"])
    # Two dots down: the bottom half of a cell, the top half of the next.
    assert blocks.braille([(x, y + 2) for x, y in cell]) == ((0, 0), ["⣤", "⠛"])
    # One dot left, from cell 3: the right of cell 2, the left of cell 3.
    assert blocks.braille([(x + 5, y - 4) for x, y in cell]) == ((2, -1), ["⢸⡇"])
    assert blocks.braille([(0, 0), (4, 0)]) == ((0, 0), ["⠁ ⠁"])


@pytest.mark.parametrize("sideways", [False, True])
def test_game(sideways: bool) -> None:
    game = blocks.Game(random.Random(3), sideways=sideways)
    events = play(game, 3000)
    assert game.lines >= 10 and any(e[0] == "break" for e in events)
    assert {e[0] for e in events} <= {"break", "lost", "over"}
    assert all(e[3] == "line" for e in events if e[0] == "break")
    # Same seed, same game.
    twin = blocks.Game(random.Random(3), sideways=sideways)
    play(twin, 3000)
    assert (twin.blocks, twin.score) == (game.blocks, game.score)
    # The pictures: braille in the colours of the blocks.
    at, cells = game.heap_picture()
    assert at == (min(x for x, _ in game.blocks), min(y for _, y in game.blocks))
    assert sum(r["t"].count("⣿") for row in cells for r in row) == len(game.blocks)
    for piece in game.pieces:
        at, cells = game.picture(piece)
        text = "".join(r["t"] for row in cells for r in row)
        assert set(text) <= BRAILLE | {" "}
        assert cells[0][0]["fg"] == blocks.INK[piece.kind]
    # Scrolled out of sight: the heap is gone, the game goes on there.
    there = (10, 200, 34, 210)
    play(game, 200, there, set())
    assert game.pieces and all(200 <= y < 210 for _, y in game.blocks)


def test_fall() -> None:
    game = blocks.Game(random.Random(1), count=1)
    box = (0, 0, 10, 6)
    game.step(box, set())
    (piece,) = game.pieces
    piece_start = game.cells(piece)
    looks = []
    while game.pieces:
        at, cells = game.picture(piece)
        looks.append((at, tuple(r["t"] for row in cells for r in row)))
        game.step(box, set())
    # A dot at a time: four pictures a row, three of them between two cells.
    rows = max(y for _, y in game.blocks) - max(y for _, y in piece_start)
    assert rows >= 3 and len(set(looks)) >= 4 * rows
    assert sum(set("".join(text)) != {"⣿"} for _, text in looks) >= 3 * rows
    assert len(game.blocks) == 4 and max(y for _, y in game.blocks) == 5
    assert game.period == 4 and blocks.Game(random.Random(1), sideways=True).period == 2


def test_lines() -> None:
    # A line is a run between walls: here 3 cells, then 4.
    walls = {(3, 5)}
    box = (0, 0, 8, 6)
    game = blocks.Game(random.Random(1), count=0)
    game.step(box, walls)
    game.heap.update({(0, 5): "red", (1, 5): "red", (2, 5): "blue", (1, 4): "green"})
    game.heap.update({(4, 5): "red", (5, 5): "red", (6, 5): "red"})
    game._touch()
    game._clear()
    assert sorted(e[1] for e in game.events) == [(0, 5), (1, 5), (2, 5)]
    assert game.score == 3 and game.lines == 1
    game.step(box, walls)
    # What stood on the line came down; the other run is not full.
    assert game.blocks == {(1, 5): "green", (4, 5): "red", (5, 5): "red", (6, 5): "red"}
    # Text written on a block: gone.
    game.step(box, walls | {(5, 5)})
    assert (5, 5) not in game.blocks and len(game.blocks) == 3


def test_lines_sideways() -> None:
    # Pieces go left: a line is a column, between a wall and the floor.
    walls = {(0, 2)}
    box = (0, 0, 6, 5)
    game = blocks.Game(random.Random(1), sideways=True, count=0)
    game.step(box, walls)
    for y, color in ((3, "red"), (4, "blue")):
        game.heap[game._in((0, y))] = color
    game.heap[game._in((1, 4))] = "green"
    game._touch()
    game._clear()
    assert sorted(e[1] for e in game.events) == [(0, 3), (0, 4)]
    game.step(box, walls)
    assert game.blocks == {(0, 4): "green"}


def test_chains() -> None:
    box = (0, 0, 6, 6)
    heap = {
        (0, 5): "red",
        (1, 5): "blue",
        (2, 5): "blue",
        (3, 5): "blue",
        (4, 5): "blue",
        (5, 5): "blue",
        (0, 4): "red",
        (0, 3): "red",
        (1, 3): "red",
        (1, 4): "green",
        (2, 4): "blue",
        (0, 2): "green",
    }
    plain, chained = (
        blocks.Game(random.Random(1), chains=c, count=0) for c in (False, True)
    )
    for game in (plain, chained):
        game.step(box, set())
        game.heap.update(heap)
        game._touch()
        game._clear()
        assert len(game.events) == 6 and game.score == 6
    plain.step(box, set())
    assert plain.score == 6 and len(plain.blocks) == 6
    # Red to red to red, a step each; blue to blue; green stays.
    waves = []
    for _ in range(5):
        chained.step(box, set())
        assert all(e[3] == "chain" for e in chained.events)
        waves.append(sorted((e[1], e[2]) for e in chained.events))
    assert waves == [
        [((0, 4), "red"), ((2, 4), "blue")],
        [((0, 3), "red")],
        [((1, 3), "red")],
        [],
        [],
    ]
    assert chained.score == 10
    # Then the heap comes down: the greens, each on the floor.
    assert chained.blocks == {(1, 5): "green", (0, 5): "green"}


def test_over() -> None:
    box = (0, 0, 6, 4)
    game = blocks.Game(random.Random(1), count=1)
    game.step(box, set())
    game.pieces.clear()
    game.heap.update({(x, y): "red" for x in range(5) for y in range(4)})
    game.score = 7
    game._touch()
    game._due = 0
    game.step(box, set())
    (event,) = game.events
    assert event[0] == "over" and len(event[1]) == 20
    assert game.blocks == {} and game.score == 0
    play(game, 100, box, set())
    assert game.blocks


@pytest.mark.parametrize("sideways", [False, True])
def test_cursor(sideways: bool) -> None:
    box = (0, 0, 30, 16)
    game = blocks.Game(random.Random(2), sideways=sideways, count=1)
    cursor = (22, 3)
    falling: dict[int, list] = {}
    landed = []
    for _ in range(250):
        game.step(box, set(), cursor)
        assert all(p.goal is not None for p in game.pieces)
        now = {p.n: game.cells(p) for p in game.pieces}
        landed += [cells for n, cells in falling.items() if n not in now]
        falling = now
    # Each piece landed on the cursor's column (its row, sideways).
    assert len(landed) >= 3
    assert all(any(c[sideways] == cursor[sideways] for c in new) for new in landed)
    # Of several pieces, the nearest one is the player's.
    game = blocks.Game(random.Random(2), sideways=sideways, count=3)
    for _ in range(40):
        game.step(box, set(), cursor)
        aimed = [p for p in game.pieces if p.goal is not None]
        assert len(aimed) == 1

        def far(piece: blocks.Piece) -> int:
            return min(
                abs(x - cursor[0]) + abs(y - cursor[1]) for x, y in game.cells(piece)
            )

        # Chosen before the pieces move: a move and a dot of fall each.
        assert far(aimed[0]) <= min(map(far, game.pieces)) + 4
    assert game.picture(aimed[0])[1][0][0].get("bold")


def test_art() -> None:
    rng = random.Random(1)
    one = _blocks.crumb(rng, "red")
    glyphs = [f["cells"][0][0]["t"] for f in one["frames"]]
    assert glyphs[0] == "⣿" and set(glyphs) <= BRAILLE
    assert one["loop"] is False and one["motion"]["ay"] > 0
    side = _blocks.crumb(rng, "red", sideways=True)["motion"]
    assert side["ax"] < 0 and side["ay"] == 0
    assert _blocks.board(12)[0][0]["t"] == "⣿ 12"


def shown(pilot: Pilot) -> str:
    return "\n".join(pilot.dump()["host"])


def watch(pilot: Pilot, run: Callable[[], int], seen: Callable[[str], bool]) -> None:
    """Run the app in a thread; `seen` must hold on the host at some
    point, and the host must be clean once it is gone."""
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
        time.sleep(0.02)
    assert ok, "the app never showed"
    assert pilot.sprites() == []
    assert pilot.dump()["host"] == pilot.text()


def page(pilot: Pilot, *lines: str) -> None:
    text = "\\n".join(lines)
    pilot.send(f"clear; printf '{text}\\n'\n")
    pilot.wait_for(max(lines, key=len).strip(), frame="raw")
    deadline = time.monotonic() + 4
    while not pilot.panes() and time.monotonic() < deadline:
        time.sleep(0.05)
    assert pilot.panes()


@pytest.mark.parametrize("sideways", [False, True])
def test_blocks(session: Pilot, sideways: bool) -> None:
    page(session, "", "", "words in the way", "", "   more words here")
    args = ["-s", session.session, "--seed", "4", "--speed", "40"]
    args += ["--sideways"] if sideways else []
    glyphs: set[str] = set()

    def look(host: str) -> bool:
        glyphs.update(host)
        # A whole block, and one between two cells.
        return "⣿" in glyphs and len(glyphs & BRAILLE) >= 2

    watch(session, lambda: blocks.main(args, seconds=2.5), look)


def test_blocks_all(session: Pilot) -> None:
    page(session, "", "", "words in the way", "", "   more words here")
    args = ["-s", session.session, "--seed", "4", "--speed", "60", "-a", "--cursor"]
    args += ["--count", "5"]
    watch(
        session,
        lambda: blocks.main(args, seconds=3),
        lambda host: "⣿ " in host.split("\n")[0],
    )


def test_blocks_usage(capsys: pytest.CaptureFixture[str]) -> None:
    for bad in (["--speed", "0"], ["--count", "0"]):
        with pytest.raises(SystemExit):
            blocks.main(bad)
    capsys.readouterr()
