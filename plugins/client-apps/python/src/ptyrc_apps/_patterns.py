# SPDX-License-Identifier: GPL-3.0-or-later
"""Patterns of Conway's game of life, by kind.

    LIBRARY["glider"].art           # rows; `O`: a cell
    of(SHIP, GUN)                   # the patterns of these kinds

The table holds each pattern as the body of its RLE (`b` dead, `o`
alive, `$` end of row, a count before each), under the name the
LifeWiki (conwaylife.com) gives it. What it says of a pattern is
checked by running it (`tests/test_life.py`):

- `STILL`: a still life, `period` 1.
- `OSCILLATOR`: itself again after `period` generations.
- `SHIP`: itself again after `period` generations, `move` cells away,
  `(x, y)`.
- `SEED`: small and long-lived, a methuselah or the start of something
  else; some grow without end on a plane.
- `GUN`: sends gliders.
- `PUFFER`: moves and leaves debris, or ships (a rake).
- `FILLER`: grows in every direction until the world is full.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Final

__all__ = [
    "FILLER",
    "GUN",
    "KINDS",
    "LIBRARY",
    "OSCILLATOR",
    "PUFFER",
    "SEED",
    "SHIP",
    "STILL",
    "Pattern",
    "decode",
    "heading",
    "of",
    "turn",
]

STILL: Final = "still"
OSCILLATOR: Final = "oscillator"
SHIP: Final = "ship"
SEED: Final = "seed"
GUN: Final = "gun"
PUFFER: Final = "puffer"
FILLER: Final = "filler"

KINDS: Final = (STILL, OSCILLATOR, SHIP, SEED, GUN, PUFFER, FILLER)


@dataclass(frozen=True, slots=True)
class Pattern:
    """`art`: rows of equal width, `O` a cell, `.` none. `period`: 0
    when the pattern never comes back. `move`: cells a period."""

    name: str
    title: str
    kind: str
    art: tuple[str, ...]
    period: int = 0
    move: tuple[int, int] = (0, 0)

    @property
    def w(self) -> int:
        return len(self.art[0])

    @property
    def h(self) -> int:
        return len(self.art)

    @property
    def cells(self) -> int:
        return sum(row.count("O") for row in self.art)


def decode(rle: str) -> tuple[str, ...]:
    """The rows of the body of an RLE, padded to one width."""
    rows: list[str] = []
    row: list[str] = []
    n = ""
    for c in rle:
        if c.isdigit():
            n += c
            continue
        count = int(n or 1)
        n = ""
        if c == "b":
            row.append("." * count)
        elif c == "o":
            row.append("O" * count)
        elif c == "$":
            rows.append("".join(row))
            rows.extend([""] * (count - 1))
            row = []
        elif c == "!":
            break
        elif not c.isspace():
            raise ValueError(f"not RLE: {c!r}")
    rows.append("".join(row))
    w = max(len(line) for line in rows)
    return tuple(line.ljust(w, ".") for line in rows)


def turn(art: tuple[str, ...] | list[str], k: int) -> list[str]:
    """`art` in the orientation `k` of its 8: bit 0 transposes, then
    bit 1 mirrors `x`, bit 2 mirrors `y`."""
    out = list(art)
    if k & 1:
        out = ["".join(col) for col in zip(*out, strict=True)]
    if k & 2:
        out = [line[::-1] for line in out]
    if k & 4:
        out.reverse()
    return out


def heading(move: tuple[int, int], k: int) -> tuple[int, int]:
    """`move` of a pattern, once turned by `turn(art, k)`."""
    dx, dy = move
    if k & 1:
        dx, dy = dy, dx
    return (-dx if k & 2 else dx, -dy if k & 4 else dy)


def _p(
    kind: str,
    name: str,
    title: str,
    rle: str,
    period: int = 0,
    move: tuple[int, int] = (0, 0),
) -> Pattern:
    if kind == STILL:
        period = 1
    return Pattern(name, title, kind, decode(rle), period, move)


# fmt: off
_TABLE: Final = (
    # still: 32
    _p(STILL, "block", "Block", "2o$2o"),
    _p(STILL, "snake", "Snake", "2obo$ob2o"),
    _p(STILL, "boat", "Boat", "2ob$obo$bo"),
    _p(STILL, "ship", "Ship", "2ob$obo$b2o"),
    _p(STILL, "tub", "Tub", "bob$obo$bo"),
    _p(STILL, "bi-block", "Bi-block", "2ob2o$2ob2o"),
    _p(STILL, "aircraft-carrier", "Aircraft carrier", "2o2b$o2bo$2b2o"),
    _p(STILL, "beehive", "Beehive", "b2ob$o2bo$b2o"),
    _p(STILL, "shillelagh", "Shillelagh", "2o3b$o2b2o$b2obo"),
    _p(STILL, "barge", "Barge", "bo2b$obob$bobo$2bo"),
    _p(STILL, "eater-1", "Eater 1", "2o2b$obob$2bob$2b2o"),
    _p(STILL, "loaf", "Loaf", "b2ob$o2bo$bobo$2bo"),
    _p(STILL, "long-boat", "Long boat", "bo2b$obob$bobo$2b2o"),
    _p(STILL, "long-ship", "Long ship", "2o2b$obob$bobo$2b2o"),
    _p(STILL, "pond", "Pond", "b2ob$o2bo$o2bo$b2o"),
    _p(STILL, "block-on-table", "Block on table", "2b2o$2b2o2$4o$o2bo"),
    _p(STILL, "hat", "Hat", "2bo2b$bobob$bobob$2ob2o"),
    _p(STILL, "mango", "Mango", "b2o2b$o2bob$bo2bo$2b2o"),
    _p(STILL, "canoe", "Canoe", "3b2o$4bo$3bob$obo2b$2o"),
    _p(STILL, "cis-boat-with-tail", "Cis-boat with tail", "3b2o$3bob$2obob$obo2b$bo"),
    _p(STILL, "integral-sign", "Integral sign", "3b2o$2bobo$2bo2b$obo2b$2o"),
    _p(STILL, "long-barge", "Long barge", "bo3b$obo2b$bobob$2bobo$3bo"),
    _p(STILL, "bookends", "Bookends", "2o3b2o$obobobo$2bobo2b$b2ob2o"),
    _p(STILL, "beehive-with-tail", "Beehive with tail",
        "b2o3b$o2bo2b$b2obob$4bob$4b2o",
    ),
    _p(STILL, "honeycomb", "Honeycomb", "2b2o2b$bo2bob$ob2obo$bo2bob$2b2o"),
    _p(STILL, "dead-spark-coil", "Dead spark coil",
        "2o3b2o$obobobo$2bobo2b$obobobo$2o3b2o",
    ),
    _p(STILL, "elevener", "Elevener", "4b2o$3bobo$3bo2b$b3o2b$o5b$2o"),
    _p(STILL, "scorpion", "Scorpion", "3bo3b$b3o3b$o3b2ob$obobobo$b2obobo$5bo"),
    _p(STILL, "eater-2", "Eater 2", "3bob2o$b3ob2o$o6b$b3ob2o$3bobob$3bobob$4bo"),
    _p(STILL, "bakery", "Bakery",
        "4b2o4b$3bo2bo3b$3bobo4b$b2obo3bob$o2bo3bobo$obo3bo2bo$bo3bob2ob$4bobo3b$3bo2"
        "bo3b$4b2o",
    ),
    _p(STILL, "cthulhu", "Cthulhu",
        "bo3bo3bo$obobobobobo$obobobobobo$b2obobob2o$4bobo$4bobo$3b2ob2o$2bo4bo$2b2ob"
        "2o$3bobo$2bo2bo$2bobo$b2ob2o",
    ),
    _p(STILL, "honey-farm", "Honey farm",
        "6bo6b$5bobo5b$5bobo5b$6bo6b2$b2o7b2ob$o2bo5bo2bo$b2o7b2ob2$6bo6b$5bobo5b$5bo"
        "bo5b$6bo",
    ),
    # oscillator: 109
    _p(OSCILLATOR, "blinker", "Blinker", "3o", 2),
    _p(OSCILLATOR, "toad", "Toad", "b3o$3o", 2),
    _p(OSCILLATOR, "beacon", "Beacon", "2o2b$o3b$3bo$2b2o", 2),
    _p(OSCILLATOR, "clock", "Clock", "2bob$obob$bobo$bo", 2),
    _p(OSCILLATOR, "bipole", "Bipole", "2o3b$obo2b2$2bobo$3b2o", 2),
    _p(OSCILLATOR, "pentadecathlon", "Pentadecathlon", "2bo4bo2b$2ob4ob2o$2bo4bo", 15),
    _p(OSCILLATOR, "monogram", "Monogram", "2o3b2o$bobobob$b2ob2ob$bobobob$2o3b2o", 4),
    _p(OSCILLATOR, "figure-eight", "Figure eight",
        "2o4b$2obo2b$4bob$bo4b$2bob2o$4b2o",
        8,
    ),
    _p(OSCILLATOR, "mold", "Mold", "3b2ob$2bo2bo$o2bobo$4bob$ob2o2b$bo", 4),
    _p(OSCILLATOR, "quad", "Quad", "2o2b2o$o2bobo$bo4b$4bob$obo2bo$2o2b2o", 2),
    _p(OSCILLATOR, "tripole", "Tripole", "2o4b$obo3b2$2bobob$5bo$4b2o", 2),
    _p(OSCILLATOR, "spark-coil", "Spark coil",
        "2o4b2o$obo2bobo$2bo2bo2b$obo2bobo$2o4b2o",
        2,
    ),
    _p(OSCILLATOR, "test-tube-baby", "Test tube baby",
        "2o4b2o$obo2bobo$2bo2bo2b$2bo2bo2b$3b2o",
        2,
    ),
    _p(OSCILLATOR, "jam", "Jam", "3b2ob$2bo2bo$o2bobo$o3bob$o5b$3bo2b$b2o", 3),
    _p(OSCILLATOR, "tumbler", "Tumbler",
        "bo5bob$obo3bobo$o2bobo2bo$2bo3bo2b$2b2ob2o",
        14,
    ),
    _p(OSCILLATOR, "caterer", "Caterer", "2bo5b$o3b4o$o3bo3b$o7b$3bo4b$b2o", 3),
    _p(OSCILLATOR, "1-beacon", "1 beacon",
        "2b2o3b$bobo3b$o2bob2o$2obo2bo$bobo3b$bo2bo2b$2b2o",
        2,
    ),
    _p(OSCILLATOR, "fox", "Fox", "4bo2b$4bo2b$2bo2bob$2o5b$4bobo$2bobobo$6bo", 2),
    _p(OSCILLATOR, "mazing", "Mazing", "3b2o2b$bobo3b$o5bo$bo3b2o2$3bobob$4bo", 4),
    _p(OSCILLATOR, "skewed-quad", "Skewed quad",
        "b2o4b$bo3b2o$2bobobo2$obobo2b$2o3bob$4b2o",
        2,
    ),
    _p(OSCILLATOR, "smiley", "Smiley", "3ob3o$bobobob2$bo3bob2$obobobo$2bobo", 8),
    _p(OSCILLATOR, "traffic-light", "Traffic light", "2b3o2b2$o5bo$o5bo$o5bo2$2b3o", 2),
    _p(OSCILLATOR, "why-not", "Why not", "3bo3b$3bobob$bo5b$ob5o$bo5b$3bobob$3bo", 2),
    _p(OSCILLATOR, "blocker", "Blocker", "6bobob$5bo4b$2o2bo4bo$2obo2bob2o$4b2o", 8),
    _p(OSCILLATOR, "candlefrobra", "Candlefrobra",
        "5bo4b$bob2obob2o$obo3bob2o$bo4bo3b$5b2o",
        3,
    ),
    _p(OSCILLATOR, "fumarole", "Fumarole",
        "3b2o3b$bo4bob$bo4bob$bo4bob$2bo2bo2b$obo2bobo$2o4b2o",
        5,
    ),
    _p(OSCILLATOR, "light-bulb", "Light bulb",
        "b2obo2b$bob2o2b2$2b3o2b$bo3bob$bo3bob$2bobo2b$obobobo$2o3b2o",
        2,
    ),
    _p(OSCILLATOR, "4-boats", "4 boats",
        "3bo4b$2bobo3b$bob2o3b$obo2b2ob$b2o2bobo$3b2obob$3bobo2b$4bo",
        2,
    ),
    _p(OSCILLATOR, "cross", "Cross",
        "2b4o2b$2bo2bo2b$3o2b3o$o6bo$o6bo$3o2b3o$2bo2bo2b$2b4o",
        3,
    ),
    _p(OSCILLATOR, "great-on-off", "Great on-off",
        "2b2o4b$bo2bo3b$bobo4b$2obo2bob$4b2obo$7bo$4b3ob$4bo",
        2,
    ),
    _p(OSCILLATOR, "octagon-2", "Octagon 2",
        "3b2o3b$2bo2bo2b$bo4bob$o6bo$o6bo$bo4bob$2bo2bo2b$3b2o",
        5,
    ),
    _p(OSCILLATOR, "pentapole", "Pentapole", "2o6b$obo5b2$2bobo3b2$4bobob$7bo$6b2o", 2),
    _p(OSCILLATOR, "phoenix-1", "Phoenix 1",
        "3bo4b$3bobo2b$bo6b$6b2o$2o6b$6bob$2bobo3b$4bo",
        2,
    ),
    _p(OSCILLATOR, "pulsar-quadrant", "Pulsar quadrant",
        "5bo2b$3b3o2b$2bo3b2o$o2bo2bob$o3bobob$o4bo2b2$2b3o",
        3,
    ),
    _p(OSCILLATOR, "unix", "Unix", "b2o5b$b2o5b2$bo6b$obo5b$o2bo2b2o$4bob2o$2b2o", 6),
    _p(OSCILLATOR, "coes-p8", "Coe's p8",
        "2o10b$2o2b2o6b$5b2o5b$4bo2bo4b$7bo2b2o$5bobo2b2o",
        8,
    ),
    _p(OSCILLATOR, "elkies-p5", "Elkies' p5",
        "bo7b$o2b3o3b$2bo6b$3bobo2bo$2b2ob4o$4bo4b$4bobo2b$5b2o",
        5,
    ),
    _p(OSCILLATOR, "two-eaters", "Two eaters",
        "2o7b$bo7b$bobo5b$2b2o5b$5b2o2b$5bobob$7bob$7b2o",
        3,
    ),
    _p(OSCILLATOR, "koks-galaxy", "Kok's galaxy",
        "2bo2bobob$2obob3ob$bo6bo$2o5bob2$bo5b2o$o6bob$b3obob2o$bobo2bo",
        8,
    ),
    _p(OSCILLATOR, "two-pulsar-quadrants", "Two pulsar quadrants",
        "4bo4b$4bo4b$3b2o4b$2bo6b$o2bo2b3o$o3bobo2b$o4bo3b2$2b3o",
        3,
    ),
    _p(OSCILLATOR, "laputa", "Laputa",
        "3b2ob2o4b$3b2obo3b2o$8bo2bo$b6ob3ob$o2bobo6b$2o3bob2o3b$4b2ob2o",
        2,
    ),
    _p(OSCILLATOR, "1-2-3", "1-2-3",
        "2b2o6b$o2bo6b$2obob2o3b$bobo2bo3b$bo4bob2o$2b3obob2o$5bo4b$4bo5b$4b2o",
        3,
    ),
    _p(OSCILLATOR, "surprise", "Surprise",
        "3bo4b2o$3b3o2bob$b2o3bobob$o2b2obob2o$bo6bob$2obob2o2bo$bobo3b2ob$bo2b3o3b$2"
        "o4bo",
        3,
    ),
    _p(OSCILLATOR, "blonker", "Blonker",
        "o2b2o4bo$2o2bob2obo$4bobo$5b2o$7bo$7bo3bo$9bobo$10bo",
        6,
    ),
    _p(OSCILLATOR, "candelabra", "Candelabra",
        "4b2o4b2o4b$bo2bo6bo2bob$obobo6bobobo$bo2bob4obo2bob$4bobo2bobo4b$5bo4bo",
        3,
    ),
    _p(OSCILLATOR, "achims-p4", "Achim's p4",
        "2b2o3b2o2b$bo2bobo2bob$bob2ob2obob$2o7b2o$2bobobobo2b$2o7b2o$bob2ob2obob$bo2"
        "bobo2bob$2b2o3b2o",
        4,
    ),
    _p(OSCILLATOR, "mathematician", "Mathematician",
        "4bo4b$3bobo3b$3bobo3b$2b2ob2o2b$o7bo$3o3b3o2$9o$o7bo$3b4o2b$3bo2b2o",
        5,
    ),
    _p(OSCILLATOR, "burloaferimeter", "Burloaferimeter",
        "4b2o4b$5bo4b$4bo5b$3bob3o2b$3bobo2bob$2obo3bobo$2obo4bob$4b4o2b2$4b2o4b$4b2o",
        7,
    ),
    _p(OSCILLATOR, "karels-p15", "Karel's p15",
        "2bo4bo2b$2b6o2b$2bo4bo2b4$2b6o2b$bo6bob$o8bo$bo6bob$2b6o",
        15,
    ),
    _p(OSCILLATOR, "t-nosed-p4", "T-nosed p4",
        "4b3o4b$5bo5b2$4b3o4b$3bo3bo3b$2bo5bo2b$2bobobobo2b$b2obobob2ob$o2b2ob2o2bo$2"
        "o7b2o",
        4,
    ),
    _p(OSCILLATOR, "revolver", "Revolver",
        "o12bo$3o4bo3b3o$3bobobo2bo3b$2bo6bobo2b$2bobo6bo2b$3bo2bobobo3b$3o3bo4b3o$o1"
        "2bo",
        2,
    ),
    _p(OSCILLATOR, "pentant", "Pentant",
        "2o8b$bo8b$bobo6b$2b2o4b2o$9bo$5b4ob$5bo4b$2bo3b3ob$2b4o2bob$5bo4b$4bo5b$4b2o",
        5,
    ),
    _p(OSCILLATOR, "confused-eaters", "Confused eaters",
        "o10b$3o8b$3bo7b$2bo8b$2bo2bo5b$5bo5b$3bobo5b$3b2o2b2o2b$7bobob$9bob$9b2o",
        4,
    ),
    _p(OSCILLATOR, "heart", "Heart",
        "5bo5b$4bo2bo3b$bo2bo2bo3b$obobobob2ob$bo2bo2bo3b$4bo5bo$5b5ob2$7bo3b$6bobo2b"
        "$7bo",
        5,
    ),
    _p(OSCILLATOR, "star", "Star",
        "4b3o4b2$2bobobobo2b2$obo5bobo$o9bo$obo5bobo2$2bobobobo2b2$4b3o",
        3,
    ),
    _p(OSCILLATOR, "hertz-oscillator", "Hertz oscillator",
        "3b2obo4b$3bob2o4b2$4b3o4b$3bobobob2o$3bo3bob2o$2obo3bo3b$2obo3bo3b$4b3o4b2$4"
        "b2obo3b$4bob2o",
        8,
    ),
    _p(OSCILLATOR, "hustler", "Hustler",
        "5b2o4b$5b2o4b2$3b4o4b$obo4bo3b$2obo3bo3b$3bo3bob2o$3bo4bobo$4b4o3b2$4b2o5b$4"
        "b2o",
        3,
    ),
    _p(OSCILLATOR, "negentropy", "Negentropy",
        "3b2obo4b$3bob2o4b2$4b3o4b$3bobobob2o$3b2o2bob2o$2obo3bo3b$2obo3bo3b$4b3o4b2$"
        "4b2obo3b$4bob2o",
        2,
    ),
    _p(OSCILLATOR, "rats", "$rats",
        "5b2o5b$6bo5b$4bo7b$2obob4o3b$2obo5bobo$3bo2b3ob2o$3bo4bo3b$4b3obo3b$7bo4b$6b"
        "o5b$6b2o",
        6,
    ),
    _p(OSCILLATOR, "toaster", "Toaster",
        "2bo6b2o$bobob2o2bob$bobobobobob$2obo3bob2o$o2b2ob2o2bo$bo7bob$bo7bob$o2b2ob2"
        "o2bo$2obo3bob2o$bobobobobob$bobob2o2bob$2bo6b2o",
        5,
    ),
    _p(OSCILLATOR, "38p7.2", "38P7.2",
        "4bo3bo4b$o2bobobobo2bo$obo2bobo2bobo$bo2b2ob2o2bob$5bobo5b$2b2o5b2o2b$2bo7bo"
        "2b$4bo3bo4b2$bo2bo3bo2bob$2b2o5b2o",
        7,
    ),
    _p(OSCILLATOR, "cauldron", "Cauldron",
        "5bo5b$4bobo4b$5bo5b2$3b5o3b$obo5bobo$2obo3bob2o$3bo3bo3b$3bo3bo3b$4b3o4b2$4b"
        "2obo3b$4bob2o",
        8,
    ),
    _p(OSCILLATOR, "clock-2", "Clock 2",
        "6b2o4b$6b2o4b2$4b4o4b$2obo4bo3b$2obo2bobo3b$3bo2bobob2o$3bobo2bob2o$4b4o4b2$"
        "4b2o6b$4b2o",
        4,
    ),
    _p(OSCILLATOR, "pinwheel", "Pinwheel",
        "6b2o4b$6b2o4b2$4b4o4b$2obo4bo3b$2obo2bobo3b$3bo3b2ob2o$3bobo2bob2o$4b4o4b2$4"
        "b2o6b$4b2o",
        4,
    ),
    _p(OSCILLATOR, "pseudo-barberpole", "Pseudo-barberpole",
        "10b2o$11bo$9bo2b$7bobo2b2$5bobo4b2$3bobo6b2$2b2o8b$o11b$2o",
        5,
    ),
    _p(OSCILLATOR, "queen-bee-shuttle", "Queen bee shuttle",
        "9bo12b$7bobo12b$6bobo13b$2o3bo2bo11b2o$2o4bobo11b2o$7bobo12b$9bo",
        30,
    ),
    _p(OSCILLATOR, "pentoad", "Pentoad",
        "11b2o$11bob$9bobob$9b2o2b$5b2o6b$6bo6b$6bo6b$6b2o5b$2b2o9b$bobo9b$bo11b$2o",
        5,
    ),
    _p(OSCILLATOR, "t-nosed-p6", "T-nosed p6",
        "4b2o3b2o4b$4bobobobo4b$5bo3bo5b2$o3b2o3b2o3bo$obob2o3b2obobo$o3b2o3b2o3bo2$5"
        "bo3bo5b$4bobobobo4b$4b2o3b2o",
        6,
    ),
    _p(OSCILLATOR, "achims-p16", "Achim's p16",
        "7b2o4b$7bobo3b$2bo4bob2o2b$b2o5bo4b$o2bo9b$3o10b2$10b3o$9bo2bo$4bo5b2ob$2b2o"
        "bo4bo2b$3bobo7b$4b2o",
        16,
    ),
    _p(OSCILLATOR, "diamond-ring", "Diamond ring",
        "6bo6b$5bobo5b$4bobobo4b$4bo3bo4b$2b2o2bo2b2o2b$bo4bo4bob$obob2ob2obobo$bo4bo"
        "4bob$2b2o2bo2b2o2b$4bo3bo4b$4bobobo4b$5bobo5b$6bo",
        3,
    ),
    _p(OSCILLATOR, "dinner-table", "Dinner table",
        "bo11b$b3o7b2o$4bo6bob$3b2o4bobob$9b2o2b$6bo6b$4b2obo5b2$2bo3bo2bo3b$bob2o4bo"
        "3b$bo6bo4b$2o7b3ob$11bo",
        12,
    ),
    _p(OSCILLATOR, "pulsar", "Pulsar",
        "2b3o3b3o2b2$o4bobo4bo$o4bobo4bo$o4bobo4bo$2b3o3b3o2b2$2b3o3b3o2b$o4bobo4bo$o"
        "4bobo4bo$o4bobo4bo2$2b3o3b3o",
        3,
    ),
    _p(OSCILLATOR, "worker-bee", "Worker bee",
        "2o12b2o$bo12bob$bobo8bobob$2b2o8b2o2b2$5b6o5b2$2b2o8b2o2b$bobo8bobob$bo12bob"
        "$2o12b2o",
        9,
    ),
    _p(OSCILLATOR, "chemist", "Chemist",
        "7bo7b$7b3o5b$10bo4b$5b3o2bo2b2o$4bobobobobobo$4bo3bobobo2b$b2obo5bob2ob$2bob"
        "obo3bo4b$obobobobobo4b$2o2bo2b3o5b$4bo10b$5b3o7b$7bo",
        5,
    ),
    _p(OSCILLATOR, "rattlesnake", "Rattlesnake",
        "8b2o$8bo$9bo$8b2o4$5bo$5b2o$2obobob3o$ob2obobobo$6bo2b3o$7b2o3bo$9b3o$9bo",
        11,
    ),
    _p(OSCILLATOR, "roteightor", "Roteightor",
        "bo12b$b3o8b2o$4bo7bob$3b2o5bobob$10b2o2b2$6b2o6b$5b2obo5b$6b3o5b$2b2o3b3o4b$"
        "bobo5b2o3b$bo7bo4b$2o8b3ob$12bo",
        8,
    ),
    _p(OSCILLATOR, "unicycle", "Unicycle",
        "2b2o3b4o3b$2b2o2bo4bo2b$2obo2bo3bobob$3o4bo3bobo$13bo$13bo$b2o7bo2bo$o2bo7b2"
        "ob$o13b$o13b$obo3bo4b3o$bobo3bo2bob2o$2bo4bo2b2o2b$3b4o3b2o",
        6,
    ),
    _p(OSCILLATOR, "eureka", "Eureka",
        "bo14bob$obo4bo7bobo$bo3b2ob2o6bob$7bo10b4$7bo10b$bo3b2ob2o6bob$obo4bo7bobo$b"
        "o14bo",
        30,
    ),
    _p(OSCILLATOR, "sombreros", "Sombreros",
        "3b2o8b2o3b$3bobo6bobo3b$5bo6bo5b$3bob2o4b2obo3b$b3o10b3ob$o3bobo4bobo3bo$b3o"
        "10b3ob$3bob2o4b2obo3b$5bo6bo5b$3bobo6bobo3b$3b2o8b2o",
        6,
    ),
    _p(OSCILLATOR, "airforce", "Airforce",
        "7bo6b$6bobo5b$7bo6b2$5b5o4b$4bo5bob2o$3bob2o3bob2o$3bobo2bobo3b$2obo3b2obo3b"
        "$2obo5bo4b$4b5o5b2$6bo7b$5bobo6b$6bo",
        7,
    ),
    _p(OSCILLATOR, "statorless-p5", "Statorless p5",
        "o13bo$b2o9b2o$o4b2ob2o4bo$2obo7bob2o$bobo2bobo2bobo$2bobo5bobo$2b3o5b3o$2b3o"
        "5b3o$2bobo5bobo$bobo2bobo2bobo$2obo7bob2o$o4b2ob2o4bo$b2o9b2o$o13bo",
        5,
    ),
    _p(OSCILLATOR, "ellison-p4-hw-emulator", "Ellison p4 HW emulator",
        "11b2o11b$4bo3bo6bo3bo4b$3bobo12bobo3b$3bobo12bobo3b$2obobob10obobob2o$2obo16"
        "bob2o$3bo16bo3b$3bobo4b2obo4bobo3b$4b2o4bob2o4b2o",
        4,
    ),
    _p(OSCILLATOR, "snacker", "Snacker",
        "2o16b2o$bo16bob$bobo12bobob$2b2o12b2o2b$7bo4bo7b$5b2ob4ob2o5b$7bo4bo7b$2b2o1"
        "2b2o2b$bobo12bobob$bo16bob$2o16b2o",
        9,
    ),
    _p(OSCILLATOR, "jolson", "Jolson",
        "b2o6b2o2b$o2bo4bo2bob$o2bo4bo2bob$o2bo4bo2bob$b2o6b2o2b3$7bo5b$5bo2bob2ob$6b"
        "2o2b2ob3$6b4o3b$5b6o2b$4b8ob$3b2o6b2o$4b8ob$5b6o2b$6b4o",
        15,
    ),
    _p(OSCILLATOR, "bakers-dozen", "Baker's dozen",
        "2o9b2o10b$4obo5b2o10b$obo2b3o15b$11bo11b$4b2o4bobo10b$4bo5bo2bo4bo4b$11b2o4b"
        "2o4b2$15b3o2bobo$10b2o5bob4o$10b2o9b2o",
        12,
    ),
    _p(OSCILLATOR, "44p7.2", "44P7.2",
        "11bo4b$10bobo3b$10bobo3b$9b2obob2o$12b2obo$8bob2o4b$11bo4b$8b3o5b$5bobo8b$3b"
        "o3bob2o5b$b3obobob2o5b$o4b2o9b$b4o11b$4bo11b$3bo12b$3b2o",
        7,
    ),
    _p(OSCILLATOR, "octagon-4", "Octagon 4",
        "7b2o$7b2o2$6b4o$5bo4bo$4bo6bo$3bo8bo$2obo8bob2o$2obo8bob2o$3bo8bo$4bo6bo$5bo"
        "4bo$6b4o2$7b2o$7b2o",
        4,
    ),
    _p(OSCILLATOR, "carnival-shuttle", "Carnival shuttle",
        "33bo3bo$2o3b2o26b5o$bobobo3bo2bo6b2o3bo2bo7bo2b$b2ob2o2b2o3b2o4b2o2b2o3b2o4b"
        "obob$bobobo3bo2bo6b2o3bo2bo7bo2b$2o3b2o26b5o$33bo3bo",
        12,
    ),
    _p(OSCILLATOR, "harbor", "Harbor",
        "5b2o3b2o5b$5bobobobo5b$6bo3bo6b2$5b2o3b2o5b$2o2bobo3bobo2b2o$obob2o5b2obobo$"
        "bo13bob2$bo13bob$obob2o5b2obobo$2o2bobo3bobo2b2o$5b2o3b2o5b2$6bo3bo6b$5bobob"
        "obo5b$5b2o3b2o",
        5,
    ),
    _p(OSCILLATOR, "turning-toads", "Turning toads",
        "15bo6bo14b$14b2o5b2o6b2o6b$6b3obobob2obobob2obobo10b$2b2obo6bobo4bobo4bobo2b"
        "ob2o2b$o2bobo3bo18b4obo2bo$2obobo27bob2o$3bo29bo3b$3b2o27b2o",
        4,
    ),
    _p(OSCILLATOR, "twin-bees-shuttle", "Twin bees shuttle",
        "17b2o10b$2o15bobo7b2o$2o17bo7b2o$17b3o9b4$17b3o9b$2o17bo9b$2o15bobo9b$17b2o",
        46,
    ),
    _p(OSCILLATOR, "crown", "Crown",
        "16b2o5b$3b2o13bo4b$2bo12bo7b$5bo3b3o3bo3bo3b$bo3bo4bo4bo3b4o$obo2bo11bo5b$o2"
        "bo19b$b2o20b$6b2o8b2o5b$6bo2b6o2bo5b$7b2o6b2o6b$4b3o10b3o3b$4bo2bobo4bobo2bo"
        "3b$5b2o4b2o4b2o",
        12,
    ),
    _p(OSCILLATOR, "ship-in-a-bottle", "Ship in a bottle",
        "4b2o6b2o4b$3bo2bo4bo2bo3b$3bobo6bobo3b$b2o2b3o2b3o2b2ob$o6bo2bo6bo$ob2o10b2o"
        "bo$bobo10bobob$3b2o3b2o3b2o3b$7bobo8b$7b2o9b$3b2o8b2o3b$bobo10bobob$ob2o10b2"
        "obo$o6bo2bo6bo$b2o2b3o2b3o2b2ob$3bobo6bobo3b$3bo2bo4bo2bo3b$4b2o6b2o",
        16,
    ),
    _p(OSCILLATOR, "windmill", "Windmill",
        "11bo6b$9b2obo5b$7b2o9b$10b2o6b$7b3o8b2$3o15b$3b2o2b3ob2o5b$10b7ob$b7o10b$5b2"
        "ob3o2b2o3b$15b3o2$8b3o7b$6b2o10b$9b2o7b$5bob2o9b$6bo",
        4,
    ),
    _p(OSCILLATOR, "swine", "Swine",
        "33bo$9bo2b2o7bo10bobo$o2b2o2bobobo5bo3bo7bo2b2o$o10bo2bobo3bo4bo2bobob2ob2o$"
        "o3bo4bo4b2obobo2bob2obo4b2o$3b2o4bob2obo2bobob2o4bo4bo3bo$2ob2obobo2bo4bo3bo"
        "bo2bo10bo$3b2o2bo7bo3bo5bobobo2b2o2bo$2bobo10bo7b2o2bo$3bo",
        5,
    ),
    _p(OSCILLATOR, "gourmet", "Gourmet",
        "10b2o8b$10bo9b$4b2ob2obo4b2o3b$2bo2bobobo5bo4b$2b2o4bo8bo2b$16b2o2b2$16b2o2b"
        "$o10bo3bobo2b$3o7b3o3bo3b$3bo6bobo4b3o$2bobo14bo$2b2o16b2$2b2o16b$2bo8bo4b2o"
        "2b$4bo5bobobo2bo2b$3b2o4bob2ob2o4b$9bo10b$8b2o",
        32,
    ),
    _p(OSCILLATOR, "hebdarole", "Hebdarole",
        "4b2o3bo4bo3b2o4b$bo2bo2bo2bo2bo2bo2bo2bob$obobob2obo4bob2obobobo$bo2bo2bobob"
        "2obobo2bo2bob$4b2o3b2o2b2o3b2o4b$8bo6bo8b$7bo8bo7b$6bo10bo6b$7b2o6b2o7b2$3b2"
        "o2b2o6b2o2b2o3b$3bo2bo10bo2bo3b$4bobo3bo2bo3bobo4b$3b2obobob4obobob2o3b$7b2o"
        "6b2o7b$9b2o2b2o9b$9bo2bobo9b$10b2o",
        7,
    ),
    _p(OSCILLATOR, "gabriels-p138", "Gabriel's p138",
        "8b3o10b$7bo2bo10b$7bo2bo10b$7b2o12b4$17b3ob$17bo2bo$20bo$3o15b3o$o20b$o2bo17"
        "b$b3o17b4$12b2o7b$10bo2bo7b$10bo2bo7b$10b3o",
        138,
    ),
    _p(OSCILLATOR, "sixty-nine", "Sixty-nine",
        "9bo11b$8bobo10b2$6bo3b2o9b$5bo5bo9b$6bobo12b$8b2o6bo4b$16bo4b$2bo5b2o4b3o4b$"
        "2bo11b2o5b$3o7b2o2b2o2b3o$2o6bob2o4b3o2b$2o2b3obobo5b3o2b$2b3o16b$2b3o6bo9b$"
        "10bobo8b2$8bo3b2o7b$7bo5bo7b$8bobo10b$10b2o",
        4,
    ),
    _p(OSCILLATOR, "p54-shuttle", "p54 shuttle",
        "2o25b2o$bo25bob$bobo7bo13bobob$2b2o5bo2bo5bo6b2o2b$12bo5b2o9b$8bo10b2o8b$8bo"
        "3b2o4b2o9b$9b5o15b2$9b5o15b$8bo3b2o4b2o9b$8bo10b2o8b$12bo5b2o9b$2b2o5bo2bo5b"
        "o6b2o2b$bobo7bo13bobob$bo25bob$2o25b2o",
        54,
    ),
    _p(OSCILLATOR, "achims-p144", "Achim's p144",
        "2o24b2o$2o24b2o$18b2o8b$17bo2bo7b$18b2o8b$14bo13b$13bobo12b$12bo3bo11b$12bo2"
        "bo12b2$12bo2bo12b$11bo3bo12b$12bobo13b$13bo14b$8b2o18b$7bo2bo17b$8b2o18b$2o2"
        "4b2o$2o24b2o",
        144,
    ),
    _p(OSCILLATOR, "jasons-p11", "Jason's p11",
        "8bob2ob2obo$8b2obobob2o2$8b3o3b3o$7bo3bobo3bo$7bobobobobobo$11bobo$4b2o2b3o3"
        "b3o2b2o$2obo3bo9bo3bob2o$bobobobo9bobobobo$o2bo3bo9bo3bo2bo$2o2b3o11b3o2b2o2"
        "$2o2b3o11b3o2b2o$o2bo3bo9bo3bo2bo$bobobobo9bobobobo$2obo3bo9bo3bob2o$4b2o2b3"
        "o3b3o2b2o$11bobo$7bobobobobobo$7bo3bobo3bo$8b3o3b3o2$8b2obobob2o$8bob2ob2obo",
        11,
    ),
    _p(OSCILLATOR, "pre-pulsar-shuttle-29", "Pre-pulsar shuttle 29",
        "15bo12b$13b3o12b$12bo15b$12b2o14b2$bo26b$obo4bo20b$bo5b2o6b2o11b$7bo7b2o11b3"
        "$19b2o7b$7bo11b2o5b2o$bo5b2o17bob$obo4bo16bobob$bo22b2o2b4$14bo5bo7b$13b3o3b"
        "3o6b5$13bo7bo6b$12bobo5bobo5b$13bo7bo",
        29,
    ),
    _p(OSCILLATOR, "centinal", "Centinal",
        "2o48b2o$bo48bob$bobo21b2o21bobob$2b2o8bo12b2o12b2o7b2o2b$11b2o26bobo10b$10b2"
        "o29bo10b$11b2o2b2o22b3o10b4$11b2o2b2o22b3o10b$10b2o29bo10b$11b2o26bobo10b$2b"
        "2o8bo12b2o12b2o7b2o2b$bobo21b2o21bobob$bo48bob$2o48b2o",
        100,
    ),
    _p(OSCILLATOR, "ring-of-fire", "Ring of fire",
        "16bo17b$14bobobo15b$12bobobobobo13b$10bobobobobobobo11b$8bobobo2b2obobobobo9"
        "b$6bobobobo6bo2bobobo7b$4bobobo2bo10bobobobo5b$5b2obo14bo2bobobo3b$3bo3bo18b"
        "ob2o4b$4b3o20bo3bo2b$2bo25b3o3b$3b2o27bob$bo3bo24b2o2b$2b4o23bo3bo$o29b3ob$b"
        "3o29bo$o3bo23b4o2b$2b2o24bo3bob$bo27b2o3b$3b3o25bo2b$2bo3bo20b3o4b$4b2obo18b"
        "o3bo3b$3bobobo2bo14bob2o5b$5bobobobo10bo2bobobo4b$7bobobo2bo6bobobobo6b$9bob"
        "obobob2o2bobobo8b$11bobobobobobobo10b$13bobobobobo12b$15bobobo14b$17bo",
        2,
    ),
    _p(OSCILLATOR, "popover", "Popover",
        "21bo10b$20bobo9b2$13b2o3bo3b2o8b$13b2o2bo5bo8b$18bobo11b$20b2o10b$14b2o16b$3"
        "b2o8bo2bo15b$bobo9bobo4bo11b$o5bo7bo5bo11b$bo3b2o2b3o8bo11b2$3bobo16b2o3b2o3"
        "b$4bo12bo3bo2bo2b2o3b$8bo7b3o3bobo7b$7bobo6bobo4bo8b$3b2o2bo2bo16bo4b$3b2o3b"
        "2o16bobo3b2$11bo8b3o2b2o3bob$11bo13bo5bo$11bo5b2o9bobob$16bobo8b2o3b$16b3o13"
        "b$10b2o20b$11bobo18b$8bo5bo2b2o13b$8b2o3bo3b2o13b2$9bobo20b$10bo",
        32,
    ),
    _p(OSCILLATOR, "superfountain", "Superfountain",
        "15b2o6b2o6b2o2b2o9b$9b2o5bo6bo5bo2bo2bo2bob2o4b$4bo4bobob3o4b2obo5b2o5b3obob"
        "o3b$3bobo5bobo5bo2bo8b5o4bo2bo2b$4bo5bo2bo4bobo6b4o2bo3b2o2bobo2b$4bo7b4o2bo"
        "b2o4bo4b5o3bobo2b2o$4b3o3bo5bobo2bo4b2o2bo3bo2b2obobobob$8b2o2bob2obob2o4b2o"
        "2bo4bo6b2obob$4bo3bo6bobo3b2ob2o3b4o4b2obo2bo2b$3bo5bobo5bo3bobob5obo6bob3o3"
        "b$4bo3b3o5bo4bo10b3o2bo8b$4b3ob2o2b2o2bo2b4o2bobob2obobo2bob2o5b$o3bo9bo3b2o"
        "b2obo2bo6bo2bo2bo5b$4b3ob2o2b2o2bo2bo3b3o6b2o4b2o6b$4bo3b3o5bo3bo3bo21b$3bo5"
        "bobo5bo5bo3bo18b$4bo3bo6bobo2b3o3b2o18b$8b2o2bob2obobo3b2o21b$4b3o3bo5bo2bob"
        "o3b3o18b$4bo7b4o3bob4o2bo18b$4bo5bo2bo6bo4bo20b$3bobo5bobo8bo2b2o19b$4bo4bob"
        "ob3o5b2o23b$9b2o5bo29b$15b2o",
        4,
    ),
    _p(OSCILLATOR, "pre-pulsar-shuttle-26", "Pre-pulsar shuttle 26",
        "16bo3bo16b$10b2o4bo3bo4b2o10b$10bo5bo3bo5bo10b$7b2obo15bob2o7b$6bobob2o13b2o"
        "bobo6b$6bobo6bo5bo6bobo6b$4b2o2bo5b3o3b3o5bo2b2o4b$3bo4b2o17b2o4bo3b$3b5o21b"
        "5o3b$7bo21bo7b$b4o27b4ob$bo2bo27bo2bob2$15b2o3b2o15b$6bo9bo3bo9bo6b$5b2o6bo9"
        "bo6b2o5b$3o3bo6b2o7b2o6bo3b3o4$3o3bo6b2o7b2o6bo3b3o$5b2o6bo9bo6b2o5b$6bo9bo3"
        "bo9bo6b$15b2o3b2o15b2$bo2bo27bo2bob$b4o27b4ob$7bo21bo7b$3b5o21b5o3b$3bo4b2o1"
        "7b2o4bo3b$4b2o2bo5b3o3b3o5bo2b2o4b$6bobo6bo5bo6bobo6b$6bobob2o13b2obobo6b$7b"
        "2obo15bob2o7b$10bo5bo3bo5bo10b$10b2o4bo3bo4b2o10b$16bo3bo",
        26,
    ),
    _p(OSCILLATOR, "newshuttle", "Newshuttle",
        "19bo2b2obo3bo19b$17b3o2bob2o3b3o17b$7b2o7bo15bo7b2o7b$8bo7b2o5b3o5b2o7bo8b$8"
        "bobo11bo3bo11bobo8b$9b2o11b2ob2o11b2o9b2$2bo10b3o17b3o10bo2b$2b3o9bo19bo9b3o"
        "2b$5bo13b3o5b3o13bo5b$4b2o14bo7bo14b2o4b3$7bo33bo7b$7b2o31b2o7b$7bo33bo7b$2b"
        "2o18b2ob2o18b2o2b$bobo18bo3bo18bobob$bo21b3o21bob$2o7bo15bobo11bo7b2o$9b2o15"
        "b2o10b2o9b$9bo9b2o18bo9b$4b2o10b2o2bo10b2o10b2o2b2o$2obobo10bob2o10bobo10bob"
        "o2bo$bobo14bo11bo14bobob$o2bobo10bobo10b2obo10bobob2o$2o2b2o10b2o10bo2b2o10b"
        "2o4b$9bo18b2o9bo9b$9b2o10b2o15b2o9b$2o7bo11bobo15bo7b2o$bo21b3o21bob$bobo18b"
        "o3bo18bobob$2b2o18b2ob2o18b2o2b$7bo33bo7b$7b2o31b2o7b$7bo33bo7b3$4b2o14bo7bo"
        "14b2o4b$5bo13b3o5b3o13bo5b$2b3o9bo19bo9b3o2b$2bo10b3o17b3o10bo2b2$9b2o11b2ob"
        "2o11b2o9b$8bobo11bo3bo11bobo8b$8bo7b2o5b3o5b2o7bo8b$7b2o7bo15bo7b2o7b$17b3o3"
        "b2obo2b3o17b$19bo3bob2o2bo",
        28,
    ),
    # ship: 47
    _p(SHIP, "glider", "Glider", "bob$2bo$3o", 4, (1, 1)),
    _p(SHIP, "lwss", "Lightweight spaceship", "bo2bo$o4b$o3bo$4o", 4, (-2, 0)),
    _p(SHIP, "mwss", "Middleweight spaceship", "3bo2b$bo3bo$o5b$o4bo$5o", 4, (-2, 0)),
    _p(SHIP, "hwss", "Heavyweight spaceship", "3b2o2b$bo4bo$o6b$o5bo$6o", 4, (-2, 0)),
    _p(SHIP, "25p3h1v0.2", "25P3H1V0.2",
        "7b2obo5b$4b2obob2ob3ob$b4o2b2o6bo$o4bo3bo3b2ob$b2o",
        3, (0, -1),
    ),
    _p(SHIP, "sidecar", "Sidecar",
        "bo6b$o5bob$o5bob$5obob2$4b2o2b$2bo4bo$bo6b$bo5bo$b6o",
        4, (-2, 0),
    ),
    _p(SHIP, "loafer", "Loafer",
        "b2o2bob2o$o2bo2b2o$bobo$2bo$8bo$6b3o$5bo$6bo$7b2o",
        7, (-1, 0),
    ),
    _p(SHIP, "coe-ship", "Coe ship",
        "4b6o$2b2o5bo$2obo5bo$4bo3bob$6bo3b$6b2o2b$5b4ob$5b2ob2o$7b2o",
        16, (8, 0),
    ),
    _p(SHIP, "copperhead", "Copperhead",
        "b2o2b2o$3b2o$3b2o$obo2bobo$o6bo2$o6bo$b2o2b2o$2b4o2$3b2o$3b2o",
        10, (0, -1),
    ),
    _p(SHIP, "x66", "X66",
        "2bo6b$2o7b$o2b3o2bo$o4b3ob$b3o2b2ob2$b3o2b2ob$o4b3ob$o2b3o2bo$2o7b$2bo",
        4, (-2, 0),
    ),
    _p(SHIP, "edge-repair-spaceship-1", "Edge-repair spaceship 1",
        "8bo7b$7b4o5b$2bo3bo3b2ob2ob$b4o5bo2b2ob$o3bo7bo2bo$bobo2bo9b$5bo",
        3, (0, -1),
    ),
    _p(SHIP, "turtle", "Turtle",
        "b3o7bo$b2o2bob2ob2o$3b3o4bob$bo2bobo3bob$o4bo4bob$o4bo4bob$bo2bobo3bob$3b3o4"
        "bob$b2o2bob2ob2o$b3o7bo",
        3, (-1, 0),
    ),
    _p(SHIP, "31p8h4v0", "31P8H4V0",
        "b5o$bo4bo$bo$2bo4bo$4bo$5b2o2bo$3bo2bo3bo$bo7bo$o5bo2bo$o3b2o$4o2b2o",
        8, (-4, 0),
    ),
    _p(SHIP, "25p3h1v0.1", "25P3H1V0.1",
        "10bo5b$8b3ob3ob$7b2o6bo$2bo3bo2bo3b2ob$b4o11b$o3bo11b$bobo2bo9b$5bo",
        3, (0, -1),
    ),
    _p(SHIP, "b29", "B29",
        "3b2o3b$2b2o4b$4bo3b$6b2o$5bo2b2$4bo2bo$b2obo3b$2o4bob$2bobo2bo$7bo$4bo2bo$5b"
        "obo$5bobo$6b2o$6bo",
        4, (-1, -1),
    ),
    _p(SHIP, "pushalong-1", "Pushalong 1",
        "3bo7b$bo2bo6b$o3bo6b$o3bo6b$2obo7b$5b2o4b$2bo2b2o4b$4bo6b$7b3ob$6b5o$5b2ob3o"
        "$6b2o",
        4, (-2, 0),
    ),
    _p(SHIP, "30p5h2v0", "30P5H2V0",
        "4bo8b$3b3o7b$2b2ob2o6b2$bobobobo2bo2b$2o3bo3b3ob$2o3bo6bo$10bobo$8bobo2b$9bo"
        "2bo$12bo",
        5, (0, -2),
    ),
    _p(SHIP, "dart", "Dart",
        "7bo7b$6bobo6b$5bo3bo5b$6b3o6b2$4b2o3b2o4b$2bo3bobo3bo2b$b2o3bobo3b2ob$o5bobo"
        "5bo$bob2obobob2obo",
        3, (0, -1),
    ),
    _p(SHIP, "canada-goose", "Canada goose",
        "3o10b$o9b2ob$bo6b3obo$3b2o2b2o4b$4bo8b$8bo4b$4b2o3bo3b$3bobob2o4b$3bobo2bob2"
        "ob$2bo4b2o4b$2b2o9b$2b2o",
        4, (-1, -1),
    ),
    _p(SHIP, "crab", "Crab",
        "8b2o3b$7b2o4b$9bo3b$11b2o$10bo2b2$9bo2bo$b2o5b2o3b$2o5bo5b$2bo4bobo3b$4b2o2b"
        "o4b$4b2o",
        4, (-1, -1),
    ),
    _p(SHIP, "44p5h2v0", "44P5H2V0",
        "4bo5bo4b$3b3o3b3o3b$2bo2bo3bo2bo2b$b3o7b3ob$2bobo5bobo2b$4b2o3b2o4b$o4bo3bo4"
        "bo$5bo3bo5b$2o3bo3bo3b2o$2bo2bo3bo2bo2b$4bo5bo",
        5, (0, -2),
    ),
    _p(SHIP, "hivenudger", "Hivenudger",
        "4o5bo2bo$o3bo3bo4b$o7bo3bo$bo2bo3b4ob2$5b2o6b$5b2o6b$5b2o6b2$bo2bo3b4ob$o7bo"
        "3bo$o3bo3bo4b$4o5bo2bo",
        4, (-2, 0),
    ),
    _p(SHIP, "orion-2", "Orion 2",
        "b2o10b$2o11b$2bo10b$4bo4b3ob$4b3o4b2o$5b3obobob$12bo$5bobo5b$4b2obo5b$5bo7b$"
        "3b2obo6b$6bo6b$4b2o",
        4, (-1, -1),
    ),
    _p(SHIP, "weekender", "Weekender",
        "bo12bob$bo12bob$obo10bobo$bo12bob$bo12bob$2bo3b4o3bo2b$6b4o6b$2b4o4b4o2b2$4b"
        "o6bo4b$5b2o2b2o",
        7, (0, -2),
    ),
    _p(SHIP, "brain", "Brain",
        "b3o9b3ob$obob2o5b2obobo$obobo7bobobo$bob2ob2ob2ob2obob$5bobobobo5b$3bobobobo"
        "bobo3b$2b2obobobobob2o2b$2b3o2bobo2b3o2b$2b2o2bo3bo2b2o2b$bo4b2ob2o4bob$bo13"
        "bo",
        3, (0, -1),
    ),
    _p(SHIP, "46p4h1v0", "46P4H1V0",
        "3bo11bo3b$3bo11bo3b$2bobo9bobo2b2$bo3bo7bo3bob$bob6ob6obob$o7bobo7bo$o7bobo7"
        "bo$o17bo$bob2ob2o3b2ob2obo",
        4, (0, -1),
    ),
    _p(SHIP, "wasp", "Wasp",
        "10bo3bo7b$7bobob2ob3o5b$6bo2bo6b2ob2ob$b2o2b2o2bo3bo2bo2b2ob$b2obob2o2bo2bo4"
        "bo2bo$o3bo4b2o11b$obobo2bo2b2o10b$9bo12b$b3o",
        3, (0, -1),
    ),
    _p(SHIP, "60p5h2v0", "60P5H2V0",
        "5bo7bo5b$3b2ob2o3b2ob2o3b$6b2o3b2o6b$8bobo8b$bo4bobobobo4bob$3o5bobo5b3o$o5b"
        "obobobo5bo$2bo2bo2bobo2bo2bo2b$2b2o3b2ob2o3b2o2b$o7bobo7bo$o6b2ob2o6bo",
        5, (0, -2),
    ),
    _p(SHIP, "orion", "Orion",
        "3b2o9b$3bobo8b$3bo10b$2obo10b$o4bo8b$ob2o6b3ob$5b3o4b2o$6b3obobob$13bo$6bobo"
        "5b$5b2obo5b$6bo7b$4b2obo6b$7bo6b$5b2o",
        4, (-1, -1),
    ),
    _p(SHIP, "spider", "Spider",
        "9bo7bo9b$3b2obobob2o3b2obobob2o3b$3obob3o9b3obob3o$o3bobo5bobo5bobo3bo$4b2o6"
        "bobo6b2o4b$b2o9bobo9b2ob$b2ob2o15b2ob2ob$5bo15bo",
        5, (0, -1),
    ),
    _p(SHIP, "schick-engine", "Schick engine",
        "bo2bo15b$o19b$o3bo15b$4o9b2o5b$6b3o5b2o4b$6b2ob2o6b3o$6b3o5b2o4b$4o9b2o5b$o3"
        "bo15b$o19b$bo2bo",
        12, (-6, 0),
    ),
    _p(SHIP, "37p4h1v0", "37P4H1V0",
        "bo$bo8bo$obo5bo3bo$8bo3b2o$5bob2o5b2o$b6o2bo6bo$2b2o6bo3b3o$10bo3bob2o$13bo$"
        "18bo$17bo$17bo",
        4, (0, -1),
    ),
    _p(SHIP, "64p2h1v0", "64P2H1V0",
        "5b3o15b3o5b$4bo3bo13bo3bo4b$3b2o4bo11bo4b2o3b$2bobob2ob2o3b3o3b2ob2obobo2b$b"
        "2obo4bob2ob3ob2obo4bob2ob$o4bo3bo4bobo4bo3bo4bo$12bo5bo12b$2o7b2o9b2o7b2o",
        2, (0, -1),
    ),
    _p(SHIP, "fireship", "Fireship",
        "4bo2bo$4bo2bo$3bo4bo$3bo4bo$3bo4bo$3bo4bo$2b3o2b3o$2bob4obo$3bo4bo3$5b2o$5b2"
        "o$5b2o$3bo4bo$b3o4b3o$3o6b3o$2o8b2o$b10o$2b8o$4b4o",
        10, (0, -1),
    ),
    _p(SHIP, "hammerhead", "Hammerhead",
        "5o13b$o4bo7b2o3b$o11b2ob3o$bo9b2ob4o$3b2o3b2ob2o2b2ob$5bo4bo2bo4b$6bobobobo5"
        "b$7bo10b$7bo10b$6bobobobo5b$5bo4bo2bo4b$3b2o3b2ob2o2b2ob$bo9b2ob4o$o11b2ob3o"
        "$o4bo7b2o3b$5o",
        4, (-2, 0),
    ),
    _p(SHIP, "swan", "Swan",
        "bo10b2o10b$5o6b2o11b$o2b2o8bo7b2ob$2b2obo5b2o6b3obo$11b2o3bob2o4b$5bobo6b2o8"
        "b$10b3obo4bo4b$7b3o3bo4bo5b$8bo7bo7b$8bo6bo8b2$11bo",
        4, (-1, -1),
    ),
    _p(SHIP, "56p6h1v0", "56P6H1V0",
        "5b3o10b3o5b$3obo7b2o7bob3o$4bo3bo2bo2bo2bo3bo4b$4bo5bo4bo5bo4b$10b2o2b2o10b$"
        "7bo3bo2bo3bo7b$7bobo6bobo7b$8b10o8b$10bo4bo10b$8bo8bo8b$7bo10bo7b$8bo8bo",
        6, (0, -1),
    ),
    _p(SHIP, "big-glider", "Big glider",
        "3b3o12b$3bo2b3o9b$4bobo11b$2o7bo8b$obo4bo2bo7b$o8b2o7b$b2o15b$bo2bo5bob2o4b$"
        "bo9b2obo3b$3bobo6b2o2bob$4b2obo4b2o3bo$8bo7bob$7b4o3bobob$7bob2o3b4o$8bo3b2o"
        "bo2b$13b2o3b$9bob3o4b$10bo2bo",
        4, (-1, -1),
    ),
    _p(SHIP, "sparky", "Sparky",
        "10b2obo12b2o2b$6b3obob3o8bob3o2b$2bobo10bo3b2obo4bo2b$2o4b2o5bo3b4obo3b2o2b$"
        "2bob2o2bo4b3obo5b4obob$8bo4bo13b3o$2bob2o2bo4b3obo5b4obob$2o4b2o5bo3b4obo3b2"
        "o2b$2bobo10bo3b2obo4bo2b$6b3obob3o8bob3o2b$10b2obo12b2o",
        4, (-1, 0),
    ),
    _p(SHIP, "enterprise", "Enterprise",
        "8bo12b$6b2obo11b$4b2o3bo11b$3bo3b2o12b$2bo4b2obo10b$bo2b2ob3obo9b$bo19b$3bob"
        "o3b2o10b$2o3bobo13b$o3bo3b2o11b$bo2b2o2bo2bo9b$4b2o3bo8bo2b$10bobo3bobobo$12"
        "b2ob3o2bo$12b2o4b2ob$13bob2o4b2$11bobo7b$11bo9b$11b3o7b$12bobo",
        4, (-1, -1),
    ),
    _p(SHIP, "ecologist", "Ecologist",
        "bo2bo5b2o15b$o8b4o14b$o3bo3b2ob2o14b$4o5b2o16b4$11b2o14b$2b3o7b2o13b$2bo6bo2"
        "bo14b$2bobo6bo15b$3b2o3b2o17b$25b2o$25bob$bo2bo18b3ob$o26b$o3bo22b$4o",
        20, (-10, 0),
    ),
    _p(SHIP, "58p5h1v1", "58P5H1V1",
        "20b2ob$20b2ob$19bo2bo$16b2obo2bo$22bo$14b2o3bo2bo$14b2o5bob$15bob5ob$16bo6b3"
        "$13b3o7b$13bo9b$11b2o10b$5b2o4bo11b$5b3o3bo11b$3bo4bo14b$3bo3bo15b$7bo15b$2b"
        "2obobo15b$2o5bo15b$2o4b2o15b$2b4o",
        5, (-1, -1),
    ),
    _p(SHIP, "fly", "Fly",
        "2bo31b$bobo30b$bobo22bobo3bob$bo23b2obobo2bo$11b3o8bo9bob$2o9b2o2bob2o3bo2b4"
        "o5b$bobo9b4o2bobo2b2o4b2o2b$b2o8bo2bo3b3o5b3o5b$2bo7bo4bo2b2o2b2o2bo2bo4b$3b"
        "o2bo3bo4bo2b3obobo4b2o3b$7bob2o4bo2b4o5bo6b$4b2o3b2o4bo2b4o5bo6b$4bobo3bo4bo"
        "2b3obobo4b2o3b$3b2o5bo4bo2b2o2b2o2bo2bo4b$4bobo4bo2bo3b3o5b3o5b$5bo7b4o2bobo"
        "2b2o4b2o2b$11b2o2bob2o3bo2b4o5b$11b3o8bo9bob$25b2obobo2bo$26bobo3bo",
        3, (-1, 0),
    ),
    _p(SHIP, "snail", "Snail",
        "bo36b$bo36b$o37b$b3o17b3o3b3o8b$b2obo9bo3bobo6b3o8b$2bo11b2obo7bo4b4o4b$6bo6"
        "bo3bobo3b2obo5b2o4b$3bo2bob3o3b2o9bo8b2obo$3b2obo5bo5bo17bob$9bob7o20b2$9bob"
        "7o20b$3b2obo5bo5bo17bob$3bo2bob3o3b2o9bo8b2obo$6bo6bo3bobo3b2obo5b2o4b$2bo11"
        "b2obo7bo4b4o4b$b2obo9bo3bobo6b3o8b$b3o17b3o3b3o8b$o37b$bo36b$bo",
        5, (-1, 0),
    ),
    _p(SHIP, "seal", "Seal",
        "2b2o30b$b3ob3o26b$3bo3b2o25b$2ob2o2bo2bobo21b$o6bob4o21b$6b2obo2bo21b$o4b2o3"
        "bob2o20b$2o8bobo2bo18b$o2b5ob3o4bo17b$bo9bo4bo17b$b2obo2bo3bo3bo18b$2bo9b2o2"
        "0b$9b2o2b2o4bo14b$2bo3b2obo4b3o2bo14b$3b2obob5o5bob2o12b$7b2ob2ob2ob2o2b3o11"
        "b$17bobo4bo9b$8b2o4bobo3b6o8b$13bo7b2o11b$13bo3bo16b$13bo4bo3b3o6bo2b$14b5o3"
        "b2ob2o2b2ob2o$17b2obo2bob2o2bo4b$16b2o5bo4bo2b3o$15b3obobobobo8b$22bo2bo8b$1"
        "8bobo13b$19bo14b2$21b3o10b$21b3o10b2$20b2o12b$20b2o12b$21bo",
        6, (-1, -1),
    ),
    _p(SHIP, "77p6h1v1", "77P6H1V1",
        "34bob$33bobo$31b2o2bo2$30bo5b$27b2o2b2o3b$27b2o7b$27bo8b$25bobo8b$25b2o9b$23"
        "bo2bo9b$22bo4bo8b$21bo5bo8b$18bo3bo13b$17bobo3b2o11b$16b2ob2o15b$15b2o4bobo1"
        "2b$14b2o7bo12b$13bo9bo12b$14b2o20b$15bo20b$12bo3bo19b$11bobo22b$10bo3bob3o17"
        "b$14bo21b$8b2o26b$9b2o25b$5b4o2b2o23b$5b2o29b2$4bo31b$2bo2bo30b$2bo2bo30b$bo"
        "34b$o35b$b2o",
        6, (-1, -1),
    ),
    _p(SHIP, "p15-pre-pulsar-spaceship", "P15 pre-pulsar spaceship",
        "3b3o10b3o23b3o10b3o3b$4bobo8bobo25bobo8bobo4b$6bo8bo29bo8bo6b$bob5o6b5obo19b"
        "ob5o6b5obob$bob2o3b2o2b2o3b2obo19bob2o3b2o2b2o3b2obob$2o5b3o2b3o5b2o17b2o5b3"
        "o2b3o5b2o$5b2ob2o2b2ob2o27b2ob2o2b2ob2o5b2$4bo12bo25bo12bo4b$5b3o6b3o27b3o6b"
        "3o5b$4b4o6b4o25b4o6b4o4b$3b2o12b2o23b2o12b2o3b$4bo12bo25bo12bo4b$2b3o12b3o21"
        "b3o12b3o2b$2bo16bo21bo16bo2b$5bo10bo27bo10bo5b$3bobo10bobo23bobo10bobo3b$2bo"
        "3b2o6b2o3bo21bo3b2o6b2o3bo2b$3bo5bo2bo5bo23bo5bo2bo5bo3b$9bo2bo35bo2bo9b$bob"
        "2o4bo2bo4b2obo19bob2o4bo2bo4b2obob$bo3b3obo2bob3o3bo19bo3b3obo2bob3o3bob$2bo"
        "6bo2bo6bo21bo6bo2bo6bo2b$bo2b2o3bo2bo3b2o2bo19bo2b2o3bo2bo3b2o2bob$bo2b2obob"
        "o2bobob2o2bo19bo2b2obobo2bobob2o2bob$5bo2bo4bo2bo27bo2bo4bo2bo5b$6bobo4bobo2"
        "9bobo4bobo6b$27bo5bo27b$4b2obob4obob2o9bo5bo9b2obob4obob2o4b$10b2o14bobo3bob"
        "o14b2o10b$2bob2o10b2obo7bo5bo7bob2o10b2obo2b$2o2bo3b2o2b2o3bo2b2o5bo5bo5b2o2"
        "bo3b2o2b2o3bo2b2o$b2obo12bob2o19b2obo12bob2ob$3b2obo8bob2o23b2obo8bob2o3b$4b"
        "o2bo6bo2bo25bo2bo6bo2bo4b$b2o2bo2bo4bo2bo2b2o4b2o7b2o4b2o2bo2bo4bo2bo2b2ob$b"
        "o5b2o4b2o5bo5bobo3bobo5bo5b2o4b2o5bob$b3o3bo6bo3b3o6bo5bo6b3o3bo6bo3b3ob$3b4"
        "o8b4o23b4o8b4o3b$4bo2bo6bo2bo25bo2bo6bo2bo4b$4bo12bo25bo12bo4b$4bob2o6b2obo2"
        "5bob2o6b2obo4b$5bo10bo27bo10bo",
        15, (0, -3),
    ),
    # seed: 31
    _p(SEED, "t-tetromino", "T-tetromino", "3o$bo"),
    _p(SEED, "pi-heptomino", "Pi-heptomino", "3o$obo$obo"),
    _p(SEED, "r-pentomino", "R-pentomino", "b2o$2ob$bo"),
    _p(SEED, "b-heptomino", "B-heptomino", "ob2o$3ob$bo"),
    _p(SEED, "c-heptomino", "C-heptomino", "b3o$3ob$bo"),
    _p(SEED, "century", "Century", "2b2o$3ob$bo"),
    _p(SEED, "e-heptomino", "E-heptomino", "b3o$2o$b2o"),
    _p(SEED, "herschel", "Herschel", "o2b$3o$obo$2bo"),
    _p(SEED, "stairstep-hexomino", "Stairstep hexomino", "2o2b$b2ob$2b2o"),
    _p(SEED, "gliders-by-the-dozen", "Gliders by the dozen", "2o2bo$o3bo$o2b2o"),
    _p(SEED, "thunderbird", "Thunderbird", "3o2$bob$bob$bo"),
    _p(SEED, "teardrop", "Teardrop", "b3o$o2bo$o2bo$b2o"),
    _p(SEED, "wing", "Wing", "b2ob$o2bo$bobo$2b2o"),
    _p(SEED, "acorn", "Acorn", "bo5b$3bo3b$2o2b3o"),
    _p(SEED, "rabbits", "Rabbits", "o3b3o$3o2bob$bo"),
    _p(SEED, "7468m", "7468M", "4bob$4b2o$2ob2ob$o"),
    _p(SEED, "die-hard", "Die hard", "6bob$2o6b$bo3b3o"),
    _p(SEED, "multum-in-parvo", "Multum in parvo", "3b3o$2bo2bo$bo4b$o"),
    _p(SEED, "switch-engine", "Switch engine", "bobo2b$o5b$bo2bob$3b3o"),
    _p(SEED, "5x5-infinite-growth", "5x5 infinite growth", "3obo$o4b$3b2o$b2obo$obobo"),
    _p(SEED, "bunnies", "Bunnies", "o5bob$2bo3bob$2bo2bobo$bobo"),
    _p(SEED, "queen-bee", "Queen bee", "3bo3b$2bobo2b$bo3bob$2b3o2b$2o3b2o"),
    _p(SEED, "bunnies-10", "Bunnies 10", "bo4b$2obo2b$4b2o$o2bo2b$o5b$o"),
    _p(SEED, "10-cell-infinite-growth", "10-cell infinite growth",
        "6bob$4bob2o$4bobob$4bo3b$2bo5b$obo",
    ),
    _p(SEED, "bunnies-9", "Bunnies 9", "bo6b$2o5bo$6bob$6bob$5bo2b$4bo3b$4bo"),
    _p(SEED, "blom", "Blom", "o10bo$b4o6bo$2b2o7bo$10bob$8bobo"),
    _p(SEED, "time-bomb", "Time bomb",
        "bo11b2o$obo4bo6bo$7bo4bo2b$2bo2bo3bo2bo2b$2b2o6bo4b$3bo",
    ),
    _p(SEED, "lidka", "Lidka", "bo7b$obo6b$bo7b8$8bo$6bobo$5b2obo2$4b3o"),
    _p(SEED, "justyna", "Justyna",
        "17bo4b$16bo2bo2b$17b3o2b$17bo2bob2$2o16bo3b$bo16bo3b$18bo3b8$19b3o$11b3o",
    ),
    _p(SEED, "iwona", "Iwona",
        "14b3o3b6$2bo17b$3b2o15b$3bo14bob$18bob$18bob$19bo$18b2o$7b2o11b$8bo11b5$2o18"
        "b$bo",
    ),
    _p(SEED, "block-laying-switch-engine", "Block-laying switch engine",
        "18bo10b$b3o8bo5bo10b$o3bo6bo7bo9b$b2o9b4o2b2o9b$3b2ob2o9b3o9b$5b2o11bobo8b$1"
        "9bo7b2o$19bo7b2o11$7b2o20b$7b2o20b7$15b2o12b$15b2o",
    ),
    # gun: 9
    _p(GUN, "gosper-glider-gun", "Gosper glider gun",
        "24bo11b$22bobo11b$12b2o6b2o12b2o$11bo3bo4b2o12b2o$2o8bo5bo3b2o14b$2o8bo3bob2"
        "o4bobo11b$10bo5bo7bo11b$11bo3bo20b$12b2o",
    ),
    _p(GUN, "simkin-glider-gun", "Simkin glider gun",
        "2o5b2o$2o5b2o2$4b2o$4b2o5$22b2ob2o$21bo5bo$21bo6bo2b2o$21b3o3bo3b2o$26bo4$20"
        "b2o$20bo$21b3o$23bo",
    ),
    _p(GUN, "bi-gun", "Bi-gun",
        "11bo38b$10b2o38b$9b2o39b$10b2o2b2o34b$38bo11b$38b2o8b2o$39b2o7b2o$10b2o2b2o1"
        "8b2o2b2o10b$2o7b2o39b$2o8b2o38b$11bo38b$34b2o2b2o10b$39b2o9b$38b2o10b$38bo",
    ),
    _p(GUN, "b-52-bomber", "B-52 bomber",
        "b2o36b$b2o17bo18b$19bobo12bobo2b$20bo12bo5b$2o7b2o23bo2bob$2obo5b2o23bobobo$"
        "3bo23bo7bo2bo$3bo23b2o7b2ob$o2bo17b2o5bo10b$b2o18bo17b$21b3o15b$36b2ob$36b2o"
        "b$b2o36b$o2bo35b$obobo16bobo4b2o5b2o2b$bo2bo17b2o4b2o5b2obo$5bo12bo3bo15bo$2"
        "bobo12bobo18bo$18bo16bo2bo$36b2o",
    ),
    _p(GUN, "twogun", "Twogun",
        "27bo11b$25bobo11b$15b2o6b2o12b2o$14bo3bo4b2o12b2o$3b2o8bo5bo3b2o14b$3b2o8bo3"
        "bob2o4bobo11b$13bo5bo7bo11b$14bo3bo20b$15b2o22b$26bo12b$27b2o10b$26b2o11b4$2"
        "1b2o16b$9bobo10b2o15b$9bo2bo8bo17b$2o10b2o11b2o12b$2o8bo3b2o8bobo12b$5b2o5b2"
        "o9bo6b2o7b$4bo4bo2bo10bo2bo2bo2bo6b$9bobo11bo6b3o6b$24bobo5b3o4b$25b2o6bobo3"
        "b$35bo3b$35b2o",
    ),
    _p(GUN, "new-gun-1", "New gun 1",
        "25b2o5b2o$25b2o5b2o12$27b2ob2o2b$26bo5bob2$25bo7bo$25bo2bobo2bo$25b3o3b3o5$1"
        "7bo16b$2o15b2o15b$2o16b2o14b$13b2o2b2o15b4$13b2o2b2o15b$2o16b2o7b2o5b$2o15b2"
        "o8b2o5b$17bo",
    ),
    _p(GUN, "new-gun-2", "New gun 2",
        "23b2o24b2o$23b2o24b2o$41b2o8b$40bo2bo7b$41b2o8b2$36b3o12b$36bobo12b$9b2o25b3"
        "o12b$9b2o25b2o13b$8bo2bo23b3o13b$8bo2bob2o20bobo13b$8bo4b2o20b3o13b$10b2ob2o"
        "36b$31b2o18b$21b2o7bo2bo17b$21b2o8b2o18b$49b2o$49b2o2$4b2o18bo26b$2o4b4o10b2"
        "o2b2ob3o21b$2o2b2ob3o10b2o4b4o21b$4bo19b2o",
    ),
    _p(GUN, "period-45-glider-gun", "Period-45 glider gun",
        "15bo2bo2bo$15b7o2$15b7o$15bo2bo2bo6$16bo2bo$12b2o2bo2bo$12b2o2bo2bo7b2o$27b2"
        "o4b2o3bo2bo3b2o$33b5o4b5o$33b2o3bo2bo3b2o$27b3o2$2ob2o$bobo23b3o$bobo6b3o$2o"
        "b2o29bobo$bobo31b2o$bobo6b3o22bo$2ob2o2$11b2o$11b2o7bo2bo2b2o$20bo2bo2b2o$20"
        "bo2bo3$47bo$45bobo$46b2o$18bo2bo2bo$18b7o2$18b7o$18bo2bo2bo",
    ),
    _p(GUN, "vacuum-gun", "Vacuum (gun)",
        "b2o23b2o21b$b2o23bo22b$24bobo22b$15b2o7b2o23b$2o13bobo31b$2o13bob2o30b$16b2o"
        "31b$16bo32b$44b2o3b$16bo27b2o3b$16b2o31b$2o13bob2o13bo3bo12b$2o13bobo13bo5bo"
        "7b2o2b$15b2o14bo13b2o2b$31b2o3bo12b$b2o30b3o13b$b2o46b$33b3o13b$31b2o3bo12b$"
        "31bo13b2o2b$31bo5bo7b2o2b$32bo3bo12b2$44b2o3b$44b2o3b5$37b2o10b$37bobo7b2o$3"
        "9bo7b2o$37b3o9b$22bobo24b$21b3o25b$21b3o25b$21bo15b3o9b$25bobo11bo9b$21b2o4b"
        "o9bobo9b$16b2o4bo3b2o9b2o10b$15bobo6bo24b$15bo33b$14b2o",
    ),
    # puffer: 10
    _p(PUFFER, "puffer-2", "Puffer 2",
        "b3o11b3o$o2bo10bo2bo$3bo4b3o6bo$3bo4bo2bo5bo$2bo4bo8bo",
    ),
    _p(PUFFER, "blinker-puffer-1", "Blinker puffer 1",
        "3bo5b$bo3bo3b$o8b$o4bo3b$5o4b4$b2o6b$2ob3o3b$b4o4b$2b2o5b2$5b2o2b$3bo4bo$2bo"
        "6b$2bo5bo$2b6o",
    ),
    _p(PUFFER, "pufferfish", "Pufferfish",
        "3bo7bo$2b3o5b3o$b2o2bo3bo2b2o$3b3o3b3o2$4bo5bo$2bo2bo3bo2bo$o5bobo5bo$2o4bob"
        "o4b2o$6bobo$3bobo3bobo$4bo5bo",
    ),
    _p(PUFFER, "puffer-1", "Puffer 1",
        "b3o6bo5bo6b3ob$o2bo5b3o3b3o5bo2bo$3bo4b2obo3bob2o4bo3b$3bo19bo3b$3bo2bo13bo2"
        "bo3b$3bo2b2o11b2o2bo3b$2bo3b2o11b2o3bo",
    ),
    _p(PUFFER, "noahs-ark", "Noah's ark",
        "10bobo2b$9bo5b$10bo2bob$12b3o6$bo13b$obo12b2$o2bo11b$2b2o11b$3bo",
    ),
    _p(PUFFER, "space-rake", "Space rake",
        "11b2o5b4o$9b2ob2o3bo3bo$9b4o8bo$10b2o5bo2bob2$8bo13b$7b2o8b2o3b$6bo9bo2bo2b$"
        "7b5o4bo2bo2b$8b4o3b2ob2o2b$11bo4b2o4b4$18b4o$o2bo13bo3bo$4bo16bo$o3bo12bo2bo"
        "b$b4o",
    ),
    _p(PUFFER, "blinker-puffer-2", "Blinker puffer 2",
        "13b3ob$12b5o$11b2ob3o$12b2o3b3$9bobo5b$2bo5bo2bo5b$b5o3bobo5b$2o3b2ob2o7b$bo"
        "7bo7b$2b2o2bo2bo7b$10bo6b$2b2o2bo2bo7b$bo7bo7b$2o3b2ob2o7b$b5o3bobo5b$2bo5bo"
        "2bo5b$9bobo5b3$12b2o3b$11b2ob3o$12b5o$13b3o",
    ),
    _p(PUFFER, "backrake-1", "Backrake 1",
        "5b3o11b3o5b$4bo3bo9bo3bo4b$3b2o4bo7bo4b2o3b$2bobob2ob2o5b2ob2obobo2b$b2obo4b"
        "ob2ob2obo4bob2ob$o4bo3bo2bobo2bo3bo4bo$12bobo12b$2o7b2obobob2o7b2o$12bobo12b"
        "$6b3o9b3o6b$6bo3bo9bo6b$6bobo4b3o11b$12bo2bo4b2o5b$15bo11b$11bo3bo11b$11bo3b"
        "o11b$15bo11b$12bobo",
    ),
    _p(PUFFER, "backrake-2", "Backrake 2",
        "3bo15b$2b3o14b$b2obo5bo8b$b3o5b3o7b$2b2o4bo2b2o3b3o$8b3o4bo2bo$18bo$18bo$18b"
        "o$2b3o12bob$2bo2bo13b$2bo16b$2bo16b$3bo15b7$3o16b$o2bo11bo3b$o13b3o2b$o12b2o"
        "bo2b$o12b3o3b$bo12b2o",
    ),
    _p(PUFFER, "frothing-puffer", "Frothing puffer",
        "7bo17bo$6b3o15b3o$5b2o4b3o5b3o4b2o$3b2obo2b3o2bo3bo2b3o2bob2o$4bobo2bobo3bob"
        "o3bobo2bobo$b2obobobobo4bobo4bobobobob2o$b2o3bobo4bo5bo4bobo3b2o$b3obo3bo4bo"
        "bobo4bo3bob3o$2o9b2obobobob2o9b2o$12bo7bo$9b2obo7bob2o$10bo11bo$7b2obo11bob2"
        "o$7b2o15b2o$7bobobob3ob3obobobo$6b2o3bo3bobo3bo3b2o$6bo2bo3bobobobo3bo2bo$9b"
        "2o4bobo4b2o$5b2o4bo3bobo3bo4b2o$9bob2obo3bob2obo$10bobobobobobobo$12bo2bobo2"
        "bo$11bobo5bobo",
    ),
    # filler: 2
    _p(FILLER, "max", "Max",
        "18bo8b$17b3o7b$12b3o4b2o6b$11bo2b3o2bob2o4b$10bo3bobo2bobo5b$10bo4bobobobob2"
        "o2b$12bo4bobo3b2o2b$4o5bobo4bo3bob3o2b$o3b2obob3ob2o9b2ob$o5b2o5bo13b$bo2b2o"
        "bo2bo2bob2o10b$7bobobobobobo5b4o$bo2b2obo2bo2bo2b2obob2o3bo$o5b2o3bobobo3b2o"
        "5bo$o3b2obob2o2bo2bo2bob2o2bob$4o5bobobobobobo7b$10b2obo2bo2bob2o2bob$13bo5b"
        "2o5bo$b2o9b2ob3obob2o3bo$2b3obo3bo4bobo5b4o$2b2o3bobo4bo12b$2b2obobobobo4bo1"
        "0b$5bobo2bobo3bo10b$4b2obo2b3o2bo11b$6b2o4b3o12b$7b3o17b$8bo",
    ),
    _p(FILLER, "spacefiller-1", "Spacefiller 1",
        "5bobo21b$4bo2bo21b$3b2o24b$2bo26b$b4o24b$o4bo23b$o2bo25b$o2bo25b$bo9b3o3b3o9"
        "b$2b4obo2bo2bo3bo2bo8b$3bo3bo5bo3bo11b$4bo8bo3bo11b$4bobo6bo3bo11b2$3b3o5b3o"
        "3b3o9b$3b2o7bo5bo10b$3b3o6b7o10b$11bo7bo9b$4bobo3b11o8b$3bo2bo2bo12b2o5b$3bo"
        "5b12o3bo4b$3bo3bo13bo3bo3b$4bo3b12o5bo3b$5b2o12bo2bo2bo3b$8b11o3bobo4b$9bo7b"
        "o11b$10b7o6b3o3b$10bo5bo7b2o3b$9b3o3b3o5b3o3b2$11bo3bo6bobo4b$11bo3bo8bo4b$1"
        "1bo3bo5bo3bo3b$8bo2bo3bo2bo2bob4o2b$9b3o3b3o9bob$25bo2bo$25bo2bo$23bo4bo$24b"
        "4ob$26bo2b$24b2o3b$21bo2bo4b$21bobo",
    ),
)
# fmt: on

LIBRARY: Final[dict[str, Pattern]] = {p.name: p for p in _TABLE}
"""Every pattern, by name."""


def of(*kinds: str) -> list[Pattern]:
    """The patterns of `kinds`, in the order of the table."""
    return [p for p in _TABLE if p.kind in kinds]
