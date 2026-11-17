# SPDX-License-Identifier: GPL-3.0-or-later
"""Ask: a question in a widget, the answer on stdout.

    ptyrc-app ask "Deploy?" yes no later     # a choice
    ptyrc-app ask -m "Which?" a b c          # some of them, one a line
    ptyrc-app ask -o "Which?" a b            # or another, typed
    ptyrc-app ask "Name?"                    # a line
    ptyrc-app ask -t "Why?"                  # several lines

The widget waits: the user focuses it (ctrl+t twice), answers, `enter`
validates. Status 0 and the answer on stdout; 1 and nothing when it is
dismissed (ctrl+x), timed out or the session ended; 3 when no session
is there.

`model(args)` is the `ptyrc.ui` model; `ptyrc.ui.ask` runs it.
"""

from __future__ import annotations

import argparse
import json
import sys
from collections.abc import Sequence
from typing import Any

from ptyrc import Pilot, PtyrcError, ui

from . import _cli, _widgets
from .monitor import _seconds

__all__ = ["main", "model", "options", "written"]


def model(args: argparse.Namespace) -> ui.Model:
    """The model the arguments name: a `Choice` with options, else a
    `Text` (`--text`) or a `Line`."""
    default = args.default or ""
    if args.options:
        choice = ui.Choice(
            args.question, args.options, multi=args.multi, other=args.other
        )
        if default in args.options:
            choice.listing.sel = args.options.index(default)
            if args.multi:
                choice.ticked.add(choice.listing.sel)
        return choice
    if args.text:
        return ui.Text(args.question, default=default)
    return ui.Line(args.question, default=default)


def written(result: Any, as_json: bool = False) -> str:
    """The answer as printed: itself, a line an option for a list;
    `as_json`: one JSON value."""
    if as_json:
        return json.dumps(result, ensure_ascii=False)
    return "\n".join(result) if isinstance(result, list) else str(result)


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser("ask", "A question in a widget; the answer on stdout.")
    out.add_argument("question", metavar="QUESTION", help="what is asked")
    out.add_argument(
        "options",
        nargs="*",
        metavar="OPTION",
        help="the choices; none: a line is typed",
    )
    out.add_argument("-m", "--multi", action="store_true", help="several options")
    out.add_argument("-o", "--other", action="store_true", help="a last option, typed")
    out.add_argument("-t", "--text", action="store_true", help="several lines typed")
    out.add_argument(
        "--default", metavar="TEXT", help="the text to start from, or the option"
    )
    out.add_argument("--title", metavar="TITLE", help="on the border [default: ask]")
    out.add_argument(
        "--timeout",
        type=_seconds,
        metavar="SECONDS",
        help="give up after this long [default: never]",
    )
    out.add_argument("--json", action="store_true", help="the answer as JSON")
    _widgets.options(out, border="round", prefer="center")
    return _cli.parse(out, argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = options(argv)
    try:
        pilot = Pilot(args.session)
    except PtyrcError as exc:
        print(f"ptyrc-app: {exc}", file=sys.stderr)
        return 3
    show: dict[str, Any] = {
        "title": args.title,
        "border": args.border,
        "prefer": args.prefer,
        "frame": args.frame,
        "layer": "front" if args.front else None,
    }
    try:
        with pilot:
            result = ui.ask(pilot, model(args), timeout=args.timeout, **show)
    except KeyboardInterrupt:
        return 1
    except PtyrcError as exc:
        if exc.code == "gone":
            return 1
        print(f"ptyrc-app: {exc}", file=sys.stderr)
        return 3
    if result is None:
        return 1
    print(written(result, args.json))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
