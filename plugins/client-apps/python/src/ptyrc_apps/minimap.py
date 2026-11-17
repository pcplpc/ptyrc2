# SPDX-License-Identifier: GPL-3.0-or-later
"""Minimap: the text of a frame, small, with the view marked.

    ptyrc run less server.log        # or vim, man, a pane of tmux
    ptyrc-app minimap
    ptyrc-app minimap -a --mark 'TODO|FIXME' --mark ERROR
    ptyrc-app minimap --print 10x16 --file README.md

What it shows:

- The remembered buffer of a frame (`frame.text`): the rows the driver
  has seen, in frame rows. What was never shown is blank and fills as
  one scrolls.
- Ink, not letters: a braille cell holds 2 by 4 dots, a dot is `cols`
  columns by `rows` lines, on when half of its lines hold ink there.
  The scale is the least that fits the buffer in the map.
- The view as a bright band (`origin`, `rect` of the frame), kept by
  the topics `text` and `frames`: read again when told, never on a
  period.
- A widget: a column of one width, the driver picks the height, of
  those the text needs: the tag is declared again as it grows. Its
  border is drawn by the app, a cell from the edge of the view.
"""

from __future__ import annotations

import argparse
import math
import re
import sys
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from ptyrc import Frame, Pilot, PtyrcError, Sprite, clock

from . import _cli, _widgets
from .marks import INKS

__all__ = [
    "HEIGHTS",
    "WIDTH",
    "Doc",
    "Look",
    "face",
    "framed",
    "heights",
    "inks",
    "line_at",
    "main",
    "scale",
]

Size = tuple[int, int]
Rows = list[list[dict[str, Any]]]

WIDTH = 100  # columns of the text mapped; the rest is cut
HEIGHTS = (4, 6, 8, 12, 16, 24, 32, 48)  # inner rows of the variants
BRAILLE = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))
NOTES = ("# ", "#!", "//", "-- ", "/*", ";;")  # what starts a comment
# Foregrounds: out of the view, in it.
CODE, NOTE = (242, 255), (65, 114)
GROUND, CURSOR = 237, 60  # backgrounds: the view, the line of the cursor
# Heat, newest first: edited, visited, both.
EDITED, VISITED, BOTH = (208, 172, 130), (75, 68, 60), (213, 170, 133)
SETTLE = 0.03  # seconds of silence that end a burst of changes
BURST = 0.2  # seconds a burst may last before a look
TIP = 48  # cells of a tip, at most


@dataclass(frozen=True)
class Look:
    cursor: bool = False
    heat: bool = False
    notes: bool = False
    fade: float = 60.0  # seconds a heat lasts


@dataclass
class Doc:
    """What is known of a frame. `lines[i]` is the frame row `y0 + i`;
    the view is the rows `top <= y < top + view`, `width` columns."""

    y0: int = 0
    lines: list[str] = field(default_factory=list)
    top: int = 0
    view: int = 0
    width: int = 80
    cursor: int | None = None  # frame row
    marks: dict[int, int] = field(default_factory=dict)  # frame row: pattern
    edited: dict[int, float] = field(default_factory=dict)  # frame row: when
    visited: dict[int, float] = field(default_factory=dict)

    def mark(self, patterns: Sequence[re.Pattern[str]]) -> None:
        """Set `marks`: the first pattern found in each line."""
        self.marks = {}
        for i, line in enumerate(self.lines):
            for p, pattern in enumerate(patterns):
                if pattern.search(line):
                    self.marks[self.y0 + i] = p
                    break

    def visit(self, now: float) -> None:
        for y in range(self.top, self.top + self.view):
            self.visited[y] = now

    def forget(self, now: float, fade: float) -> None:
        """Drop the heat that has faded."""
        for heat in (self.edited, self.visited):
            for y in [y for y, at in heat.items() if now - at >= fade]:
                del heat[y]


def scale(count: int, width: int, w: int, h: int) -> tuple[int, int]:
    """Columns and lines a dot: the least that fit `count` lines of
    `width` columns in `w` by `h` cells."""
    cols = max(1, math.ceil(min(width, WIDTH) / (2 * w)))
    return cols, max(1, math.ceil(count / (4 * h)))


def inks(line: str, cols: int, w: int) -> int:
    """The dot columns of `line` that hold ink, a bit each."""
    bits = 0
    for x in range(2 * w):
        if line[x * cols : (x + 1) * cols].strip():
            bits |= 1 << x
    return bits


def line_at(doc: Doc, w: int, h: int, y: int) -> int | None:
    """The frame row told for the cell row `y` of the map: the first of
    its lines that holds ink, None when none does."""
    _, rows = scale(len(doc.lines), doc.width, w, h)
    for i in range(4 * y * rows, min(len(doc.lines), 4 * (y + 1) * rows)):
        if doc.lines[i].strip():
            return doc.y0 + i
    return None


def _level(heat: dict[int, float], rows: range, now: float, fade: float) -> int | None:
    """The step of the newest heat on `rows`: 0 to 2, None when cold."""
    ages = [now - heat[y] for y in rows if y in heat]
    if not ages or min(ages) >= fade:
        return None
    return min(2, int(3 * max(0.0, min(ages)) / fade))


def face(doc: Doc, w: int, h: int, look: Look | None = None, now: float = 0.0) -> Rows:
    """The map, `h` rows of runs, `w` cells each.

    A cell has one colour. Foreground, the first that holds: a mark
    (its pattern's), an edit (`look.heat`), the view, a visit, a
    comment (`look.notes`), code. Background: the line of the cursor
    (`look.cursor`), then the view. A marked line is drawn alone: its
    dots show whatever the lines it shares them with."""
    look = look or Look()
    cols, rows = scale(len(doc.lines), doc.width, w, h)
    ink = [inks(line, cols, w) for line in doc.lines]
    note = [line.lstrip().startswith(NOTES) for line in doc.lines]
    out: Rows = []
    for cy in range(h):
        first = 4 * cy * rows
        block = range(doc.y0 + first, doc.y0 + min(len(ink), first + 4 * rows))
        seen = block.start < doc.top + doc.view and doc.top < block.stop
        bg = GROUND if seen else None
        if look.cursor and doc.cursor is not None and doc.cursor in block:
            bg = CURSOR
        edited = visited = None
        if look.heat:
            edited = _level(doc.edited, block, now, look.fade)
            visited = _level(doc.visited, block, now, look.fade)
        pens = [doc.marks[y] for y in block if y in doc.marks]
        runs: list[dict[str, Any]] = []
        for cx in range(w):
            glyph, code, noted = 0, False, False
            for dy in range(4):
                lo = first + dy * rows
                part = range(lo, min(len(ink), lo + rows))
                for dx in range(2):
                    bit = 1 << (2 * cx + dx)
                    on = [i for i in part if ink[i] & bit]
                    if not on:
                        continue
                    marked = any(doc.y0 + i in doc.marks for i in on)
                    if marked or 2 * len(on) >= len(part):
                        glyph |= BRAILLE[dy][dx]
                        noted |= all(note[i] for i in on)
                        code |= not all(note[i] for i in on)
            if pens:
                fg = INKS[min(pens) % len(INKS)][0]
            elif edited is not None and visited is not None and not seen:
                fg = BOTH[max(edited, visited)]
            elif edited is not None:
                fg = EDITED[edited]
            elif seen or visited is None:
                fg = (NOTE if look.notes and noted and not code else CODE)[seen]
            else:
                fg = VISITED[visited]
            run = {"t": chr(0x2800 + glyph) if glyph else " ", "fg": fg}
            if bg is not None:
                run["bg"] = bg
            if runs and all(runs[-1].get(k) == run.get(k) for k in ("fg", "bg")):
                runs[-1]["t"] += run["t"]
            else:
                runs.append(run)
        out.append(runs)
    return out


def title(doc: Doc) -> str:
    """The lines known: `412`, `3.4k`."""
    count = len(doc.lines)
    return str(count) if count < 1000 else f"{count / 1000:.1f}k"


def heights(count: int) -> list[int]:
    """The heights offered for `count` lines: those of `HEIGHTS` under
    the one that shows a line a dot row, and that one. The widget is
    no taller than its text."""
    need = min(max(math.ceil(count / 4), HEIGHTS[0]), HEIGHTS[-1])
    return [h for h in HEIGHTS if h < need] + [need]


def framed(rows: Rows, w: int, name: str, left: bool = False) -> Rows:
    """`rows`, `w` cells wide, in a round border with `name` on its
    bottom edge, and a blank column on one side: `w + 3` cells by two
    rows more. The driver puts a widget against the edge of the view;
    the blank keeps the border a cell from it."""
    name = f" {name} " if len(name) + 3 <= w else ""

    def row(*runs: dict[str, Any]) -> list[dict[str, Any]]:
        pad = {"t": " "}
        return [pad, *runs] if left else [*runs, pad]

    def edge(text: str) -> dict[str, Any]:
        return {"t": text, "dim": True}

    foot = [edge("╰" + "─" * (w - len(name)))]
    foot += [{"t": name}, edge("╯")] if name else [edge("╯")]
    return [
        row(edge("╭" + "─" * w + "╮")),
        *(row(edge("│"), *runs, edge("│")) for runs in rows),
        row(*foot),
    ]


# -- print ------------------------------------------------------------------


def show(args: argparse.Namespace, look: Look) -> int:
    """`--print`: the map of a file, no driver. The view is 40 lines a
    third of the way down, the cursor in its middle; the first lines
    were edited, those before the view visited."""
    path = args.file or Path(__file__)
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        print(f"ptyrc-app: {exc}", file=sys.stderr)
        return 1
    lines = [line.expandtabs(4) for line in text.splitlines()]
    top = len(lines) // 3
    doc = Doc(0, lines, top, min(40, len(lines)), max(map(len, lines), default=1))
    doc.cursor = top + doc.view // 2
    doc.mark(args.mark)
    doc.edited = dict.fromkeys(range(min(12, top)), 0.0)
    doc.visited = dict.fromkeys(range(top), look.fade / 2)
    colour = sys.stdout.isatty()
    sizes = [args.print] if args.print else [(args.width, h) for h in HEIGHTS[2:6]]
    for w, h in sizes:
        print(f"{w}x{h}")
        rows = face(doc, w, h, look, look.fade / 2)
        for row in framed(rows, w, title(doc)):
            print("".join(_ansi(run) if colour else run["t"] for run in row))
    return 0


def _ansi(run: dict[str, Any]) -> str:
    if "fg" not in run:
        return f"\x1b[2m{run['t']}\x1b[0m" if run.get("dim") else run["t"]
    bg = f";48;5;{run['bg']}" if "bg" in run else ""
    return f"\x1b[38;5;{run['fg']}{bg}m{run['t']}\x1b[0m"


# -- live -------------------------------------------------------------------


class Map:
    """The doc of a frame, read from the driver when told."""

    def __init__(self, pilot: Pilot, frame: Frame, args: argparse.Namespace) -> None:
        self.pilot = pilot
        self.frame = frame
        self.patterns: list[re.Pattern[str]] = args.mark
        self.doc = Doc()
        self.at: tuple[int, int] | None = None  # the cursor, a screen cell

    def read(self) -> bool:
        """Read the buffer and the view; False while the frame cannot
        be read (gone, or settling): the doc stands."""
        frame = self.frame.id
        try:
            with self.pilot.batch() as batch:
                self.pilot.call("frame.get", frame=frame)
                self.pilot.call("frame.text", frame=frame)
                shown = self.pilot.text(frame=frame)
        except PtyrcError as exc:
            if exc.code not in ("no-frame", "not-usable"):
                raise
            return False
        info, known = batch.results[0], batch.results[1]
        if any(isinstance(r, PtyrcError) for r in batch.results):
            return False
        self.frame.info = info["frame"]
        doc, (_, top), (_, _, width, view) = (
            self.doc,
            self.frame.origin,
            self.frame.rect,
        )
        y0, lines = min(known["y0"], top), list(known["lines"])
        lines = [""] * (known["y0"] - y0) + lines
        # The view is told as it is: not yet remembered, it is shown.
        lines += [""] * (top + view - y0 - len(lines))
        lines[top - y0 : top - y0 + len(shown)] = shown
        doc.y0, doc.lines = y0, lines
        doc.top, doc.view, doc.width = top, view, width
        doc.mark(self.patterns)
        doc.visit(clock.monotonic())
        self.cursor()
        return True

    def cursor(self, data: dict[str, Any] | None = None) -> None:
        """An event of the topic `cursor`. Its screen cell is kept: the
        line is that of the view as it is now, at each read."""
        if data is not None:
            shown = data.get("visible", True)
            self.at = (data["x"], data["y"]) if shown else None
        pos = None if self.at is None else self.frame.to_frame(*self.at)
        self.doc.cursor = None if pos is None else pos[1]


class Tip:
    """The line under the host mouse, told beside the widget: one
    sprite on the front layer, in screen cells."""

    def __init__(self, pilot: Pilot) -> None:
        self.pilot = pilot
        self.sprite: Sprite | None = None
        self.said: tuple[Any, ...] | None = None

    def say(self, widget: Any, y: int, text: str) -> None:
        rect, size = widget.rect, widget.size
        text = " ".join(text.split())
        if rect is None or size is None or not text:
            return self.hush()
        text = text if len(text) <= TIP else text[: TIP - 1] + "…"
        row = rect[1] + (rect[3] - size[1]) // 2 + y
        x = rect[0] - len(text) - 3
        if x < 0:  # no room on the left
            x = rect[0] + rect[2] + 1
        said = (x, row, text)
        if said == self.said:
            return None
        self.hush()
        self.sprite = self.pilot.draw(
            cells=[[{"t": f" {text} ", "fg": 255, "bg": GROUND}]],
            at=(x, row),
            layer="front",
            z=9,
            opaque=True,
        )
        self.said = said
        return None

    def hush(self) -> None:
        sprite, self.sprite, self.said = self.sprite, None, None
        if sprite is not None:
            try:
                sprite.remove()
            except PtyrcError as exc:
                if exc.code != "no-sprite":
                    raise


def _place(pilot: Pilot, frame: Frame, sel: int | str) -> Frame | None:
    """The frame to map once `frame` left the screen: the pane that
    took its place, else the one `sel` names now."""
    panes = pilot.frames()
    if any(f.id == frame.id and f.state != "inactive" for f in panes):
        return None
    if (new := _cli.heir(frame, panes)) is not None:
        return new
    if isinstance(sel, int):
        return None
    try:
        new = _cli.pick(pilot, sel)
    except PtyrcError as exc:
        if exc.code != "no-frame":
            raise
        return None
    return new if new.usable and new.id != frame.id else None


def live(
    pilot: Pilot, args: argparse.Namespace, look: Look, seconds: float | None
) -> None:
    chart = Map(pilot, _cli.pick(pilot, args.frame), args)
    chart.read()
    tip = Tip(pilot)
    over: list[int | None] = [None]  # the cell row under the mouse
    # No border asked for: the app draws its own, a cell from the edge.
    own = args.border is None
    lead = "left" in (args.prefer or "")  # the blank is on the left
    if own:
        args.border = "none"

    def inner(size: Size) -> Size:
        return (size[0] - 3, size[1] - 2) if own else size

    def sizes() -> list[Size]:
        w, extra = (args.width + 3, 2) if own else (args.width, 0)
        return [(w, h + extra) for h in heights(len(chart.doc.lines))]

    def render(w: int, h: int) -> Rows:
        w, h = inner((w, h))
        rows = face(chart.doc, w, h, look, clock.monotonic())
        return framed(rows, w, title(chart.doc), lead) if own else rows

    def tell() -> None:
        size = widget.size
        if over[0] is None or size is None:
            return tip.hush()
        y = line_at(chart.doc, *inner(size), over[0])
        if y is None:
            return tip.hush()
        return tip.say(widget, over[0] + own, chart.doc.lines[y - chart.doc.y0])

    def on_move(x: int, y: int) -> bool:
        size = widget.size
        if own and size is not None:  # the map inside the app's border
            x, y = x - 1 - lead, y - 1
            w, h = inner(size)
            if not (0 <= x < w and 0 <= y < h):
                return on_leave()
        over[0] = y
        tell()
        return False

    def on_leave() -> bool:
        over[0] = None
        tip.hush()
        return False

    deck = _widgets.Deck(pilot, args)
    deck.shared["frame"] = chart.frame.id
    offered = sizes()
    widget = deck.add(
        "minimap",
        offered,
        render,
        fill=1.0,
        title="" if own else title(chart.doc),
    )
    pilot.subscribe("text", "frames", "cursor", snapshot=True)
    deck.hover_timeout = None  # a mouse at rest sends nothing
    deck.on_hover(widget, on_move, on_leave)

    def read() -> None:
        """Read; the widget is declared again when the text wants
        other heights."""
        nonlocal offered, sent
        if not chart.read():
            return
        if sizes() != offered:
            offered, sent = sizes(), ()
            deck.resize(widget, offered)
        tell()

    end = None if seconds is None else clock.monotonic() + seconds
    sent: tuple[object, ...] = ()
    dirty, since = False, 0.0  # a burst of changes, and when it began
    while True:
        now = clock.monotonic()
        if end is not None and now >= end:
            return
        if dirty and now >= since + BURST:
            dirty = False
            read()
        size = widget.size
        if size is not None and not dirty:
            chart.doc.forget(now, look.fade)
            rows, name = render(*size), title(chart.doc)
            if (size, rows, name) != sent:
                widget.set(rows, title=None if own else name)
                sent = (size, rows, name)
        # The heat fades by itself: a look now and then while there is some.
        warm = look.heat and (chart.doc.edited or chart.doc.visited)
        waits = [SETTLE if dirty else look.fade / 6 if warm else None]
        waits.append(None if end is None else end - now)
        left = min((w for w in waits if w is not None), default=None)
        event = pilot.event(timeout=None if left is None else max(0.0, left))
        if event is None:
            if dirty:  # the burst is over
                dirty = False
                read()
            continue
        topic, data = event
        if topic == "exit":
            return
        if topic == "text" and data["frame"] == chart.frame.id:
            at = clock.monotonic()
            chart.doc.edited.update(dict.fromkeys(data["rows"], at))
        if topic == "cursor":
            chart.cursor(data)
            continue
        if (
            topic == "frames"
            and (new := _place(pilot, chart.frame, args.frame)) is not None
        ):
            topic, data = "move", {"frame": new.id}
        if topic == "move":
            try:
                chart.frame = pilot.frame(data["frame"])
            except PtyrcError as exc:
                if exc.code != "no-frame":
                    raise
                continue  # gone since it was asked: it stays
            chart.doc, sent = Doc(), ()
            tip.hush()
            deck.move(chart.frame.id)
        elif topic in ("widgets", "hover"):
            deck.handle(event)
            continue
        if topic in ("text", "frames", "move") and not dirty:
            dirty, since = True, clock.monotonic()


# -- options ----------------------------------------------------------------


def _size(text: str) -> Size:
    w, sep, h = text.partition("x")
    if not (sep and w.isdigit() and h.isdigit() and 0 < int(w) and 0 < int(h)):
        raise argparse.ArgumentTypeError(f"not a size WxH: {text!r}")
    return int(w), int(h)


def _width(text: str) -> int:
    if not (text.isdigit() and 2 <= int(text) <= 40):
        raise argparse.ArgumentTypeError(f"not a width, 2 to 40: {text!r}")
    return int(text)


def _positive(text: str) -> float:
    try:
        value = float(text)
    except ValueError:
        value = 0.0
    if not 0.0 < value < float("inf"):
        raise argparse.ArgumentTypeError(f"not a positive number: {text!r}")
    return value


def _pattern(text: str) -> re.Pattern[str]:
    try:
        return re.compile(text)
    except re.error as exc:
        raise argparse.ArgumentTypeError(f"not a pattern: {text!r}: {exc}") from None


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser(
        "minimap", "The text of a frame, small, the view marked.", frame="largest"
    )
    _widgets.options(out, border="round", prefer="right")
    # Round, drawn by the app; one named is the driver's, on the edge.
    out.set_defaults(border=None)
    out.add_argument(
        "-c", "--cursor", action="store_true", help="the line of the cursor"
    )
    out.add_argument(
        "-H",
        "--heat",
        action="store_true",
        help="a hue for the lines edited, another for those visited, fading",
    )
    out.add_argument("-n", "--notes", action="store_true", help="comments in green")
    out.add_argument("-a", "--all", action="store_true", help="-c -H -n")
    out.add_argument(
        "--mark",
        type=_pattern,
        action="append",
        default=[],
        metavar="PATTERN",
        help="a regular expression: its lines in a colour; may be given again",
    )
    out.add_argument(
        "--width",
        type=_width,
        default=12,
        metavar="N",
        help="cells across [default: 12]",
    )
    out.add_argument(
        "--fade",
        type=_positive,
        default=60.0,
        metavar="SECONDS",
        help="a heat lasts SECONDS [default: 60]",
    )
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(),
        default=None,
        metavar="WxH",
        help="no driver: print the map of a file, or one of the size given",
    )
    out.add_argument(
        "--file", type=Path, metavar="FILE", help="the file of --print [this one]"
    )
    args = _cli.parse(out, argv)
    if args.all:
        args.cursor = args.heat = args.notes = True
    return args


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    args = options(argv)
    look = Look(args.cursor, args.heat, args.notes, args.fade)
    if args.print is not None:
        return show(args, look)
    return _cli.run(lambda pilot, a: live(pilot, a, look, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
