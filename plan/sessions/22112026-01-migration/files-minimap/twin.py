# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's faces as fixtures of minimap/src/twin.json: for each doc,
look, time and size, the FNV-1a 64 hash of `face` (each run
`t US fg US bg RS`, rows by LF); the rows whole at 12x16; `scale`,
`line_at`, `title`, `heights`. Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/files-minimap/twin.py > ../../store-apps/minimap/src/twin.json"""

import json
import re
from pathlib import Path

from ptyrc_apps import minimap
from ptyrc_apps.minimap import Doc, Look

SOURCE = Path(minimap.__file__).read_text().splitlines()[:400]


def code() -> Doc:
    lines = [line.expandtabs(4) for line in SOURCE]
    top = len(lines) // 3
    doc = Doc(0, lines, top, 40, max(map(len, lines)))
    doc.cursor = top + 20
    doc.mark([re.compile("def "), re.compile("return")])
    doc.edited = dict.fromkeys(range(12), 0.0)
    doc.visited = dict.fromkeys(range(top), 30.0)
    return doc


def long() -> Doc:
    lines = [("    " * (i % 5)) + "word " * (i % 13) for i in range(2500)]
    lines[2321] = "  ERROR here"
    doc = Doc(-200, lines, 1500, 50, 160, cursor=1520)
    doc.mark([re.compile("ERROR")])
    doc.edited = {y: 50.0 + (y % 7) for y in range(1490, 1600)}
    doc.visited = {y: 10.0 * (y % 9) for y in range(0, 2400, 7)}
    return doc


def shifted() -> Doc:
    lines = ["x" * (i % 30) if i % 4 else "" for i in range(90)]
    lines[10] = "// a note"
    lines[11] = "# another"
    lines[12] = "-- sql"
    return Doc(100, lines, 170, 24, 80, cursor=180, edited={175: 95.0}, visited={120: 40.0})


def heat() -> Doc:
    doc = Doc(lines=["x"] * 32, top=28, view=4, width=4)
    doc.edited = {0: 100.0, 4: 70.0}
    doc.visited = {4: 100.0, 8: 60.0, 12: 30.0}
    return doc


DOCS = {
    "code": code,
    "long": long,
    "shifted": shifted,
    "heat": heat,
    "small": lambda: Doc(lines=["xx", "x", " x", "", "x", "été 中"], width=6, top=1, view=2),
    "empty": lambda: Doc(),
    "wide": lambda: Doc(lines=["ab " * 120] * 7 + ["  /* c */"] * 3, width=360, view=3),
}
LOOKS = {
    "plain": Look(),
    "all": Look(True, True, True),
    "notes": Look(notes=True),
    "cursor": Look(cursor=True),
    "heat": Look(heat=True, fade=60.0),
    "quick": Look(heat=True, fade=7.5),
}
NOWS = (0.0, 30.0, 100.0)
SIZES = [(12, h) for h in minimap.HEIGHTS] + [(2, 1), (6, 8), (40, 48), (3, 5), (17, 13)]


def fnv(text: str) -> str:
    h = 0xCBF29CE484222325
    for b in text.encode():
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def canon(rows: minimap.Rows) -> str:
    return "\n".join(
        "".join(f"{r['t']}\x1f{r['fg']}\x1f{r.get('bg', '')}\x1e" for r in row) for row in rows
    )


def wire(doc: Doc) -> dict:
    return {
        "y0": doc.y0,
        "lines": doc.lines,
        "top": doc.top,
        "view": doc.view,
        "width": doc.width,
        "cursor": doc.cursor,
        "marks": sorted(doc.marks.items()),
        "edited": sorted(doc.edited.items()),
        "visited": sorted(doc.visited.items()),
    }


def main() -> None:
    docs = {}
    faces = []
    for name, make in DOCS.items():
        doc = make()
        docs[name] = {
            "doc": wire(doc),
            "title": minimap.title(doc),
            "heights": minimap.heights(len(doc.lines)),
            "scale": [list(minimap.scale(len(doc.lines), doc.width, w, h)) for w, h in SIZES],
            "line_at": [[minimap.line_at(doc, w, h, y) for y in range(h)] for w, h in SIZES],
        }
        for look_name, look in LOOKS.items():
            for now in NOWS:
                for w, h in SIZES:
                    rows = minimap.face(doc, w, h, look, now)
                    entry = {"doc": name, "look": look_name, "now": now, "size": [w, h], "hash": fnv(canon(rows))}
                    if (w, h) == (12, 16) and now == 30.0:
                        entry["rows"] = [[[r["t"], r["fg"], r.get("bg")] for r in row] for row in rows]
                    faces.append(entry)
    looks = {k: [v.cursor, v.heat, v.notes, v.fade] for k, v in LOOKS.items()}
    sizes = [list(s) for s in SIZES]
    print(json.dumps({"sizes": sizes, "looks": looks, "docs": docs, "faces": faces}, ensure_ascii=False))


main()
