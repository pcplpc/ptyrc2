# SPDX-License-Identifier: GPL-3.0-or-later
"""`ask`: the model the arguments name, the answer as printed, a
question answered on a host keyboard."""

from __future__ import annotations

import json
import os
import threading
import time
from typing import TYPE_CHECKING, Any

import pytest
from ptyrc import Pilot, ui
from ptyrc_apps import APPS, _catalog, ask

if TYPE_CHECKING:
    from conftest import Host


def test_model() -> None:
    choice = ask.model(ask.options(["Deploy?", "yes", "no", "--default", "no"]))
    assert isinstance(choice, ui.Choice) and choice.question == "Deploy?"
    assert choice.options == ["yes", "no"] and choice.listing.sel == 1
    assert not choice.multi and choice.other is None
    assert choice.key("enter") == ui.DONE and choice.result == "no"
    some = ask.model(ask.options(["-m", "-o", "Which?", "a", "b", "--default", "b"]))
    assert isinstance(some, ui.Choice) and some.multi and some.other is not None
    assert some.key("enter") == ui.DONE and some.result == ["b"]
    line = ask.model(ask.options(["Name?", "--default", "you"]))
    assert isinstance(line, ui.Line) and line.editor.text == "you"
    text = ask.model(ask.options(["-t", "Why?", "--default", "a\nb"]))
    assert isinstance(text, ui.Text) and text.text == "a\nb"
    # Options given: a choice, `--text` or not.
    assert isinstance(ask.model(ask.options(["-t", "Q", "a"])), ui.Choice)


def test_options() -> None:
    args = ask.options(["Q", "a", "--timeout", "2.5", "--title", "t", "--front"])
    assert (args.timeout, args.title, args.front) == (2.5, "t", True)
    assert (args.border, args.prefer, args.frame) == ("round", "center", "raw")
    with pytest.raises(SystemExit):
        ask.options([])
    with pytest.raises(SystemExit):
        ask.options(["Q", "--timeout", "0"])


def test_written() -> None:
    assert ask.written("yes") == "yes"
    assert ask.written(["a", "b"]) == "a\nb"
    assert ask.written([]) == ""
    assert json.loads(ask.written(["a", "é"], True)) == ["a", "é"]
    assert ask.written("a\nb", True) == '"a\\nb"'


def test_catalog() -> None:
    entry = _catalog.describe("ask")
    assert entry["help"] == APPS["ask"] and entry["random"] == []
    assert [p["name"] for p in entry["positionals"]] == ["QUESTION", "OPTION"]
    assert entry["widgets"]


def test_no_session(capsys: pytest.CaptureFixture[str]) -> None:
    assert ask.main(["-s", "no-such-session", "Q"]) == 3
    out = capsys.readouterr()
    assert out.out == "" and out.err.startswith("ptyrc-app: ")


def test_times_out(session: Pilot, capsys: pytest.CaptureFixture[str]) -> None:
    argv = ["-s", session.session, "Q", "a", "b", "--timeout", "0.5"]
    assert ask.main(argv) == 1
    assert capsys.readouterr().out == ""
    assert not [w for w in session.widgets() if w["state"] != "gone"]


class Asked:
    """`ask.main` in a thread; its widget, as the session tells it."""

    def __init__(self, pilot: Pilot, *argv: str) -> None:
        self.pilot = pilot
        self.status: Any = None
        self.thread = threading.Thread(
            target=lambda: setattr(
                self, "status", ask.main(["-s", pilot.session, *argv])
            )
        )
        self.thread.start()

    def until(self, test: Any) -> dict[str, Any]:
        deadline = time.monotonic() + 8
        while True:
            found = [w for w in self.pilot.widgets() if w["tag"] == "ask" and test(w)]
            if found:
                return found[0]
            assert time.monotonic() < deadline and self.thread.is_alive()
            time.sleep(0.03)

    def end(self) -> int:
        self.thread.join(8)
        assert not self.thread.is_alive()
        return self.status


def test_a_choice_answered(
    hosted: tuple[Host, Pilot], capsys: pytest.CaptureFixture[str]
) -> None:
    host, pilot = hosted
    asked = Asked(pilot, "Deploy?", "yes", "no", "later", "--title", "deploy")
    info = asked.until(lambda w: w["asks"] == "loud" and w["state"] == "placed")
    assert info["title"] == "deploy" and not info["focused"]
    assert "Deploy?" in "\n".join(pilot.dump()["host"])
    # Not focused: the keys are the shell's.
    os.write(host.master, b"echo h$((1+1))\r")
    pilot.wait_for("h2")
    assert pilot.focus(info["id"])["focused"]
    os.write(host.master, b"\x1b[B")
    os.write(host.master, b"\r")
    assert asked.end() == 0
    assert capsys.readouterr().out == "no\n"
    assert not [w for w in pilot.widgets() if w["tag"] == "ask"]
    assert pilot.call("session.info")["focus"] is None


def test_a_line_typed_then_dismissed(
    hosted: tuple[Host, Pilot], capsys: pytest.CaptureFixture[str]
) -> None:
    host, pilot = hosted
    asked = Asked(pilot, "Name?", "--json", "--default", "an")
    info = asked.until(lambda w: w["asks"] == "loud")
    pilot.focus(info["id"])
    os.write(host.master, "né".encode())
    asked.until(lambda w: w["focused"])
    deadline = time.monotonic() + 5
    while "anné" not in "\n".join(pilot.dump()["host"]):
        assert time.monotonic() < deadline
        time.sleep(0.03)
    # Escape: the question stays; the keys are the shell's again.
    os.write(host.master, b"\x1b")
    asked.until(lambda w: not w["focused"] and w["asks"] == "loud")
    pilot.focus(info["id"])
    os.write(host.master, b"\r")
    assert asked.end() == 0
    assert capsys.readouterr().out == '"anné"\n'

    asked = Asked(pilot, "Sure?", "yes", "no")
    info = asked.until(lambda w: w["asks"] == "loud")
    pilot.focus(info["id"])
    os.write(host.master, b"\x03")
    assert asked.end() == 1
    assert capsys.readouterr().out == ""

    # Closed by its key: dismissed, the widget gone.
    asked = Asked(pilot, "Sure?", "yes", "no")
    info = asked.until(lambda w: w["asks"] == "loud")
    pilot.focus(info["id"])
    os.write(host.master, b"\x18")
    assert asked.end() == 1
    assert capsys.readouterr().out == ""
    assert not [w for w in pilot.widgets() if w["tag"] == "ask"]
