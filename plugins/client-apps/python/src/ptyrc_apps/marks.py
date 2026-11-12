# SPDX-License-Identifier: GPL-3.0-or-later
"""Marks: a highlighter for whatever runs in the terminal.

    ptyrc run less server.log        # or vim, a shell, tmux
    ptyrc-app marks ERROR WARN
    ptyrc-app marks -e 'TODO|FIXME' '\\b\\d+ ms\\b' -i

What it shows:

- `blend="tint"`: a sprite that keeps the app's glyphs and gives them
  its colours. One per match, one colour per pattern.
- Frame coordinates and `find(known=True)`: a mark is put on a line of
  the pane, in view or remembered. Scroll, and the driver moves the
  marks with the text, at once; nothing waits for the client.
- Topics `screen` and `frames`: the app looks again when the text or
  the layout changed, and only then. All the searches of a look go in
  one batch, all its changes in another: one repaint.
- Topic `draw`: the driver says which sprites went with the rows it
  forgot; the app forgets them too.
- `auto=`, the front layer: a small display of the counts, in view and
  known, placed by the driver where the app shows nothing.
"""

from __future__ import annotations

import argparse
import time
from collections.abc import Sequence
from typing import Any

from ptyrc import Match, Pilot, PtyrcError, Sprite

from . import _cli

__all__ = ["INKS", "MOST", "Marks", "hud", "main", "wanted"]

# Marker pens: background, foreground.
INKS = ((226, 16), (120, 16), (81, 16), (213, 16), (215, 16), (203, 16))
MOST = 400  # marks at once, at most
SETTLE = 0.03  # seconds of silence that end a burst of changes
BURST = 0.2  # seconds a burst may last before a look
PANEL = 236

Key = tuple[int, int, int, int, str]  # frame, x, y, pattern, text


def wanted(
    found: Sequence[tuple[int, int, Sequence[Match]]],
    views: dict[int, tuple[int, int]],
    most: int = MOST,
) -> tuple[list[Key], list[list[int]]]:
    """What to mark, and what to count.

    `found`: `(frame, pattern, matches)` of each search; `views`: first
    row and height of each frame's view. Returns the marks, those
    nearest to a view first, `most` at most, and per pattern `[in
    view, known]`.
    """
    counts = [[0, 0] for _ in range(1 + max((p for _, p, _ in found), default=-1))]
    ranked = []
    for frame, pattern, matches in found:
        top, height = views[frame]
        for m in matches:
            if not m.len:
                continue
            counts[pattern][1] += 1
            counts[pattern][0] += m.screen is not None
            off = max(top - m.y, m.y - (top + height - 1), 0)
            ranked.append((off, (frame, m.x, m.y, pattern, m.text)))
    ranked.sort(key=lambda r: r[0])
    return [key for _, key in ranked[:most]], counts


def hud(patterns: Sequence[str], counts: Sequence[Sequence[int]]) -> list[list[Any]]:
    """The display: a row per pattern, in its ink, with its count in
    view, and `/known` when there is more."""
    names = [p if len(p) <= 16 else p[:15] + "…" for p in patterns]
    width = max(map(len, names))
    notes = [
        str(seen) if seen == known else f"{seen}/{known}" for seen, known in counts
    ]
    room = max(map(len, notes))
    rows = []
    for i, (name, note) in enumerate(zip(names, notes, strict=True)):
        bg, fg = INKS[i % len(INKS)]
        rows.append(
            [
                {"t": " ", "bg": PANEL},
                {"t": f" {name:{width}} ", "bg": bg, "fg": fg},
                {"t": f" {note:>{room}} ", "bg": PANEL, "fg": bg, "bold": True},
            ]
        )
    return rows


class Marks:
    """The marks on the driver, kept equal to the matches."""

    def __init__(self, pilot: Pilot, args: argparse.Namespace) -> None:
        self.pilot = pilot
        self.patterns: list[str] = args.patterns
        self.query = {"regex": args.regex, "ignore_case": args.ignore_case}
        self.frame = args.frame
        self.sprites: dict[Key, Sprite] = {}
        self.hud: Sprite | None = Sprite(pilot, None) if args.hud else None
        self.counts: list[list[int]] | None = None

    def frames(self) -> dict[int, tuple[int, int]]:
        """The frames to mark, with the first row and the height of
        their views: the pane asked for, else every pane; the raw
        frame where there is none (a plain shell)."""
        if self.frame != "all":
            frames = [_cli.pick(self.pilot, self.frame)]
        else:
            frames = self.pilot.panes() or [self.pilot.frame("raw")]
        return {f.id: (f.origin[1], f.size[1]) for f in frames if f.usable}

    def search(self, frames: Sequence[int]) -> list[tuple[int, int, list[Match]]]:
        """Every pattern on every frame, in one request."""
        asked = [(f, p) for f in frames for p in range(len(self.patterns))]
        with self.pilot.batch() as batch:
            for frame, p in asked:
                self.pilot.call(
                    "screen.find",
                    pattern=self.patterns[p],
                    frame=frame,
                    known=True,
                    **self.query,
                )
        out = []
        for (frame, p), result in zip(asked, batch.results, strict=True):
            if isinstance(result, PtyrcError):
                if result.code in ("no-frame", "not-usable"):
                    continue  # the layout changed meanwhile: next look
                raise result
            out.append((frame, p, [Match.from_wire(m) for m in result["matches"]]))
        return out

    def look(self) -> None:
        """Search, then add and remove what differs."""
        views = self.frames()
        marks, counts = wanted(self.search(list(views)), views)
        want = set(marks)
        with self.pilot.batch():
            for key in [k for k in self.sprites if k not in want]:
                self.sprites.pop(key).remove()
            for key in marks:
                if key in self.sprites:
                    continue
                frame, x, y, p, text = key
                bg, fg = INKS[p % len(INKS)]
                self.sprites[key] = self.pilot.draw(
                    # The glyphs shown are the app's: these only say where.
                    ["".join("·" if c.isspace() else c for c in text)],
                    frame=frame,
                    at=(x, y),
                    space="frame",
                    blend="tint",
                    bg=bg,
                    fg=fg,
                )
            counts += [[0, 0]] * (len(self.patterns) - len(counts))
            if self.hud is not None and counts != self.counts:
                content = {"cells": hud(self.patterns, counts), "opaque": True}
                if self.hud.id is None:
                    self.hud = self.pilot.draw(
                        auto="top-right", layer="front", z=9, **content
                    )
                else:
                    self.hud.set(**content)
            self.counts = counts

    def forget(self, ids: Sequence[int]) -> None:
        """Sprites the driver removed with the rows they were on."""
        gone = set(ids)
        self.sprites = {k: s for k, s in self.sprites.items() if s.id not in gone}


def app(pilot: Pilot, args: argparse.Namespace, seconds: float | None = None) -> None:
    marks = Marks(pilot, args)
    pilot.subscribe("screen", "frames", "draw")
    end = None if seconds is None else time.monotonic() + seconds
    marks.look()
    while pilot.connected:
        left = None if end is None else end - time.monotonic()
        if left is not None and left <= 0:
            return
        # Sleep until something changes; then let the burst end.
        dirty = False
        for topic, data in pilot.events(timeout=left):
            if topic == "draw":
                marks.forget(data["removed"])
            dirty = True
            break
        if not dirty:
            continue
        burst = time.monotonic() + BURST
        for topic, data in pilot.events(timeout=SETTLE):
            if topic == "draw":
                marks.forget(data["removed"])
            if time.monotonic() > burst:
                break
        marks.look()


def main(argv: Sequence[str] | None = None, *, seconds: float | None = None) -> int:
    """`seconds`: stop after this long (tests); else until interrupted."""
    out = _cli.parser("marks", "Highlight the matches of patterns.", frame="all")
    out.add_argument("patterns", nargs="+", metavar="PATTERN", help="text to mark")
    out.add_argument(
        "-e", "--regex", action="store_true", help="patterns are regular expressions"
    )
    out.add_argument("-i", "--ignore-case", action="store_true")
    out.add_argument(
        "--no-hud", dest="hud", action="store_false", help="no display of the counts"
    )
    # Intermixed: `marks ERROR -i WARN` reads both patterns.
    args = out.parse_intermixed_args(argv)
    args.frame = _cli.frame_arg(args.frame)
    return _cli.run(lambda pilot, args: app(pilot, args, seconds), args)


if __name__ == "__main__":
    raise SystemExit(main())
