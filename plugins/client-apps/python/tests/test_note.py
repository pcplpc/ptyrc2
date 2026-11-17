# SPDX-License-Identifier: GPL-3.0-or-later
"""`note`: the reduced markdown, every variant in its box, a file
read again when it changes."""

from __future__ import annotations

import os
import threading
import time
import unicodedata
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from ptyrc import Pilot, clock
from ptyrc_apps import note

if TYPE_CHECKING:
    from conftest import Host

TEXT = """# Groceries

- [x] bread


- [ ] milk, the **whole** one, from the [shop](http://x) at the corner
* [X] eggs
- [ ] coffee
  - [ ] filters

## Later ##

1. call the `plumber`
> quoted
plain text

"""


class Skip:
    """The session clock, for the app alone: `end` jumps past the
    `seconds` it was given."""

    def __init__(self) -> None:
        self.by = 0.0

    def monotonic(self) -> float:
        return clock.monotonic() + self.by

    def end(self) -> None:
        self.by = 1e6


def text(rows: list[list[dict[str, object]]]) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


def made(source: str = TEXT, name: str = "TODO.md") -> note.Note:
    return note.Note(name, tuple(note.parse(source)))


def test_parse() -> None:
    assert [(i.kind, i.text, i.indent) for i in note.parse(TEXT)] == [
        ("head", "Groceries", 0),
        ("blank", "", 0),
        ("done", "bread", 0),
        ("blank", "", 0),
        ("open", "milk, the whole one, from the shop at the corner", 0),
        ("done", "eggs", 0),
        ("open", "coffee", 0),
        ("open", "filters", 1),
        ("blank", "", 0),
        ("head", "Later", 0),
        ("blank", "", 0),
        ("bullet", "call the plumber", 0),
        ("text", "quoted", 0),
        ("text", "plain text", 0),
    ]
    assert note.parse("") == [] and note.parse("\n\n") == []
    assert note.tally(made()) == (2, 5) and note.tally(made("words")) == (0, 0)


def test_wrap() -> None:
    assert note.wrap("one two three", 7) == ["one two", "three"]
    assert note.wrap("abcdefghij kl", 4) == ["abcd", "efgh", "ij", "kl"]
    assert note.wrap("", 5) == [""]


def test_one_line() -> None:
    n = made()
    assert text(note.render(n, 5, 1)) == ["2/5"]
    assert text(note.render(n, 18, 1)) == ["2/5 milk, the who…"]
    done = made("- [x] a\n- [x] b\n")
    assert text(note.render(done, 18, 1)) == ["2/2 all done"]
    assert text(note.render(made("# Title\nwords"), 18, 1)) == ["Title"]
    assert text(note.render(made(""), 18, 1)) == ["empty"]
    gone = note.Note("x.md", missing=True)
    assert text(note.render(gone, 18, 3)) == ["no x.md"]
    assert text(note.render(gone, 5, 1)) == ["—"]


def test_fits_or_not() -> None:
    n = made()
    full = text(note.render(n, 48, 16))
    assert full[0] == "Groceries" and full[2] == "☑ bread"
    assert full[4:6] == ["☐ milk, the whole one, from the shop at the", "  corner"]
    assert "  ☐ filters" in full and full[-3:] == [
        "• call the plumber",
        "quoted",
        "plain text",
    ]
    # Too long: without what is done.
    less = text(note.render(n, 48, 12))
    assert "☑ bread" not in less and "Later" in less and len(less) == 12
    # Shorter yet: the open boxes alone; then cut.
    assert text(note.render(n, 30, 6)) == [
        "☐ milk, the whole one, from",
        "  the shop at the corner",
        "☐ coffee",
        "  ☐ filters",
    ]
    assert text(note.render(n, 24, 3)) == [
        "☐ milk, the whole one,",
        "  from the shop at the",
        "  corner …",
    ]
    assert text(note.render(made("a\nb\nc\nd"), 10, 2)) == ["a", "b …"]


def test_looks() -> None:
    n = made()
    plain = note.render(n, 48, 16)
    assert plain[0] == [{"t": "Groceries", "bold": True}] and plain[2] == [
        {"t": "☑ bread"}
    ]
    assert note.render(n, 48, 16, note.Look(done=True))[2][0]["fg"] == "bright-black"
    hidden = text(note.render(n, 48, 16, note.Look(open=True)))
    assert not any("☑" in row for row in hidden) and "☐ coffee" in hidden
    counted = text(note.render(n, 20, 6, note.Look(count=True)))
    assert counted[0] == "━━━━━━╾───────── 2/5" and counted[1].startswith("☐ milk")
    assert note.title(n, note.Look(count=True)) == "TODO.md 2/5"
    assert note.title(n) == note.title(made("x"), note.Look(count=True)) == "TODO.md"
    paper = note.render(n, 30, 6, note.Look(postit=True, done=True))
    for row in paper:
        assert sum(len(r["t"]) for r in row) == 30
        assert all(r["bg"] == note.PAPER for r in row)
    assert (
        note.POSTIT["style"] == {"fg": note.PAPER} and len(note.POSTIT["corners"]) == 4
    )


@pytest.mark.parametrize(
    "look",
    [note.Look(), note.Look(count=True, done=True, postit=True), note.Look(open=True)],
)
def test_every_variant(look: note.Look) -> None:
    sources = (TEXT, "", "word " * 80, "- [x] a\n", "x" * 300, "- [ ] " + "y" * 90)
    for n in [*map(made, sources), note.Note("gone", missing=True)]:
        for w, h in [*note.SIZES, (1, 1), (3, 2), (7, 3), (120, 40)]:
            rows = note.render(n, w, h, look)
            assert 0 < len(rows) <= h, (w, h)
            for row in text(rows):
                assert len(row) <= w, (w, h, row)
                assert all(unicodedata.east_asian_width(c) != "W" for c in row)
        sizes = note.trimmed(n, look)
        assert [w for w, _ in sizes] == [w for w, _ in note.SIZES]
        assert all(1 <= h <= s[1] for (_, h), s in zip(sizes, note.SIZES, strict=True))
        assert len(note.named(sizes)) == len(note.SIZES)


def test_box() -> None:
    n = made()
    look = note.Look(postit=True, count=True)
    lines = note.box(
        note.title(n, look), note.render(n, 24, 3, look), 24, 3, look, "round", False
    )
    assert lines[0] == "█ TODO.md 2/5 ███████████◣" and lines[-1] == "▀" * 26
    assert all(len(line) == 26 and line[0] == line[-1] == "█" for line in lines[1:-1])
    lines = note.box(
        "TODO.md", note.render(n, 18, 1), 18, 1, note.Look(), "round", False
    )
    assert lines == [
        "╭" + "─" * 18 + "╮",
        "│2/5 milk, the who…│",
        "╰───────── TODO.md ╯",
    ]
    coloured = note.box("t", note.render(n, 18, 1, look), 18, 1, look, "round", True)
    assert f"\x1b[48;5;{note.PAPER}m" in coloured[1]


def test_options(
    tmp_path: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    args = note.options([])
    assert args.files == [Path("TODO.md")] and args.look == note.Look()
    assert (args.border, args.merged, args.print) == ("round", False, None)
    assert note.options(["-a"]).look == note.Look(count=True, done=True, postit=True)
    args = note.options(["a.md", "b.md", "-o", "--merged", "--print", "30x4"])
    assert args.files == [Path("a.md"), Path("b.md")] and args.look.open
    assert args.merged and args.print == (30, 4)
    with pytest.raises(SystemExit):
        note.options(["--cycle", "0"])
    capsys.readouterr()
    path = tmp_path / "TODO.md"
    path.write_text(TEXT)
    monkeypatch.chdir(tmp_path)
    assert note.main(["--print"]) == 0
    out = capsys.readouterr().out
    assert out.count("╭") == len(note.SIZES) and "─ TODO.md ╯" in out
    assert note.main(["--print", "20x2", "-p", "nope.md"]) == 0
    assert "█no nope.md" in capsys.readouterr().out
    assert path.read_text() == TEXT


def test_live(session: Pilot, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A widget a file; a file written is read again, and never
    written by the app."""
    monkeypatch.setattr(note, "clock", skip := Skip())
    one, two = tmp_path / "TODO.md", tmp_path / "ideas.md"
    one.write_text("- [ ] first thing\n- [x] second\n")
    two.write_text("an idea\n")
    seen: dict[str, object] = {}
    argv = ["-s", session.session, "-c", str(one), str(two)]
    thread = threading.Thread(
        target=lambda: seen.update(status=note.main(argv, seconds=600))
    )
    thread.start()

    def until(what: str, test: object) -> None:
        deadline = time.monotonic() + 8
        while not test():  # type: ignore[operator]
            assert time.monotonic() < deadline and thread.is_alive(), what
            time.sleep(0.03)

    def titles() -> set[object]:
        return {w["title"] for w in session.widgets() if w["state"] == "placed"}

    def shown() -> str:
        return "\n".join(session.dump()["host"])

    try:
        until("placed", lambda: titles() == {"TODO.md 1/2", "ideas.md"})
        until("drawn", lambda: "first thing" in shown() and "an idea" in shown())
        stamp = one.stat().st_mtime_ns
        one.write_text("- [x] first thing\n- [x] second\n- [ ] a third, new\n")
        os.utime(one, ns=(stamp + 10**9, stamp + 10**9))
        until(
            "read again",
            lambda: "TODO.md 2/3" in titles() and "a third, new" in shown(),
        )
        two.unlink()
        until("gone", lambda: "no ideas.md" in shown())
    finally:
        skip.end()
        thread.join()
    assert seen == {"status": 0}
    assert one.read_text() == "- [x] first thing\n- [x] second\n- [ ] a third, new\n"


def test_live_merged(session: Pilot, tmp_path: Path) -> None:
    one, two = tmp_path / "a.md", tmp_path / "b.md"
    one.write_text("- [ ] alpha\n")
    two.write_text("beta\n")
    seen: dict[str, object] = {}
    argv = [
        "-s",
        session.session,
        "--merged",
        "--cycle",
        "0.5",
        "-p",
        str(one),
        str(two),
    ]
    thread = threading.Thread(
        target=lambda: seen.update(status=note.main(argv, seconds=5))
    )
    thread.start()
    titles: set[object] = set()
    tags: set[object] = set()
    while thread.is_alive():
        widgets = session.widgets()
        titles.update(w["title"] for w in widgets)
        tags.update(w["tag"] for w in widgets)
        time.sleep(0.02)
    thread.join()
    assert seen == {"status": 0} and titles == {"a.md", "b.md"} and tags == {"note"}


# -i


def desk_keys(desk: note.Desk, source: str, *names: str) -> list[object]:
    """Feed keys; the changes asked, each made in `source` at once."""
    out: list[object] = []
    for name in names:
        what = desk.key(note.listed(made(source)), name)
        if what is not None:
            out.append(what)
    return out


def test_items_know_their_line() -> None:
    items = note.parse(TEXT)
    assert [(i.line, i.raw) for i in items if i.kind != "blank"] == [
        (0, "Groceries"),
        (2, "bread"),
        (5, "milk, the **whole** one, from the [shop](http://x) at the corner"),
        (6, "eggs"),
        (7, "coffee"),
        (8, "filters"),
        (10, "Later"),
        (12, "call the `plumber`"),
        (13, "quoted"),
        (14, "plain text"),
    ]
    # Where it is written is not what it is.
    assert note.Item("open", "a", 0, 3, "a") == note.Item("open", "a")
    assert [i.text for i in note.listed(made(), note.Look(open=True))][:3] == [
        "Groceries",
        "milk, the whole one, from the shop at the corner",
        "coffee",
    ]


SMALL = (
    "# List\n\n- [x] bread\n* [ ] milk, the **whole** one\n  1. [ ] filters\nplain\n"
)


def test_change() -> None:
    lines = SMALL.splitlines()

    def changed(*what: object, source: str = SMALL) -> list[str]:
        return note.change(source, what).splitlines()

    assert changed("tick", 2)[2] == "- [ ] bread"
    assert changed("tick", 3)[3] == "* [x] milk, the **whole** one"
    assert changed("tick", 5) == lines  # no box there
    assert changed("words", 3, "cream")[3] == "* [ ] cream"
    assert changed("words", 0, "Food")[0] == "# Food"
    assert changed("words", 4, "paper")[4] == "  1. [ ] paper"
    assert changed("words", 5, "bare")[5] == "bare"
    assert changed("delete", 4) == lines[:4] + lines[5:]
    assert changed("swap", 2, 3)[2:4] == [
        "* [ ] milk, the **whole** one",
        "- [x] bread",
    ]
    # Added: the indent and the mark of the line above, an open box.
    assert changed("add", 2, "jam")[2:4] == ["- [x] bread", "- [ ] jam"]
    assert changed("add", 3, "jam")[4] == "* [ ] jam"
    assert changed("add", 4, "jam")[5] == "  1. [ ] jam"
    assert changed("add", 0, "jam")[1] == "- [ ] jam"
    assert changed("add", -1, "jam") == [*lines, "- [ ] jam"]
    # The ends of the lines are kept; a last line without one gets one.
    assert note.change("- a\r\n- b", ("add", 1, "c")) == "- a\r\n- b\n- [ ] c\n"
    assert note.change("- a\r\n- b\r\n", ("add", 0, "c")) == "- a\r\n- [ ] c\r\n- b\r\n"
    assert note.change("- a\r\n- b", ("swap", 0, 1)) == "- b\r\n- a"
    assert note.change("", ("add", -1, "c")) == "- [ ] c\n"
    assert note.change("x", ("add", -1, "c")) == "x\n- [ ] c\n"
    assert note.change("> said\n", ("words", 0, "told")) == "> told\n"


def test_desk() -> None:
    desk = note.Desk()
    assert desk_keys(desk, SMALL, "down", "space") == [("tick", 2)]
    assert desk_keys(desk, SMALL, "j", "enter", "end", "space") == [("tick", 3)]
    assert desk.sel == 4
    # A move: the bar follows its item.
    assert desk_keys(desk, SMALL, "shift+up", "K") == [("swap", 5, 4), ("swap", 4, 3)]
    assert desk.sel == 2
    assert desk_keys(desk, SMALL, "home", "shift+up", "J") == [("swap", 0, 2)]
    assert desk.sel == 1
    # Added under the bar; the bar goes on it. Nothing typed: nothing.
    assert desk_keys(desk, SMALL, "a", "j", "a", "space", "m", "enter") == [
        ("add", 2, "ja m")
    ]
    assert desk.sel == 2 and desk.mode is None
    assert desk_keys(desk, SMALL, "a", "enter", "a", "x", "ctrl+c") == []
    assert desk.sel == 2 and desk.mode is None
    # Edited: the words as written; the same words: nothing.
    assert desk_keys(desk, SMALL, "e", "enter") == []
    assert desk_keys(desk, SMALL, "e", "ctrl+u", "o", "k", "enter") == [
        ("words", 3, "ok")
    ]
    desk.key(note.listed(made(SMALL)), "e")
    assert desk.editor.text == "milk, the **whole** one"
    desk.paste("more\nof it")
    assert desk.editor.text.endswith("onemore of it")
    # The focus lost: dropped.
    assert desk.rest() and desk.mode is None and not desk.rest()
    # Deleted when `y` follows; any other key keeps.
    assert desk_keys(desk, SMALL, "d", "n", "d", "y") == [("delete", 3)]
    # An empty note: a line can be added, nothing else.
    desk = note.Desk()
    assert desk_keys(desk, "", "space", "e", "d", "y", "J", "a", "x", "enter") == [
        ("add", -1, "x")
    ]


@pytest.mark.parametrize("look", [note.PLAIN, note.Look(True, True, True, True)])
def test_desk_every_size(look: note.Look) -> None:
    for source in (SMALL, TEXT, ""):
        for mode in (None, "add", "edit", "delete"):
            desk = note.Desk()
            desk.sel, desk.mode = 2, mode
            for w, h in [*note.SIZES, (1, 1), (3, 2), (4, 3), (60, 20)]:
                rows = note.render(made(source), w, h, look, desk)
                assert len(rows) == h
                assert all(sum(map(cells, text([row]))) == w for row in rows), (w, h)
                if look.postit:
                    assert all(r["bg"] == note.PAPER for row in rows for r in row)


def cells(line: str) -> int:
    return sum(2 if unicodedata.east_asian_width(c) in "WF" else 1 for c in line)


def test_desk_render() -> None:
    desk = note.Desk()
    desk.sel = 1
    rows = note.render(made(SMALL), 30, 7, note.PLAIN, desk)
    assert text(rows) == [
        "List".ljust(30),
        "☑ bread".ljust(30),
        "☐ milk, the whole one".ljust(30),
        "  ☐ filters".ljust(30),
        "plain".ljust(30),
        " " * 30,
        "space tick  a add  esc leave".ljust(30),
    ]
    assert [bool(row[0].get("inverse")) for row in rows] == [False, True] + [False] * 5
    # Adding: the line typed is under the bar, with a cursor.
    for name in ("a", "j"):
        desk.key(note.listed(made(SMALL)), name)
    rows = note.render(made(SMALL), 30, 7, note.PLAIN, desk)
    assert text(rows)[2] == "☐ j".ljust(30) and text(rows)[3].startswith("☐ milk")
    assert any(run.get("inverse") for run in rows[2])
    assert text(rows)[-1].startswith("enter ok  ^C cancel")
    desk.rest()
    # Too long for the rows: it scrolls with the bar; two rows: no foot.
    desk.sel = 4
    assert text(note.render(made(SMALL), 12, 2, note.PLAIN, desk)) == [
        "  ☐ filters ",
        "plain".ljust(12),
    ]
    desk.key(note.listed(made(SMALL)), "d")
    assert text(note.render(made(SMALL), 20, 3, note.PLAIN, desk))[-1].startswith(
        "y delete"
    )
    # A row more for the foot, from two rows on.
    plain, foot = note.trimmed(made(SMALL)), note.trimmed(made(SMALL), foot=True)
    assert [(w, h + (h > 1 or n > 1)) for n, (w, h) in enumerate(plain)] == foot


def test_edit(tmp_path: Path) -> None:
    path = tmp_path / "TODO.md"
    path.write_text(SMALL)
    path.chmod(0o640)
    read = note.read(path)
    assert note.edit(path, read, ("tick", 3))
    assert path.read_text().splitlines()[3] == "* [x] milk, the **whole** one"
    assert path.stat().st_mode & 0o777 == 0o640
    assert os.listdir(tmp_path) == ["TODO.md"]
    # The file is no longer as it was read: nothing is written.
    assert not note.edit(path, read, ("tick", 3))
    assert not note.edit(path, read, ("swap", 2, 3))
    assert note.edit(path, read, ("tick", 2))  # that line stands
    path.write_text("- [ ] one\n")
    assert not note.edit(path, read, ("delete", 4))
    assert path.read_text() == "- [ ] one\n"
    # A file that is not there is made by a line added.
    new = tmp_path / "new.md"
    assert note.edit(new, note.read(new), ("add", -1, "first"))
    assert new.read_text() == "- [ ] first\n"
    assert not note.edit(tmp_path / "no" / "x.md", note.Note("x"), ("add", -1, "a"))


def test_live_interactive(
    hosted: tuple[Host, Pilot], tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Focused, the keys of the host keyboard write the file."""
    host, pilot = hosted
    monkeypatch.setattr(note, "clock", skip := Skip())
    path = tmp_path / "TODO.md"
    path.write_text("# List\n\n- [ ] first\n- [x] second\n")
    seen: dict[str, object] = {}
    argv = ["-s", pilot.session, "-i", "-c", str(path)]
    thread = threading.Thread(
        target=lambda: seen.update(status=note.main(argv, seconds=600))
    )
    thread.start()

    def until(what: str, test: object) -> None:
        deadline = time.monotonic() + 8
        while not test():  # type: ignore[operator]
            assert time.monotonic() < deadline and thread.is_alive(), what
            time.sleep(0.03)

    def info() -> dict[str, object]:
        return next(iter(w for w in pilot.widgets() if w["tag"] == "note"), {})

    def shown() -> str:
        return "\n".join(pilot.dump()["host"])

    def typed(keys: bytes, what: str, lines: list[str]) -> None:
        os.write(host.master, keys)
        until(what, lambda: path.read_text().splitlines() == lines)

    try:
        until("asks", lambda: info().get("asks") == "quiet" and "first" in shown())
        assert "space tick" not in shown()
        pilot.focus(info()["id"])  # type: ignore[arg-type]
        until("focused", lambda: "space tick" in shown())
        typed(b"j ", "ticked", ["# List", "", "- [x] first", "- [x] second"])
        until("counted", lambda: info()["title"] == "TODO.md 2/2")
        typed(
            b"athird\r",
            "added",
            ["# List", "", "- [x] first", "- [ ] third", "- [x] second"],
        )
        typed(
            b"\x1b[1;2A",
            "moved",
            ["# List", "", "- [ ] third", "- [x] first", "- [x] second"],
        )
        typed(
            b"e\x15\x1b[200~3rd\r\x1b[201~\r",
            "edited",
            ["# List", "", "- [ ] 3rd", "- [x] first", "- [x] second"],
        )
        typed(b"jdy", "deleted", ["# List", "", "- [ ] 3rd", "- [x] second"])
        # Escape while a line is typed: dropped, the keys are the shell's.
        os.write(host.master, b"alost")
        until("typing", lambda: "lost" in shown())
        os.write(host.master, b"\x1b")
        until("left", lambda: not info()["focused"] and "lost" not in shown())
        assert "space tick" not in shown()
        os.write(host.master, b"echo h$((1+1))\r")
        pilot.wait_for("h2")
    finally:
        skip.end()
        thread.join()
    assert seen == {"status": 0}
    assert path.read_text() == "# List\n\n- [ ] 3rd\n- [x] second\n"


def test_interactive_is_not_in_all() -> None:
    assert not note.options(["-a"]).interactive
    assert note.options(["-i"]).interactive and not note.options(["-i"]).look.count
