# SPDX-License-Identifier: GPL-3.0-or-later
"""Reading the screen and typing."""

from __future__ import annotations

import time

import pytest
from ptyrc import Pilot, PtyrcError


def test_hello_and_info(shell: Pilot) -> None:
    assert shell.hello["protocol"] == 1
    assert shell.argv[-1] == "sh"
    assert shell.size == (80, 24)
    assert shell.info()["headless"] is True


def test_send_text_find(shell: Pilot) -> None:
    shell.send("echo hel''lo\n")
    match = shell.wait_for("hello")
    assert match is not None
    assert (match.frame, match.x, match.len, match.text) == (0, 0, 5, "hello")
    assert match.screen == (match.x, match.y)
    lines = shell.text()
    assert len(lines) == 24
    assert lines[match.y] == "hello"
    assert shell.text(rect=(1, match.y, 3, 1)) == ["ell"]
    assert [m.text for m in shell.find(r"h\w+o", regex=True)] == ["hello"]
    assert shell.find("HELLO") == []
    assert len(shell.find("HELLO", ignore_case=True)) == 1


def test_cells_carry_style(shell: Pilot) -> None:
    shell.send("printf '\\033[1;31mred\\033[0m plain\\n'\n")
    match = shell.wait_for("red plain")
    runs = shell.cells()[match.y]
    assert runs[0] == {"t": "red", "fg": "red", "bold": True}
    assert runs[1]["t"].startswith(" plain")


def test_cursor_and_trail(shell: Pilot) -> None:
    shell.send("abc")
    shell.wait_for("$ abc")
    cursor = shell.cursor(trail=True)
    assert (cursor.x, cursor.y, cursor.visible) == (5, 0, True)
    assert cursor.trail and cursor.trail[-1][1:] == (5, 0)


def test_keys_and_paste(shell: Pilot) -> None:
    shell.send("echo ab")
    shell.keys("backspace", "c", "enter")
    shell.wait_for(r"^ac *$", regex=True)
    # A pasted newline does not run the line when the paste is bracketed.
    shell.paste("echo pasted")
    shell.keys("enter")
    shell.wait_for(r"^pasted *$", regex=True)
    shell.write(b"echo raw\n")
    shell.wait_for(r"^raw *$", regex=True)


def test_wait_for(shell: Pilot) -> None:
    shell.send("sleep 0.3; echo done-now\n")
    assert shell.wait_for(r"^done-now", regex=True, timeout=5) is not None
    with pytest.raises(TimeoutError):
        shell.wait_for("never shown", timeout=0.2)
    # Gone: wait for the screen to be cleared.
    shell.send("clear\n")
    assert shell.wait_for("done-now", gone=True) is None
    # Stable: nothing has changed for a while.
    start = time.monotonic()
    shell.wait_for(stable=0.3)
    assert time.monotonic() - start < 3
    with pytest.raises(PtyrcError) as err:
        shell.wait_for("(", regex=True)
    assert err.value.code == "bad-params"


def test_resize(shell: Pilot) -> None:
    shell.resize(50, 10)
    assert shell.size == (50, 10)
    shell.send("stty size\n")
    shell.wait_for("10 50")
    assert len(shell.text()) == 10


def test_events(shell: Pilot) -> None:
    shell.subscribe("screen", "exit")
    shell.send("echo hi\n")
    seen = []
    for topic, data in shell.events(timeout=3):
        seen.append(topic)
        if topic == "screen" and "screen" not in seen[:-1]:
            assert data["rows"]
            shell.send("exit\n")
        if topic == "exit":
            assert data["status"] == 0
            break
    assert "screen" in seen and seen[-1] == "exit"


def test_silence_is_not_the_end(shell: Pilot) -> None:
    shell.subscribe("screen")
    shell.wait_for(stable=0.3)
    assert list(shell.events(timeout=0.2)) == []
    assert shell.event(timeout=0.1) is None
    assert shell.connected


def test_snapshot(shell: Pilot) -> None:
    shell.send("echo hi\n")
    shell.wait_for(stable=0.3)
    # Nothing moves: without a snapshot, no event.
    shell.subscribe("cursor", "frames", "idle", "size", snapshot=True)
    seen = dict(shell.events(timeout=0.3))
    assert set(seen) == {"cursor", "frames", "idle", "size"}
    cursor = shell.cursor()
    assert (seen["cursor"]["x"], seen["cursor"]["y"]) == (cursor.x, cursor.y)
    assert tuple(seen["cursor"]["pos"]) == cursor.pos
    assert seen["cursor"]["frame"] == cursor.frame
    assert seen["size"] == {"size": [80, 24]}
    assert seen["frames"]["frames"][0]["id"] == 0
    assert seen["idle"]["idle"] is False


def test_text_events(shell: Pilot) -> None:
    shell.wait_for(stable=0.3)
    shell.subscribe("text")
    shell.send("echo hi\n")
    shell.wait_for(stable=0.3)
    raw = [d for t, d in shell.events(timeout=0.3) if t == "text" and d["frame"] == 0]
    rows = {y for d in raw for y in d["rows"]}
    assert {0, 1, 2} <= rows, raw
    assert all(d["origin"] == [0, 0] for d in raw)


def test_reads_inside_a_batch(shell: Pilot) -> None:
    shell.send("echo fox dog fox\n")
    shell.wait_for(stable=0.3)
    with shell.batch() as batch:
        foxes = shell.find("fox")
        dogs = shell.find("dog")
        lines = shell.text()
        tint = shell.draw("   ", at=(0, 0), blend="tint", fg="red", opaque=True)
        info = tint.info()
        listed = shell.sprites()
        assert foxes == [] and info == {}  # not sent yet
    assert len(batch.results) == 6
    assert len(foxes) >= 2 and len(dogs) >= 1
    assert lines == shell.text()
    assert info["id"] == tint.id and info["style"] == {"fg": "red"}
    assert [s["id"] for s in listed] == [tint.id]
    assert tint.info()["style"] == info["style"]
    tint.remove()
    assert tint.info() is None


def test_errors(shell: Pilot) -> None:
    with pytest.raises(PtyrcError) as err:
        shell.call("no.such")
    assert err.value.code == "unknown-method"
    with pytest.raises(PtyrcError) as err:
        shell.text(frame=999)
    assert err.value.code == "no-frame"
    with pytest.raises(PtyrcError) as err:
        shell.frame("nowhere")
    assert err.value.code == "no-frame"
    with pytest.raises(PtyrcError) as err:
        shell.draw(["x"], frame=999)
    assert err.value.code == "no-frame"
    with pytest.raises(PtyrcError) as err:
        shell.put({"content": {}})
    assert err.value.code == "bad-params"
    with pytest.raises(TypeError):
        shell.draw(["x"], colour="red")
