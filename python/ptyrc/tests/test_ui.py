# SPDX-License-Identifier: GPL-3.0-or-later
"""`ptyrc.ui`: the models (pure), the runner on a scripted pilot, then
on a driver."""

from __future__ import annotations

import base64
from typing import Any

import pytest
from ptyrc import Pilot, PtyrcError, ui
from ptyrc.ui import CANCEL, DONE, Choice, Dialog, Editor, Form, Line, Listing, Text
from ptyrc.widgets import Widgets
from tests_support import until

SIZES = [(1, 1), (5, 1), (5, 2), (12, 3), (18, 1), (30, 6), (48, 16), (200, 60)]


def text(rows: ui.Rows) -> list[str]:
    return ["".join(run["t"] for run in row) for row in rows]


def keys(model: ui.Model, *names: str) -> str | None:
    flow = None
    for name in names:
        flow = model.key(name)
    return flow


def models() -> list[ui.Model]:
    return [
        Choice("Deploy to production?", ["yes", "no", "later"]),
        Choice("Pick", [f"option {i}" for i in range(30)], multi=True, other=True),
        Choice(),
        Line("Name?", default="a wide 漢字 line that is rather long"),
        Text("Message", default="one\ntwo\nthree\nfour\nfive\nsix"),
        Form([Line("First"), Choice("Second", ["a", "b"]), Text("Third")]),
    ]


# -- cells ------------------------------------------------------------------


def test_cells() -> None:
    assert ui.cells("abc") == 3 and ui.cells("漢字") == 4 and ui.cells("é") == 1
    assert ui.cut("hello", 5) == "hello" and ui.cut("hello", 4) == "hel…"
    assert ui.cut("漢字漢", 4) == "漢…" and ui.cut("abc", 0) == ""
    assert text([ui.fit("ab", 4)]) == ["ab  "]
    assert text([ui.fit([ui.run("a漢", bold=True)], 2)]) == ["a "]
    assert ui.fit("abc", 0) == [{"t": ""}]
    assert ui.run("a", bold=True, dim=False) == {"t": "a", "bold": True}
    assert ui.wrap("one two three four", 9, 2) == ["one two", "three fo…"]
    assert ui.wrap("one two", 20, 3) == ["one two"] and ui.wrap("", 5, 2) == []


def test_foot() -> None:
    all_ = [("↑↓ move", 2), ("enter ok", 1), ("^C dismiss", 3), ("esc later", 0)]
    assert ui.foot(all_, 60) == "↑↓ move  enter ok  ^C dismiss  esc later"
    assert ui.foot(all_, 30) == "↑↓ move  enter ok  esc later"
    assert ui.foot(all_, 20) == "enter ok  esc later"
    assert ui.foot(all_, 10) == "esc later"
    assert ui.foot(all_, 5) == "esc …"


# -- keys -------------------------------------------------------------------


def test_keys() -> None:
    name = ui.Keys.name
    assert name(b"\x1b[1;2A") == "shift+up" and name(b"\x1b[1;2D") == "shift+left"
    assert name(b"\x1b[1;5C") == "ctrl+right" and name(b"\x1b[1;3B") == "alt+down"
    assert name(b"\x1b[1;8F") == "ctrl+alt+shift+end" and name(b"\x1b[1;6H")
    assert name(b"\x1b[3;5~") == "ctrl+delete" and name(b"\x1b[5;2~") == "shift+pageup"
    for raw in (b"\x1b[A", b"\x1b[1;1A", b"\x1b[2;2A", b"\x1b[9;2~", b"a", b""):
        assert name(raw) is None, raw
    assert name(b"\x1b[<35;1;1M") is None and name(b"\x1b[200~") is None

    keys = ui.Keys()

    def feed(named: str | None, raw: bytes = b"") -> Any:
        data = {"key": named, "bytes": base64.b64encode(raw).decode(), "widget": 7}
        return keys.feed(data)

    assert feed("a", b"a") == ("key", "a") and feed("up") == ("key", "up")
    assert feed(None, b"\x1b[1;2A") == ("key", "shift+up")
    assert feed("f1", b"\x1b[1;2A") == ("key", "f1")  # the driver's name first
    assert feed(None, b"\x1b[<35;1;1M") is None and keys.feed({"key": None}) is None
    assert feed(None, b"\x1b[200~") is None
    assert feed("a", b"a") is None and feed("enter", b"\r") is None
    assert feed(None, "é".encode()) is None and feed(None, b"\x1b[1;2A") is None
    assert feed("b", b"\r\nb") is None
    assert feed(None, b"\x1b[201~") == ("paste", "a\né\x1b[1;2A\nb")
    assert feed(None, b"\x1b[200~") is None and feed("x", b"x") is None
    keys.reset()  # the focus lost: the paste is dropped
    assert feed("y", b"y") == ("key", "y")


# -- pieces -----------------------------------------------------------------


def test_editor() -> None:
    ed = Editor("hello world")
    assert ed.cursor == 11
    for name in ("ctrl+w", "x", "space", "é"):
        assert ed.key(name)
    assert ed.text == "hello x é"
    assert ed.key("home") and ed.key("delete") and ed.key("right")
    assert (ed.text, ed.cursor) == ("ello x é", 1)
    assert ed.key("ctrl+k") and ed.text == "e"
    ed.insert("a\tb\n")
    assert ed.text == "ea b " and ed.key("ctrl+u") and ed.text == ""
    assert ed.key("backspace") and ed.key("left") and ed.cursor == 0
    assert not ed.key("up") and not ed.key("enter") and not ed.key("ctrl+c")


def test_editor_render() -> None:
    ed = Editor()
    assert text([ed.render(6, False, "name")]) == ["name  "]
    assert ed.render(6, False, "name")[0].get("dim")
    row = ed.render(6, True, "name")
    assert row[0] == {"t": " ", "inverse": True} and text([row]) == [" name "]
    ed.insert("abcdefghij")
    assert text([ed.render(6, False)]) == ["abcde…"]
    assert text([ed.render(6, True)]) == ["fghij "]  # the cursor after the end
    for _ in range(7):
        ed.key("left")
    row = ed.render(6, True)
    assert text([row]) == ["defghi"] and row[0] == {"t": "d", "inverse": True}
    ed.key("end")
    assert text([ed.render(6, True)]) == ["fghij "]
    wide = Editor("漢字漢字")
    assert text([wide.render(5, True)]) == ["漢字 "] and wide.render(1, True)


def test_listing() -> None:
    ls = Listing(12)
    assert list(ls.window(4)) == [0, 1, 2, 3] and ls.mark(4) == "1/12"
    assert ls.arrow(0, 4) == "" and ls.arrow(3, 4) == "↓"
    assert ls.key("down") and ls.key("j") and ls.key("tab") and ls.key("pagedown")
    assert ls.sel == 7 and list(ls.window(4)) == [4, 5, 6, 7]
    assert ls.arrow(4, 4) == "↑" and ls.arrow(7, 4) == "↓" and ls.mark(4) == "8/12"
    assert ls.key("end") and ls.sel == 11 and ls.arrow(11, 4) == ""
    assert ls.key("k") and ls.key("up") and ls.key("home") and ls.sel == 0
    assert not ls.key("j", letters=False) and not ls.key("enter")
    assert ls.mark(12) == "" and list(Listing().window(3)) == []


# -- models -----------------------------------------------------------------


@pytest.mark.parametrize("focused", [False, True])
def test_exact_at_every_size(focused: bool) -> None:
    for model in models():
        for name in ("down", "x", "ctrl+j", "end", "space"):
            model.key(name)
        for w, h in [*SIZES, *model.sizes()]:
            rows = model.render(w, h, focused)
            assert len(rows) == h, (model, w, h)
            assert all(ui.cells(line) == w for line in text(rows)), (model, w, h)
        sizes = model.sizes()
        assert len(set(sizes)) == len(sizes) <= 16
        assert all(1 <= w <= 200 and 1 <= h <= 60 for w, h in sizes)


def test_choice() -> None:
    model = Choice("Deploy?", ["yes", "no", "later"])
    assert text(model.render(20, 5, False)) == [
        "Deploy?             ",
        "1 yes               ",
        "2 no                ",
        "3 later             ",
        "ctrl+t ctrl+t       ",
    ]
    rows = model.render(44, 5, True)
    assert text(rows)[4].rstrip() == "↑↓ move  enter ok  ^C dismiss  esc later"
    assert all(run.get("inverse") for run in rows[1])
    assert not any(run.get("inverse") for run in rows[2])
    assert keys(model, "down", "j", "j", "k") is None and model.listing.sel == 1
    assert keys(model, "3") is None and model.result is None
    assert keys(model, "enter") == DONE and model.result == "later"
    assert Choice("?", ["a"]).key("ctrl+c") == CANCEL
    # One row: the bar when focused, else the question.
    assert text(model.render(9, 1, True)) == ["3 later ↑"]
    assert text(model.render(9, 1, False)) == ["Deploy?  "]


def test_choice_scrolls() -> None:
    model = Choice("Pick", [f"o{i}" for i in range(12)])
    model.render(16, 6, True)  # a page is the 4 rows shown
    keys(model, "pagedown", "down")
    rows = text(model.render(16, 6, True))
    assert rows[0] == "Pick        6/12"
    assert rows[1:5] == [
        "3 o2           ↑",
        "4 o3            ",
        "5 o4            ",
        "6 o5           ↓",
    ]


def test_choice_multi_other() -> None:
    model = Choice("Which?", ["a", "b", "c"], multi=True, other=True)
    assert text(model.render(12, 4, True))[1] == "1 [ ] a     "
    keys(model, "space", "down", "down", "space", "down")
    assert text(model.render(12, 6, False))[1:5] == [
        "1 [x] a     ",
        "2 [ ] b     ",
        "3 [x] c     ",
        "4 other…    ",
    ]
    # On the last row the keys are typed, the arrows still move.
    keys(model, "j", "1", "space", "k")
    model.paste(" pasted\n")
    assert model.other is not None and model.other.text == "j1 k pasted"
    assert keys(model, "up", "space", "enter") == DONE
    assert model.result == ["a", "j1 k pasted"]
    single = Choice("?", ["a"], other=True)
    assert keys(single, "down", "enter") is None  # nothing typed
    assert keys(single, "z", "enter") == DONE and single.result == "z"


def test_line() -> None:
    model = Line("Name?", default="bob", placeholder="you")
    keys(model, "ctrl+u")
    assert text(model.render(10, 3, False)) == [
        "Name?     ",
        "you       ",
        "ctrl+t ct…",
    ]
    keys(model, "a", "l")
    model.paste("ice\n")
    row = model.render(10, 3, True)[1]
    assert text([row]) == ["alice     "] and row[1] == {"t": " ", "inverse": True}
    assert model.key("ctrl+c") == CANCEL and model.result is None
    assert model.key("enter") == DONE and model.result == "alice"


def test_text() -> None:
    model = Text("Message")
    keys(model, "a", "b", "alt+enter", "c", "ctrl+j", "d")
    assert model.text == "ab\nc\nd"
    keys(model, "up", "up", "end", "right")  # to the start of the next line
    assert (model.row, model.lines[1].cursor) == (1, 0)
    keys(model, "backspace")  # joined, the cursor between
    assert model.text == "abc\nd" and (model.row, model.lines[0].cursor) == (0, 2)
    keys(model, "end", "delete")
    assert model.text == "abcd" and keys(model, "end") is None
    model.paste("1\n2\n3")
    assert model.text == "abcd1\n2\n3" and model.row == 2
    rows = text(model.render(12, 4, True))
    assert rows[0] == "Message  3/3" and rows[1:3] == ["2           ", "3           "]
    assert keys(model, "left", "left") is None and model.row == 1
    assert model.key("enter") == DONE and model.result == "abcd1\n2\n3"


def test_form() -> None:
    form = Form([Line("Name?"), Choice("Colour?", ["red", "blue"]), Text("Why?")])
    assert form.question == "Name?"
    assert text(form.render(20, 3, True))[0] == "Name?            1/3"
    assert keys(form, "shift+tab", "z", "enter") is None and form.at == 1
    assert "shift+tab back" in text(form.render(70, 4, True))[3]
    assert keys(form, "down", "shift+tab", "ctrl+u", "y", "enter") is None
    assert keys(form, "enter", "o", "k") is None and form.at == 2
    assert text(form.render(20, 4, False))[0] == "Why?             3/3"
    assert form.key("enter") == DONE and form.result == ["y", "blue", "ok"]
    assert Form([Line()]).key("ctrl+c") == CANCEL
    with pytest.raises(ValueError):
        Form([])


# -- the runner, on a scripted pilot ----------------------------------------


class Script(Widgets):
    """What `Dialog` needs of a pilot: the calls are kept, the events
    are those queued."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self.queue: list[tuple[str, Any] | None] = []
        self.topics: list[str] = []
        self.info = {
            "id": 7,
            "state": "placed",
            "variant": "30x6",
            "size": [30, 6],
            "asks": None,
            "focused": False,
        }

    def call(self, method: str, **params: Any) -> Any:
        return self._request(method, params)

    def _request(self, method: str, params: dict[str, Any], then: Any = None) -> Any:
        self.calls.append((method, params))
        if method == "widget.ask":
            self.info = {
                **self.info,
                "asks": "quiet" if params.get("quiet") else "loud",
            }
        result = {"widget": dict(self.info)}
        if then is not None:
            then(result)
        return result

    def subscribe(self, *topics: str) -> None:
        self.topics += topics

    def event(self, timeout: float | None = None) -> tuple[str, Any] | None:
        if not self.queue:
            raise PtyrcError("gone", "connection closed")
        return self.queue.pop(0)

    def named(self, method: str) -> list[dict[str, Any]]:
        return [params for name, params in self.calls if name == method]

    def shown(self) -> list[str]:
        return text(self.named("widget.set")[-1]["content"]["cells"])


def key(name: str | None, raw: bytes = b"") -> tuple[str, Any]:
    data = {"key": name, "bytes": base64.b64encode(raw).decode(), "widget": 7}
    return ("input", data)


def test_dialog() -> None:
    pilot = Script()
    model = Choice("Deploy?", ["yes", "no"])
    dialog = Dialog(pilot, model, tag="deploy", title="q", prefer="top")  # type: ignore[arg-type]
    assert pilot.topics == ["widgets"]
    declared = pilot.named("widget.declare")[0]["widgets"]
    assert [tuple(v["size"]) for v in declared] == model.sizes()
    assert all(v["tag"] == "deploy" and v["content"]["cells"] for v in declared)
    show = pilot.named("widget.show")[0]
    assert show == {"tag": "deploy", "title": "q", "prefer": "top", "fill": 1.0}
    assert pilot.named("widget.ask") == [{"id": 7}] and dialog.widget.asks == "loud"

    # Not its own: another widget, a key of another, another topic.
    assert not dialog.take(("widgets", {"id": 8, "state": "placed"}))
    assert not dialog.take(("input", {"key": "a", "bytes": "", "widget": 8}))
    assert not dialog.take(("cursor", {})) and not dialog.take(None)
    assert pilot.named("widget.set") == []

    focused = {**pilot.info, "asks": "loud", "focused": True}
    assert dialog.take(("widgets", focused))
    assert pilot.shown()[1] == "1 yes".ljust(30) and "enter ok" in pilot.shown()[5]
    assert dialog.take(("widgets", focused)) and len(pilot.named("widget.set")) == 1
    assert dialog.take(key("down")) and len(pilot.named("widget.set")) == 2
    assert dialog.take(key(None, b"\x1b[<0;1;1M"))  # no name: nothing
    # Escape: the question stays, drawn as not focused.
    assert dialog.take(("input", {"widget": 7, "released": "escape"}))
    assert "ctrl+t ctrl+t" in pilot.shown()[5] and not dialog.done
    # Another variant: drawn at its size.
    small = {**pilot.info, "asks": "loud", "variant": "18x1", "size": [18, 1]}
    assert dialog.take(("widgets", small)) and pilot.shown() == ["Deploy?".ljust(18)]
    assert dialog.take(("widgets", {**small, "variant": None, "size": None}))

    assert dialog.take(("widgets", focused)) and dialog.take(key("enter"))
    assert dialog.done and dialog.result == "no"
    assert [name for name, _ in pilot.calls[-2:]] == ["widget.release", "widget.hide"]
    assert not dialog.take(key("a"))


def test_dialog_options_and_cancel() -> None:
    pilot = Script()
    model = Line("Name?")
    dialog = Dialog(pilot, model, sizes=[(30, 6)], quiet=True, escape=False, fill=0.5)  # type: ignore[arg-type]
    assert pilot.named("widget.ask") == [{"id": 7, "quiet": True, "escape": False}]
    assert pilot.named("widget.show")[0] == {"tag": "ask", "fill": 0.5}
    assert model.escape is None and dialog.widget.asks == "quiet"
    dialog.take(key("a"))
    assert "later" not in pilot.shown()[5] and "^C dismiss" in pilot.shown()[5]
    dialog.take(key("ctrl+c"))
    assert dialog.done and dialog.result is None and model.result is None
    assert pilot.named("widget.release") == [{"id": 7}]


def test_dialog_paste() -> None:
    pilot = Script()
    model = Text("Message")
    dialog = Dialog(pilot, model, sizes=[(30, 6)])  # type: ignore[arg-type]
    events = [
        key("a", b"a"),
        key(None, b"\x1b[200~"),
        key("b", b"b"),
        key("enter", b"\r"),  # a new line, not the end
        key("é", "é".encode()),
        key("ctrl+j", b"\n"),
        key("tab", b"\t"),
        key("c", b"c"),
        key(None, b"\x1b[201~"),
    ]
    for event in events:
        assert dialog.take(event) and not dialog.done
    assert model.text == "ab\né\n c"
    # A paste cut by the focus lost is dropped.
    dialog.take(key(None, b"\x1b[200~"))
    dialog.take(key("x", b"x"))
    dialog.take(("input", {"widget": 7, "released": "hotkey"}))
    dialog.take(key("enter", b"\r"))
    assert dialog.done and dialog.result == "ab\né\n c"


def test_dialog_gone() -> None:
    pilot = Script()
    dialog = Dialog(pilot, Line("?"))  # type: ignore[arg-type]
    gone = {"id": 7, "state": "gone", "variant": None, "size": None}
    assert dialog.take(("widgets", gone)) and dialog.done and dialog.result is None
    dialog.close()
    assert pilot.named("widget.release") == pilot.named("widget.hide") == []


def test_ask_scripted() -> None:
    pilot = Script()
    pilot.queue = [None, ("cursor", {}), key("b"), key("o"), key("enter")]
    assert ui.ask(pilot, Line("Name?"), tag="name") == "bo"  # type: ignore[arg-type]
    assert pilot.topics == ["exit", "widgets"]
    assert pilot.named("widget.set")  # drawn before the first event

    pilot = Script()
    pilot.queue = [key("b"), ("exit", {"status": 0}), key("enter")]
    assert ui.ask(pilot, Line("Name?")) is None  # type: ignore[arg-type]
    assert pilot.named("widget.hide") == [{"id": 7}]

    pilot = Script()  # the connection closed: `event` raises
    assert ui.ask(pilot, Line("Name?")) is None  # type: ignore[arg-type]

    pilot = Script()
    pilot.queue = [key("a")]
    assert ui.ask(pilot, Line("Name?"), timeout=0) is None  # type: ignore[arg-type]
    assert pilot.queue and pilot.named("widget.release") == [{"id": 7}]


# -- on a driver ------------------------------------------------------------


def asking(pilot: Pilot, model: ui.Model, **options: Any) -> Dialog:
    """A dialog; skipped on a driver without `widget.ask`."""
    try:
        return Dialog(pilot, model, **options)
    except PtyrcError as exc:
        if exc.code == "unknown-method":
            pytest.skip("the driver has no widget.ask")
        raise


def pump(pilot: Pilot, dialog: Dialog, check: Any, timeout: float = 5.0) -> None:
    """Give the dialog its events until `check()`."""

    def step() -> Any:
        dialog.take(pilot.event(timeout=0.2))
        return check()

    until(step, timeout=timeout)


def test_driver_asks_focus_answer(shell: Pilot) -> None:
    dialog = asking(shell, Choice("Deploy?", ["yes", "no", "later"]), tag="deploy")
    widget = dialog.widget
    assert widget.asks == "loud" and not widget.focused
    until(lambda: widget.refresh().placed)
    dialog.draw()
    assert any("Deploy?" in row for row in shell.shown())
    assert shell.call("session.info")["focus"] is None

    info = shell.focus()
    assert info is not None and info["id"] == widget.id and info["focused"]
    pump(shell, dialog, lambda: widget.focused)
    assert shell.call("session.info")["focus"] == widget.id
    assert any("enter ok" in row for row in shell.shown())
    assert shell.focus() is None  # past the last: dropped
    pump(shell, dialog, lambda: not widget.focused)
    assert not dialog.done and widget.asks == "loud"

    shell.focus(widget)
    assert widget.focused
    shell.blur()
    pump(shell, dialog, lambda: not widget.focused)
    with pytest.raises(PtyrcError) as err:
        shell.focus(999)
    assert err.value.code in ("no-sprite", "not-usable")

    dialog.close()
    assert dialog.done and shell.widgets() == [] and shell.focus() is None


def test_driver_release(shell: Pilot) -> None:
    dialog = asking(shell, Line("Name?"), quiet=True, escape=False)
    widget = dialog.widget
    assert widget.asks == "quiet"
    shell.focus(widget)
    assert shell.call("session.info")["focus"] == widget.id
    widget.release()
    assert widget.refresh().asks is None and not widget.focused
    assert shell.call("session.info")["focus"] is None
    with pytest.raises(PtyrcError) as err:
        shell.focus(widget)
    assert err.value.code == "not-usable"
    widget.release()  # twice: fine
    widget.hide()
    widget.release()  # gone: fine
