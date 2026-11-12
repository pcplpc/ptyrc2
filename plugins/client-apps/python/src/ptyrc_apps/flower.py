# SPDX-License-Identifier: GPL-3.0-or-later
"""Flower: a drawing tied to a word.

    ptyrc-app flower                       # next to the word "flower"
    ptyrc-app flower --word TODO --side above --color bright-red
    ptyrc-app flower --front

What it shows:

- `near=`: the driver finds the word and keeps the sprite next to it.
  Scroll, edit, switch buffers: the flower follows, and hides while the
  word is not on screen. The client does nothing for that.
- `Sprite.set`: the flower opens in three steps sent by the client;
- `frames=`: then it sways, an animation the driver plays alone.
"""

from __future__ import annotations

import argparse
import time
from collections.abc import Sequence

from ptyrc import Pilot

from . import _cli

__all__ = ["BLOOM", "SWAY", "main", "paint"]

# Growth, then the open flower leaning left, upright, right. Seven
# columns, seven rows; the last two rows are the stem.
BLOOM = [
    ["", "", "", "", "   .   ", "   |   ", "   |   "],
    ["", "", "  ,-,  ", " ( o ) ", "  `-'  ", "  \\|/  ", "   |   "],
]
SWAY = [
    [",-.-,  ", "( \\ / )", " ) o ( ", "( / \\ )", " `-'-' ", "  \\|/  ", "   |   "],
    [" ,-.-, ", "( \\ / )", " ) o ( ", "( / \\ )", " `-'-' ", "  \\|/  ", "   |   "],
    ["  ,-.-,", "( \\ / )", " ) o ( ", "( / \\ )", " `-'-' ", "  \\|/  ", "   |   "],
]
STEM_ROWS = 2


def paint(rows: Sequence[str], petals: str) -> list[list[dict[str, object]]]:
    """Rows of text to rows of styled runs: a yellow heart, a green stem,
    petals of the given colour. One run per glyph keeps it simple."""
    out = []
    for y, row in enumerate(rows):
        stem = y >= len(rows) - STEM_ROWS
        runs: list[dict[str, object]] = []
        for ch in row:
            if ch == " ":
                runs.append({"t": " "})  # transparent
            elif ch == "o":
                runs.append({"t": ch, "fg": "bright-yellow", "bold": True})
            else:
                runs.append({"t": ch, "fg": "green" if stem else petals})
        out.append(runs)
    return out


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    # The layer is a choice. Behind the text (the default of every
    # sprite) the flower grows in the free cells around the word and
    # never hides a letter: right for a decoration in a document.
    # `--front` shows it whole, over what is next to the word.
    layer = "front" if args.front else "back"
    first = paint(BLOOM[0], args.color)
    flower = pilot.draw(
        cells=first,
        frame=args.frame,
        near=args.word,
        nth=args.nth,
        side=args.side,
        gap=1,
        ignore_case=True,
        layer=layer,
    )
    time.sleep(0.4)
    flower.set(cells=paint(BLOOM[1], args.color))
    time.sleep(0.4)
    # left, upright, right, upright: a slow loop, played by the driver.
    order = (SWAY[0], SWAY[1], SWAY[2], SWAY[1])
    sway = [{"cells": paint(rows, args.color), "ms": 600} for rows in order]
    flower.set(frames=sway, loop=True)
    # Nothing more to send. Stay connected: the sprite lives as long as
    # this connection does.
    _cli.hold(pilot, seconds)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until the session ends."""
    out = _cli.parser("flower", "A flower next to a word.")
    out.add_argument("--word", default="flower", help="the word [default: flower]")
    out.add_argument("--nth", type=int, default=0, help="which match [default: 0]")
    out.add_argument(
        "--side",
        default="right",
        choices=("right", "left", "above", "below"),
        help="where, from the word [default: right]",
    )
    out.add_argument("--color", default="bright-magenta", help="colour of the petals")
    out.add_argument("--front", action="store_true", help="over the text")
    return _cli.run(
        lambda pilot, args: app(pilot, args, seconds), _cli.parse(out, argv)
    )


if __name__ == "__main__":
    raise SystemExit(main())
