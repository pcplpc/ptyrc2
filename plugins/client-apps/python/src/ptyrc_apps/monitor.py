# SPDX-License-Identifier: GPL-3.0-or-later
"""Monitor: the state of this machine, in widgets.

    ptyrc-app monitor --print                  # no driver: every variant
    ptyrc-app monitor --print 44x6 --monitor disk-space
    ptyrc-app monitor --monitor glance,tcp-listen
    ptyrc-app monitor --merged --cycle 10

Monitors, each one tag of variants:

- `glance`: CPU, cores, memory, swap, load, uptime, network, top
  processes;
- `connected`: who is logged in, by user and origin;
- `tcp-listen`: TCP sockets listening on an address seen from outside;
- `disk-space`: used and free space of the local disks.

`render(data, w, h)` lays out a monitor for an inner size: at most `h`
rows of styled runs, `{"t": text, "fg": colour, "bold": True}`, each at
most `w` cells; the driver pads. Any size works; `sizes` are the ones
declared. `--merged`: one tag, `monitor`, showing one monitor at a time
in turn; its title follows.
"""

from __future__ import annotations

import argparse
import math
import os
import sys
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Any

from ptyrc import Pilot

from . import _cli
from . import _probe as probe

__all__ = [
    "MERGED",
    "MONITORS",
    "Col",
    "Monitor",
    "bar",
    "box",
    "fit",
    "live",
    "main",
    "merged",
    "merged_sizes",
    "render_connected",
    "render_disks",
    "render_glance",
    "render_listen",
    "size",
    "table",
    "trimmed",
    "width",
]

type Run = dict[str, Any]
type Row = list[Run]
type Rows = list[Row]

DIM = "bright-black"
ALERT = "red"
WARN = "yellow"
GAP = 2  # cells between two columns
SPARK = "▁▂▃▄▅▆▇█"


# Runs


def lab(text: str) -> Run:
    """A label: dim."""
    return {"t": text, "fg": DIM}


def val(text: str, fg: str | None = None) -> Run:
    """A value: bold, in `fg` when given."""
    return {"t": text, "bold": True} | ({"fg": fg} if fg else {})


def sp(n: int = 1) -> Run:
    return {"t": " " * n}


def width(row: Row) -> int:
    return sum(len(run["t"]) for run in row)


def clip(text: str, n: int) -> str:
    """`text` in `n` cells at most; cut with `…`."""
    if len(text) <= n:
        return text
    return text[: n - 1] + "…" if n > 0 else ""


def _rstrip(row: Row) -> Row:
    out = [run for run in row if run["t"]]
    while out and not out[-1]["t"].rstrip():
        out.pop()
    if out:
        out[-1] = {**out[-1], "t": out[-1]["t"].rstrip()}
    return out


def fit(row: Row, w: int) -> Row:
    """`row` in `w` cells, trailing blanks dropped; cut with a dim `…`."""
    row = _rstrip(row)
    if width(row) <= w:
        return row
    out, room = [], w - 1
    for run in row:
        if len(run["t"]) >= room:
            out.append({**run, "t": run["t"][:room]})
            break
        out.append(run)
        room -= len(run["t"])
    return [*_rstrip(out), lab("…")] if w > 0 else []


def items(parts: Sequence[Row], w: int, gap: int = GAP, more: bool = True) -> Row:
    """Those of `parts` that fit in `w`, in order, `gap` apart; `+N` for
    the others when `more`."""
    out: Row = []
    hidden = 0
    for n, part in enumerate(parts):
        left = len(parts) - n - 1 + hidden  # may yet be left out
        tail = len(f"+{left}") + gap if more and left else 0
        if width(out) + (gap if out else 0) + width(part) + tail <= w:
            out += ([sp(gap)] if out else []) + part
        else:
            hidden += 1
    if not out:
        return fit(parts[0], w) if parts else []
    return fit(out + ([sp(gap), lab(f"+{hidden}")] if more and hidden else []), w)


def plainly(phrases: Sequence[str], w: int) -> Rows:
    """An empty state: the longest of `phrases` that fits."""
    for text in phrases:
        if len(text) <= w:
            return [[lab(text)]]
    return [fit([lab(phrases[-1])], w)]


# Units


def size(n: float) -> str:
    """Bytes, binary, three digits at most: `512B`, `3.2G`, `120G`."""
    for unit in "BKMGTPE":
        if n < 999.5 or unit == "E":
            if unit == "B":
                return f"{n:.0f}B"
            return f"{n:.1f}{unit}" if n < 9.95 else f"{n:.0f}{unit}"
        n /= 1024
    raise AssertionError


def rate(n: float) -> str:
    return f"{size(n)}/s"


def percent(share: float) -> str:
    return f"{round(100 * share)}%"


def span(seconds: float) -> str:
    """Two units: `3d 4h`, `4h 12m`, `12m`."""
    m = int(seconds // 60)
    d, h, m = m // 1440, m // 60 % 24, m % 60
    if d:
        return f"{d}d {h}h"
    return f"{h}h {m:02d}m" if h else f"{m}m"


def ago(seconds: float | None) -> str:
    """One unit: `40s`, `5m`, `2h`, `3d`; empty when unknown."""
    if seconds is None:
        return ""
    for unit, n in (("d", 86400), ("h", 3600), ("m", 60)):
        if seconds >= n:
            return f"{int(seconds // n)}{unit}"
    return f"{int(seconds)}s"


def since(login: float, now: float) -> str:
    """A login time: `14:09` today, `Tue 14:09` this week, else `Nov 02`."""
    at, today = time.localtime(login), time.localtime(now)
    if now - login < 86400 and at.tm_yday == today.tm_yday:
        return time.strftime("%H:%M", at)
    if now - login < 6 * 86400:
        return time.strftime("%a %H:%M", at)
    return time.strftime("%b %d", at)


# Marks


def level(share: float, fg: str, warn: float = 0.9) -> str:
    return ALERT if share >= warn else fg


def bar(share: float, n: int, fg: str) -> Row:
    """A thin bar of `n` cells, by half cells: `━━━╾───`; a share above
    zero shows."""
    halves = round(min(1.0, max(0.0, share)) * n * 2)
    if share > 0 and halves == 0:
        halves = 1
    full, half = divmod(halves, 2)
    filled = {"t": "━" * full + "╾" * half, "fg": fg}
    return _rstrip([filled, {"t": "─" * (n - full - half), "fg": DIM}]) or [lab("")]


def spark(share: float, fg: str) -> Run:
    """One cell, eighths high; zero dim."""
    if share <= 0:
        return lab(SPARK[0])
    return {"t": SPARK[min(7, max(0, math.ceil(share * 8) - 1))], "fg": fg}


# Tables

type Cell = tuple[str, dict[str, Any]] | Callable[[int], Row]


@dataclass(frozen=True)
class Col:
    """A column of `table`. `flex`: cut to fit, the widest first, down to
    `least`; `grow`: takes the room left, `least` at least (its cells are
    functions of the width); `drop`: when the table is still too wide,
    the highest goes first (0: never); `most`: the widest it gets (0: no
    limit)."""

    head: str = ""
    right: bool = False
    flex: bool = False
    grow: bool = False
    drop: int = 0
    least: int = 4
    most: int = 0


def table(
    cols: Sequence[Col], cells: Sequence[Sequence[Cell]], w: int, head: bool
) -> Rows:
    """Rows of aligned columns, `GAP` apart, in `w` cells."""
    live = list(range(len(cols)))

    def shrink(widths: dict[int, int], excess: int, least: Callable[[int], int]) -> int:
        """Cut the flex columns, the widest first, a cell at a time."""
        while excess > 0:
            cut = [i for i in widths if cols[i].flex and widths[i] > least(i)]
            if not cut:
                break
            widths[max(cut, key=lambda i: widths[i])] -= 1
            excess -= 1
        return excess

    def natural(i: int) -> int:
        if cols[i].grow:
            return cols[i].least
        lens = [len(row[i][0]) for row in cells if isinstance(row[i], tuple)]
        most = cols[i].most or w
        return min(most, max(lens + ([len(cols[i].head)] if head else [0])))

    while True:
        widths = {i: natural(i) for i in live}
        excess = sum(widths.values()) + GAP * (len(live) - 1) - w
        excess = shrink(widths, excess, lambda i: cols[i].least)
        drops = [i for i in live if cols[i].drop]
        if excess <= 0 or not drops:
            break
        live.remove(max(drops, key=lambda i: cols[i].drop))
    # Still too wide: below `least`, to 3 cells.
    excess = shrink(widths, excess, lambda i: min(3, cols[i].least))
    if excess < 0 and (grow := [i for i in live if cols[i].grow]):
        widths[grow[0]] -= excess

    def line(texts: Sequence[Cell]) -> Row:
        out: Row = []
        for n, i in enumerate(live):
            out += [sp(GAP)] if n else []
            cell = texts[i]
            if callable(cell):
                runs = cell(widths[i])
                out += runs + [sp(widths[i] - width(runs))]
                continue
            text, style = cell
            text = clip(text, widths[i])
            pad = sp(widths[i] - len(text))
            run = {"t": text} | style
            out += [pad, run] if cols[i].right else [run, pad]
        return fit(out, w)

    rows = [line([(c.head, {"fg": DIM}) for c in cols])] if head else []
    return rows + [line(row) for row in cells]


def listed(
    cols: Sequence[Col], cells: Sequence[Sequence[Cell]], w: int, h: int
) -> Rows:
    """`table` in `h` rows: a head from 4 rows on, or when all fits with
    it; `+N more` for the rows left out."""
    head = 0 < len(cells) and (h >= 4 or len(cells) < h)
    room = h - head
    if len(cells) > room:
        rows = table(cols, cells[: room - 1], w, head)
        return rows + [fit([lab(f"+{len(cells) - room + 1} more")], w)]
    return table(cols, cells, w, head)


# glance

GLANCE = "bright-cyan"


def _load(g: probe.Glance) -> str:
    """The colour of the load: against the number of cores."""
    cores = max(1, len(g.cores))
    one = g.load[0]
    return ALERT if one > 2 * cores else WARN if one > cores else GLANCE


def render_glance(g: probe.Glance, w: int, h: int) -> Rows:
    mem = g.memory.used / g.memory.total if g.memory.total else 0.0
    swap = g.memory.swap_used / g.memory.swap_total if g.memory.swap_total else 0.0
    up = [lab("up "), val(span(g.uptime))]
    short = [
        [lab("cpu "), val(percent(g.cpu), level(g.cpu, GLANCE))],
        [lab("mem "), val(percent(mem), level(mem, GLANCE))],
        [lab("load "), val(f"{g.load[0]:.2f}", _load(g))],
        up,
    ]
    if h == 1:
        return [items(short, w, more=False)]
    if w < 20:
        return [fit(part, w) for part in short][:h]

    extras = {
        "mem": f"{size(g.memory.used)}/{size(g.memory.total)}",
        "swap": f"{size(g.memory.swap_used)}/{size(g.memory.swap_total)}",
    }
    extra = max(map(len, extras.values()))
    n = w - 10 - (GAP + extra)
    if n < 12:
        n, extra = w - 10, 0

    def meter(name: str, share: float) -> Row:
        row = [lab(f"{name:5}"), *bar(share, n, level(share, GLANCE))]
        row += [
            sp(n - width(row) + 5),
            val(f"{percent(share):>5}", level(share, GLANCE)),
        ]
        if extra and name in extras:
            used, total = extras[name].split("/")
            row += [sp(GAP), {"t": used}, lab(f"/{total}")]
        return row

    def load() -> Row:
        rest = [sp(), {"t": f"{g.load[1]:.2f}"}, sp(), {"t": f"{g.load[2]:.2f}"}]
        left = [lab("load "), val(f"{g.load[0]:.2f}", _load(g)), *rest]
        for row in (left, left[:2]):
            if width(row) + GAP + width(up) <= w:
                return [*row, sp(w - width(row) - width(up)), *up]
        return left

    def net() -> Row:
        rx, tx = [lab("↓ "), val(rate(g.rx))], [lab("↑ "), val(rate(g.tx))]
        return [lab("net  "), *rx, sp(3), *tx]

    def cores() -> Row:
        return [lab("core "), *(spark(c, level(c, GLANCE)) for c in g.cores)]

    # Rows by priority, shown in reading order.
    parts = {"cpu": lambda: meter("cpu", g.cpu), "mem": lambda: meter("mem", mem)}
    parts |= {"load": load, "net": net}
    if g.memory.swap_total:
        parts["swap"] = lambda: meter("swap", swap)
    if len(g.cores) > 1:
        parts["cores"] = cores
    chosen = list(parts)[:h]
    order = ["cpu", "cores", "mem", "swap", "load", "net"]
    rows = [fit(parts[k](), w) for k in order if k in chosen]
    room = h - len(rows)
    if room >= 2 and g.top:
        procs = list(g.top[: room - 1])
        if room - 1 > len(procs):
            rows.append([])
        cols = [
            Col("pid", right=True, drop=2),
            Col("process", flex=True),
            Col("cpu", right=True),
            Col("mem", right=True, drop=1),
        ]
        cells = [
            [
                (str(p.pid), {"fg": DIM}),
                (p.name, {}),
                (percent(p.cpu), {"fg": GLANCE}),
                (size(p.mem), {}),
            ]
            for p in procs
        ]
        rows += table(cols, cells, w, head=True)
    return rows[:h]


# connected

CONNECTED = "bright-magenta"


def render_connected(data: probe.Logins, w: int, h: int) -> Rows:
    seats = probe.seats(data.sessions)
    if not seats:
        return plainly(["no one logged in", "no one"], w)
    if h == 1:
        counts: dict[str, int] = {}
        for seat in seats:
            counts[seat.user] = counts.get(seat.user, 0) + len(seat.ttys)
        parts = [[val(u, CONNECTED), lab(f" {n}")] for u, n in counts.items()]
        return [items(parts, w)]
    cols = [
        Col("user", flex=True, least=5),
        Col("from", flex=True, least=12),
        Col("tty", drop=1),
        Col("since", drop=2),
        Col("idle", right=True),
        Col("what", flex=True, drop=3, least=6),
    ]
    cells = [
        [
            (s.user, {"fg": CONNECTED, "bold": True}),
            (s.origin, {"fg": DIM} if s.origin == "local" else {}),
            (s.ttys[0] if len(s.ttys) == 1 else f"{len(s.ttys)} ttys", {}),
            (since(s.login, data.now), {"fg": DIM}),
            (ago(s.idle), {}),
            (s.what or "", {"fg": DIM}),
        ]
        for s in seats
    ]
    return listed(cols, cells, w, h)


# tcp-listen

LISTEN = "bright-yellow"


def render_listen(data: Sequence[probe.Listener], w: int, h: int) -> Rows:
    if not data:
        return plainly(["nothing listens outside", "none outside", "none"], w)
    if h == 1:
        ports = sorted({x.port for x in data})
        return [items([[val(str(p), LISTEN)] for p in ports], w, gap=1)]
    cols = [
        Col("port", right=True),
        Col("address", flex=True, least=9),
        Col("user", drop=2),
        Col("process", flex=True, drop=1, least=5),
    ]
    cells = [
        [
            (str(x.port), {"fg": LISTEN, "bold": True}),
            (x.addr, {}),
            (x.user, {"fg": DIM}),
            (x.process or "?", {} if x.process else {"fg": DIM}),
        ]
        for x in data
    ]
    if not any(x.process for x in data):  # none of the user's own
        cols, cells = cols[:3], [row[:3] for row in cells]
    return listed(cols, cells, w, h)


# disk-space

DISKS = "bright-green"


def render_disks(data: Sequence[probe.Disk], w: int, h: int) -> Rows:
    if not data:
        return plainly(["no local disks", "no disks"], w)
    if h == 1:
        parts = [
            [{"t": d.point + " "}, val(percent(d.share), level(d.share, DISKS))]
            for d in sorted(data, key=lambda d: -d.share)  # the fullest first
        ]
        return [items(parts, w)]
    cols = [
        Col("mount", flex=True, least=8, most=max(12, w // 3)),
        Col("", grow=True, drop=2, least=6),
        Col("used", right=True),
        Col("free", right=True, drop=1),
        Col("size", right=True, drop=3),
    ]

    def meter(share: float) -> Callable[[int], Row]:
        return lambda n: bar(share, n, level(share, DISKS))

    cells = [
        [
            (d.point, {"fg": DISKS, "bold": True}),
            meter(d.share),
            (percent(d.share), {"fg": level(d.share, DISKS), "bold": True}),
            (size(d.free), {}),
            (size(d.size), {"fg": DIM}),
        ]
        for d in data
    ]
    return listed(cols, cells, w, h)


# Registry


@dataclass(frozen=True)
class Monitor:
    """One tag: `read()` gives the data, `render(data, w, h)` the rows."""

    name: str
    title: str
    sizes: tuple[tuple[int, int], ...]
    read: Callable[[], Any]
    render: Callable[[Any, int, int], Rows]


MONITORS: dict[str, Monitor] = {
    m.name: m
    for m in (
        Monitor(
            "glance",
            "glance",
            ((18, 1), (30, 4), (44, 9), (56, 13)),
            probe.GlanceReader().read,
            render_glance,
        ),
        Monitor(
            "connected",
            "connected",
            ((20, 1), (32, 3), (44, 6), (56, 10)),
            probe.read_logins,
            render_connected,
        ),
        Monitor(
            "tcp-listen",
            "listening",
            ((18, 1), (30, 3), (44, 6), (56, 10)),
            probe.read_listeners,
            render_listen,
        ),
        Monitor(
            "disk-space",
            "disks",
            ((20, 1), (32, 3), (44, 5), (56, 8)),
            probe.read_disks,
            render_disks,
        ),
    )
}

MERGED = "monitor"  # the tag of `--merged`


def trimmed(monitor: Monitor, data: Any) -> list[tuple[int, int]]:
    """The sizes of `monitor` cut to the rows `data` fills, when the render
    in fewer rows is the same. Widths stay: they would follow the data
    at each refresh. Declared again when they change, a widget wastes no
    row."""
    out = []
    for w, h in monitor.sizes:
        rows = monitor.render(data, w, h)
        if 0 < len(rows) < h and monitor.render(data, w, len(rows)) == rows:
            h = len(rows)
        if (w, h) not in out:
            out.append((w, h))
    return out


def merged_sizes(names: Sequence[str]) -> list[tuple[int, int]]:
    """The sizes of the merged tag: the n-th is the largest width and
    height of the n-th sizes of `names`, so that each monitor has its
    n-th layout in it."""
    sizes = [MONITORS[n].sizes for n in names]
    ranks = max(map(len, sizes))
    return [
        (
            max(s[min(r, len(s) - 1)][0] for s in sizes),
            max(s[min(r, len(s) - 1)][1] for s in sizes),
        )
        for r in range(ranks)
    ]


def merged(
    data: Mapping[str, Any], names: Sequence[str], turn: int, w: int, h: int
) -> tuple[str, Rows]:
    """The title and rows of the merged tag at `turn`: monitor
    `names[turn % len(names)]`, on its `data`."""
    shown = MONITORS[names[turn % len(names)]]
    return shown.title, shown.render(data[shown.name], w, h)


# --print

#: Corners (top-left, top-right, bottom-left, bottom-right), top and
#: bottom edge, left and right edge: the presets of the core.
BORDERS = {
    "plain": "┌┐└┘─│",
    "round": "╭╮╰╯─│",
    "heavy": "┏┓┗┛━┃",
    "double": "╔╗╚╝═║",
    "ascii": "++++-|",
}
COLOURS = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]


def sgr(run: Run) -> str:
    """The ANSI escape for the style of `run`."""
    codes = ["0"] + ["1"] * bool(run.get("bold")) + ["2"] * bool(run.get("dim"))
    fg = run.get("fg")
    if isinstance(fg, int):
        codes.append(f"38;5;{fg}")
    elif isinstance(fg, str) and fg.startswith("#"):
        codes.append("38;2;" + ";".join(str(int(fg[i : i + 2], 16)) for i in (1, 3, 5)))
    elif isinstance(fg, str):
        bright, _, name = fg.rpartition("-")
        if name in COLOURS:
            codes.append(str((90 if bright else 30) + COLOURS.index(name)))
    return f"\x1b[{';'.join(codes)}m"


def box(
    title: str, rows: Rows, w: int, h: int, border: str = "plain", colour: bool = False
) -> list[str]:
    """`rows` as text in a border of `w` by `h` inner cells, the title
    on the top edge as the driver draws it."""
    reset = "\x1b[0m" if colour else ""

    def ink(run: Run) -> str:
        return sgr(run) + run["t"] + reset if colour else run["t"]

    def body(row: Row) -> str:
        return "".join(ink(r) for r in row) + " " * (w - width(row))

    if border == "none":
        return [body(row) for row in rows] + [" " * w] * (h - len(rows))
    tl, tr, bl, br, edge, side = BORDERS.get(border, BORDERS["plain"])
    dim = (lambda t: ink({"t": t, "fg": DIM})) if colour else (lambda t: t)
    top = ""
    if w >= 3 and title:
        top = " " + clip(title, w - 2) + " "
    lines = [dim(tl) + ink({"t": top}) + dim(edge * (w - len(top)) + tr)]
    lines += [dim(side) + body(row) + dim(side) for row in rows]
    lines += [dim(side) + " " * w + dim(side)] * (h - len(rows))
    return [*lines, dim(bl + edge * w + br)]


def show(args: argparse.Namespace) -> int:
    """`--print`: each selected monitor once, in its sizes cut to its data
    (`trimmed`), in the merged sizes with `--merged`, or in the size
    given."""
    colour = sys.stdout.isatty() and not os.environ.get("NO_COLOR")
    data = {name: MONITORS[name].read() for name in args.monitor}
    for turn, name in enumerate(args.monitor):
        sizes = merged_sizes(args.monitor)
        if not args.merged:
            sizes = trimmed(MONITORS[name], data[name])
        for w, h in [args.print] if args.print else sizes:
            if args.merged:
                title, rows = merged(data, args.monitor, turn, w, h)
            else:
                title, rows = (
                    MONITORS[name].title,
                    MONITORS[name].render(data[name], w, h),
                )
            print(f"{name} {w}x{h}")
            print("\n".join(box(title, rows, w, h, args.border, colour)))
            print()
    return 0


# Live


def live(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    """The widgets on the session: declared, shown, set every
    `args.every` seconds; with `args.merged`, one turn every
    `args.cycle`. Not wired yet: `main` does not call it."""
    raise NotImplementedError("widgets")


def _names(text: str) -> list[str]:
    names = [n.strip() for n in text.split(",") if n.strip()]
    bad = [n for n in names if n not in MONITORS]
    if bad or not names:
        what = ",".join(bad) or repr(text)
        known = ", ".join(MONITORS)
        raise argparse.ArgumentTypeError(f"unknown monitor {what}; choose from {known}")
    return list(dict.fromkeys(names))


def _size(text: str) -> tuple[int, int]:
    w, sep, h = text.partition("x")
    if not (sep and w.isdigit() and h.isdigit() and int(w) > 0 and int(h) > 0):
        raise argparse.ArgumentTypeError(f"not a size WxH: {text!r}")
    return int(w), int(h)


def _seconds(text: str) -> float:
    try:
        value = float(text)
    except ValueError:
        value = 0.0
    if not value > 0:
        raise argparse.ArgumentTypeError(f"not a positive number of seconds: {text!r}")
    return value


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser("monitor", "The state of this machine, in widgets.")
    out.add_argument(
        "--monitor",
        type=_names,
        default=list(MONITORS),
        metavar="LIST",
        help=f"comma-separated, of {','.join(MONITORS)} [default: all]",
    )
    out.add_argument(
        "--merged", action="store_true", help="one widget, the monitors in turn"
    )
    out.add_argument(
        "--every",
        type=_seconds,
        default=2.0,
        metavar="SECONDS",
        help="refresh [default: 2]",
    )
    out.add_argument(
        "--cycle",
        type=_seconds,
        default=8.0,
        metavar="SECONDS",
        help="turn of --merged [default: 8]",
    )
    out.add_argument(
        "--border",
        default="plain",
        metavar="NAME",
        help="border preset [default: plain]",
    )
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(),
        default=None,
        metavar="WxH",
        help="no driver: print the variants once, or the size given",
    )
    return _cli.parse(out, argv)


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    args = options(argv)
    if args.print is not None:
        return show(args)
    print("ptyrc-app monitor: needs the widgets wiring; try --print", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
