# SPDX-License-Identifier: GPL-3.0-or-later
"""`pace`: the keys, the words and the faces without a driver; one run
under a host keyboard."""

from __future__ import annotations

import base64
import fcntl
import hashlib
import json
import os
import pty
import random
import stat
import struct
import subprocess
import termios
import threading
import time
import unicodedata
from collections.abc import Iterator
from pathlib import Path

import pytest
from ptyrc import Pilot, PtyrcError
from ptyrc import clock as session_clock
from ptyrc_apps import pace

LOOKS = [
    pace.Look(graph=g, best=b, words=w or r, bars=r, colour=c, top=top)
    for g in (False, True)
    for b in (False, True)
    for w, r in ((False, False), (True, False), (True, True))
    for c in (False, True)
    for top in (10, 4)
]
ALL = pace.Look(True, True, True, True, True)


def plain(rows: pace.Rows) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


@pytest.mark.parametrize(
    ("data", "count"),
    [
        (b"", 0),
        (b"a", 1),
        (b"ab", 2),
        (b"\r", 1),
        (b"\x7f", 1),
        (b"\x03", 1),
        ("é".encode(), 1),
        ("日本".encode(), 2),
        (b"\x1b[A", 1),  # an arrow
        (b"\x1bOP", 1),  # F1
        (b"\x1b[1;5C", 1),  # ctrl and an arrow
        (b"\x1b[15~a", 2),
        (b"\x1b", 1),  # escape alone
        (b"\x1bb", 1),  # alt and b
        (b"\x1b[200~secret\x1b[201~", 0),  # a bracketed paste
        (b"\x1b[<35;10;5M", 0),  # a mouse report
        (b"a" * pace.PASTE, 0),  # too many at once
        (b"a" * (pace.PASTE - 1), pace.PASTE - 1),
    ],
)
def test_keys(data: bytes, count: int) -> None:
    assert pace.keys(data) == count


def test_meter() -> None:
    meter = pace.Meter(window=15.0)
    assert (meter.rate(0.0), meter.minute(0.0), meter.best) == (0, 0, 0)
    for i in range(30):  # two keys a second, for 15 s
        meter.add(100.0 + i / 2, 1)
    assert meter.rate(114.5) == 120
    assert meter.minute(114.5) == 30 and meter.total == 30
    assert meter.best == 120
    assert meter.rate(122.0) == 60  # half the window is quiet
    assert meter.rate(130.0) == 0 and meter.minute(130.0) == 30
    assert meter.minute(175.0) == 0
    assert meter.best == 120
    meter.add(200.0, 0)
    assert meter.total == 30


def test_meter_slots() -> None:
    meter = pace.Meter()
    count = int(pace.SPAN / pace.SLOT)
    assert meter.slots(5.0) == (0,) * count
    meter.add(5.0, 3)
    meter.add(9.0, 1)
    meter.add(10.0, 2)
    slots = meter.slots(12.0)
    assert len(slots) == count and slots[-2:] == (4, 2) and sum(slots) == 6
    assert meter.slots(12.0 + pace.SPAN - pace.SLOT)[:2] == (2, 0)
    assert sum(meter.slots(12.0 + pace.SPAN)) == 0
    meter.add(3 * pace.SPAN, 1)  # what is old is dropped
    assert len(meter._slots) == 1


class Screen:
    """A row of a terminal a reader looks at after each key."""

    def __init__(self) -> None:
        self.reader = pace.Reader()
        self.row, self.text = 0, "$ "
        self.words: list[str] = []
        self.look(0)

    def look(self, keyed: int = 1) -> None:
        got = self.reader.see(self.row, self.text.rstrip(), len(self.text), keyed)
        self.words += [word for word, _ in got]

    def type(self, text: str) -> Screen:
        for char in text:
            if char == "\b":
                self.text = self.text[:-1]
            elif char == "\n":
                self.row, self.text = self.row + 1, "$ "
            else:
                self.text += char
            self.look()
        return self

    def write(self, text: str, keyed: int = 0) -> Screen:
        """Text that comes at once: a program, a paste, a completion."""
        self.text += text
        self.look(keyed)
        return self


def test_reader() -> None:
    assert Screen().type("echo hello world\n").words == ["echo", "hello", "world"]
    assert Screen().type("vim notes.md ").words == ["vim", "notes", "md"]
    assert Screen().type("don't stop").words == ["don't"]
    assert Screen().type("été, là\n").words == ["été", "là"]
    # Nothing ends the word: not counted yet.
    assert Screen().type("hello").words == []


def test_reader_rubbed_out() -> None:
    assert Screen().type("helo\b\bllo wrold\b\b\b\borld\n").words == ["hello", "world"]
    assert Screen().type("abc\b\b\b\n").words == []


def test_reader_not_typed() -> None:
    # A program writes: no key.
    assert Screen().write("total 42 files ").type("\n").words == []
    # A paste: more cells than keys.
    assert Screen().write("pasted words here ", keyed=1).type("\n").words == []
    # A completion: a word counts when half of it was typed.
    assert Screen().type("doc").write("umentation", 1).type(" ").words == []
    assert Screen().type("documen").write("tation", 1).type(" ").words == [
        "documentation"
    ]
    # Two keys between two looks: both typed.
    screen = Screen().type("hell")
    screen.text += "o "
    screen.look(2)
    assert screen.words == ["hello"]
    # Not echoed: the cursor does not move, nothing is read.
    screen = Screen()
    for _ in "password":
        screen.look()
    assert screen.type("\n").words == []


def test_reader_token() -> None:
    reader = pace.Reader()
    reader.see(0, "", 0, 0)
    text, got = "", []
    for char in "key=a3f9c2deadbeef ok ":
        text += char
        got += reader.see(0, text.rstrip(), len(text), 1)
    assert ("deadbeef", "key=a3f9c2deadbeef") in got and ("ok", "ok") in got


WORDS = """
    because favourite hourglass widget terminal keyboard something through
    rhythm strength February aquarium backdrop documentation hyperlink
    function return import lambda async queue jazz echo grep mkdir rsync
    getUserName FrameId WidgetId HTTPServer README Makefile TODO
    naïve café être garçon français aujourd'hui Straße
    программа 日本語
""".split()  # noqa: SIM905

NOISE = """
    xkcd html ctrl sqrt pgrp zzzz aaaa rrggbb deadbeef abcdefab ffffffff
    QWxhZGRpbjpv aGVsbG8gd29y ZssOSysQ oZnhxC eWUwV xANy LZRoN
    tthwwhhhttt qwrtpsdfgh aeiouaeiou xqzjvkwpb bcdfghjklm
    supercalifragilisticexpialidocious
""".split()  # noqa: SIM905


def test_wordy() -> None:
    assert [w for w in WORDS if not pace.wordy(w)] == []
    assert [w for w in NOISE if pace.wordy(w)] == []
    # The token tells: digits next to the word, kinds that change.
    assert pace.wordy("cafe", "cafe")
    assert not pace.wordy("cafe", "9f8cafe01b")
    assert not pace.wordy("face", "sha256:9f86d081884c7d659a2feaa0c55ad015face")
    assert not pace.wordy("Nhbm", "U29tZSBzZWNyZXQ+Nhbm/x9Q==")
    assert pace.wordy("local", "/usr/local/share/doc")
    assert pace.wordy("widget", "self.widget.rect")
    assert pace.wordy("project", "/home/user/project42/notes")


def test_wordy_on_noise() -> None:
    """Pieces of hashes, of base64 and of base32, cut as the reader
    cuts them: few pass; words of this file: most do."""
    rng = random.Random(1)
    kept = total = 0
    for _ in range(300):
        raw = rng.randbytes(24)
        tokens = (
            base64.b64encode(raw).decode(),
            hashlib.sha256(raw).hexdigest(),
            base64.b32encode(raw).decode().lower(),
        )
        for token in tokens:
            piece = ""
            for char in token + " ":
                if char.isalpha():
                    piece += char
                    continue
                if len(piece) >= 4:
                    total += 1
                    kept += pace.wordy(piece, token)
                piece = ""
    assert total > 1000 and kept < 0.01 * total
    text = Path(__file__).read_text(encoding="utf-8").split('"""')[-1]
    words = {w for w in "".join(c if c.isalpha() else " " for c in text).split()}
    words = {w for w in words if len(w) >= 4 and w not in NOISE}
    assert sum(map(pace.wordy, words)) > 0.95 * len(words)


def test_board(tmp_path: Path) -> None:
    board = pace.Board()
    assert not board.dirty
    assert board.add("Hourglass", at=10.0) and board.add("hourglass", at=20.0)
    assert board.add("widget", at=15.0) and board.add("sand", at=30.0)
    assert not board.add("the") and not board.add("with") and not board.add("AVEC")
    assert not board.add("deadbeef") and not board.add("face", "9f8face01b")
    assert board.top(2) == [("hourglass", 2, 20.0), ("sand", 1, 30.0)]
    assert [w for w, _, _ in board.top(9)] == ["hourglass", "sand", "widget"]
    assert pace.Board(least=6).add("sand") is False
    assert pace.Board(stop=["sand"]).add("sand") is False
    assert pace.Board(stop=[]).add("with") is True

    path = tmp_path / "deep" / "pace.json"
    assert board.dirty
    board.save(path)
    assert not board.dirty
    assert stat.S_IMODE(path.stat().st_mode) == 0o600
    assert set(json.loads(path.read_text())["words"]) == {"hourglass", "sand", "widget"}
    again = pace.Board()
    again.add("sand", at=5.0)
    again.load(path)
    assert again.top(1) == [("sand", 2, 30.0)]
    again.load(tmp_path / "none.json")
    path.write_text("{]")
    again.load(path)
    assert len(again.words) == 3


def test_ago() -> None:
    assert [pace.ago(s) for s in (0, 4, 40, 60, 200, 7300, 200000)] == [
        "now",
        "now",
        "40s ago",
        "1m ago",
        "3m ago",
        "2h ago",
        "2d ago",
    ]


VIEWS = [
    pace.View(),
    pace.sample(1, 10),
    pace.sample(2, 4),
    pace.View(99999, 99999, (5,) * 180, (("a" * 24, 12345, 0.0), ("bb", 1, 9e6))),
]


@pytest.mark.parametrize("look", LOOKS)
def test_bounds(look: pace.Look) -> None:
    sizes = pace.ladder(look)
    assert 2 <= len(sizes) <= 16 and len(set(sizes)) == len(sizes)
    assert sizes == sorted(sizes, key=lambda s: (s[0] * s[1], s))
    for view in VIEWS:
        for w, h in sizes:
            for hover in (None, (0, 0), (w - 1, h - 1)):
                rows = pace.face(view, w, h, look, hover)
                assert 0 < len(rows) <= h, (w, h)
                for text in plain(rows):
                    assert len(text) <= w, (w, h, text)
                    assert all(
                        unicodedata.east_asian_width(c) not in "WF" for c in text
                    )
                    assert all(unicodedata.category(c)[0] != "C" for c in text)
                if not look.colour:
                    assert all("fg" not in run for row in rows for run in row)


def test_faces() -> None:
    top = (("hourglass", 40, 3.0), ("widget", 20, 200.0), ("sand", 5, 90000.0))
    view = pace.View(312, 345, (0,) * 170 + (10,) * 10, top)
    bare = pace.Look()
    assert plain(pace.face(view, 3, 1, bare)) == ["312"]
    assert plain(pace.face(view, 9, 1, bare)) == ["312 k/min"]
    assert plain(pace.face(view, 16, 1, pace.Look(best=True))) == ["312 k/min ▲ 345"]
    words = pace.Look(words=True)
    assert plain(pace.face(view, 24, 1, words)) == ["312 k/min ♥ hourglass"]
    assert plain(pace.face(view, 20, 4, words)) == [
        "312 k/min     62 wpm",
        "1 hourglass       40",
        "2 widget          20",
        "3 sand             5",
    ]
    rows = plain(pace.face(view, 26, 4, pace.Look(words=True, bars=True)))
    bars = [row.count("█") for row in rows[1:]]
    assert bars[0] > bars[1] > bars[2] >= 1 and rows[1].endswith(" 40")
    assert bars[0] in (2 * bars[1], 2 * bars[1] + 1)  # half, an eighth exact
    # Under the pointer: the count and when.
    over = plain(pace.face(view, 26, 4, pace.Look(words=True, bars=True), (5, 2)))
    assert over[2] == "2 widget      ×20 · 3m ago" and over[1] == rows[1]
    assert plain(pace.face(pace.View(), 20, 4, words))[1] == "no word yet"


def test_graph() -> None:
    quiet = plain(pace.graph((0,) * 180, 10, 2, pace.Look()))
    assert quiet == [" " * 10, "⣀" * 10]
    slots = (0,) * 90 + (30,) * 45 + (60,) * 45
    rows = plain(pace.graph(slots, 10, 2, pace.Look()))
    assert rows == ["     ⣀⣀⣸⣿⣿", "⣀⣀⣀⣀⣀⣿⣿⣿⣿⣿"]
    assert plain(pace.graph(slots, 4, 1, pace.Look())) == ["⣀⣀⣶⣿"]
    assert plain(pace.graph((), 3, 1, pace.Look())) == ["⣀⣀⣀"]
    full = pace.Look(graph=True, words=True)
    view = pace.sample(3, 10)
    assert plain(pace.face(view, 26, 13, full))[1:3] == plain(
        pace.graph(view.slots, 26, 2, full)
    )


def test_print(capsys: pytest.CaptureFixture[str]) -> None:
    assert pace.main(["--print"]) == 0
    assert capsys.readouterr().out.startswith("3x1\n")
    assert pace.main(["--print", "-a", "--seed", "3"]) == 0
    first = capsys.readouterr().out
    assert "32x13\n" in first and "k/min" in first and "\x1b" not in first
    assert pace.main(["--print", "-a", "--seed", "3"]) == 0
    assert capsys.readouterr().out == first
    assert pace.main(["--print", "40x6", "-w", "--top", "3"]) == 0
    assert len(capsys.readouterr().out.splitlines()) == 9


def test_options(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    args = pace.options(["-a"])
    assert args.graph and args.best and args.words and args.bars and args.colour
    assert pace.options(["-r"]).words
    stop = tmp_path / "stop"
    stop.write_text("sand\nglass  dune\n")
    assert pace.options(["--stop", str(stop)]).stop == ["sand", "glass", "dune"]
    for argv in (["--stop", str(tmp_path / "none")], ["--top", "0"], ["--min", "x"]):
        with pytest.raises(SystemExit):
            pace.options(argv)
    capsys.readouterr()


class Host:
    """A terminal that is not there: `ptyrc run` on a pty; what is
    written to it is typed on the host keyboard."""

    def __init__(self, name: str) -> None:
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        argv = [os.environ["PTYRC_BIN"], "run", "--name", name, "--"]
        argv += ["env", "PS1=$ ", "sh"]
        self.driver = subprocess.Popen(
            argv, stdin=slave, stdout=slave, stderr=slave, start_new_session=True
        )
        os.close(slave)
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self) -> None:
        try:
            while os.read(self.master, 65536):
                pass
        except OSError:
            pass

    def type(self, text: str) -> None:
        for char in text:
            os.write(self.master, char.encode())
            time.sleep(0.03)

    def close(self) -> None:
        self.driver.kill()
        self.driver.wait()
        os.close(self.master)


@pytest.fixture
def hosted() -> Iterator[tuple[Host, Pilot]]:
    name = f"pace-{os.getpid()}-{time.monotonic_ns() % 10**9}"
    host = Host(name)
    try:
        deadline = time.monotonic() + 5
        while True:
            try:
                pilot = Pilot(name)
                break
            except (PtyrcError, OSError):
                assert time.monotonic() < deadline, "no session"
                time.sleep(0.02)
        with pilot:
            pilot.wait_for("$ ")
            while not pilot.frame("raw").usable:
                assert time.monotonic() < deadline, "no usable frame"
                time.sleep(0.02)
            yield host, pilot
    finally:
        host.close()


def test_live(hosted: tuple[Host, Pilot], tmp_path: Path) -> None:
    """Typed on the host: the pace goes up, the words echoed are
    counted, a hash is not, and what is not echoed is not read."""
    host, pilot = hosted
    save = tmp_path / "pace.json"
    argv = ["-s", pilot.session, "-a", "--save", str(save)]
    status: list[int] = []
    thread = threading.Thread(target=lambda: status.append(pace.main(argv, seconds=40)))
    thread.start()
    deadline = time.monotonic() + 5
    while not any(w["state"] == "placed" for w in pilot.widgets()):
        assert time.monotonic() < deadline, "no widget"
        time.sleep(0.02)
    host.type("echo hourglass widget hourglass 9f86d081884cdeadbeef widgte\b\bet\n")
    host.type("stty -echo\n")
    pilot.wait_for("stty")
    time.sleep(0.2)
    host.type("hunter secret\n")
    seen = False
    while thread.is_alive():
        lines = pilot.shown()
        seen = seen or (
            any(
                " hourglass " in line and line.rstrip("│ ").endswith("2")
                for line in lines
            )
            and any(
                "k/min" in line and not line.strip("│╭ ").startswith("0 ")
                for line in lines
            )
        )
        session_clock.sleep(0.05)
    thread.join()
    assert status == [0] and seen
    words = {w: c for w, (c, _) in json.loads(save.read_text())["words"].items()}
    assert words == {"echo": 2, "hourglass": 2, "widget": 2, "stty": 1}
