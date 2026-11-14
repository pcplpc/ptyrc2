# SPDX-License-Identifier: GPL-3.0-or-later
"""`_canvas`: colours, braille, half blocks."""

from __future__ import annotations

from ptyrc_apps import _canvas
from ptyrc_apps._canvas import Braille, Half, ansi, hexa, mix, rgb, xterm

RED, BLUE = (255, 0, 0), (0, 0, 255)


def text(row: list[dict]) -> str:
    return "".join(run["t"] for run in row)


def test_colours() -> None:
    assert mix(RED, BLUE, 0) == RED and mix(RED, BLUE, 1) == BLUE
    assert mix(RED, BLUE, 0.5) == (128, 0, 128)
    assert mix(RED, BLUE, -3) == RED and mix(RED, BLUE, 7) == BLUE
    assert hexa((1, 171, 255)) == "#01abff"
    assert rgb(16) == (0, 0, 0) and rgb(231) == (255, 255, 255)
    assert rgb(196) == RED and rgb(232) == (8, 8, 8) and rgb(255) == (238, 238, 238)
    # Every colour of the 240 is its own nearest.
    assert all(xterm(rgb(n)) == n for n in range(16, 256))
    # A dark green stays green; a grey stays grey.
    r, g, b = rgb(xterm((40, 82, 20)))
    assert g > r and g > b
    assert xterm((90, 90, 90)) >= 232


def test_braille() -> None:
    dots = Braille(3, 2)
    assert dots.rows() == ["   ", "   "] and dots.cells() == []
    assert dots.glyph(0, 0) == "" and dots.glyph(9, 9) == ""
    dots.set(0, 0)
    dots.set(1, 3)
    dots.set(5, 7)
    dots.set(6, 0)  # outside: ignored
    dots.set(0, -1)
    assert dots.get(0, 0) and dots.get(1, 3) and not dots.get(1, 0)
    assert not dots.get(99, 0)
    assert dots.rows() == ["⢁  ", "  ⢀"]
    assert dots.cells() == [(0, 0), (2, 1)]
    dots.set(0, 0, False)
    assert dots.glyph(0, 0) == "⢀"
    line = Braille(2, 1)
    line.line(0, 0, 3, 3)
    assert line.rows() == ["⠑⢄"]
    for bits in range(1, 256):  # the eight dots are the eight bits
        cell = Braille(1, 1)
        for y in range(4):
            for x in range(2):
                if bits & _canvas.DOTS[y][x]:
                    cell.set(x, y)
        assert cell.rows() == [chr(0x2800 + bits)]


def test_half() -> None:
    half = Half(4, 2, BLUE)
    assert half.rows() == [[{"t": "    ", "bg": 21}]] * 2
    half.set(1, 0, RED)
    half.set(9, 9, RED)  # outside: ignored
    assert half.get(1, 0) == RED and half.get(9, 9) is None
    first = half.rows()[0]
    assert text(first) == " ▀  "
    # A blank joins the run before it: it shows no foreground.
    assert first[1] == {"t": "▀  ", "fg": 196, "bg": 21}
    half.set(1, 1, RED)
    assert half.rows()[0] == [
        {"t": " ", "bg": 21},
        {"t": " ", "bg": 196},
        {"t": "  ", "bg": 21},
    ]
    half.set(0, 2, RED, 0.5)
    assert half.get(0, 2) == (128, 0, 128)
    half.rect(2, 2, 2, 2, RED)
    assert [half.get(x, 3) for x in range(4)] == [BLUE, BLUE, RED, RED]
    half.fill(RED)
    assert half.rows(deep=True) == [[{"t": "    ", "bg": "#ff0000"}]] * 2
    disc = Half(5, 3)
    disc.disc(2.5, 2.5, 1.2, RED)
    lit = [(x, y) for y in range(6) for x in range(5) if disc.get(x, y) == RED]
    assert lit == [(2, 1), (1, 2), (2, 2), (3, 2), (2, 3)]


def test_glyphs() -> None:
    half = Half(4, 1, BLUE)
    half.put(1, 0, "ab", RED)
    half.put(3, 0, "cd", bg=RED)  # cut at the edge
    half.put(0, 5, "z")  # outside
    row = half.rows()[0]
    assert text(row) == " abc"
    assert row[1] == {"t": "ab", "bg": 21, "fg": 196}
    assert row[2] == {"t": "c", "bg": 196}
    # Under a glyph, the mean of the two pixels.
    two = Half(1, 1, RED)
    two.set(0, 1, BLUE)
    two.put(0, 0, "x")
    assert two.rows(deep=True)[0] == [{"t": "x", "bg": "#800080"}]


def test_ink() -> None:
    half = Half(3, 1, BLUE)
    half.set(1, 0, RED)  # an edge in the middle cell
    half.put(2, 0, "x")
    dots = Braille(3, 1)
    for x in range(6):
        dots.set(x, 0)
    half.ink(dots, (255, 255, 255), 0.5)
    # Not over the edge, not over a glyph.
    assert sorted(half.over) == [(0, 0), (2, 0)]
    assert half.over[0, 0] == ("⠉", (128, 128, 255), BLUE)
    assert half.over[2, 0][0] == "x"
    # Two pixels nearly alike: the one under the dots.
    near = Half(1, 1, (10, 10, 10))
    near.set(0, 1, (14, 14, 14))
    low = Braille(1, 1)
    low.set(0, 3)
    near.ink(low, RED)
    assert near.over[0, 0] == ("⡀", RED, (14, 14, 14))
    # Over an edge, `rough`: the pixel under the dots takes the ink.
    edge = Half(1, 1, BLUE)
    edge.set(0, 1, RED)
    edge.ink(low, (255, 255, 255), rough=0.5)
    assert not edge.over and edge.px[0][0] == BLUE
    assert edge.px[1][0] == (255, 128, 128)
    # Alike, but two colours of the 256: no glyph is shown there.
    near.fill((100, 100, 100))
    near.set(0, 1, (112, 112, 112))
    near.ink(low, RED)
    assert near.rows() == [[{"t": "▀", "fg": 241, "bg": 242}]]
    assert near.rows(deep=True)[0][0]["t"] == "⡀"


def test_ansi() -> None:
    rows = [[{"t": "a", "fg": 196, "bg": 21}, {"t": "b"}, {"t": "c", "bg": "#01abff"}]]
    assert ansi(rows) == ["\x1b[38;5;196;48;5;21ma\x1b[0mb\x1b[48;2;1;171;255mc\x1b[0m"]
