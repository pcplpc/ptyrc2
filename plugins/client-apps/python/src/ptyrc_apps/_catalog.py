# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc-app --catalog`: the apps and their options, as JSON.

Read by `ptyrc manage` to build its menu. The options come from the
argparse parser of each app: its `main` is run up to `_cli.parse`, so
nothing connects.
"""

from __future__ import annotations

import argparse
import importlib
import json
from typing import Any

from . import APPS, _cli

__all__ = ["RANDOM", "catalog", "dump", "parser_of"]

#: Argument lists `random app` draws from, a tuple per version; none:
#: never drawn. An app not named here is drawn bare.
RANDOM: dict[str, tuple[tuple[str, ...], ...]] = {
    "bricks": ((), ("-a",)),
    "confetti": ((), ("-a",)),
    "fishes": ((), ("-a",)),
    "flower": (("-a",),),  # bare, it wants its word on the screen
    "life": ((), ("-a",)),
    "marks": (),  # it wants patterns
    "sand": (("--now",),),
    "snake": ((), ("-a",)),
}

#: Options of every app the menu does not show: help, the session.
HIDDEN = {"-h", "-s"}


class _Caught(Exception):
    def __init__(self, parser: argparse.ArgumentParser) -> None:
        super().__init__()
        self.parser = parser


def parser_of(name: str) -> argparse.ArgumentParser:
    """The parser of an app, taken where its `main` would parse."""

    def caught(out: argparse.ArgumentParser, argv: Any) -> argparse.Namespace:
        raise _Caught(out)

    module = importlib.import_module(f"ptyrc_apps.{name}")
    parse = _cli.parse
    _cli.parse = caught  # type: ignore[assignment]
    try:
        module.main([])
    except _Caught as caught_:
        return caught_.parser
    finally:
        _cli.parse = parse
    raise RuntimeError(f"{name}: main does not parse through _cli.parse")


def _plain(value: Any) -> Any:
    """A default as JSON: itself when plain, else null."""
    if value is None or isinstance(value, bool | int | float | str):
        return value
    return None


def _option(action: argparse.Action) -> dict[str, Any]:
    if isinstance(action, argparse._CountAction):
        kind = "count"
    elif action.nargs == 0:
        kind = "flag"
    elif action.choices is not None:
        kind = "choice"
    else:
        kind = "value"
    metavar = action.metavar if isinstance(action.metavar, str) else None
    if kind in ("value", "choice") and metavar is None:
        metavar = action.dest.upper()
    return {
        "flags": list(action.option_strings),
        "kind": kind,
        "metavar": metavar,
        "choices": None if action.choices is None else [str(c) for c in action.choices],
        "default": _plain(action.default),
        "help": action.help or "",
    }


def _positional(action: argparse.Action) -> dict[str, Any]:
    name = action.metavar if isinstance(action.metavar, str) else action.dest.upper()
    nargs = action.nargs
    return {
        "name": name,
        "nargs": nargs if nargs is None or isinstance(nargs, str) else int(nargs),
        "help": action.help or "",
    }


def describe(name: str) -> dict[str, Any]:
    parser = parser_of(name)
    options, positionals = [], []
    for action in parser._actions:
        if action.help is argparse.SUPPRESS:
            continue
        if not action.option_strings:
            positionals.append(_positional(action))
        elif not HIDDEN.intersection(action.option_strings):
            options.append(_option(action))
    return {
        "name": name,
        "help": APPS[name],
        "options": options,
        "positionals": positionals,
        "random": [list(args) for args in RANDOM.get(name, ((),))],
    }


def catalog() -> dict[str, Any]:
    return {"apps": [describe(name) for name in APPS]}


def dump() -> str:
    return json.dumps(catalog(), ensure_ascii=False)
