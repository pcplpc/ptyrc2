# SPDX-License-Identifier: GPL-3.0-or-later
"""Clock: one widget, faces from 5x1 to 41x8.

    ptyrc-app clock
    ptyrc-app clock --prefer top-right --tz Asia/Tokyo --no-seconds
    ptyrc-app clock --print

What it shows:

- `Pilot.widget`: one tag, `clock`, and a variant per face: the same
  time laid out for each box, from `HH:MM` to block digits with the
  date. The driver picks the face and the place, in a quiet room,
  and moves or changes it when the app needs the place; `--border`:
  a preset, `round` here;
- `_widgets.Deck`: the face shown is drawn on the second (on the
  minute with `--no-seconds`), a face just put at once;
- `--print`: every face once, at its size, without a driver.
"""

from __future__ import annotations

import argparse
import math
import sys
from collections.abc import Callable, Sequence
from datetime import UTC, datetime, tzinfo
from functools import partial
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

from ptyrc import Pilot

from . import _cli, _widgets

__all__ = ["LARGE", "SIZES", "SMALL", "decode", "face", "ladder", "main"]

Size = tuple[int, int]
Run = dict[str, object]
Rows = list[list[Run]]
Draw = Callable[[datetime, int, int], Rows]

HOURS = "bright-cyan"
MINUTES = "bright-magenta"
SECONDS = "bright-yellow"
DIM = "bright-black"

DAYS = ("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")
MONTHS = (
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
)

# Digits 0 to 9 and the colon, in dots: `#` on. Two dots a cell high.
_SMALL = (
    "### .#. ### ### #.# ### ### ### ### ### .",
    "#.# ##. ..# ..# #.# #.. #.. ..# #.# #.# #",
    "#.# .#. ### ### ### ### ### ..# ### ### .",
    "#.# .#. #.. ..# ..# ..# #.# ..# #.# ..# #",
    "### ### ### ### ..# ### ### ..# ### ### .",
)


def _dots(table: Sequence[str]) -> list[list[str]]:
    cols = [row.split() for row in table]
    return [[row[i] for row in cols] for i in range(len(cols[0]))]


def _double(glyph: list[str]) -> list[str]:
    """`glyph` twice as large, the outer corner of each bend cut by one
    dot."""
    h, w = len(glyph), len(glyph[0])

    def on(y: int, x: int) -> bool:
        return 0 <= y < h and 0 <= x < w and glyph[y][x] == "#"

    out = [["."] * (2 * w) for _ in range(2 * h)]
    for y in range(h):
        for x in range(w):
            if not on(y, x):
                continue
            for dy in (0, 1):
                for dx in (0, 1):
                    v, u = y + 2 * dy - 1, x + 2 * dx - 1
                    bend = on(2 * y - v, x) and on(y, 2 * x - u)
                    cut = bend and not (on(v, x) or on(y, u) or on(v, u))
                    out[2 * y + dy][2 * x + dx] = "." if cut else "#"
    return ["".join(row) for row in out]


def _cells(glyph: list[str]) -> list[str]:
    """Dots to half blocks: two rows of dots a row of cells."""
    rows = glyph + ["." * len(glyph[0])] * (len(glyph) % 2)
    pairs = zip(rows[::2], rows[1::2], strict=True)
    return [
        "".join(" ▄▀█"[2 * (t == "#") + (b == "#")] for t, b in zip(*p)) for p in pairs
    ]


_small = _dots(_SMALL)
#: Digits 0 to 9 and `:`: 3 cells by 3, half blocks.
SMALL = [_cells(g) for g in _small]
#: Digits 0 to 9 and `:`: 6 cells by 5, half blocks, corners cut.
LARGE = [_cells(_double(g)) for g in _small[:10]] + [
    _cells(["..", "..", "##", "##", "..", "..", "##", "##", "..", ".."])
]


def decode(rows: Sequence[str], font: Sequence[Sequence[str]]) -> str:
    """The digits of `rows`, block digits of `font` one column apart;
    `:` for a colon, `?` for what is not a glyph of it."""
    out, x = "", 0
    while x < len(rows[0]):
        if all(row[x] == " " for row in rows):
            x += 1
            continue
        for i, glyph in enumerate(font):
            w = len(glyph[0])
            if [row[x : x + w] for row in rows] == list(glyph):
                out += "0123456789:"[i]
                x += w
                break
        else:
            return out + "?"
    return out


def _width(row: list[Run]) -> int:
    return sum(len(str(run["t"])) for run in row)


def _blocks(text: str, font: Sequence[Sequence[str]], colours: Sequence[str]) -> Rows:
    """`text` in block digits, a column apart, as many about a colon as
    it is wide; `colours`: one a character."""
    rows: Rows = [[] for _ in font[0]]
    for i, (char, colour) in enumerate(zip(text, colours, strict=True)):
        glyph = font[10 if char == ":" else int(char)]
        colon = ":" in (char, text[i - 1 : i])
        gap = " " * (len(font[10][0]) if colon else 1)
        for row, line in zip(rows, glyph, strict=True):
            if i:
                row.append({"t": gap})
            row.append({"t": line, "fg": colour})
    return rows


def _beside(left: Rows, right: Rows, gap: int) -> Rows:
    """`right` after `left`, bottom lines level."""
    width = max(map(_width, left))
    right = [[] for _ in range(len(left) - len(right))] + right
    return [
        a + ([{"t": " " * (width - _width(a) + gap)}] + b if b else [])
        for a, b in zip(left, right, strict=True)
    ]


def _fit(groups: Sequence[Rows], w: int, h: int) -> Rows:
    """Groups of rows one under the other, each centred across, the
    whole centred down; what exceeds `w` by `h` is cut."""
    rows: Rows = []
    for group in groups:
        pad = max(0, (w - max(map(_width, group))) // 2)
        rows += [[{"t": " " * pad}, *row] if pad and row else row for row in group]
    rows = [[] for _ in range(max(0, (h - len(rows)) // 2))] + rows
    return [_clip(row, w) for row in rows[:h]]


def _clip(row: list[Run], w: int) -> list[Run]:
    out: list[Run] = []
    for run in row:
        text = str(run["t"])[: w - _width(out)]
        if text:
            out.append({**run, "t": text})
    return out


def _time(now: datetime, seconds: bool) -> list[Run]:
    colon = {"t": ":", "fg": DIM}
    out: list[Run] = [
        {"t": f"{now:%H}", "fg": HOURS, "bold": True},
        colon,
        {"t": f"{now:%M}", "fg": MINUTES, "bold": True},
    ]
    if seconds:
        out += [colon, {"t": f"{now:%S}", "fg": SECONDS, "bold": True}]
    return out


def _date(now: datetime, full: bool) -> list[Run]:
    day, month = DAYS[now.weekday()], MONTHS[now.month - 1]
    if not full:
        return [{"t": f"{day[:3]} {now.day} {month[:3]}"}]
    return [
        {"t": day, "fg": "cyan"},
        {"t": f" {now.day} {month} {now.year}"},
    ]


def _week(now: datetime) -> list[Run]:
    return [{"t": f"week {now.isocalendar().week}", "fg": DIM}]


def line(now: datetime, w: int, h: int, *, seconds: bool) -> Rows:
    """`HH:MM` or `HH:MM:SS`."""
    return _fit([[_time(now, seconds)]], w, h)


def dated(now: datetime, w: int, h: int, *, seconds: bool) -> Rows:
    """The time over a short date."""
    return _fit([[_time(now, seconds)], [_date(now, False)]], w, h)


def block(
    now: datetime,
    w: int,
    h: int,
    *,
    font: Sequence[Sequence[str]],
    blink: bool,
    date: bool,
) -> Rows:
    """`HH:MM` in block digits, the colon dim on odd seconds when
    `blink`; the short date under it when `date`."""
    colon = DIM if blink and now.second % 2 else "white"
    digits = _blocks(f"{now:%H:%M}", font, [HOURS] * 2 + [colon] + [MINUTES] * 2)
    return _fit([digits, [[]], [_date(now, False)]] if date else [digits], w, h)


def grand(now: datetime, w: int, h: int, *, seconds: bool) -> Rows:
    """Large `HH:MM`, small seconds after it; weekday, date, week."""
    colon = DIM if seconds and now.second % 2 else "white"
    digits = _blocks(f"{now:%H:%M}", LARGE, [HOURS] * 2 + [colon] + [MINUTES] * 2)
    if seconds:
        digits = _beside(digits, _blocks(f"{now:%S}", SMALL, [SECONDS] * 2), 2)
    return _fit([digits, [[]], [_date(now, True)], [_week(now)]], w, h)


# Braille: the bit of each dot of a cell, by row and column.
_BRAILLE = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))


def dial(now: datetime, w: int, h: int, *, seconds: bool) -> Rows:
    """A round dial in braille, 17 cells by 8, two dots by four a cell:
    rim and hour marks dim, the hour hand, the minute hand, the second
    a mark on the rim. A cell has one colour: that of its foremost
    dot."""
    return _fit([_dial(now, 17, 8, seconds)], w, h)


def _dial(now: datetime, w: int, h: int, seconds: bool) -> Rows:
    cx, cy = w - 0.5, 2 * h - 0.5  # the centre, in dots
    r = min(cx, cy)
    dots: dict[tuple[int, int], int] = {}  # dot: rank, the foremost highest

    def ray(turn: float, start: float, end: float, rank: int) -> None:
        n = max(1, int(4 * (end - start)))
        for i in range(n + 1):
            d = start + (end - start) * i / n
            x = round(cx + d * math.sin(2 * math.pi * turn))
            y = round(cy - d * math.cos(2 * math.pi * turn))
            if 0 <= x < 2 * w and 0 <= y < 4 * h:
                dots[x, y] = max(dots.get((x, y), 0), rank)

    for i in range(400):
        ray(i / 400, r, r, 1)
    for hour in range(12):
        ray(hour / 12, r - (4 if hour % 3 == 0 else 2.5), r - 2.5, 1)
    minute = now.minute + (now.second / 60 if seconds else 0)
    ray((now.hour % 12 + minute / 60) / 12, 0, 0.5 * r, 3)
    ray(minute / 60, 0, 0.8 * r, 2)
    if seconds:
        ray(now.second / 60, r, r, 4)
    cells: dict[tuple[int, int], list[int]] = {}
    for (x, y), rank in dots.items():
        cell = cells.setdefault((x // 2, y // 4), [0, 0])
        cell[0] |= _BRAILLE[y % 4][x % 2]
        cell[1] = max(cell[1], rank)
    colours = (DIM, MINUTES, HOURS, SECONDS)
    rows: Rows = []
    for y in range(h):
        row: list[Run] = []
        for x in range(w):
            bits, rank = cells.get((x, y), (0, 0))
            run: Run = {"t": chr(0x2800 + bits), "fg": colours[rank - 1]}
            row.append(run if bits else {"t": " "})
        rows.append(row)
    return rows


def _ladder(seconds: bool) -> list[tuple[Size, Draw, bool]]:
    """Every face: size, draw, block digits."""
    s, m = SMALL, LARGE
    faces: list[tuple[Size, Draw, bool]] = [
        ((5, 1), partial(line, seconds=False), False),
        ((10, 2), partial(dated, seconds=seconds), False),
        ((17, 3), partial(block, font=s, blink=seconds, date=False), True),
        ((17, 5), partial(block, font=s, blink=seconds, date=True), True),
        ((17, 8), partial(dial, seconds=seconds), False),
        ((32, 5), partial(block, font=m, blink=seconds, date=False), True),
        ((32, 7), partial(block, font=m, blink=seconds, date=True), True),
        ((41 if seconds else 32, 8), partial(grand, seconds=seconds), True),
    ]
    if seconds:
        faces.insert(1, ((8, 1), partial(line, seconds=True), False))
    return faces


def ladder(seconds: bool = True, big: bool = False) -> dict[Size, Draw]:
    """The faces of a mode, by size, smallest first. `seconds`: with
    them, else without; `big`: block digits only."""
    return {size: draw for size, draw, blocks in _ladder(seconds) if blocks or not big}


#: The sizes of the faces, plain mode.
SIZES = list(ladder())


def face(
    now: datetime, w: int, h: int, *, seconds: bool = True, big: bool = False
) -> Rows:
    """The face of size `w` by `h` of a mode, as `ladder`."""
    return ladder(seconds, big)[w, h](now, w, h)


_SGR = {
    name: code
    for i, base in enumerate(
        ("black", "red", "green", "yellow", "blue", "magenta", "cyan", "white")
    )
    for name, code in ((base, f"{30 + i}"), (f"bright-{base}", f"{90 + i}"))
}


def ansi(run: Run) -> str:
    """`run` with SGR escapes."""
    codes = []
    fg = run.get("fg")
    if isinstance(fg, int):
        codes.append(f"38;5;{fg}")
    elif isinstance(fg, str) and fg.startswith("#"):
        codes.append("38;2;" + ";".join(str(int(fg[i : i + 2], 16)) for i in (1, 3, 5)))
    elif isinstance(fg, str) and fg in _SGR:
        codes.append(_SGR[fg])
    codes += [c for k, c in (("bold", "1"), ("dim", "2")) if run.get(k)]
    text = str(run["t"])
    return f"\x1b[{';'.join(codes)}m{text}\x1b[0m" if codes else text


def boxed(rows: Rows, w: int, h: int, colour: bool) -> list[str]:
    """`rows` in a plain box of inner size `w` by `h`, titled `clock`."""
    edge = (lambda t: f"\x1b[2m{t}\x1b[0m") if colour else (lambda t: t)
    paint = ansi if colour else (lambda run: str(run["t"]))
    title = " clock " if w >= 9 else ""
    out = [edge("┌─" + title + "─" * (w - len(title) - 1) + "┐")]
    for row in rows + [[] for _ in range(h - len(rows))]:
        body = "".join(paint(run) for run in row) + " " * (w - _width(row))
        out.append(edge("│") + body + edge("│"))
    out.append(edge("└" + "─" * w + "┘"))
    return out


def show(args: argparse.Namespace) -> None:
    """Print each face of the mode in its box; ANSI colours on a tty."""
    now = datetime.now(args.tz)
    colour = sys.stdout.isatty()
    for (w, h), draw in ladder(not args.no_seconds, args.big).items():
        print(f"{w}x{h}")
        print("\n".join(boxed(draw(now, w, h), w, h, colour)))


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    faces = ladder(not args.no_seconds, args.big)

    def render(w: int, h: int) -> Rows:
        return faces[w, h](datetime.now(args.tz), w, h)

    deck = _widgets.Deck(pilot, args)
    deck.add("clock", list(faces), render)
    deck.run(period=60.0 if args.no_seconds else 1.0, seconds=seconds, align=True)


def _zone(name: str) -> tzinfo:
    try:
        return ZoneInfo(name)
    except (ZoneInfoNotFoundError, ValueError) as exc:
        raise argparse.ArgumentTypeError(f"unknown time zone: {name}") from exc


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("clock", "A clock in a quiet room of the screen.")
    _widgets.options(out, border="round")
    zones = out.add_mutually_exclusive_group()
    zones.add_argument("--utc", action="store_true", help="UTC time")
    zones.add_argument("--tz", type=_zone, help="time zone, as Europe/Paris")
    out.add_argument("--no-seconds", action="store_true", help="faces without seconds")
    out.add_argument("--big", action="store_true", help="block digits only")
    out.add_argument("--print", action="store_true", help="print the faces, no driver")
    args = _cli.parse(out, argv)
    if args.utc:
        args.tz = UTC
    if args.print:
        show(args)
        return 0
    return _cli.run(lambda pilot, a: app(pilot, a, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
