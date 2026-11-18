# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc-app --catalog`: the apps and their options, as JSON.

Read by `ptyrc manage` to build its menu. The options come from the
argparse parser of each app: its `main` is run up to `_cli.parse`, so
nothing connects. `reads`: what the app reads of the screen (`READS`).
"""

from __future__ import annotations

import argparse
import importlib
import json
from dataclasses import dataclass, field
from typing import Any

from . import APPS, _cli, _widgets

__all__ = ["DEGREES", "RANDOM", "READS", "Reads", "catalog", "dump", "parser_of"]

#: Argument lists `random app` draws from, a tuple per version; none:
#: never drawn. An app not named here is drawn bare.
RANDOM: dict[str, tuple[tuple[str, ...], ...]] = {
    "ask": (),  # it wants a question
    "bricks": ((), ("-a",)),
    "confetti": ((), ("-a",)),
    "fire": ((), ("-a",)),
    "fishes": ((), ("-a",), ("--tank",), ("--tank", "-a")),
    "flock": ((), ("-a",)),
    "flower": (("-a",),),  # bare, it wants its word on the screen
    "gallery": ((), ("-a",)),
    "git": ((), ("-a",)),
    "globe": ((), ("-a",)),
    "life": ((), ("-a",)),
    "marks": (),  # it wants patterns
    "minimap": ((), ("-a",)),
    "note": (),  # it wants a file: TODO.md of the working directory
    "pace": ((), ("-a",)),
    "pond": ((), ("-a",)),
    "sand": (("--now",),),
    "shore": ((), ("-a",)),
    "snake": ((), ("-a",)),
    "stars": ((), ("-a",)),
    "timer": (("5m",), ("5m", "-a")),
    "train": ((), ("-a",)),
    "window": ((), ("-a",)),
}

#: What an app may read of the screen, least first (`plan/store-model.md`,
#: "Degrees"). `none`: no cell, nor when they change; `shape`: where
#: there is ink (`Frame.shape`), the heat, idle, the cursor, when the
#: text changes; `words`: `shape`, and the cells of its `words` as shown;
#: `text`: the characters.
DEGREES = ("none", "shape", "words", "text")


@dataclass(frozen=True)
class Reads:
    """What an app reads of the screen. `options`: an option (its first
    flag) and the degree the app reads with it, higher or lower; with
    several, the highest. `user_words`: options
    whose values are the user's words, read as `words` are; a value that
    is not a word of `Frame.shape` (`_cli.is_word`) is read in the text."""

    degree: str
    words: tuple[str, ...] = ()
    options: dict[str, str] = field(default_factory=dict)
    user_words: tuple[str, ...] = ()

    def wire(self) -> dict[str, Any]:
        return {
            "degree": self.degree,
            "words": list(self.words),
            "options": dict(self.options),
            "user_words": list(self.user_words),
        }


#: Per app; an app not named here reads `none`.
READS: dict[str, Reads] = {
    "bricks": Reads("shape"),
    "cat": Reads("words", ("food", "fish", "toy")),
    "confetti": Reads("shape"),  # the heat
    "fishes": Reads("shape", options={"--tank": "none"}),
    "flock": Reads("shape"),
    "flower": Reads("words", ("flower",), {"-a": "text"}, ("--word",)),
    "life": Reads("shape"),
    "marks": Reads("text"),  # the user's patterns, regex, remembered rows
    "minimap": Reads("text"),  # remembered rows, and a glyph is a dot
    "pace": Reads("text"),  # what is typed against what is shown
    "pond": Reads("shape"),
    "shore": Reads("shape"),
    "snake": Reads("shape"),
    "stars": Reads("shape"),
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


def _widgets_only(parser: argparse.ArgumentParser) -> bool:
    """Whether the app is made of widgets: `_widgets.options` on the
    parser itself, not on a group of it (`fishes --tank`)."""
    return any(
        action.help is _widgets.FRONT and action.container is parser._optionals
        for action in parser._actions
    )


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
        "widgets": _widgets_only(parser),
        "reads": READS.get(name, Reads("none")).wire(),
    }


def catalog() -> dict[str, Any]:
    return {"apps": [describe(name) for name in APPS]}


def dump() -> str:
    return json.dumps(catalog(), ensure_ascii=False)
