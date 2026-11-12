# SPDX-License-Identifier: GPL-3.0-or-later
"""Locators, clicks, grabbed keys, tapes."""

from __future__ import annotations

import os
import pty
import runpy
import select
import threading
import time
from pathlib import Path

import pytest
from conftest import ROOT, VIM, Spawn
from ptyrc import Locator, Pilot, PtyrcError
from tests_support import until

SHOW_KEYS = "stty raw -echo; exec cat -v"


def test_locator_finds_reads_and_waits(shell: Pilot) -> None:
    shell.send("echo 'Total: 42 items'; echo 'Other: 7 items'\r")
    total = shell.locator("Total: 4")
    assert isinstance(total, Locator)
    total.wait()
    # The command line and its output.
    assert total.count() == 2
    out = total.nth(1)
    assert out.screen() == (0, 1) and out.pos() == (0, 1)
    assert out.right(1).read(1) == "2"
    assert out.right(1).read() == "2 items"
    assert out.left(0).read(5) == "Total"
    assert out.below().read(8) == "Other: 7"
    assert out.right(3).below(1).above(1).left(1).read(2) == " i"
    assert total.nth(-1).pos() == out.pos()
    assert repr(out.right(2).below()) == "locator('Total: 4').nth(1).right(2).below(1)"

    # Looked up at each use.
    later = shell.locator(r"d[o]ne \d+", regex=True)
    assert later.match() is None and not later.visible()
    with pytest.raises(TimeoutError):
        later.wait(timeout=0.2)
    with pytest.raises(PtyrcError) as err:
        later.read()
    assert err.value.code == "not-usable" and "wait()" in err.value.hint
    threading.Timer(0.3, lambda: shell.send("echo do''ne 12\r")).start()
    assert later.wait().read() == "done 12"
    shell.keys("ctrl+l")
    later.wait(gone=True)

    # Off the screen: a place, not a cell.
    assert shell.locator("$").left(3).screen() is None
    assert shell.locator("total: 4", ignore_case=True).count() == 0  # cleared


def test_locator_on_a_frame_draws_in_frame_cells(vim: Pilot) -> None:
    pilot = vim
    pane = pilot.panes()[0]
    word = pane.locator("line 012")
    word.wait()
    assert word.frame == pane.id
    sprite = word.right(2).draw(["<-"], layer="front")
    col, row = word.right(2).screen()
    until(lambda: sprite.info().get("screen") == [col, row])
    # Scrolled, the sprite goes with its text.
    pilot.keys("ctrl+e", "ctrl+e")
    until(lambda: sprite.info().get("screen") == [col, row - 2])
    assert word.right(2).screen() == (col, row - 2)


def test_click(spawn: Spawn) -> None:
    script = f"printf 'one two\\r\\n\\033[?1000h\\033[?1006h'; {SHOW_KEYS}"
    pilot = spawn(["sh", "-c", script], size=(200, 6))
    two = pilot.locator("two").wait()
    until(lambda: _clicks(two))
    pilot.wait_for("^[[<0;5;1M^[[<0;5;1m")
    pilot.click(0, 3, "right")
    pilot.wait_for("^[[<2;1;4M^[[<2;1;4m")
    # Modifiers, a double click, a drag (press and release: the app did
    # not ask for the moves).
    two.click(mods=["shift", "ctrl"], count=2)
    pilot.wait_for("^[[<20;5;1M^[[<20;5;1m^[[<20;5;1M^[[<20;5;1m")
    pilot.locator("one").click("middle", to=two)
    pilot.wait_for("^[[<1;1;1M^[[<1;5;1m")
    with pytest.raises(PtyrcError) as err:
        pilot.click(0, 0, mods=["meta"])
    assert "shift, alt, ctrl" in err.value.hint
    with pytest.raises(PtyrcError) as err:
        pilot.click(0, 0, "fourth")
    assert "wheel-up" in err.value.hint
    quiet = spawn(["cat"])
    with pytest.raises(PtyrcError) as err:
        quiet.click(0, 0)
    assert err.value.code == "not-usable"


def _clicks(locator: Locator) -> bool:
    try:
        locator.click()
    except PtyrcError:
        return False
    return True


def test_record_and_replay(spawn: Spawn, tmp_path: Path) -> None:
    tape = tmp_path / "s.tape"
    live = spawn(["env", "PS1=$ ", "sh"], size=(40, 8), record=tape)
    live.wait_for("$ ")
    live.send("echo o''ne\r")
    live.wait_for("one")
    time.sleep(0.3)
    live.send("echo t''wo\r")
    live.wait_for("two")
    before = live.snapshot()
    live.send("exit 4\r")
    until(lambda: not live.connected)
    live.close()

    replay = Pilot.replay(tape)
    try:
        assert replay.info()["tape"]["paused"] and replay.argv == [
            "env",
            "PS1=$ ",
            "sh",
        ]
        assert replay.size == (40, 8)
        assert replay.text()[0] == ""
        end = replay.tape.end
        # Record by record, then to the end: a seek does not end the replay.
        while "one" not in replay.text():
            replay.tape.step()
        assert not replay.find("two")
        typed = [line for line in tape.read_text().splitlines() if '"i"' in line]
        assert len(typed) == 3
        status = replay.tape.seek(end)
        assert status["paused"] and status["done"] and status["t"] == end
        now = replay.snapshot()
        assert now["text"][:5] == [
            "$ echo o''ne",
            "one",
            "$ echo t''wo",
            "two",
            "$ exit 4",
        ]
        assert now["text"][:4] == before["text"][:4]
        assert [f["rect"] for f in now["frames"]] == [
            f["rect"] for f in before["frames"]
        ]
        with pytest.raises(PtyrcError) as err:
            replay.send("x")
        assert "a replay takes no input" in err.value.message
        # Back: the tape is replayed from its start.
        assert replay.tape.seek(0.0)["next"] == 0
        assert replay.text()[0] == ""
        replay.tape.seek(end)
        assert replay.snapshot() == now
        # Sprites are drawn on a replay as on a live session.
        sprite = replay.locator("two").nth(-1).right(2).draw(["!"], layer="front")
        replay.tape.seek(end + 1.0)
        until(lambda: sprite.visible)
        assert replay.shown()[3].startswith("two !")
        assert replay.text()[3] == "two"
        replay.tape.play(0)
        until(lambda: not replay.connected)
    finally:
        replay.close()
    with pytest.raises(PtyrcError) as err:
        Pilot.replay(tmp_path / "none.tape")
    assert "ptyrc run --record" in err.value.hint
    with pytest.raises(PtyrcError) as err:
        spawn(["cat"]).tape.status()
    assert "not a replay" in err.value.message


def test_grab_is_refused_with_words(spawn: Spawn) -> None:
    pilot = spawn(["cat"])
    pilot.grab("up", "f5", escape="f12")
    pilot.release()
    with pytest.raises(PtyrcError) as err:
        pilot.grab("upp+")
    assert err.value.code == "bad-params"
    with pytest.raises(PtyrcError) as err:
        pilot.grab(escape="a")
    assert "never reaches the app" in err.value.hint


class Host:
    """This test as the terminal of a driver: a pty, read in the
    background."""

    def __init__(self, argv: list[str], cwd: Path) -> None:
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(cwd)
            os.execvp(argv[0], argv)
        self.seen = b""
        threading.Thread(target=self._pump, daemon=True).start()

    def _pump(self) -> None:
        while True:
            try:
                ready, _, _ = select.select([self.fd], [], [], 0.2)
                if ready:
                    self.seen += os.read(self.fd, 65536)
            except OSError:
                return

    def type(self, data: bytes) -> None:
        os.write(self.fd, data)

    def close(self) -> None:
        try:
            os.kill(self.pid, 15)
            os.waitpid(self.pid, 0)
        except OSError:
            pass
        os.close(self.fd)


def test_arrows_move_a_sprite_and_vim_does_not_see_them(long_file: Path) -> None:
    name = f"grab-{os.getpid()}"
    binary = os.environ["PTYRC_BIN"]
    argv = [binary, "run", "--name", name, "--", VIM, "-u", "NONE", "-i", "NONE"]
    host = Host([*argv, long_file.name], long_file.parent)
    try:
        until(lambda: name in __import__("ptyrc").live_sessions())
        pilot = Pilot(name)
        pilot.wait_for("line 001")
        until(lambda: pilot.panes())
        host.type(b"jj")
        until(lambda: pilot.cursor().y == 2)

        script = runpy.run_path(str(ROOT / "scripts" / "grab-sprite.py"))
        ship = threading.Thread(target=script["main"], args=(name,), daemon=True)
        ship.start()

        def ships() -> list[list[int]]:
            return [s["screen"] for s in pilot.sprites() if s.get("screen")]

        start = until(ships)[0]
        until(lambda: pilot.info()["grabbed"])
        host.type(b"\x1b[B\x1b[C\x1b[C\x1bOA\x1b[B")
        until(lambda: ships()[0] == [start[0] + 4, start[1] + 1])
        assert pilot.cursor().y == 2, "vim did not see the arrows"
        # Any other key is vim's.
        host.type(b"j")
        until(lambda: pilot.cursor().y == 3)
        # The escape key gives the arrows back, and the app ends.
        host.type(b"\x1d")
        ship.join(timeout=5)
        assert not ship.is_alive()
        until(lambda: not ships())
        host.type(b"\x1b[B")
        until(lambda: pilot.cursor().y == 4)
        pilot.close()
    finally:
        host.close()


def test_a_sprite_is_clicked(long_file: Path) -> None:
    name = f"click-{os.getpid()}"
    binary = os.environ["PTYRC_BIN"]
    argv = [binary, "run", "--name", name, "--", VIM, "-u", "NONE", "-i", "NONE"]
    host = Host([*argv, long_file.name], long_file.parent)
    try:
        until(lambda: name in __import__("ptyrc").live_sessions())
        pilot = Pilot(name)
        pilot.wait_for("line 001")
        script = runpy.run_path(str(ROOT / "scripts" / "click-sprite.py"))
        button = threading.Thread(target=script["main"], args=(name,), daemon=True)
        button.start()
        col, row = until(lambda: _shown_at(pilot, "[clicks:0]"))
        until(lambda: b"\x1b[?1002h" in host.seen)
        report = f"\x1b[<0;{col + 3};{row + 1}".encode()
        host.type(report + b"M" + report + b"m")
        until(lambda: _shown_at(pilot, "[clicks:1]"))
        host.type(report + b"M" + report + b"m")
        until(lambda: _shown_at(pilot, "[clicks:2]"))
        # Elsewhere: not a click on the button; vim did not ask.
        host.type(b"\x1b[<0;1;3M\x1b[<0;1;3mj")
        until(lambda: pilot.cursor().y == 1)
        assert _shown_at(pilot, "[clicks:2]")
        # A right click ends it; the host mouse is given back.
        host.type(report.replace(b"<0", b"<2") + b"M")
        button.join(timeout=5)
        assert not button.is_alive()
        until(lambda: not pilot.sprites())
        until(lambda: b"\x1b[?1002l" in host.seen)
        pilot.close()
    finally:
        host.close()


def _shown_at(pilot: Pilot, text: str) -> tuple[int, int] | None:
    for row, line in enumerate(pilot.shown()):
        if (col := line.find(text)) >= 0:
            return col, row
    return None
