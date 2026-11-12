# SPDX-License-Identifier: GPL-3.0-or-later
"""What every app shares: the session option and a clean way out."""

from __future__ import annotations

import argparse
import sys
from collections.abc import Callable, Sequence

from ptyrc import Pilot, PtyrcError

__all__ = ["frame_arg", "parse", "parser", "run"]


def parser(prog: str, description: str) -> argparse.ArgumentParser:
    """An argument parser with the options common to all apps."""
    out = argparse.ArgumentParser(prog=f"ptyrc-app {prog}", description=description)
    out.add_argument(
        "-s",
        "--session",
        help="session to draw on [default: $PTYRC_SESSION, else the only one]",
    )
    out.add_argument(
        "-f",
        "--frame",
        default="raw",
        help="frame id or name: raw, active, largest, quietest [default: raw]",
    )
    return out


def run(
    app: Callable[[Pilot, argparse.Namespace], None],
    args: argparse.Namespace,
) -> int:
    """Connect, run `app`, leave quietly.

    Sprites belong to the connection: when the app returns, is interrupted
    or crashes, the driver removes what it drew. Nothing to clean up.
    """
    try:
        pilot = Pilot(args.session)
    except PtyrcError as exc:
        print(f"ptyrc-app: {exc}", file=sys.stderr)
        return 1
    try:
        with pilot:
            app(pilot, args)
    except KeyboardInterrupt:
        pass
    except PtyrcError as exc:
        if exc.code != "gone":  # the session ended: a normal way to stop
            print(f"ptyrc-app: {exc}", file=sys.stderr)
            return 1
    return 0


def frame_arg(value: str) -> int | str:
    """`-f 3` is an id, `-f active` a name."""
    return int(value) if value.isdigit() else value


def parse(
    out: argparse.ArgumentParser, argv: Sequence[str] | None
) -> argparse.Namespace:
    args = out.parse_args(argv)
    args.frame = frame_arg(args.frame)
    return args
