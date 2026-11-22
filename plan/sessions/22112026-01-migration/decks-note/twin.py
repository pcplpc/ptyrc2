# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's looks as fixtures of note/twin.js: for each source and
look, `trimmed` (with and without the foot) and the FNV-1a hash of each
render at each view; for each scenario (a source, a look, keys and
pastes), at each step the change `Desk.key` answered, the text after it
(made at once, as the app does) and the hash of the focused render at
each view. Renders normalised: runs of one style merged, empty runs out,
`[t, fg, bg, bold, inverse]`, colours as numbers. Run in
plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/decks-note/twin.py > ../../store-apps/note/twin.js"""

import json

from ptyrc_apps import note

COLOURS = {"bright-black": 8, "green": 2}
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
SMALL = "# List\n\n- [x] bread\n* [ ] milk, the **whole** one\n  1. [ ] filters\nplain\n"
SOURCES = {
    "text": TEXT,
    "small": SMALL,
    "empty": "",
    "words": "word " * 80,
    "done": "- [x] a\n",
    "long": "x" * 300,
    "box": "- [ ] " + "y" * 90,
    "deep": "- [ ] a\n  - [ ] b\n    - [x] c\n      - [ ] d\n\tx\ttab\n## \n#\n",
    "crlf": "- [ ] one\r\n- [x] two\r\n\r\n\r\n# Head #\r\n",
    "half": "- [x] a\n- [ ] b\n- [ ] c\n- [ ] d\n- [ ] e\n- [ ] f\n- [ ] g\n- [ ] h\n",
}
LOOKS = {
    "plain": note.Look(),
    "all": note.Look(count=True, done=True, postit=True),
    "open": note.Look(open=True),
    "count": note.Look(count=True),
    "every": note.Look(True, True, True, True),
}
VIEWS = [*note.SIZES, (1, 1), (3, 2), (7, 3), (10, 2), (12, 2), (20, 6), (4, 3), (60, 20), (120, 40)]
SCENARIOS = [
    ("small", "plain", ["down", "space", "j", "enter", "end", "space", "shift+up", "K", "home", "shift+up", "J"]),
    ("small", "plain", ["a", "j", "a", "space", "m", "enter", "a", "enter", "a", "x", "ctrl+c", "ctrl+u", "enter"]),
    ("small", "plain", ["e", "ctrl+u", "enter", "e", "enter", "e", "ctrl+u", "o", "k", "enter"]),
    ("small", "every", ["e", ("paste", "more\nof it"), "enter", "d", "n", "d", "y"]),
    ("empty", "plain", ["space", "e", "d", "y", "J", "a", "x", "enter", "a", "y", "enter"]),
    ("text", "open", ["down", "down", "space", "e", "home", "Z", "enter", "end", "a", "last", "enter", "K"]),
    ("text", "all", ["pagedown", "pagedown", "pageup", "tab", "shift+down", "shift+down", "d", "y", "a", "under", "enter"]),
    ("crlf", "plain", ["space", "j", "a", "new", "enter", "J", "end", "e", "ctrl+w", "Head 2", "enter"]),
    ("deep", "plain", ["j", "j", "a", "e", "enter", "j", "j", "j", "a", "after", "enter", "j", "j", "e", "ctrl+u", "h", "enter"]),
    ("done", "plain", ["a", "b", "enter", "space", "J", "d", "y", "d", "y", "a", "z", "enter"]),
    ("box", "count", ["space", "space", "e", "end", "backspace", "enter"]),
]


def norm(rows):
    out = []
    for row in rows:
        runs = []
        for r in row:
            t = str(r["t"])
            if not t:
                continue
            fg = r.get("fg")
            style = [COLOURS.get(fg, fg), r.get("bg"), bool(r.get("bold")), bool(r.get("inverse"))]
            if runs and runs[-1][1:] == style:
                runs[-1][0] += t
            else:
                runs.append([t, *style])
        out.append(runs)
    return out


def fnv(text):
    h = 0x811C9DC5
    units = text.encode("utf-16-le")
    for i in range(0, len(units), 2):
        h = ((h ^ int.from_bytes(units[i : i + 2], "little")) * 0x01000193) & 0xFFFFFFFF
    return h


def digest(rows):
    return fnv(json.dumps(norm(rows), ensure_ascii=False, separators=(",", ":")))


def made(source):
    return note.Note("TODO.md", tuple(note.parse(source)))


def faces():
    out = []
    for sname, source in [*SOURCES.items(), ("missing", None)]:
        n = made(source) if source is not None else note.Note("gone.md", missing=True)
        for lname, look in LOOKS.items():
            out.append(
                {
                    "source": sname,
                    "look": lname,
                    "title": note.title(n, look),
                    "trimmed": [list(s) for s in note.trimmed(n, look)],
                    "foot": [list(s) for s in note.trimmed(n, look, True)],
                    "views": [digest(note.render(n, w, h, look)) for w, h in VIEWS],
                }
            )
    return out


def scenarios():
    out = []
    for sname, lname, keys in SCENARIOS:
        source, look, desk = SOURCES[sname], LOOKS[lname], note.Desk()
        steps = []
        for key in keys:
            n = made(source)
            if isinstance(key, tuple):
                desk.paste(key[1])
                what = None
            else:
                what = desk.key(note.listed(n, look), key)
            if what is not None:
                source = note.change(source, what)
                n = made(source)
            views = [digest(note.render(n, w, h, look, desk)) for w, h in VIEWS]
            steps.append([list(key) if isinstance(key, tuple) else key, list(what) if what else None, source, desk.sel, desk.mode, views])
        out.append({"source": sname, "look": lname, "steps": steps})
    return out


def main():
    print("// SPDX-License-Identifier: GPL-3.0-or-later")
    print("// The twin's looks (plan/sessions/22112026-01-migration/decks-note/twin.py):")
    print("// FACES a note and a look; SCENARIOS: [key or [\"paste\", text], change, text, sel, mode, hashes].")
    print(f"export const SOURCES = {json.dumps(SOURCES, ensure_ascii=False)};")
    print(f"export const VIEWS = {json.dumps([list(v) for v in VIEWS])};")
    print(f"export const FACES = {json.dumps(faces(), ensure_ascii=False)};")
    print(f"export const SCENARIOS = {json.dumps(scenarios(), ensure_ascii=False)};")


main()
