# SPDX-License-Identifier: GPL-3.0-or-later
"""The answers cut to a degree (design: Permissions): an app below
`text` is told nothing of what the screen says, by any method, topic,
error or hint; and it goes on."""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path
from typing import Any

import pytest
from ptyrc import Pilot
from tests_support import until

GRANTS = Path(__file__).parents[3] / "rust/ptyrc-rules/src/grants.rs"

#: On the screen, and in what the frames remember.
SECRETS = ("XYZZYQUUX", "PLOVERHOLLOW")
#: The declared word, on the screen too.
WORD = "fish"


def table(name: str) -> list[str]:
    """The names of `grants::METHODS` or `grants::TOPICS`."""
    source = GRANTS.read_text()
    body = source.split(f"pub const {name}:")[1].split("];")[0]
    return re.findall(r'\("([a-z.]+)",', body)


#: Asks everything, tells nothing by itself: what it was answered goes
#: to a file. The screen is told to it by no other way.
PROBE = """
import json, sys, time
from ptyrc import Pilot, PtyrcError
out = sys.argv[1]
with open(sys.argv[2]) as f:
    methods, topics, params = json.load(f)
pilot = Pilot()
said = {"hello": pilot.hello, "answers": {}, "events": []}
def ask(name, method, **given):
    try:
        said["answers"][name] = pilot.call(method, **given)
    except PtyrcError as err:
        said["answers"][name] = {"error": [err.code, str(err), err.hint]}
for method in methods:
    if method != "auth":
        ask(method, method, **params.get(method, {}))
for name, (method, given) in params.get("more", {}).items():
    ask(name, method, **given)
for topic in topics:
    ask("subscribe " + topic, "subscribe", topics=[topic], snapshot=True)
pilot.draw(["PROBE READY"], at=(1, 12), space="view")
end = time.monotonic() + 2.5
while time.monotonic() < end:
    for topic, data in pilot.events(timeout=0.2):
        said["events"].append([topic, data])
        if topic == "exit":
            end = 0
with open(out + ".tmp", "w") as f:
    json.dump(said, f)
import os
os.rename(out + ".tmp", out)
for _ in pilot.events(timeout=None):
    pass
"""

PARAMS: dict[str, Any] = {
    "screen.find": {"pattern": SECRETS[0]},
    "screen.wait": {"pattern": SECRETS[0], "timeout_ms": 100},
    "screen.cursor": {"trail": True},
    "frame.at": {"x": 2, "y": 0},
    "frame.get": {"frame": "largest"},
    "frame.text": {"frame": "largest"},
    "draw.put": {"content": {"rows": ["x"]}, "at": [0, 0], "frame": "nowhere"},
    "draw.remove": {"id": 99999},
    "widget.hide": {"id": 99999},
    "batch": {
        "calls": [
            {"method": "screen.text", "params": {"ansi": True, "sprites": True}},
            {"method": "screen.find", "params": {"pattern": ".", "regex": True}},
            {"method": "frame.get", "params": {"frame": "no-such"}},
        ]
    },
    "more": {
        "find word": ["screen.find", {"pattern": WORD, "known": True}],
        "find regex": ["screen.find", {"pattern": "[A-Z]+", "regex": True}],
        "shape words": ["screen.shape", {"words": [WORD, SECRETS[0], SECRETS[1]]}],
        "wait word": ["screen.wait", {"pattern": WORD, "timeout_ms": 100}],
        "wait still": ["screen.wait", {"stable_ms": 1, "timeout_ms": 100}],
        "wait idle": ["screen.wait", {"idle": True, "timeout_ms": 100}],
        "cells": ["screen.cells", {"sprites": True}],
        "text of a frame": ["screen.text", {"frame": "no-such-frame"}],
    },
}


def probe(shell: Pilot, tmp_path: Path, declares: dict[str, Any]) -> dict[str, Any]:
    """What an app that declares `declares` is told of a screen that
    shows the secrets and the word."""
    shell.send(f"echo {SECRETS[0]} {WORD} my_{WORD}\n")
    shell.wait_for(f"{SECRETS[0]} {WORD}")
    out = tmp_path / "said.json"
    methods, topics = table("METHODS"), table("TOPICS")
    assert "screen.text" in methods and "text" in topics
    # Another's sprite: not listed to it, not touched by it.
    other = shell.draw(["OTHERS"], at=(1, 14), space="view")
    more = {
        "update another's": ["draw.update", {"id": other.id, "z": 3}],
        "remove another's": ["draw.remove", {"id": other.id}],
    }
    params = {**PARAMS, "more": {**PARAMS["more"], **more}}
    # Not in its argv: it reads its own entry (`app.list`).
    asked = tmp_path / "asked.json"
    asked.write_text(json.dumps([methods, topics, params]))
    argv = [sys.executable, "-c", PROBE, str(out), str(asked)]
    app = shell.app_start(argv, name="probe", declares=declares)
    until(lambda: "PROBE READY" in "\n".join(shell.shown()))
    # While it listens: the text changes, the cursor moves.
    shell.send(f"echo {SECRETS[1]}\n")
    shell.wait_for(SECRETS[1])
    until(out.exists, timeout=10)
    said = json.loads(out.read_text())
    state = next(a for a in shell.apps() if a["id"] == app["id"])["state"]
    assert state == "running"
    shell.app_stop(app["id"])
    # It sees its own (design: Permissions, Its own).
    answers = said["answers"]
    assert [a["id"] for a in answers["app.list"]["apps"]] == [app["id"]]
    assert answers["app.list"]["apps"][0]["me"]
    assert other.id not in [s["id"] for s in answers["draw.list"]["sprites"]]
    for name in ("update another's", "remove another's"):
        code, message, hint = answers[name]["error"]
        assert (code, hint) == ("no-sprite", None), name
        assert message == answers["draw.remove"]["error"][1].replace(
            "99999", str(other.id)
        )
    assert other.id in [s["id"] for s in shell.call("draw.list")["sprites"]]
    other.remove()
    return said


def leaks(said: dict[str, Any], *more: str) -> list[str]:
    """The secrets, and the words of `more`, in what was said."""
    whole = json.dumps(said).lower()
    return [s for s in (*SECRETS, "PS1", *more) if s.lower() in whole]


def test_an_app_that_reads_nothing(shell: Pilot, tmp_path: Path) -> None:
    said = probe(shell, tmp_path, {"permissions": {}})
    assert not leaks(said, WORD, "echo")
    assert set(said["hello"]) == {"protocol", "server", "size", "headless", "degree"}
    answers = said["answers"]
    info = answers["session.info"]
    assert info["degree"] == "none" and info["size"] == said["hello"]["size"]
    assert "argv" not in info and "pid" not in info and "session" not in info
    cols, rows = info["size"]
    for name in ("screen.text", "screen.shape", "shape words"):
        assert answers[name]["lines"] == [""] * rows, name
        assert answers[name]["size"] == [cols, rows]
        assert answers[name]["degree"] == "none"
    assert answers["cells"]["rows"] == [[]] * rows
    assert answers["text of a frame"]["rect"] == [0, 0, cols, rows]
    cursor = answers["screen.cursor"]
    assert not cursor["visible"] and cursor["trail"] == []
    heat = answers["frame.heat"]
    assert (
        heat["rows"] == ["9" * cols] * rows and heat["classes"] == ["." * cols] * rows
    )
    assert answers["frame.idle"] == {
        "score": 1.0,
        "idle": True,
        "free": 1.0,
        "degree": "none",
    }
    assert answers["frame.text"] == {"lines": [], "y0": 0, "degree": "none"}
    assert answers["screen.find"] == {"matches": [], "degree": "none"}
    assert answers["find word"]["matches"] == []
    assert answers["screen.wait"]["matched"] is False
    assert answers["wait still"]["matched"] and answers["wait idle"]["idle"]["idle"]
    # Its surface, as one frame.
    (frame,) = answers["frame.list"]["frames"]
    assert frame["rect"] == [0, 0, cols, rows] and frame["key"] is None
    assert answers["frame.get"]["frame"] == frame
    assert answers["frame.at"] == {
        "frame": frame["id"],
        "pos": [2, 0],
        "degree": "none",
    }
    # Subscribed, and no event of the screen comes.
    for topic in ("text", "screen", "cursor", "frames", "idle", "input", "hover"):
        assert answers["subscribe " + topic] == {}
    assert answers["subscribe apps"]["error"][0] == "denied"
    heard = {topic for topic, _ in said["events"]}
    assert heard <= {"size", "widgets", "draw", "move", "exit"}, heard
    assert "size" in heard
    # The acts: denied, the hint naming the permission.
    code, _, hint = answers["input.grab"]["error"]
    assert code == "denied" and "`keys-grab`" in hint
    assert answers["input.send"]["error"][0] == "denied"
    # Whatever frame it names, it draws on its surface.
    assert "id" in answers["draw.put"]
    results = answers["batch"]["results"]
    assert results[0]["result"]["lines"] == [""] * rows
    assert results[1]["result"]["matches"] == []


def test_an_app_that_reads_the_shape(shell: Pilot, tmp_path: Path) -> None:
    said = probe(shell, tmp_path, {"permissions": {"shape": "it walks on the lines"}})
    assert not leaks(said, WORD, "echo")
    answers = said["answers"]
    lines = answers["screen.shape"]["lines"]
    assert any("#########" in line for line in lines)
    assert "degree" not in answers["screen.shape"]
    assert answers["screen.text"] == {**answers["screen.shape"], "degree": "shape"}
    assert answers["shape words"]["lines"] == lines
    rows = answers["cells"]["rows"]
    assert rows == [[{"t": line}] if line else [] for line in lines]
    remembered = answers["frame.text"]
    assert remembered["degree"] == "shape"
    assert any("#########" in line for line in remembered["lines"])
    assert set("".join(remembered["lines"])) <= {"#", " "}
    assert answers["screen.cursor"]["visible"]
    assert answers["screen.find"]["matches"] == []
    assert answers["find word"]["matches"] == []
    frames = answers["frame.list"]
    assert frames["degree"] == "shape"
    assert all(f["key"] is None and f["support"] is None for f in frames["frames"])
    heard = {topic for topic, _ in said["events"]}
    assert {"text", "cursor"} <= heard
    assert not heard & {"input", "hover", "mouse", "apps"}
    for topic, data in said["events"]:
        if topic == "frames":
            assert all(f["key"] is None for f in data["frames"])
    assert "host_colors" in answers["session.info"]
    # A hint lists no frame.
    code, _, hint = answers["draw.put"]["error"]
    assert code == "no-frame" and hint is None


def test_the_declared_words_do_not_add_up(shell: Pilot, tmp_path: Path) -> None:
    declares = {
        "permissions": {"shape": "the lines", "words": "it goes to them"},
        "words": [WORD],
    }
    said = probe(shell, tmp_path, declares)
    assert not leaks(said, "echo")
    answers = said["answers"]
    text = "\n".join(answers["screen.text"]["lines"])
    assert answers["screen.text"]["degree"] == "words"
    # `_` bounds a word of the shape (`my_fish`); not one of `find`.
    assert f"######### {WORD} ###{WORD}" in text
    assert set(text) <= set("# \n" + WORD)
    # The words asked beyond the declared ones are not shown.
    assert answers["shape words"]["lines"] == answers["screen.text"]["lines"]
    assert answers["shape words"]["degree"] == "words"
    found = answers["find word"]
    assert found["degree"] == "words" and found["matches"]
    assert all(m["text"] == WORD for m in found["matches"])
    assert len([m for m in found["matches"] if m["screen"]]) == 2
    assert answers["wait word"]["matched"] and answers["wait word"]["degree"] == "words"
    assert answers["screen.find"]["matches"] == []
    assert answers["find regex"]["matches"] == []
    assert answers["screen.wait"]["matched"] is False
    assert WORD in "\n".join(answers["frame.text"]["lines"])


def test_a_no_to_the_shape_does_not_break(shell: Pilot, tmp_path: Path) -> None:
    config = Path(os.environ["XDG_CONFIG_HOME"]) / "ptyrc"
    config.mkdir(parents=True, exist_ok=True)
    answers = config / "permissions.toml"
    answers.write_text('[app."native/probe"]\nallow = []\ndeny = ["shape"]\n')
    try:
        said = probe(shell, tmp_path, {"permissions": {"shape": "the lines"}})
    finally:
        answers.unlink()
    assert not leaks(said, WORD)
    assert said["answers"]["screen.shape"]["degree"] == "none"
    assert not any(said["answers"]["screen.shape"]["lines"])
    assert not {topic for topic, _ in said["events"]} & {"text", "cursor", "screen"}


def test_a_whole_app_reads_as_before(shell: Pilot, tmp_path: Path) -> None:
    shell.send(f"echo {SECRETS[0]}\n")
    shell.wait_for(SECRETS[0])
    assert "degree" not in shell.call("screen.text")
    assert "argv" in shell.hello and "argv" in shell.call("session.info")
    with pytest.raises(Exception, match="no such frame") as err:
        shell.call("frame.get", frame="no-such")
    assert err.value.hint  # type: ignore[attr-defined]
