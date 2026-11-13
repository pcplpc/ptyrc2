# SPDX-License-Identifier: GPL-3.0-or-later
"""Canvases for the widgets that draw: colours, braille, half blocks.

Pure: no driver, no clock.

- A colour is `(r, g, b)`, each `0..255`. `mix(a, b, t)`: from `a`
  (`t` 0) to `b` (`t` 1). `xterm(c)`: the nearest of the 240 fixed
  colours of the 256 (16 to 255), a grey only for a grey; `rgb(n)`:
  its value. `hexa(c)`:
  `#rrggbb`.
- `Braille(w, h)`: `2w` by `4h` dots, on or off. `set`, `get`, `line`;
  `glyph(cx, cy)`: the cell, `""` when no dot is on; `rows()`: strings,
  a space where empty.
- `Half(w, h)`: `w` by `2h` pixels of colour, two a cell, drawn with
  `▀`. `fill`, `set` (with an `alpha`), `get`, `rect`, `disc`. Glyphs
  go over the pixels: `put(cx, cy, text, fg)` writes cells, the
  background the mean of the two pixels under each unless `bg` is
  given; `ink(braille, fg)` puts every cell of a `Braille` that has a
  dot. `rows()`: rows of runs `{"t", "fg", "bg"}`, colours of the 256;
  `rows(deep=True)`: `#rrggbb`.
- `ansi(rows)`: the rows with SGR escapes, for a terminal.

Coordinates: `x` right, `y` down, from 0. A pixel or a dot outside the
canvas is ignored.
"""

from __future__ import annotations

from collections.abc import Sequence
from functools import cache
from typing import Any

__all__ = ["RGB", "Braille", "Half", "Rows", "ansi", "hexa", "mix", "rgb", "xterm"]

RGB = tuple[int, int, int]
Run = dict[str, Any]
Rows = list[list[Run]]

#: Bit of the dot `(x, y)` of a braille cell: `DOTS[y][x]`.
DOTS = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))
_CUBE = (0, 95, 135, 175, 215, 255)


def mix(a: RGB, b: RGB, t: float) -> RGB:
    """From `a` to `b`: `t` 0 is `a`, 1 is `b`; clamped."""
    t = min(1.0, max(0.0, t))
    return (
        round(a[0] + (b[0] - a[0]) * t),
        round(a[1] + (b[1] - a[1]) * t),
        round(a[2] + (b[2] - a[2]) * t),
    )


def rgb(n: int) -> RGB:
    """The value of the colour `n` of the 256, `16..255`."""
    if n >= 232:
        v = 8 + 10 * (n - 232)
        return (v, v, v)
    n -= 16
    return (_CUBE[n // 36], _CUBE[n // 6 % 6], _CUBE[n % 6])


@cache
def xterm(c: RGB) -> int:
    """The nearest colour of the 256 to `c`, among 16 to 255."""
    r, g, b = c
    chroma = max(c) - min(c)

    def far(n: int) -> int:
        x, y, z = rgb(n)
        # Eyes weigh green most, blue least; a dark colour keeps its
        # hue rather than turn grey.
        lost = max(0, chroma - (max(x, y, z) - min(x, y, z)))
        return 3 * (x - r) ** 2 + 4 * (y - g) ** 2 + 2 * (z - b) ** 2 + 2 * lost**2

    return min(range(16, 256), key=far)


def hexa(c: RGB) -> str:
    return f"#{c[0]:02x}{c[1]:02x}{c[2]:02x}"


class Braille:
    """`2w` by `4h` dots in `w` by `h` cells."""

    def __init__(self, w: int, h: int) -> None:
        self.w, self.h = w, h
        self.bits = [[0] * w for _ in range(h)]

    def set(self, x: int, y: int, on: bool = True) -> None:
        if 0 <= x < 2 * self.w and 0 <= y < 4 * self.h:
            bit = DOTS[y & 3][x & 1]
            if on:
                self.bits[y >> 2][x >> 1] |= bit
            else:
                self.bits[y >> 2][x >> 1] &= ~bit

    def get(self, x: int, y: int) -> bool:
        if 0 <= x < 2 * self.w and 0 <= y < 4 * self.h:
            return bool(self.bits[y >> 2][x >> 1] & DOTS[y & 3][x & 1])
        return False

    def line(self, x0: float, y0: float, x1: float, y1: float) -> None:
        """Every dot from `(x0, y0)` to `(x1, y1)`."""
        n = max(1, round(max(abs(x1 - x0), abs(y1 - y0))))
        for i in range(n + 1):
            self.set(round(x0 + (x1 - x0) * i / n), round(y0 + (y1 - y0) * i / n))

    def glyph(self, cx: int, cy: int) -> str:
        """The cell `(cx, cy)`; `""` when no dot of it is on."""
        bits = self.bits[cy][cx] if 0 <= cx < self.w and 0 <= cy < self.h else 0
        return chr(0x2800 + bits) if bits else ""

    def rows(self) -> list[str]:
        return [
            "".join(chr(0x2800 + bits) if bits else " " for bits in row)
            for row in self.bits
        ]


class Half:
    """`w` by `2h` pixels in `w` by `h` cells, glyphs over them."""

    def __init__(self, w: int, h: int, colour: RGB = (0, 0, 0)) -> None:
        self.w, self.h = w, h
        self.px = [[colour] * w for _ in range(2 * h)]
        self.over: dict[tuple[int, int], tuple[str, RGB | None, RGB | None]] = {}

    def fill(self, colour: RGB) -> None:
        self.px = [[colour] * self.w for _ in range(2 * self.h)]

    def get(self, x: int, y: int) -> RGB | None:
        if 0 <= x < self.w and 0 <= y < 2 * self.h:
            return self.px[y][x]
        return None

    def set(self, x: int, y: int, colour: RGB, alpha: float = 1.0) -> None:
        """Paint a pixel; `alpha` under 1 mixes with what is there."""
        if 0 <= x < self.w and 0 <= y < 2 * self.h:
            row = self.px[y]
            row[x] = colour if alpha >= 1.0 else mix(row[x], colour, alpha)

    def rect(
        self, x: int, y: int, w: int, h: int, colour: RGB, alpha: float = 1.0
    ) -> None:
        for j in range(y, y + h):
            for i in range(x, x + w):
                self.set(i, j, colour, alpha)

    def disc(
        self, cx: float, cy: float, r: float, colour: RGB, alpha: float = 1.0
    ) -> None:
        """The pixels whose centre is within `r` of `(cx, cy)`, in
        pixels; the centre of the pixel `(x, y)` is `(x + .5, y + .5)`."""
        for y in range(int(cy - r - 1), int(cy + r + 2)):
            for x in range(int(cx - r - 1), int(cx + r + 2)):
                if (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r:
                    self.set(x, y, colour, alpha)

    def put(
        self,
        cx: int,
        cy: int,
        text: str,
        fg: RGB | None = None,
        bg: RGB | None = None,
    ) -> None:
        """Write `text` from the cell `(cx, cy)`, a cell a character."""
        for i, char in enumerate(text):
            if 0 <= cx + i < self.w and 0 <= cy < self.h:
                self.over[cx + i, cy] = (char, fg, bg)

    def ink(self, dots: Braille, fg: RGB | None = None) -> None:
        """Put every cell of `dots` that has a dot on."""
        for cy in range(min(self.h, dots.h)):
            for cx in range(min(self.w, dots.w)):
                glyph = dots.glyph(cx, cy)
                if glyph:
                    self.over[cx, cy] = (glyph, fg, None)

    def rows(self, deep: bool = False) -> Rows:
        """Rows of runs; cells alike are one run."""
        name: Any = hexa if deep else xterm
        out: Rows = []
        for cy in range(self.h):
            top, low = self.px[2 * cy], self.px[2 * cy + 1]
            row: list[Run] = []
            for cx in range(self.w):
                glyph = self.over.get((cx, cy))
                if glyph is not None:
                    text, fg, bg = glyph
                    run = {"t": text, "bg": name(bg or mix(top[cx], low[cx], 0.5))}
                    if fg is not None:
                        run["fg"] = name(fg)
                elif name(top[cx]) == name(low[cx]):
                    run = {"t": " ", "bg": name(low[cx])}
                else:
                    run = {"t": "▀", "fg": name(top[cx]), "bg": name(low[cx])}
                last = row[-1] if row else None
                if last and (last.get("fg"), last["bg"]) == (run.get("fg"), run["bg"]):
                    last["t"] += run["t"]
                elif last and run["t"] == " " and last["bg"] == run["bg"]:
                    last["t"] += " "  # a blank shows no foreground
                else:
                    row.append(run)
            out.append(row)
        return out


def _sgr(value: Any, base: int) -> str:
    if isinstance(value, int):
        return f"{base};5;{value}"
    return f"{base};2;" + ";".join(str(int(value[i : i + 2], 16)) for i in (1, 3, 5))


def ansi(rows: Sequence[Sequence[Run]]) -> list[str]:
    """`rows` with SGR escapes, a string a row."""
    out = []
    for row in rows:
        line = ""
        for run in row:
            codes = [_sgr(run[k], b) for k, b in (("fg", 38), ("bg", 48)) if k in run]
            line += f"\x1b[{';'.join(codes)}m{run['t']}\x1b[0m" if codes else run["t"]
        out.append(line)
    return out
