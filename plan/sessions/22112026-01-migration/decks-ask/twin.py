# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's looks as fixtures of ask/twin.js: for each scenario (the
arguments, then keys and pastes), at each step the flow `key` answered
and the FNV-1a hash of each render (the views: the model's sizes, then
VIEWS; focused, then not; in this order, as render moves the bars),
normalised (runs of one style merged, empty runs out); the result; the
first render whole. Run in plugins/client-apps/python:
uv run python ../../../plan/sessions/22112026-01-migration/decks-ask/twin.py > ../../store-apps/ask/twin.js"""
import json

from ptyrc_apps import ask

VIEWS = [(10, 1), (20, 2), (30, 3), (16, 5), (40, 8)]
LONG = [f"option {i}" for i in range(1, 21)]
SCENARIOS = [
    (["Deploy?", "yes", "no", "later"], ["down", "enter"]),
    (["Deploy?", "yes", "no", "--default", "no"], ["enter"]),
    (["Pick", "a", "b", "c"], ["3", "k", "j", "tab", "tab", "9", "0", "x", "enter"]),
    (["Which of these?", *LONG], ["pagedown", "pagedown", "end", "up", "pageup", "home", "down", "1", "5", "enter"]),
    (["Many", *LONG[:12]], ["end", "9", "up", "enter"]),
    (["-m", "Which?", "a", "b", "c"], ["space", "down", "down", "space", "up", "space", "space", "enter"]),
    (["-m", "Which?", "a", "b", "--default", "b"], ["enter"]),
    (["-m", "Which?", "a", "b"], ["enter"]),
    (["-o", "Which?", "a", "b"], ["end", "enter", "x", "y", "1", "j", "left", "home", "Z", "end", "backspace", "up", "down", "enter"]),
    (["-o", "Which?", "a", "b"], ["3", "space", "w", "home", "enter"]),
    (["-m", "-o", "Some?", "a", "b", "c"], ["space", "end", "n", "e", "w", "enter"]),
    (["-m", "-o", "Some?", "a", "b"], ["3", "z", "up", "up", "space", "enter"]),
    (["-o", "Paste?", "a"], ["down", ("paste", "pasted\n"), "enter"]),
    (["-t", "Q", "a"], ["enter"]),
    (["A question long enough to wrap on two lines or three, and more", "ok", "an option rather longer than the question is"], ["down", "enter"]),
    (["Name?"], ["a", "b", "space", "c", "enter"]),
    (["Name?", "--default", "you"], ["left", "left", "X", "ctrl+e", "backspace", "home", "delete", "ctrl+k", "enter"]),
    (["Name?", "--default", "one two three"], ["ctrl+w", "ctrl+w", "left", "ctrl+u", "ctrl+f", "ctrl+b", "ctrl+a", "ctrl+d", "ctrl+h", "enter"]),
    (["Name?", "--default", "an"], ["n", "é", ("paste", "x\ny\n"), "enter"]),
    (["Name?", "--default", "a rather long line of text that scrolls when it is typed on"], ["home", "right", "right", "end", "left", "enter"]),
    (["Wide?", "--default", "日本語のテキストです"], ["left", "left", "left", "x", "enter"]),
    (["名前は？"], ["名", "enter"]),
    (["-t", "Why?"], ["a", "alt+enter", "b", "ctrl+j", "c", "up", "up", "end", "down", "enter"]),
    (["-t", "Why?", "--default", "one\ntwo\nthree"], ["up", "home", "backspace", "end", "delete", "left", "right", "right", "down", "down", "enter"]),
    (["-t", "Long?", "--default", "1\n2\n3\n4\n5\n6\n7"], ["up", "up", "up", "up", "up", "up", "down", ("paste", "p\nq"), "enter"]),
    (["-t", "Edit?", "--default", "ab"], ["home", "alt+enter", "ctrl+d", "up", "delete", "enter"]),
    (["-t", "Empty?"], ["backspace", "up", "down", "left", "right", "enter"]),
]


def norm(rows):
    out = []
    for row in rows:
        runs = []
        for r in row:
            t = str(r["t"])
            if not t:
                continue
            style = [r.get("fg"), bool(r.get("bold")), bool(r.get("dim")), bool(r.get("inverse"))]
            if runs and runs[-1][1:] == style:
                runs[-1][0] += t
            else:
                runs.append([t, *style])
        out.append(runs)
    return json.dumps(out, ensure_ascii=False, separators=(",", ":"))


def fnv(text):
    """Over the UTF-16 code units, as JS strings are."""
    h = 0x811C9DC5
    data = text.encode("utf-16-le")
    for i in range(0, len(data), 2):
        h = ((h ^ (data[i] | data[i + 1] << 8)) * 0x01000193) & 0xFFFFFFFF
    return h


def looks(m, views):
    return [fnv(norm(m.render(w, h, f))) for (w, h) in views for f in (True, False)]


cases = []
for argv, steps in SCENARIOS:
    args = ask.options(argv)
    m = ask.model(args)
    o = {"question": args.question, "choices": args.options, "multi": args.multi,
         "other": args.other, "text": args.text, "default": args.default or ""}
    sizes = [list(s) for s in m.sizes()]
    views = [tuple(s) for s in sizes] + VIEWS
    whole = json.loads(norm(m.render(*sizes[0], False)))
    seen = [[None, None, looks(m, views)]]
    for step in steps:
        if isinstance(step, tuple):
            m.paste(step[1])
            flow = None
            step = list(step)
        else:
            flow = m.key(step)
        seen.append([step, flow, looks(m, views)])
    cases.append({"argv": argv, "o": o, "sizes": sizes, "whole": whole, "steps": seen, "result": m.result})

print("// SPDX-License-Identifier: GPL-3.0-or-later")
print("// The twin's looks (plan/sessions/22112026-01-migration/decks-ask/twin.py):")
print("// a case a scenario; `steps`: [key or [\"paste\", text], flow, hashes].")
print("export const VIEWS = " + json.dumps(VIEWS) + ";")
print("export const CASES = [")
for c in cases:
    print("  " + json.dumps(c, ensure_ascii=False) + ",")
print("];")
