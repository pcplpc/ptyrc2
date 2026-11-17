# SPDX-License-Identifier: GPL-3.0-or-later
"""Sand: a thin stream piles on the text of an idle pane.

    ptyrc-app sand                 # the most idle pane, once it is idle
    ptyrc-app sand -f 3 --rate 300 --now
    ptyrc-app sand -f active --cursor --now
    ptyrc-app sand --hover         # the mouse takes the sand it is over
    ptyrc-app sand --bomb          # a click on the pile blows it up
    ptyrc-app sand -a              # --hover and --bomb

What it shows:

- `Pilot.wait_idle`: the driver knows which panes have been still for
  a while, and answers when one is. A big animation starts there, not
  where someone is typing. Nothing is polled.
- A native effect from a draw extension (`plugins/draw-ext/sand`): the
  simulation runs in the driver, 80 steps a second, with nothing on the
  socket. One request starts it; the app then only stays connected.
- An effect that sees the screen: the text under it is its walls, read
  by the driver at each render. Text written on the pile takes the
  place of its grains; a resize cuts off what no longer fits; the pile
  scrolls with the text.
- `--cursor`: the stream falls on the cursor's column.
- `--hover`: an effect told the pointer (`_cli.Nozzle`): the grains
  under the host mouse are taken, three cells of its row; the pile
  over the hole falls in, the slopes beside it avalanche.
- `--bomb`: an effect told the clicks (`_cli.Nozzle`, topic `mouse`):
  a click on the pile is a bomb there, on release; the sand around is
  thrown up and away, flies, and lands. The longer the press, the
  wider and the faster, up to 1.5 s. While it runs the clicks on the
  pane are the app's: the program in it gets none.
- A frame replaced: of two panes with little text, resized together,
  the driver cannot tell which is which and allocates new frames. The
  app is told by the topic `frames` and moves its sprite to the pane
  that took the place (`_cli.follow`, `Sprite.update(frame=...)`): the
  effect, and its pile, go on. Nothing is polled: the app sleeps on
  the socket.
"""

from __future__ import annotations

import argparse
import random
from collections.abc import Sequence

from ptyrc import Frame, Pilot, PtyrcError, Sprite, clock

from . import _cli
from ._cli import follow, heir

__all__ = ["follow", "heir", "main", "pour", "still"]

POLL = 0.5  # seconds between two looks at a layout still settling


def pour(pilot: Pilot, frame: Frame, rate: float, cursor: bool = False) -> Sprite:
    """Start the sand on `frame`. `cursor`: the stream follows the
    cursor's column."""
    params = {"rate": rate, "cursor": cursor, "seed": random.getrandbits(32)}
    # Behind the text: a grain never hides a letter.
    return pilot.effect("sand", params, frame=frame.id, layer="back")


def still(pilot: Pilot, sel: int | str, timeout: float) -> Frame | None:
    """The frame `sel` once it is idle; None after `timeout` seconds.
    One request: the driver answers when it is so. It resolves a name
    at each look: the most idle pane now may not be the one of a moment
    ago. A name that finds no pane (a plain shell) is the raw frame."""
    try:
        return pilot.wait_idle(sel, timeout=timeout)
    except TimeoutError:
        return None
    except PtyrcError as exc:
        if exc.code != "no-frame" or isinstance(sel, int):
            raise
    try:
        return pilot.wait_idle("raw", timeout=min(timeout, 1.0))
    except TimeoutError:
        return None


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    end = None if seconds is None else clock.monotonic() + seconds
    while True:
        left = 30.0 if end is None else end - clock.monotonic()
        if left <= 0:
            return
        if args.now:
            frame: Frame | None = _cli.pick(pilot, args.frame)
        else:
            frame = still(pilot, args.frame, left)
        if frame is not None and frame.usable:
            break
        if args.now or frame is not None:
            clock.sleep(POLL)  # a layout still settling
    sand = pour(pilot, frame, args.rate, args.cursor)
    left = None if end is None else end - clock.monotonic()
    follow(pilot, frame, sand, left, hover=args.hover, bomb=args.bomb)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("sand", "Sand falling in an idle pane.", frame="idle")
    out.add_argument(
        "--rate", type=float, default=60.0, help="grains per second [default: 60]"
    )
    out.add_argument(
        "--cursor",
        action="store_true",
        help="pour over the cursor's column; from the middle while the "
        "cursor is out of the pane or on its first column",
    )
    out.add_argument(
        "--hover",
        action="store_true",
        help="the host mouse takes the sand it is over; the pile falls in",
    )
    out.add_argument(
        "--bomb",
        action="store_true",
        help="a click on the pile blows it up, the longer held the stronger "
        "(1.5 s at most); the clicks on the pane do not reach its program",
    )
    out.add_argument("-a", "--all", action="store_true", help="--hover and --bomb")
    out.add_argument(
        "--now", action="store_true", help="do not wait for the pane to be idle"
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.hover = args.bomb = True
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
