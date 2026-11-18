# SPDX-License-Identifier: GPL-3.0-or-later
"""Each app reads no more of the screen than the catalog says (`READS`)."""

from __future__ import annotations

import importlib
import inspect
import time
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import APPS, _catalog, _cli, cat

ROOT = Path(__file__).resolve().parents[4]

#: Requests that read characters.
TEXT = {"screen.text", "screen.cells", "screen.find", "frame.text"}
#: Requests and topics that read the shape, or when the text changes.
SHAPE = {"screen.shape", "frame.heat", "frame.idle", "screen.wait", "screen.cursor"}
TOPICS = {"text", "screen", "idle", "cursor"}

#: Arguments the apps are run with, besides those `random app` draws.
EXTRA: dict[str, list[list[str]]] = {
    "ask": [["Q?", "a", "b", "--timeout", "1"]],
    "confetti": [["--one-shot"], ["--one-shot", "-a"]],
    "flower": [[], ["--word", "WORDS"], ["--word", "of a"], ["-a"]],
    "marks": [["food"]],
    "note": [[str(ROOT / "AGENTS.md")]],
}


def runs() -> Iterator[tuple[str, list[str]]]:
    for name in APPS:
        drawn = [list(v) for v in _catalog.RANDOM.get(name, ((),))]
        for args in EXTRA.get(name, drawn):
            yield name, args


def options(name: str) -> dict[str, list[str]]:
    """Each flag of an app, to the flags of its option."""
    parser = _catalog.parser_of(name)
    return {f: a.option_strings for a in parser._actions for f in a.option_strings}


def allowed(name: str, args: list[str]) -> tuple[str, set[str]]:
    """The degree and the words `args` take `name` to, by its `READS`."""
    reads = _catalog.READS.get(name, _catalog.Reads("none"))
    flags = options(name)
    words = {w.lower() for w in reads.words}
    taken = [d for f, d in reads.options.items() if set(args) & set(flags[f])]
    degree = max(taken, key=_catalog.DEGREES.index) if taken else reads.degree
    for i, arg in enumerate(args):
        for flag in reads.user_words:
            if arg in flags[flag]:
                word = args[i + 1]
                if not _cli.is_word(word):
                    degree = "text"
                words.add(word.lower())
    return degree, words


def test_reads_are_declared() -> None:
    assert set(_catalog.READS) <= set(APPS)
    for name, reads in _catalog.READS.items():
        assert reads.degree in _catalog.DEGREES
        assert bool(reads.words) == (reads.degree == "words"), name
        assert all(map(_cli.is_word, reads.words)), name
        flags = options(name)
        assert set(reads.options) | set(reads.user_words) <= set(flags), name
        assert all(flags[f][0] == f for f in [*reads.options, *reads.user_words])
    assert _catalog.READS["cat"].words == cat.WORDS


@pytest.mark.parametrize(("name", "args"), list(runs()))
def test_reads(
    name: str, args: list[str], session: Pilot, monkeypatch: pytest.MonkeyPatch
) -> None:
    session.send("clear; echo 'food fish_toy flower WORDS of a'; echo; echo t''oy\n")
    session.wait_for("toy")
    time.sleep(0.2)
    sent: list[tuple[str, dict[str, Any]]] = []
    request = Pilot._request

    def record(
        self: Pilot, method: str, params: dict[str, Any], *a: Any, **kw: Any
    ) -> Any:
        if self is not session:
            sent.append((method, dict(params)))
        return request(self, method, params, *a, **kw)

    monkeypatch.setattr(Pilot, "_request", record)
    main = importlib.import_module(f"ptyrc_apps.{name}").main
    kw = {"seconds": 1.0} if "seconds" in inspect.signature(main).parameters else {}
    main(["-s", session.session, *args], **kw)
    degree, words = allowed(name, args)
    methods = {method for method, _ in sent}
    topics = {t for m, p in sent if m == "subscribe" for t in p.get("topics", ())}
    if degree != "text":
        assert not methods & TEXT, (degree, methods & TEXT)
        for method, params in sent:
            if method == "screen.shape":
                assert {w.lower() for w in params.get("words", ())} <= words
    if degree == "shape":
        assert all(not p.get("words") for m, p in sent if m == "screen.shape")
    if degree == "none":
        assert not methods & SHAPE and not topics & TOPICS, (methods, topics)
    # What is declared is used: a degree is not set higher than needed.
    if degree in ("shape", "words") and not methods & SHAPE:
        assert topics & TOPICS, (name, args, methods)
