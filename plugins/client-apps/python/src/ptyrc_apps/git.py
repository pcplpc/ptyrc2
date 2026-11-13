# SPDX-License-Identifier: GPL-3.0-or-later
"""Git: a repository, in widgets. It reads, it never writes.

    ptyrc-app git                      # the head: branch, upstream, counts
    ptyrc-app git -t -l                # and the status, the log
    ptyrc-app git -a                   # every tag, the graph, the warnings
    ptyrc-app git --print              # no driver: every variant
    ptyrc-app git --print 48x8 --monitor log -g --repo ~/src/linux

Tags, each a set of variants:

- `head`: branch, upstream, ahead and behind, an operation left open,
  the counts of the status: `main ↑2 ↓1 ●3 +1 ?4`;
- `status` (`-t`): the paths, as `git status -s`, with their lines;
- `log` (`-l`): the last commits; `-g`: as a graph, with the refs;
- `branches` (`-b`): the recent ones, age, ahead and behind;
- `stash` (`-S`), `worktrees` (`-w`): hidden while there is none;
- `pulse` (`-p`): commits a day over the last weeks, in braille.

`-W`: the border of `head` chases while something wants a look
(`warnings`). `-a`: all of them. `--monitor LIST` names the tags
instead; `--merged`: one tag, `git`, showing them in turn.

The repository: `--repo`, else the one the wrapped program is in
(`/proc/<pid>/cwd`, followed when it moves), else the one the app is
started in. Read every `--every` seconds, and within a second of a
change of the git directory (`_gitprobe.Watch`).

The host mouse over a commit of `log` tells it in full: its subject
wrapped, its author, date and refs (`Hover`).

`render_*(repo, w, h, look)` are pure: rows of styled runs, at most
`h` of at most `w` cells; `repo` none: no repository.
"""

from __future__ import annotations

import argparse
import os
import sys
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ptyrc import Pilot, clock

from . import _cli, _widgets
from . import _gitprobe as probe
from .monitor import (
    ALERT,
    DIM,
    SPARK,
    WARN,
    Col,
    Monitor,
    _seconds,
    _size,
    ago,
    box,
    clip,
    fit,
    items,
    lab,
    plainly,
    sp,
    table,
    val,
    width,
)

__all__ = [
    "FLAGS",
    "MERGED",
    "PLAIN",
    "TAGS",
    "Hover",
    "Look",
    "chase",
    "live",
    "log_rows",
    "main",
    "named",
    "options",
    "quiet",
    "render_branches",
    "render_head",
    "render_log",
    "render_pulse",
    "render_stash",
    "render_status",
    "render_worktrees",
    "tags",
    "trimmed",
    "warnings",
]

type Run = dict[str, Any]
type Row = list[Run]
type Rows = list[Row]

BRANCH = "bright-cyan"
AHEAD = "green"
BEHIND = "yellow"
STAGED = "green"
UNSTAGED = "yellow"
OID = "yellow"
STATE = "bright-magenta"
PULSE = "green"
#: The lanes of the graph, a colour each, in turn.
LANES = ("cyan", "magenta", "blue", "green", "yellow")

NONE = ["not a git repository", "no repository", "no repo", "—"]


@dataclass(frozen=True)
class Look:
    """How the tags are drawn. `graph`: the log as a graph, with its
    refs; `warn`: the warnings told in `head`; a tree is `dirty` after
    that many seconds, an operation left `open` after that many."""

    graph: bool = False
    warn: bool = False
    dirty: float = 4 * 3600
    open: float = 15 * 60


#: The bare look.
PLAIN = Look()


def left(text: str, n: int) -> str:
    """`text` in `n` cells at most, its end kept: `…pps/git.py`."""
    if len(text) <= n:
        return text
    return "…" + text[len(text) - n + 1 :] if n > 0 else ""


def warnings(repo: probe.Repo | None, look: Look = PLAIN) -> list[str]:
    """What wants a look, the most pressing first."""
    if repo is None:
        return []
    out = []
    if n := repo.count("conflict"):
        out.append(f"{n} conflict{'s' * (n > 1)}")
    since = repo.state_since if repo.state else None
    if since is not None and (age := repo.now - since) >= look.open:
        out.append(f"{repo.state.split()[0]} open for {ago(age)}")
    if repo.head.gone:
        out.append("upstream gone")
    if repo.head.behind:
        out.append(f"{repo.head.behind} behind {repo.head.upstream}")
    since = repo.dirty_since
    if since is not None and (age := repo.now - since) >= look.dirty:
        out.append(f"dirty for {ago(age)}")
    return out


def chase(border: str) -> dict[str, Any]:
    """`border` with the chase of a warning."""
    style = {"fg": "bright-yellow", "bold": True}
    return {"base": border, "chase": {"len": 4, "ms": 90, "runs": 1, "style": style}}


# head


def _name(head: probe.Head, n: int) -> Row:
    """The branch in `n` cells, or the commit when detached."""
    if head.branch is None:
        return [lab("@"), val(clip(head.oid or "?", max(1, n - 1)), OID)]
    return [val(clip(head.branch, n), BRANCH)]


def _track(head: probe.Head, zero: bool = False) -> Row:
    """`↑2 ↓1`; `zero`: a side at zero is told too, dim."""
    out: Row = []
    for mark, n, fg in (("↑", head.ahead, AHEAD), ("↓", head.behind, BEHIND)):
        if n or (zero and head.upstream and not head.gone):
            out += [sp()] if out else []
            out.append(val(f"{mark}{n}", fg) if n else lab(f"{mark}0"))
    return out


def _counts(repo: probe.Repo, words: bool = False) -> Row:
    """`✖1 ●3 +1 ?4`, `✓` when clean; `words`: each named."""
    out: Row = []
    for mark, kind, fg, word in (
        ("✖", "conflict", ALERT, "conflict"),
        ("●", "staged", STAGED, "staged"),
        ("+", "unstaged", UNSTAGED, "changed"),
        ("?", "untracked", None, "new"),
    ):
        if n := repo.count(kind):
            out += [sp(2 if words else 1)] if out else []
            out.append(val(f"{mark}{n}", fg) if fg else lab(f"{mark}{n}"))
            out += [lab(f" {word}")] if words else []
    return out or [{"t": "✓", "fg": STAGED}, *([lab(" clean")] if words else [])]


def _last(repo: probe.Repo, w: int) -> Row:
    """The commit of `HEAD` in a line."""
    if not repo.commits:
        return [lab("no commit yet")]
    c = repo.commits[0]
    return fit(
        [{"t": c.oid, "fg": OID}, sp(), lab(ago(repo.now - c.at)), sp(), _t(c.subject)],
        w,
    )


def _t(text: str) -> Run:
    return {"t": text}


def render_head(repo: probe.Repo | None, w: int, h: int, look: Look = PLAIN) -> Rows:
    if repo is None:
        return plainly(NONE, w)
    head = repo.head
    state: Row = [val(repo.state, STATE)] if repo.state else []
    alerts = warnings(repo, look) if look.warn else []
    if h == 1:
        # Told in order: name, state, track, counts. An operation left
        # open and the counts weigh more than the name; else the name
        # has six cells kept for it.
        parts = {1: state, 2: _track(head), 3: _counts(repo)}
        least = min(6, len(head.branch or "@" + head.oid))
        kept: dict[int, Row] = {}
        room = w if state else w - least - 1
        for n in (1, 3, 0, 2) if state else (2, 3):
            if n == 0:
                if room >= least:
                    kept[0], room = [], room - least - 1
            elif parts[n] and width(parts[n]) <= room:
                kept[n] = parts[n]
                room -= width(parts[n]) + 1
        if not state or not kept:
            kept[0] = []
        if 3 not in kept and repo.changes and room >= 1:
            kept[3] = [val("*", UNSTAGED)]  # not clean, and no room to tell
        if 0 in kept:  # the name takes what the others left
            others = sum(width(p) + 1 for n, p in kept.items() if n)
            kept[0] = _name(head, max(1, w - others))
        return [fit([run for n in sorted(kept) for run in (*kept[n], sp())], w)]
    first = _name(head, w)
    if head.upstream:
        tail = [lab(" → "), lab(head.upstream)]
        tail += [sp(), val("gone", ALERT)] if head.gone else []
        track = _track(head, zero=True)
        tail += [sp(2), *track] if track else []
        if width(first) + width(tail) > w:  # the counts before the name of it
            tail = [sp(), *(track or [val("gone", ALERT)])]
        first = _name(head, max(6, w - width(tail))) + tail
    elif head.branch is not None and width(first) + 13 <= w:
        first += [lab("  no upstream")]
    rows = [fit(first, w)]
    words = width(_counts(repo, True)) + (width(state) + 2 if state else 0) <= w
    second = _counts(repo, words)
    rows.append(fit([*state, sp(2), *second] if state else second, w))
    lines = [[{"t": "▲ ", "fg": WARN}, {"t": a, "fg": WARN}] for a in alerts]
    rows += [fit(row, w) for row in lines]
    rows.append(_last(repo, w))
    extra: Row = []
    if n := len(repo.stashes):
        extra += [val(f"≡{n}"), lab(" stashed")]
    if (n := len(repo.worktrees)) > 1:
        extra += ([sp(2)] if extra else []) + [val(f"{n}"), lab(" worktrees")]
    rows += [fit(extra, w)] if extra else []
    return rows[:h]


# status

KINDS = {"conflict": 0, "staged": 1, "unstaged": 2, "untracked": 3}


def _more(cols: Sequence[Col], cells: Sequence[Sequence[Any]], w: int, h: int) -> Rows:
    """`table` in `h` rows, no head; `+N more` for the rows left out."""
    if len(cells) <= h:
        return table(cols, cells, w, False)
    rows = table(cols, cells[: h - 1], w, False)
    return rows + [fit([lab(f"+{len(cells) - h + 1} more")], w)]


def _lines(repo: probe.Repo) -> tuple[int, int]:
    added = sum(c.added or 0 for c in repo.changes)
    return added, sum(c.deleted or 0 for c in repo.changes)


def render_status(repo: probe.Repo | None, w: int, h: int, look: Look = PLAIN) -> Rows:
    if repo is None:
        return plainly(NONE, w)
    if not repo.changes:
        return plainly(["working tree clean", "clean", "✓"], w)
    added, deleted = _lines(repo)
    total: Row = []
    if added:
        total += [{"t": f"+{added}", "fg": STAGED}]
    if deleted:
        total += ([sp()] if total else []) + [{"t": f"−{deleted}", "fg": ALERT}]
    if h == 1:
        return [
            items([_counts(repo), total] if total else [_counts(repo)], w, more=False)
        ]
    paths: dict[str, dict[str, probe.Change]] = {}
    for c in repo.changes:
        paths.setdefault(c.path, {})[c.kind] = c
    order = sorted(paths, key=lambda p: min(KINDS[k] for k in paths[p]))
    cells = []
    for path in order:
        got = paths[path]
        if "conflict" in got:
            x, y = (
                ("U", {"fg": ALERT, "bold": True}),
                ("U", {"fg": ALERT, "bold": True}),
            )
        elif "untracked" in got:
            x, y = ("?", {"fg": DIM}), ("?", {"fg": DIM})
        else:
            x = (got["staged"].code if "staged" in got else " ", {"fg": STAGED})
            y = (got["unstaged"].code if "unstaged" in got else " ", {"fg": UNSTAGED})
        plus = sum(c.added or 0 for c in got.values())
        minus = sum(c.deleted or 0 for c in got.values())
        style = {"fg": DIM} if "untracked" in got else {}

        def name(n: int, path: str = path, style: dict[str, Any] = style) -> Row:
            return [{"t": left(path, n)} | style]

        cells.append(
            [
                lambda n, x=x, y=y: [{"t": x[0]} | x[1], {"t": y[0]} | y[1]],
                name,
                (f"+{plus}" if plus else "", {"fg": STAGED}),
                (f"−{minus}" if minus else "", {"fg": ALERT}),
            ]
        )
    return _status_rows(cells, w, h, repo)


def _status_rows(
    cells: Sequence[Sequence[Any]], w: int, h: int, repo: probe.Repo
) -> Rows:
    """A row a path: its two letters, the path with its end kept, the
    lines added and deleted in two columns at the right."""
    cut = sum(repo.count(k) for k in KINDS) - len(repo.changes)  # not read
    room = h if len(cells) <= h and not cut else h - 1
    rows = []
    counts = [(c[2], c[3]) for c in cells[:room]]
    wide = [max((len(c[i][0]) for c in counts), default=0) for i in (0, 1)]
    if w < 24:
        wide = [0, 0]
    tail = sum(n + 1 for n in wide if n)
    for line in cells[:room]:
        row = [*line[0](2), sp(), *line[1](max(1, w - 3 - tail))]
        if tail:
            row += [sp(w - tail - width(row))]
            for (text, style), n in zip(line[2:], wide, strict=True):
                row += [sp(1 + n - len(text)), {"t": text} | style] if n else []
        rows.append(fit(row, w))
    if len(cells) + cut > len(rows):
        rows.append(fit([lab(f"+{len(cells) + cut - len(rows)} more")], w))
    return rows


# log


def _refs(c: probe.Commit) -> Row:
    """The refs of a commit as tags: `main origin/main v1.0`."""
    out: Row = []
    for ref in c.refs:
        if ref == "HEAD":
            run = val("HEAD", BRANCH)
        elif ref.startswith("HEAD -> "):
            run = val(ref[8:], BRANCH)
        elif ref.startswith("tag: "):
            run = {"t": ref[5:], "fg": "bright-yellow"}
        elif "/" in ref:
            run = {"t": clip(ref, 16), "fg": "red"}
        else:
            run = {"t": clip(ref, 16), "fg": "green"}
        out += ([sp()] if out else []) + [run]
    return out


GRAPH = str.maketrans({"*": "●", "|": "│", "/": "╱", "\\": "╲", "_": "─"})


def _lanes(text: str) -> Row:
    """A row of `git log --graph`, a colour a lane."""
    out: Row = []
    for x, ch in enumerate(text):
        fg = LANES[(x // 2) % len(LANES)]
        out.append(_t(" ") if ch == " " else {"t": ch.translate(GRAPH), "fg": fg})
    return out


def log_rows(
    repo: probe.Repo, w: int, h: int, look: Look = PLAIN, focus: int | None = None
) -> list[tuple[int, Row]]:
    """The rows of the log, each with the index of its commit; the
    commit `focus` is told in full: its subject wrapped, its author
    and date under it."""
    out: list[tuple[int, Row]] = []
    lane = max((len(c.graph[0]) for c in repo.commits[:h]), default=1)
    lane = min(lane, max(1, w // 4))
    for i, c in enumerate(repo.commits):
        if len(out) >= h:
            break
        age = ago(repo.now - c.at)
        row: Row = []
        if look.graph:
            row = [*_lanes(c.graph[0][:lane]), sp(lane - len(c.graph[0]) + 1)]
        row += [{"t": c.oid, "fg": OID}, sp(max(1, 4 - len(age))), lab(age), sp()]
        room = w - width(row)
        if i == focus and room >= 8:
            words, lines = c.subject.split(), [""]
            for word in words:
                if lines[-1] and len(lines[-1]) + 1 + len(word) > room:
                    lines.append("")
                lines[-1] += (" " if lines[-1] else "") + word
            at = time.strftime("%b %d %H:%M", time.localtime(c.at))
            pad = sp(w - room)
            out.append((i, fit([*row, val(lines[0])], w)))
            out += [(i, fit([pad, val(text)], w)) for text in lines[1:3]]
            out.append((i, fit([pad, lab(f"{c.author} · {at}")], w)))
            if refs := _refs(c):
                out.append((i, fit([pad, *refs], w)))
            continue
        refs = _refs(c) if look.graph else []
        if refs and 2 * width(refs) <= room:
            row += [*refs, sp()]
        out.append((i, fit([*row, _t(c.subject)], w)))
        if look.graph:
            for extra in c.graph[1:]:
                out.append((i, fit(_lanes(extra[:lane]), w)))
    return out[:h]


def render_log(
    repo: probe.Repo | None,
    w: int,
    h: int,
    look: Look = PLAIN,
    focus: int | None = None,
) -> Rows:
    if repo is None:
        return plainly(NONE, w)
    if not repo.commits:
        return plainly(["no commit yet", "no commit", "—"], w)
    if h == 1:
        return [_last(repo, w)]
    return [row for _, row in log_rows(repo, w, h, look, focus)]


class Hover:
    """The commit under the pointer, for `render_log(focus=)`.
    `on_move`, `on_leave`: true when the log is to be drawn again."""

    def __init__(self) -> None:
        self.focus: int | None = None
        self.rows: list[int] = []

    def drawn(self, repo: probe.Repo | None, w: int, h: int, look: Look) -> Rows:
        """`render_log` with the focus; the rows drawn are kept, to
        tell which commit a cell is of."""
        if repo is None or not repo.commits or h == 1:
            self.rows = []
            return render_log(repo, w, h, look)
        rows = log_rows(repo, w, h, look, self.focus)
        self.rows = [i for i, _ in rows]
        return [row for _, row in rows]

    def on_move(self, x: int, y: int) -> bool:
        focus = self.rows[y] if 0 <= y < len(self.rows) else None
        changed = focus != self.focus
        self.focus = focus
        return changed

    def on_leave(self) -> bool:
        changed = self.focus is not None
        self.focus = None
        return changed


# branches, stash, worktrees


def _mark(current: bool) -> str:
    return "● " if current else "  "


def _tracked(b: probe.Branch) -> tuple[str, dict[str, Any]]:
    if b.gone:
        return "gone", {"fg": ALERT}
    text = " ".join(f"{m}{n}" for m, n in (("↑", b.ahead), ("↓", b.behind)) if n)
    return text, {"fg": BEHIND if b.behind else AHEAD}


def render_branches(
    repo: probe.Repo | None, w: int, h: int, look: Look = PLAIN
) -> Rows:
    if repo is None:
        return plainly(NONE, w)
    if not repo.branches:
        return plainly(["no branch yet", "no branch", "—"], w)
    if h == 1:
        others = len(repo.branches) - (repo.head.branch is not None)
        tail = [lab(f"+{others}")] if others else []
        room = w - (width(tail) + 1 if tail else 0)
        return [fit([*_name(repo.head, max(4, room)), sp(), *tail], w)]
    tracked = any(b.gone or b.ahead or b.behind for b in repo.branches)
    cols = [Col(flex=True, least=10), Col(right=True)]
    cols += [Col(drop=1)] if tracked else []
    cols += [Col(flex=True, drop=2, least=10)]
    cells = [
        [
            (
                _mark(b.current) + b.name,
                {"fg": BRANCH, "bold": True} if b.current else {},
            ),
            (ago(repo.now - b.at), {"fg": DIM}),
            *([_tracked(b)] if tracked else []),
            (b.subject, {"fg": DIM}),
        ]
        for b in repo.branches
    ]
    return _more(cols, cells, w, h)


def _stashed(subject: str, current: str | None = None) -> str:
    """`WIP on topic: 1a2b3c4 subject` as `topic: subject`; the branch
    `current` is not named."""
    for lead in ("WIP on ", "On "):
        if subject.startswith(lead):
            branch, sep, rest = subject[len(lead) :].partition(": ")
            if lead == "WIP on " and sep:
                rest = rest.partition(" ")[2] or rest
            if not sep:
                return subject
            return rest if branch == current else f"{branch}: {rest}"
    return subject


def render_stash(repo: probe.Repo | None, w: int, h: int, look: Look = PLAIN) -> Rows:
    if repo is None:
        return plainly(NONE, w)
    if not repo.stashes:
        return plainly(["nothing stashed", "no stash", "—"], w)
    if h == 1:
        n, age = len(repo.stashes), ago(repo.now - repo.stashes[0].at)
        return [
            items(
                [[val(f"≡{n}"), lab(" stashed")], [lab(f"last {age}")]], w, more=False
            )
        ]
    cols = [Col(right=True, least=1), Col(right=True), Col(flex=True, least=8)]
    cells = [
        [
            (str(s.n), {"fg": OID}),
            (ago(repo.now - s.at), {"fg": DIM}),
            (_stashed(s.subject, repo.head.branch), {}),
        ]
        for s in repo.stashes
    ]
    return _more(cols, cells, w, h)


def render_worktrees(
    repo: probe.Repo | None, w: int, h: int, look: Look = PLAIN
) -> Rows:
    if repo is None:
        return plainly(NONE, w)
    trees = repo.worktrees
    if len(trees) < 2:
        return plainly(["no other worktree", "one worktree", "—"], w)
    if h == 1:
        return [fit([val(str(len(trees))), lab(" worktrees")], w)]
    cols = [Col(flex=True, least=8), Col(flex=True, least=6), Col(drop=1)]
    cells = []
    for t in trees:
        flags = " ".join(f for f in t.flags if f != "detached")
        at = (t.branch, {"fg": "green"}) if t.branch else ("@" + t.oid, {"fg": OID})
        cells.append(
            [
                (
                    _mark(t.current) + Path(t.path).name,
                    {"bold": True} if t.current else {},
                ),
                at if "bare" not in t.flags else ("", {}),
                (flags, {"fg": WARN if "prunable" in t.flags else DIM}),
            ]
        )
    return _more(cols, cells, w, h)


# pulse

#: Dots of a braille cell, by column then row from the bottom.
DOTS = ((0x40, 0x04, 0x02, 0x01), (0x80, 0x20, 0x10, 0x08))


def _week(repo: probe.Repo) -> int:
    return sum(repo.days[-7:])


def render_pulse(repo: probe.Repo | None, w: int, h: int, look: Look = PLAIN) -> Rows:
    """Commits a day: one cell a day in one row, eighths high; a dot
    column a day in more, on a base line, the last day at the right."""
    if repo is None:
        return plainly(NONE, w)
    if repo.last is None:
        return plainly(["no commit yet", "no commit", "—"], w)
    age = ago(repo.now - repo.last)
    if h == 1:
        n = max(0, min(len(repo.days), w - len(age) - 1))
        days = repo.days[len(repo.days) - n :] if n else ()
        top = max(1, *days) if days else 1
        cells: Row = []
        for d in days:
            glyph = SPARK[min(7, -(-d * 7 // top))] if d else SPARK[0]
            cells.append({"t": glyph, "fg": PULSE} if d else lab(glyph))
        return [fit([*cells, sp(), lab(age)] if cells else [lab(age)], w)]
    rows_n = h - 1
    n = min(len(repo.days), 2 * w)
    days = repo.days[len(repo.days) - n :]
    top, levels = max(1, *days), 4 * rows_n
    heights = [1 + (-(-d * (levels - 1) // top) if d else 0) for d in days]
    heights = [0] * (2 * w - n) + heights
    chart: Rows = []
    for r in range(rows_n - 1, -1, -1):
        text = ""
        for x in range(w):
            bits = 0
            for side in (0, 1):
                filled = heights[2 * x + side] - 4 * r
                for dot in range(max(0, min(4, filled))):
                    bits |= DOTS[side][dot]
            text += chr(0x2800 + bits) if bits else " "
        chart.append([{"t": text.rstrip(), "fg": PULSE}] if text.strip() else [])
    week = _week(repo)
    caption = [
        [val(str(week)), lab(" this week")],
        [lab("last "), val(age)],
        [lab(f"{n} days, top "), val(str(top))],
    ]
    return [*chart, items(caption, w, more=False)]


# The tags

type Render = Callable[[probe.Repo | None, int, int, Look], Rows]

_TAGS: tuple[tuple[str, str, tuple[tuple[int, int], ...], Render], ...] = (
    ("head", "git", ((12, 1), (24, 1), (34, 3), (46, 5)), render_head),
    ("status", "status", ((16, 1), (28, 4), (40, 8), (56, 12)), render_status),
    ("log", "log", ((24, 1), (34, 4), (48, 8), (64, 14)), render_log),
    ("branches", "branches", ((16, 1), (30, 3), (44, 6), (60, 10)), render_branches),
    ("stash", "stash", ((16, 1), (30, 3), (44, 5)), render_stash),
    ("worktrees", "worktrees", ((14, 1), (30, 3), (44, 5)), render_worktrees),
    ("pulse", "pulse", ((14, 1), (24, 3), (40, 4), (56, 5)), render_pulse),
)
TAGS = tuple(name for name, *_ in _TAGS)
#: The option that adds each tag to `head`.
FLAGS = {
    "status": "-t",
    "log": "-l",
    "branches": "-b",
    "stash": "-S",
    "worktrees": "-w",
    "pulse": "-p",
}
MERGED = "git"  # the tag of `--merged`
#: Session seconds a commit stays told in full after the last move.
HOVER = 3.0


def tags(look: Look = PLAIN, log: Render | None = None) -> dict[str, Monitor]:
    """The tags drawn with `look`, as monitors of a `Repo`; `log`: in
    place of `render_log` (a `Hover`)."""

    def bound(name: str, render: Render) -> Callable[[Any, int, int], Rows]:
        render = log if name == "log" and log is not None else render
        return lambda repo, w, h: render(repo, w, h, look)

    return {
        name: Monitor(name, title, sizes, lambda: None, bound(name, render))
        for name, title, sizes, render in _TAGS
    }


def trimmed(tag: Monitor, repo: probe.Repo | None) -> list[tuple[int, int]]:
    """The sizes of `tag` cut to the rows `repo` fills; one row left:
    cut to its cells too, no narrower than the smallest size. One
    size a declared size: two may be equal."""
    out = []
    for w, h in tag.sizes:
        rows = tag.render(repo, w, h)
        if len(rows) == 1 < h:
            w = min(w, max(width(rows[0]), tag.sizes[0][0]))
        out.append((w, max(1, min(h, len(rows)))))
    return out


def named(sizes: Sequence[tuple[int, int]]) -> dict[str, tuple[int, int]]:
    """Variants by rank: as many at each declaration, two of one size
    allowed. A tag declared again with fewer, a variant past them
    still placed, is more than the driver takes."""
    return {f"v{n}": size for n, size in enumerate(sizes)}


def quiet(name: str, repo: probe.Repo | None) -> bool:
    """A tag with nothing to tell: hidden, unless named by `--monitor`."""
    if repo is None:
        return name != "head"
    if name == "stash":
        return not repo.stashes
    return name == "worktrees" and len(repo.worktrees) < 2


def title(name: str, repo: probe.Repo | None, known: dict[str, Monitor]) -> str:
    return repo.name if name == "head" and repo is not None else known[name].title


# --print


def show(args: argparse.Namespace) -> int:
    """`--print`: each tag once, in its sizes cut to its data, or in
    the size given."""
    colour = sys.stdout.isatty() and not os.environ.get("NO_COLOR")
    repo = probe.read(args.repo or Path.cwd())
    known = tags(args.look)
    for name in args.monitor:
        if not args.named and quiet(name, repo):
            continue
        sizes = [args.print] if args.print else trimmed(known[name], repo)
        for w, h in dict.fromkeys(sizes):
            rows = known[name].render(repo, w, h)
            print(f"{name} {w}x{h}")
            print(
                "\n".join(
                    box(title(name, repo, known), rows, w, h, args.border, colour)
                )
            )
            print()
    for warning in warnings(repo, args.look) if args.look.warn else []:
        print(f"warning: {warning}")
    return 0


# Live


class Source:
    """The repository shown: `--repo`, else where the wrapped program
    is, else where the app is. `poll` reads it when it is time."""

    def __init__(self, pilot: Pilot, args: argparse.Namespace) -> None:
        self.fixed: Path | None = args.repo
        self.pid: int | None = pilot.hello.get("pid")
        self.every: float = args.every
        self.at: Path | None = None
        self.watch: probe.Watch | None = None
        self.repo: probe.Repo | None = None
        self.due = 0.0
        self.poll(force=True)

    def where(self) -> Path:
        return self.fixed or probe.child_cwd(self.pid) or Path.cwd()

    def poll(self, force: bool = False) -> bool:
        """Read when asked, when due, when the program moved or the
        git directory changed; true when read."""
        at = self.where()
        now = clock.monotonic()
        moved = at != self.at
        quiet_ = self.watch is None or not self.watch.changed()
        if not (force or moved or now >= self.due) and quiet_:
            return False
        self.at, self.due = at, now + self.every
        self.repo = probe.read(at)
        if moved or self.watch is None:
            found = probe.locate(at) if self.repo is not None else None
            self.watch = None if found is None else probe.Watch(found[1], found[2])
        elif self.watch is not None:
            self.watch.changed()
        return True


def live(pilot: Pilot, args: argparse.Namespace, seconds: float | None) -> None:
    """The widgets on the session: a tag is shown while it has
    something to tell, drawn again when the repository is read."""
    look: Look = args.look
    hover = Hover()
    known = tags(look, hover.drawn)
    source = Source(pilot, args)
    deck = _widgets.Deck(pilot, args)
    names: list[str] = args.monitor
    shown: dict[str, Any] = {}
    sizes: dict[str, list[tuple[int, int]]] = {}
    border: dict[str, Any] = {}
    hidden: set[str] = set()
    turn, since = 0, clock.monotonic()

    def repo() -> probe.Repo | None:
        return source.repo

    def loud() -> list[str]:
        return [n for n in names if args.named or not quiet(n, repo())] or names[:1]

    def current() -> str:
        live_ = loud()
        return live_[turn % len(live_)]

    def draw(name: str) -> _widgets.Render:
        if name == MERGED:
            return lambda w, h: known[current()].render(repo(), w, h)
        return lambda w, h: known[name].render(repo(), w, h)

    def want(name: str) -> list[tuple[int, int]]:
        if name == MERGED:
            every = [known[n].sizes for n in names]
            return [
                (
                    max(s[min(r, len(s) - 1)][0] for s in every),
                    max(s[min(r, len(s) - 1)][1] for s in every),
                )
                for r in range(max(map(len, every)))
            ]
        return trimmed(known[name], repo())

    def edge() -> Any:
        return chase(args.border) if look.warn and warnings(repo(), look) else None

    def name_of(tag: str) -> str:
        return title(current() if tag == MERGED else tag, repo(), known)

    def put(tag: str) -> None:
        """Show `tag`, or show it again: other sizes, another border."""
        sizes[tag] = want(tag)
        border[tag] = edge() if tag in (MERGED, names[0]) else None
        extra = {"border": border[tag]} if border[tag] else {}
        if tag not in shown:
            shown[tag] = deck.add(
                tag, named(sizes[tag]), draw(tag), title=name_of(tag), **extra
            )
            return
        show_ = {**deck.shared, "title": name_of(tag), **extra}
        widget = shown[tag]
        widget.info = pilot.widget(
            tag, named(sizes[tag]), content=draw(tag), **show_
        ).info
        hidden.discard(tag)

    def sync() -> None:
        """After a read: the tags that come and go, their sizes."""
        if args.merged:
            if MERGED not in shown or border[MERGED] != edge():
                put(MERGED)
            shown[MERGED].set(title=name_of(MERGED))
            return
        for name in names:
            if not args.named and quiet(name, repo()):
                if name in shown and name not in hidden:
                    shown[name].hide()
                    shown[name].info = {"state": "gone"}
                    hidden.add(name)
                continue
            lead = name == names[0]
            if (
                name not in shown
                or name in hidden
                or want(name) != sizes[name]
                or (lead and border[name] != edge())
            ):
                put(name)
            elif name == "head":
                shown[name].set(title=name_of(name))

    sync()
    # The commit under the pointer is told in full; the deck's own
    # timeout is `run`'s: this loop keeps the time of the last move.
    deck.hover_timeout = None
    over: float | None = None
    if "log" in shown:
        deck.on_hover(shown["log"], hover.on_move, hover.on_leave)
    end = None if seconds is None else clock.monotonic() + seconds
    due = clock.monotonic() + 1.0
    while True:
        now = clock.monotonic()
        if end is not None and now >= end:
            return
        if now >= due:
            due = now + 1.0
            turned = args.merged and now - since >= args.cycle
            if turned:
                turn, since = turn + 1, now
            if source.poll() or turned:
                sync()
                deck.draw()
        if over is not None and now >= over:
            over = None
            deck.leave()
        wake = min(t for t in (due, end, over) if t is not None)
        event = pilot.event(timeout=max(0.0, wake - now))
        if event is None:
            continue
        if event[0] == "exit":
            return
        if event[0] == "hover":
            deck.hover(event[1])
            over = clock.monotonic() + HOVER
        elif event[0] == "move":
            deck.move(event[1]["frame"])
            for tag in list(shown):
                if tag not in hidden and border.get(tag):
                    put(tag)
        elif event[0] == "widgets":
            before = {id(w): w.variant for w, _ in deck.widgets}
            widget = deck.take(event[1])
            if widget is not None and widget.variant != before[id(widget)]:
                deck.draw(widget)


def _names(text: str) -> list[str]:
    names = [n.strip() for n in text.split(",") if n.strip()]
    bad = [n for n in names if n not in TAGS]
    if bad or not names:
        what = ",".join(bad) or repr(text)
        raise argparse.ArgumentTypeError(
            f"unknown tag {what}; choose from {', '.join(TAGS)}"
        )
    return list(dict.fromkeys(names))


def _hours(text: str) -> float:
    return _seconds(text) * 3600


def options(argv: Sequence[str] | None = None) -> argparse.Namespace:
    out = _cli.parser("git", "A repository, in widgets; read only.")
    for name, flag in FLAGS.items():
        out.add_argument(
            flag, f"--{name}", action="store_true", help=f"and the tag {name}"
        )
    out.add_argument(
        "-g", "--graph", action="store_true", help="the log as a graph, with the refs"
    )
    out.add_argument(
        "-W", "--warn", action="store_true", help="a chase on the border of a warning"
    )
    out.add_argument("-a", "--all", action="store_true", help="all of the above")
    out.add_argument(
        "--monitor",
        type=_names,
        default=None,
        metavar="LIST",
        help=f"the tags, comma-separated, of {','.join(TAGS)}; shown even "
        "with nothing to tell",
    )
    out.add_argument(
        "--merged", action="store_true", help="one widget, the tags in turn"
    )
    out.add_argument(
        "--repo", type=Path, metavar="PATH", help="a path in the repository"
    )
    out.add_argument(
        "--every",
        type=_seconds,
        default=5.0,
        metavar="SECONDS",
        help="read again, when nothing told of a change [default: 5]",
    )
    out.add_argument(
        "--cycle",
        type=_seconds,
        default=8.0,
        metavar="SECONDS",
        help="turn of --merged [default: 8]",
    )
    out.add_argument(
        "--dirty",
        type=_hours,
        default=4 * 3600.0,
        metavar="HOURS",
        help="-W: a tree changed for that long is a warning [default: 4]",
    )
    _widgets.options(out, border="round")
    out.add_argument(
        "--print",
        nargs="?",
        type=_size,
        const=(),
        default=None,
        metavar="WxH",
        help="no driver: print the variants once, or the size given",
    )
    args = _cli.parse(out, argv)
    args.named = args.monitor is not None
    if not args.named:
        args.monitor = ["head"] + [
            n
            for n in FLAGS
            if args.all or getattr(args, n) or (n == "log" and args.graph)
        ]
    args.look = Look(
        graph=args.all or args.graph, warn=args.all or args.warn, dirty=args.dirty
    )
    return args


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    args = options(argv)
    if args.print is not None:
        return show(args)
    return _cli.run(lambda pilot, args: live(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
