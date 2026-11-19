# SPDX-License-Identifier: GPL-3.0-or-later
"""Each app reads no more of the screen than the catalog says (`READS`),
and needs nothing it does not declare (`ASKS`): its requests, the files
it opens, the programs it runs and the wall clock, observed."""

from __future__ import annotations

import importlib
import inspect
import os
import shutil
import sys
import sysconfig
import threading
import time
from collections.abc import Iterator
from pathlib import Path
from types import FrameType, ModuleType
from typing import Any

import pytest
from ptyrc import Pilot
from ptyrc_apps import APPS, _catalog, _cli, cat

ROOT = Path(__file__).resolve().parents[4]
PACKAGE = Path(_catalog.__file__).parent

#: Requests that read characters.
TEXT = {"screen.text", "screen.cells", "screen.find", "frame.text", "session.info"}
#: Requests and topics that read the shape, or when the text changes.
SHAPE = {"screen.shape", "frame.heat", "frame.idle", "screen.wait", "screen.cursor"}
TOPICS = {"text", "screen", "idle", "cursor", "colors"}
#: Requests and topics of the frames as rectangles.
LAYOUT = {"frame.list", "frame.get", "frame.at"}
LAYOUT_TOPICS = {"frames"}
#: Open to every app (`grants::METHODS`); `input.grab` is told below.
OPEN = {"auth", "batch", "subscribe", "input.release", "input.grab", "app.list"}
OPEN |= {f"draw.{m}" for m in ("put", "update", "remove", "clear", "list", "ext")}
OPEN |= {"draw.exts"}
OPEN |= {f"widget.{m}" for m in ("declare", "show", "set", "hide", "ask", "release")}
OPEN |= {"widget.list", "widget.borders"}
OPEN_TOPICS = {"draw", "size", "widgets", "move", "exit"}
#: Topics of a permission.
ASKED_TOPICS = {"input": "keys-read", "mouse": "clicks", "hover": "pointer"}
#: Named keys that are not text: a grab of these alone is `keys-arrows`.
NOT_TEXT = {"up", "down", "left", "right", "home", "end", "pageup", "pagedown"}
NOT_TEXT |= {"insert", "delete"} | {f"f{n}" for n in range(1, 13)}
#: Options of the user's hand (`plan/store-model.md`, principle 4).
USER_HAND = {"--front"}

#: Arguments the apps are run with, besides those `random app` draws.
EXTRA: dict[str, list[list[str]]] = {
    "ask": [["Q?", "a", "b", "--timeout", "1"]],
    "confetti": [["--one-shot"], ["--one-shot", "-a"]],
    "flower": [[], ["--word", "WORDS"], ["--word", "of a"], ["-a"]],
    "marks": [["food"]],
    "note": [[str(ROOT / "AGENTS.md")]],
}
#: The options that need more, each run once; `{pic}`, `{dir}`: a
#: picture, a directory of one; `{tmp}`: a directory of the test.
OPTIONS: list[tuple[str, list[str]]] = [
    ("ask", ["Q?", "a", "b", "--front", "--timeout", "1"]),
    ("bricks", ["--grab"]),
    ("cat", ["--hunt"]),
    ("cat", ["-a"]),
    ("clock", ["--tz", "Europe/Paris"]),
    ("gallery", ["--file", "{pic}"]),
    ("gallery", ["--dir", "{dir}"]),
    ("life", ["--cursor"]),
    ("marks", ["--no-hud", "food"]),
    ("pace", ["--save", "{tmp}/counts.json"]),
    ("sand", ["--bomb", "--now"]),
    ("snake", ["--grab"]),
    ("snow", ["--hover"]),
    ("timer", ["1s", "--modal"]),
    ("timer", ["5m", "--click"]),
]


def runs() -> Iterator[tuple[str, list[str]]]:
    for name in APPS:
        drawn = [list(v) for v in _catalog.RANDOM.get(name, ((),))]
        for args in EXTRA.get(name, drawn):
            yield name, args
    yield from OPTIONS


def options(name: str) -> dict[str, list[str]]:
    """Each flag of an app, to the flags of its option."""
    parser = _catalog.parser_of(name)
    return {f: a.option_strings for a in parser._actions for f in a.option_strings}


def test_reads_are_declared() -> None:
    assert set(_catalog.READS) <= set(APPS)
    for name, reads in _catalog.READS.items():
        assert reads.degree in _catalog.DEGREES
        assert bool(reads.words) == (reads.degree == "words"), name
        assert all(map(_cli.is_word, reads.words)), name
        assert reads.why, name
        flags = options(name)
        assert set(reads.options) | set(reads.user_words) <= set(flags), name
        assert all(flags[f][0] == f for f in [*reads.options, *reads.user_words])
    assert _catalog.READS["cat"].words == cat.WORDS


def test_asks_are_declared() -> None:
    assert set(_catalog.ASKS) | set(_catalog.WHOLE) <= set(APPS)
    for name, asks in _catalog.ASKS.items():
        flags = options(name)
        assert set(asks.options) <= set(flags), name
        assert all(flags[f][0] == f for f in asks.options), name
        assert asks.always or asks.options, name
        for needs in [asks.always, *asks.options.values()]:
            assert set(needs) <= set(_catalog.PERMISSIONS), name
            assert all(needs.values()), name


def test_declares() -> None:
    def names(name: str, *args: str) -> list[str]:
        return list((_catalog.declares(name, args) or {})["permissions"])

    assert names("ask", "Q?") == []
    assert names("snow") == ["layout"]
    assert names("snake") == ["shape"]
    assert names("snake", "--grab") == ["shape", "keys-arrows"]
    assert names("snake", "--grab", "--speed", "2", "-f", "idle") == names(
        "snake", "--grab"
    )
    flower = _catalog.declares("flower", ["--word", "Rose"])
    assert flower is not None and flower["words"] == ["flower", "rose"]
    assert list(flower["permissions"]) == ["shape", "words", "front"]
    assert names("flower", "-a") == ["shape", "words", "text", "front"]
    assert names("pace") == ["shape", "text", "keys-read", "clock"]
    assert names("timer", "5m", "--click") == ["clicks"]
    # Words it cannot parse: the options as written.
    assert names("ask", "--front") == []
    assert _catalog.declares("nope", []) is None


#: What the run of an app did beyond its requests: a permission each.
_SEEN: list[set[str]] = []
_SKIP = tuple(
    {sys.prefix, sys.base_prefix, *sysconfig.get_paths().values()}
    | {str(PACKAGE), str(ROOT / "python")}
)


def _in_apps(frame: FrameType | None) -> set[str]:
    """The modules of `ptyrc_apps` on the stack."""
    out = set()
    while frame is not None:
        module = frame.f_globals.get("__name__", "")
        if module.startswith("ptyrc_apps."):
            out.add(module)
        frame = frame.f_back
    return out


def _audit(event: str, args: tuple[Any, ...]) -> None:
    if not _SEEN or event not in (
        "open",
        "os.listdir",
        "os.scandir",
        "subprocess.Popen",
    ):
        return
    modules = _in_apps(sys._getframe(1))
    path = args[0]
    if not modules or not isinstance(path, str | bytes | os.PathLike):
        return
    path = os.fsdecode(path)
    if "ptyrc_apps._gitprobe" in modules:
        _SEEN[0].add("probe-git")
    elif "ptyrc_apps._probe" in modules:
        _SEEN[0].add("probe-system")
    elif event == "subprocess.Popen":
        _SEEN[0].add(f"run {path}")
    elif "zoneinfo" in path:
        _SEEN[0].add("clock")
    elif path.startswith(("/proc/", "/sys/", "/run/", "/var/run/")):
        _SEEN[0].add("probe-system")
    elif path.startswith("/dev/") or path.startswith(_SKIP):
        pass
    else:
        mode, flags = (args[1] or "", args[2] or 0) if event == "open" else ("", 0)
        write = any(c in mode for c in "wax+") or flags & (os.O_WRONLY | os.O_RDWR)
        _SEEN[0].add("files-write" if write else "files")


sys.addaudithook(_audit)

#: The wall clock: `(owner, function)` of the C calls that read it.
WALL = {
    ("time", "time"),
    ("time", "time_ns"),
    ("time", "localtime"),
    ("time", "ctime"),
    ("datetime", "now"),
    ("datetime", "today"),
    ("datetime", "utcnow"),
    ("date", "today"),
}


def _profile(frame: FrameType, event: str, arg: Any) -> None:
    module = frame.f_globals.get("__name__", "")
    if event != "c_call" or not _SEEN or not module.startswith("ptyrc_apps."):
        return
    owner = getattr(arg, "__self__", None)
    name = getattr(arg, "__name__", "")
    if isinstance(owner, type | ModuleType) and (owner.__name__, name) in WALL:
        _SEEN[0].add("clock")


def needs(sent: list[tuple[str, dict[str, Any]]], args: list[str]) -> set[str]:
    """The permissions beyond reading that the requests `sent` need, as
    `grants::gate` tells them; `never ...`: a method closed to apps."""
    out = set()
    hand = bool(USER_HAND & set(args))
    for method, params in sent:
        if method not in OPEN | TEXT | SHAPE | LAYOUT:
            out.add(f"never {method}")
        if method == "input.grab":
            keys = params.get("keys") or []
            arrows = keys and all(k in NOT_TEXT for k in keys)
            out.add("keys-arrows" if arrows else "keys-grab")
        if method == "widget.show" and params.get("modal") is True:
            out |= {"keys-grab", "front-widget"}
        for topic in params.get("topics", ()) if method == "subscribe" else ():
            if topic in ASKED_TOPICS:
                out.add(ASKED_TOPICS[topic])
            elif topic not in OPEN_TOPICS | TOPICS | LAYOUT_TOPICS:
                out.add(f"never {topic}")
        layer = params.get("layer")
        tint = params.get("blend") in ("tint", "over") and layer is None
        front = layer in ("front", "hud") or (type(layer) is int and layer > 0)
        if (front or tint) and not hand:
            out.add("front-widget" if method.startswith("widget.") else "front")
    return out


def placed(args: list[str], tmp_path: Path) -> list[str]:
    """`args`, its places filled in `tmp_path`."""
    pictures = tmp_path / "pictures"
    pictures.mkdir()
    shutil.copy(PACKAGE / "pictures" / "banana.pic", pictures)
    places = {"pic": pictures / "banana.pic", "dir": pictures, "tmp": tmp_path}
    return [a.format(**places) for a in args]


@pytest.mark.parametrize(("name", "args"), list(runs()))
def test_reads(
    name: str,
    args: list[str],
    session: Pilot,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    args = placed(args, tmp_path)
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
    _SEEN.append(set())
    threading.setprofile_all_threads(_profile)
    try:
        main(["-s", session.session, *args], **kw)
    finally:
        threading.setprofile_all_threads(None)
        done = _SEEN.pop()
    degree, words = _catalog.reads_of(name, args)
    methods = {method for method, _ in sent}
    topics = {t for m, p in sent if m == "subscribe" for t in p.get("topics", ())}
    if degree != "text":
        assert not methods & TEXT, (degree, methods & TEXT)
        for method, params in sent:
            if method == "screen.shape":
                assert {w.lower() for w in params.get("words", ())} <= words
    if degree in ("none", "layout"):
        assert not methods & SHAPE and not topics & TOPICS, (methods, topics)
    if degree == "shape":
        assert all(not p.get("words") for m, p in sent if m == "screen.shape")
    if degree == "none":
        assert not methods & LAYOUT and not topics & LAYOUT_TOPICS, (methods, topics)
    # What is declared is used: a degree is not set higher than needed.
    if degree in ("shape", "words") and not methods & SHAPE:
        assert topics & TOPICS, (name, args, methods)
    if degree == "layout":
        assert methods & LAYOUT or topics & LAYOUT_TOPICS, (name, args, methods)
    # Beyond reading: declared, as `ptyrc-app --declares` gives it.
    declared = _catalog.declares(name, args)
    held = (
        set(_catalog.PERMISSIONS) if declared is None else set(declared["permissions"])
    )
    assert needs(sent, args) | done <= held, (needs(sent, args) | done) - held


@pytest.mark.parametrize(("name", "args"), list(runs()))
def test_started_as_declared(
    name: str, args: list[str], session: Pilot, tmp_path: Path
) -> None:
    """Started by the driver with its declaration, the app holds that
    alone (`grants::check`), and is refused nothing."""
    args = placed(args, tmp_path)
    session.send("clear; echo 'food fish_toy flower WORDS of a'; echo; echo t''oy\n")
    session.wait_for("toy")
    argv = [sys.executable, "-m", "ptyrc_apps", name, *args]
    declares = _catalog.declares(name, args)
    app = session.app_start(argv, name=name, declares=declares)
    time.sleep(1.5)
    info = next(a for a in session.apps() if a["id"] == app["id"])
    session.app_stop(app["id"])
    log = Path(info["log"]).read_text() if info.get("log") else ""
    assert "denied" not in log and "Traceback" not in log, log[-800:]
