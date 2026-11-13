# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc-app --catalog`: the apps and their options."""

from __future__ import annotations

import json

import pytest
from ptyrc_apps import APPS, _catalog, _cli
from ptyrc_apps.__main__ import main


def test_catalog(capsys: pytest.CaptureFixture[str]) -> None:
    parse = _cli.parse
    assert main(["--catalog"]) == 0
    assert _cli.parse is parse
    apps = json.loads(capsys.readouterr().out)["apps"]
    assert [app["name"] for app in apps] == list(APPS)
    by_name = {app["name"]: app for app in apps}
    for app in apps:
        assert app["help"] == APPS[app["name"]]
        flags = [flag for option in app["options"] for flag in option["flags"]]
        # The frame is an option of every app; help and session are not shown.
        assert "--frame" in flags
        assert not {"-h", "--help", "-s", "--session"} & set(flags)
        for option in app["options"]:
            assert option["kind"] in ("flag", "value", "choice", "count")
            assert (option["kind"] == "choice") == bool(option["choices"])
            assert option["kind"] == "flag" or option["metavar"]
    bricks = by_name["bricks"]
    rules = next(o for o in bricks["options"] if "--rules" in o["flags"])
    assert rules["flags"] == ["-r", "--rules"] and rules["kind"] == "flag"
    assert rules["help"] and rules["default"] is False
    assert bricks["random"] == [[], ["-a"]] and bricks["positionals"] == []
    frame = next(o for o in bricks["options"] if "--frame" in o["flags"])
    assert (frame["kind"], frame["default"]) == ("value", "largest")
    # What `random app` draws: never marks, flower with its `-a` only.
    assert by_name["marks"]["random"] == []
    assert by_name["flower"]["random"] == [["-a"]]
    assert by_name["clock"]["random"] == [[]]
    (patterns,) = by_name["marks"]["positionals"]
    assert patterns["nargs"] == "+" and patterns["name"]


def test_parser_of_does_not_connect(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("PTYRC_SESSION", "no-such-session")
    for name in APPS:
        assert _catalog.parser_of(name).prog == f"ptyrc-app {name}"


def test_demo_is_gone(capsys: pytest.CaptureFixture[str]) -> None:
    assert main(["demo", "0", "0"]) == 2
    assert "no app named demo" in capsys.readouterr().err
