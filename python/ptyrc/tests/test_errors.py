# SPDX-License-Identifier: GPL-3.0-or-later
"""Errors carry the driver's code, message and hint."""

from __future__ import annotations

import pytest
from ptyrc import Pilot, PtyrcError


def raised(call, *args, **kw) -> PtyrcError:
    with pytest.raises(PtyrcError) as err:
        call(*args, **kw)
    return err.value


def test_reply_errors_have_hints(shell: Pilot) -> None:
    err = raised(shell.text, frame="activ")
    assert (err.code, err.message) == ("no-frame", "no such frame: activ")
    assert err.hint.startswith("did you mean active? frames: 0")
    assert str(err) == f"no-frame: no such frame: activ\n  {err.hint}"

    err = raised(shell.keys, "Enter")
    assert (err.code, err.message) == ("bad-params", "not a key: Enter")
    assert err.hint.startswith("did you mean enter?")

    err = raised(shell.draw, ["x"], fg="gren")
    assert (err.code, err.message) == ("bad-params", "bad colour: gren")
    assert "did you mean green?" in err.hint

    err = raised(shell.put, {"content": {"rows": ["x"]}, "anchor": {"nope": 1}})
    assert (err.code, err.message) == ("bad-params", "bad anchor")
    assert '"auto"' in err.hint

    err = raised(shell.call, "screen.txt")
    assert err.code == "unknown-method"
    assert err.hint.startswith("did you mean screen.text?")

    err = raised(shell.call, "subscribe", topics=["screens"])
    assert err.message == "no such topic: screens"

    err = raised(shell.call, "draw.remove", id=77)
    assert (err.code, err.hint) == ("no-sprite", "no sprite is up")


def test_batch_errors_have_hints(shell: Pilot) -> None:
    with shell.batch() as batch:
        shell.draw(["x"], fg="gren")
    (err,) = batch.results
    assert isinstance(err, PtyrcError)
    assert "did you mean green?" in err.hint


def test_no_session_says_what_to_try() -> None:
    err = raised(Pilot)
    assert (err.code, err.message) == ("gone", "no session")
    assert err.hint == "start one: ptyrc run -- CMD"
    err = raised(Pilot, "missing")
    assert err.message == "no such session: missing"
    assert "ptyrc run --name missing" in err.hint


def test_spawn_reports_the_driver(shell: Pilot) -> None:
    err = raised(Pilot.spawn, ["/no/such/program"], timeout=3)
    assert err.code == "gone"
    assert err.message == "driver exited 127: /no/such/program: no such file"
    assert "/no/such does not exist" in err.hint
    err = raised(Pilot.spawn, ["sh"], name=shell.session, timeout=3)
    assert f"session {shell.session} is already running" in err.message
    err = raised(Pilot, "nearly")
    assert err.hint == f"running: {shell.session}"
