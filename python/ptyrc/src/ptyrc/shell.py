# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc python`: a Python session with `pilot` connected.

    ptyrc-python                  interactive
    ptyrc-python FILE [ARGS...]   run FILE with `pilot` defined

The session is that of `$PTYRC_SESSION`, else the only live one.
"""

from __future__ import annotations

import code
import runpy
import sys
from collections.abc import Sequence

import ptyrc

from ._wire import PtyrcError
from .pilot import Pilot

__all__ = ["main"]


def _readline(namespace: dict[str, object]) -> None:
    try:
        import readline
        import rlcompleter
    except ImportError:
        return
    readline.set_completer(rlcompleter.Completer(namespace).complete)
    readline.parse_and_bind("tab: complete")


def main(argv: Sequence[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if args[:1] in (["-h"], ["--help"]):
        print(__doc__.strip())
        return 0
    try:
        pilot = Pilot()
    except PtyrcError as e:
        print(f"ptyrc: {e.message}", file=sys.stderr)
        if e.hint:
            print(e.hint, file=sys.stderr)
        return 3
    namespace: dict[str, object] = {"pilot": pilot, "ptyrc": ptyrc}
    try:
        if args:
            sys.argv = args
            runpy.run_path(args[0], init_globals=namespace, run_name="__main__")
            return 0
        _readline(namespace)
        banner = f'Connected to "{" ".join(pilot.argv)}"'
        code.interact(banner=banner, local=namespace, exitmsg="")
        return 0
    finally:
        pilot.close()
