# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc-app --catalog`: the apps and their options, as JSON.

Read by `ptyrc manage` to build its menu. The options come from the
argparse parser of each app: its `main` is run up to `_cli.parse`, so
nothing connects. `reads`: what the app reads of the screen (`READS`);
`asks`: what else it needs (`ASKS`). `ptyrc-app --declares NAME ARGS`:
the `declares` of `app.start` for those arguments (`declares`).
"""

from __future__ import annotations

import argparse
import contextlib
import importlib
import io
import json
from collections.abc import Sequence
from dataclasses import dataclass, field
from typing import Any

from . import APPS, _cli, _widgets

__all__ = [
    "ASKS",
    "DEGREES",
    "PERMISSIONS",
    "RANDOM",
    "READS",
    "WHOLE",
    "Asks",
    "Reads",
    "catalog",
    "declares",
    "dump",
    "given",
    "parser_of",
    "reads_of",
]

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
#: "Degrees"). `none`: no cell, nor when they change; `layout`: the
#: frames as rectangles; `shape`: where there is ink (`Frame.shape`),
#: the heat, idle, the cursor, when the text changes; `words`: `shape`,
#: and the cells of its `words` as shown; `text`: the characters.
DEGREES = ("none", "layout", "shape", "words", "text")

#: The degrees declared for a degree: each a yes or no of its own, the
#: lower kept when a higher is denied. `layout` is in `shape`.
RUNGS = {
    "none": (),
    "layout": ("layout",),
    "shape": ("shape",),
    "words": ("shape", "words"),
    "text": ("shape", "words", "text"),
}


@dataclass(frozen=True)
class Reads:
    """What an app reads of the screen. `options`: an option (its first
    flag) and the degree the app reads with it, higher or lower; with
    several, the highest. `user_words`: options
    whose values are the user's words, read as `words` are; a value that
    is not a word of `Frame.shape` (`_cli.is_word`) is read in the text.
    `why`: the reason the pop-up shows, for every degree."""

    degree: str
    words: tuple[str, ...] = ()
    options: dict[str, str] = field(default_factory=dict)
    user_words: tuple[str, ...] = ()
    why: str = ""

    def wire(self) -> dict[str, Any]:
        return {
            "degree": self.degree,
            "words": list(self.words),
            "options": dict(self.options),
            "user_words": list(self.user_words),
            "why": self.why,
        }


#: Per app; an app not named here reads `none`.
READS: dict[str, Reads] = {
    "bricks": Reads("shape", why="the text is the wall the balls break"),
    "cat": Reads(
        "words",
        ("food", "fish", "toy"),
        why="where the text is, to walk on it; its words, to find its things",
    ),
    "confetti": Reads("shape", why="the heat: it lands where the text is still"),
    "fishes": Reads(
        "shape",
        options={"--tank": "none"},
        why="where the text and the cursor are, to swim around them",
    ),
    "flock": Reads("shape", why="where the text is, to perch on it"),
    "flower": Reads(
        "words",
        ("flower",),
        {"-a": "text"},
        ("--word",),
        why="where its word is, to grow next to it (-a: any word)",
    ),
    "life": Reads("shape", why="where the text is: no cell lives on it"),
    "marks": Reads("text", why="the text, the remembered rows too: the patterns"),
    "minimap": Reads("text", why="the text the frames remember, drawn small"),
    "pace": Reads("text", why="what is typed against what is shown"),
    "pond": Reads("shape", why="where the text is: the fish swim around it"),
    "rain": Reads("layout", why="the frame it falls in"),
    "sand": Reads("layout", why="the frame it falls in"),
    "shore": Reads("shape", why="where the text is: the waves break on it"),
    "snake": Reads("shape", why="the text is the wall"),
    "snow": Reads("layout", why="the frame it falls in"),
    "stars": Reads("shape", why="where the text is: the sky is around it"),
}

#: The permissions beyond reading, as the driver names them
#: (`grants::Permission`).
PERMISSIONS = (
    "front-widget",
    "front",
    "screen-wide",
    "keys-arrows",
    "keys-grab",
    "keys-read",
    "pointer",
    "clicks",
    "clicks-grab",
    "clock",
    "storage",
    "files",
    "files-write",
    "probe-git",
    "probe-system",
    "limit-memory",
    "limit-time",
)


@dataclass(frozen=True)
class Asks:
    """What an app needs beyond reading: a permission (`PERMISSIONS`) to
    its reason, which the pop-up shows. `options`: an option (its first
    flag) to what it needs more."""

    always: dict[str, str] = field(default_factory=dict)
    options: dict[str, dict[str, str]] = field(default_factory=dict)

    def wire(self) -> dict[str, Any]:
        return {"always": dict(self.always), "options": dict(self.options)}


GRAB = {"keys-arrows": "the arrows steer it, until ctrl+t"}

#: Per app; an app not named here needs nothing more. As measured
#: (`tests/test_reads.py`).
ASKS: dict[str, Asks] = {
    "bricks": Asks(options={"--grab": GRAB}),
    "cat": Asks(
        options={
            "--hunt": {"pointer": "the cats hunt the pointer"},
            "-a": {"pointer": "the cats hunt the pointer"},
        }
    ),
    "clock": Asks({"clock": "the time of day, in its zone"}),
    "flock": Asks({"pointer": "the birds take flight from the pointer"}),
    "flower": Asks({"front": "its word redrawn in bold, over the text"}),
    "gallery": Asks(
        options={
            "--file": {"files": "the pictures named"},
            "--dir": {"files": "the pictures of the directories named"},
        }
    ),
    "git": Asks(
        {
            "probe-git": "the state of the repository: runs git",
            "clock": "the dates of the commits, in the local zone",
        }
    ),
    "life": Asks(
        options={
            "--cursor": {"pointer": "the pointer clears the cells near it"},
            "-a": {"pointer": "the pointer clears the cells near it"},
        }
    ),
    "marks": Asks({"front": "the matches tinted over the text; the counts"}),
    "monitor": Asks(
        {
            "probe-system": "the processes, the sessions, the machine",
            "clock": "the times of the logins",
        }
    ),
    "note": Asks(
        {
            "files": "the files named",
            "files-write": "a box ticked is written back to its file",
        }
    ),
    "pace": Asks(
        {
            "keys-read": "every key typed, against what is shown",
            "clock": "the dates of the words counted",
        },
        {
            "--save": {
                "files": "the file named keeps the counts",
                "files-write": "the file named keeps the counts",
            },
        },
    ),
    "pond": Asks(
        {
            "pointer": "the fish flee the pointer",
            "clock": "the moon and the season of today",
        }
    ),
    "sand": Asks(
        options={
            flag: {
                "clicks": "a click on the pile blows it up",
                "clicks-grab": "those clicks do not reach the program",
            }
            for flag in ("--bomb", "-a")
        }
    ),
    "shore": Asks(options={"-a": {"pointer": "the pointer on the water"}}),
    "snake": Asks(options={"--grab": GRAB}),
    "snow": Asks(
        options={
            flag: {"pointer": "the pointer takes the snow it is over"}
            for flag in ("--hover", "-a")
        }
    ),
    "stars": Asks(
        {"pointer": "the stars under the pointer", "clock": "the phase of the moon"}
    ),
    "timer": Asks(
        options={
            "--modal": {
                "keys-grab": "at the end, a modal: it takes the keys",
                "front-widget": "at the end, a modal: over the text",
            },
            "--until": {"clock": "the time of day it ends at"},
        }
    ),
    "train": Asks({"clock": "the time of day and the season"}),
    "window": Asks({"clock": "the time of day and the season"}),
}

#: Apps that hold the whole session, and why: started with no
#: declaration. None of these: `manage`, the driver's own, is not one
#: of `ptyrc-app`.
WHOLE: dict[str, str] = {}

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
        "asks": ASKS.get(name, Asks()).wire(),
    }


def given(name: str, args: Sequence[str]) -> set[str]:
    """The options of `name` (each its first flag) that `args` gives: as
    its parser reads them; words it does not parse, as written."""
    parser = parser_of(name)
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            space: argparse.Namespace | None = parser.parse_known_args(args)[0]
    except SystemExit:
        space = None
    out = set()
    for action in parser._actions:
        flags = action.option_strings
        if not flags:
            continue
        if space is not None:
            hit = getattr(space, action.dest, action.default) != action.default
        else:
            hit = any(a in flags or a.split("=")[0] in flags for a in args)
        if hit:
            out.add(flags[0])
    return out


def reads_of(name: str, args: Sequence[str]) -> tuple[str, set[str]]:
    """The degree and the words `args` take `name` to, by its `READS`."""
    reads = READS.get(name, Reads("none"))
    words = {w.lower() for w in reads.words}
    on = given(name, args)
    taken = [d for f, d in reads.options.items() if f in on]
    degree = max(taken, key=DEGREES.index) if taken else reads.degree
    if reads.user_words:
        flags = {
            f: a.option_strings
            for a in parser_of(name)._actions
            for f in a.option_strings
        }
        for i, arg in enumerate(args):
            for flag in reads.user_words:
                key, eq, value = arg.partition("=")
                if (arg if not eq else key) not in flags[flag]:
                    continue
                word = value if eq else (args[i + 1] if i + 1 < len(args) else "")
                if not _cli.is_word(word):
                    degree = "text"
                words.add(word.lower())
    return degree, words


def declares(name: str, args: Sequence[str]) -> dict[str, Any] | None:
    """The `declares` of `app.start` for `ptyrc-app NAME ARGS`: each
    permission with its reason, and the words. None: the whole session
    (`WHOLE`, or no such app)."""
    if name not in APPS or name in WHOLE:
        return None
    degree, words = reads_of(name, args)
    why = READS.get(name, Reads("none")).why
    rungs = [r for r in RUNGS[degree] if r != "words" or words]
    permissions = dict.fromkeys(rungs, why)
    asks = ASKS.get(name, Asks())
    permissions.update(asks.always)
    for flag in sorted(given(name, args) & set(asks.options)):
        permissions.update(asks.options[flag])
    listed = sorted(words) if "words" in permissions else []
    return {"permissions": permissions, "words": listed}


def catalog() -> dict[str, Any]:
    return {"apps": [describe(name) for name in APPS]}


def dump() -> str:
    return json.dumps(catalog(), ensure_ascii=False)
