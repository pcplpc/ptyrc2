# SPDX-License-Identifier: GPL-3.0-or-later
"""Clock: a sprite that places itself and is updated in place.

    ptyrc-app clock
    ptyrc-app clock --prefer bottom-right --big

What it shows:

- `auto=`: the driver picks a spot that is free and still, near the
  preferred corner, and moves the sprite if the app starts using it;
- `layer="front"`: a head-up display, in front of the app's text;
- `Sprite.set`: new content for the same sprite, once a second;
- `--big`: a draw extension builds the sprite on the driver, and
  `Sprite.ext` redraws it in place when the minute changes.
"""

from __future__ import annotations

import argparse
import time
from collections.abc import Sequence
from datetime import datetime

from ptyrc import Pilot, Sprite

from . import _cli

__all__ = ["face", "main"]

BACKGROUND = 236  # a dark grey of the 256-colour palette


def face(now: datetime) -> list[list[dict[str, object]]]:
    """The clock as one row of styled runs: `{"t": text, "fg": colour}`."""
    colon = {"t": ":", "fg": "bright-black"}
    return [
        [
            {"t": " "},
            {"t": f"{now:%H}", "fg": "bright-cyan"},
            colon,
            {"t": f"{now:%M}", "fg": "bright-magenta"},
            colon,
            {"t": f"{now:%S}", "fg": "bright-yellow"},
            {"t": " "},
        ]
    ]


def small(pilot: Pilot, args: argparse.Namespace, sprite: Sprite | None) -> Sprite:
    content = {"cells": face(datetime.now()), "bg": BACKGROUND, "bold": True}
    if sprite is None:
        # `opaque`: the spaces of the face hide what is below.
        return pilot.draw(
            frame=args.frame, auto=args.prefer, layer="front", opaque=True, **content
        )
    sprite.set(opaque=True, **content)
    return sprite


def big(pilot: Pilot, args: argparse.Namespace, sprite: Sprite | None) -> Sprite:
    text = f"{datetime.now():%H:%M}"
    if sprite is not None and text == args.shown:
        return sprite
    args.shown = text
    digits = {"text": text, "fg": "bright-cyan"}
    if sprite is None:
        return pilot.ext(
            "bigtext",
            "text",
            digits,
            frame=args.frame,
            anchor={"auto": {"prefer": args.prefer}},
            layer="front",
        )
    # Same sprite, same spot: only what it shows changes.
    sprite.ext("bigtext", "text", digits)
    return sprite


def app(pilot: Pilot, args: argparse.Namespace) -> None:
    draw = big if args.big else small
    args.shown = None
    sprite = None
    end = None if args.seconds is None else time.monotonic() + args.seconds
    while end is None or time.monotonic() < end:
        sprite = draw(pilot, args, sprite)
        # Sleep to the next second, so the digits change on time.
        time.sleep(1.0 - time.time() % 1.0)


def main(argv: Sequence[str] | None = None) -> int:
    out = _cli.parser("clock", "A floating clock in a quiet corner.")
    out.add_argument(
        "--prefer",
        default="top-right",
        help="top-left, top, top-right, left, center, right, bottom-left, "
        "bottom, bottom-right, any [default: top-right]",
    )
    out.add_argument("--big", action="store_true", help="block digits")
    out.add_argument("--seconds", type=float, help="stop after this long")
    return _cli.run(app, _cli.parse(out, argv))


if __name__ == "__main__":
    raise SystemExit(main())
