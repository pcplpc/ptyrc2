# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's looks as fixtures of git/twin.js: repository states (those
of test_git.py) as `probe.git` answers, and for each tag, state, look,
size and focus the FNV-1a hash of the rows normalised (runs of one style
merged, colours as numbers); `trimmed`, `warnings`, `quiet` whole. Also
writes the states as `--probe` fixtures of the cases (git/tests/*.json).
Run, in plugins/client-apps/python:
  TZ=UTC uv run python ../../../plan/sessions/22112026-01-migration/probes-git/twin.py \
    ../../store-apps/git"""

import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path("tests").resolve()))
from ptyrc_apps import _gitprobe as probe  # noqa: E402
from ptyrc_apps import git  # noqa: E402
from test_git import NOW, repo  # noqa: E402

time.tzset()
COLOURS = {
    "bright-cyan": 14,
    "green": 2,
    "yellow": 3,
    "bright-magenta": 13,
    "bright-black": 8,
    "red": 1,
    "cyan": 6,
    "magenta": 5,
    "blue": 4,
    "bright-yellow": 11,
}
KINDS = ("staged", "unstaged", "untracked", "conflict")


def lines():
    r = repo()
    counts = {"src/staged.py": (10, 2), "both.py": (1, 1)}
    return tuple(
        probe.Change(c.kind, c.code, c.path, *counts.get(c.path, (None, None)))
        for c in r.changes
    )


def current_trees():
    return tuple(
        probe.Worktree(w.path, w.oid, w.branch, w.flags, w.path == "/src/app")
        for w in repo().worktrees
    )


STATES = {
    "base": repo(),
    "lines": repo(changes=lines(), worktrees=current_trees()),
    "busy": repo(state="rebase 3/7", state_since=NOW - 7200, dirty_since=NOW - 5 * 3600),
    "merge": repo(state="merge", state_since=NOW - 7200),
    "lost": repo(head=probe.Head("abcdef1"), changes=()),
    "long": repo(head=probe.Head("abcdef1", "feature/" + "long-" * 12), changes=()),
    "many": repo(counts={"untracked": 1000}),
    "gone": repo(changes=(), head=probe.Head("a", "x", "o/x", gone=True)),
    "empty": repo(
        head=probe.Head(initial=True, branch="trunk"),
        changes=(),
        commits=(),
        stashes=(),
        branches=(),
        worktrees=(probe.Worktree("/src/app", current=True),),
        days=(0,) * probe.DAYS,
    ),
    "none": None,
}


def answer(r):
    if r is None:
        return {"repo": None, "stamp": ""}
    return {
        "repo": {
            "name": r.name,
            "now": r.now,
            "head": {
                "oid": r.head.oid,
                "branch": r.head.branch,
                "upstream": r.head.upstream,
                "ahead": r.head.ahead,
                "behind": r.head.behind,
                "gone": r.head.gone,
                "initial": r.head.initial,
            },
            "changes": [
                {
                    "kind": c.kind,
                    "code": c.code,
                    "path": c.path,
                    "added": c.added,
                    "deleted": c.deleted,
                }
                for c in r.changes
            ],
            "counts": {k: r.count(k) for k in KINDS},
            "state": r.state,
            "state_since": r.state_since,
            "dirty_since": r.dirty_since,
            "log": [
                {
                    "oid": c.oid,
                    "at": c.at,
                    "author": c.author,
                    "subject": c.subject,
                    "refs": list(c.refs),
                    "graph": list(c.graph),
                }
                for c in r.commits
            ],
            "days": list(r.days),
            "stashes": [{"n": s.n, "at": s.at, "subject": s.subject} for s in r.stashes],
            "branches": [
                {
                    "name": b.name,
                    "current": b.current,
                    "at": b.at,
                    "upstream": b.upstream,
                    "ahead": b.ahead,
                    "behind": b.behind,
                    "gone": b.gone,
                    "subject": b.subject,
                }
                for b in r.branches
            ],
            "worktrees": [
                {
                    "name": Path(w.path).name,
                    "oid": w.oid,
                    "branch": w.branch,
                    "flags": list(w.flags),
                    "current": w.current,
                }
                for w in r.worktrees
            ],
        },
        "stamp": "s0",
    }


def norm(rows):
    out = []
    for row in rows:
        runs = []
        for r in row:
            t = str(r["t"])
            if not t:
                continue
            fg = r.get("fg")
            style = [COLOURS.get(fg, fg), bool(r.get("bold"))]
            if runs and runs[-1][1:] == style:
                runs[-1][0] += t
            else:
                runs.append([t, *style])
        out.append(runs)
    return json.dumps(out, ensure_ascii=False, separators=(",", ":"))


def fnv(text):
    h = 0x811C9DC5
    for c in text:
        h = ((h ^ ord(c)) * 0x01000193) & 0xFFFFFFFF
    return h


LOOKS = [
    git.Look(),
    git.Look(graph=True),
    git.Look(warn=True),
    git.Look(graph=True, warn=True),
    git.Look(warn=True, dirty=60.0),
]
EXTRA = [(1, 1), (3, 2), (7, 3), (12, 6), (20, 2), (40, 8), (64, 2), (80, 20), (200, 50)]

faces, trims, warns, whole = [], [], [], []
for si, (sn, r) in enumerate(STATES.items()):
    for li, look in enumerate(LOOKS):
        warns.append([si, li, git.warnings(r, look)])
        known = git.tags(look)
        for ti, name in enumerate(git.TAGS):
            tag = known[name]
            trims.append([si, li, ti, [list(s) for s in git.trimmed(tag, r)]])
            for w, h in [*tag.sizes, *EXTRA]:
                focuses = [None, 0, 1, 2, 9] if name == "log" else [None]
                for fi, focus in enumerate(focuses):
                    if name == "log":
                        rows = git.render_log(r, w, h, look, focus)
                    else:
                        rows = tag.render(r, w, h)
                    faces.append([si, li, ti, w, h, fi, fnv(norm(rows))])
                    if li in (0, 3) and sn in ("base", "lines") and (w, h) in tag.sizes:
                        whole.append([si, li, ti, w, h, fi, json.loads(norm(rows))])
quiet = [[git.quiet(n, r) for n in git.TAGS] for r in STATES.values()]

out = Path(sys.argv[1])
for sn, r in STATES.items():
    (out / "tests" / f"{sn}.json").write_text(json.dumps(answer(r), ensure_ascii=False, indent=1) + "\n")
dump = lambda v: json.dumps(v, ensure_ascii=False, separators=(",", ":"))  # noqa: E731
(out / "twin.js").write_text(
    "// SPDX-License-Identifier: GPL-3.0-or-later\n"
    "// The twin's looks (plan/sessions/22112026-01-migration/probes-git/twin.py), TZ=UTC:\n"
    "// STATES the probe.git answers; LOOKS; FACES [state, look, tag, w, h, focus, hash],\n"
    "// focus of FOCUS (log alone); TRIMS [state, look, tag, sizes]; WARNS [state, look, list];\n"
    "// QUIET [state][tag]; WHOLE [state, look, tag, w, h, focus, runs].\n"
    f"export const STATES = {dump({k: answer(v) for k, v in STATES.items()})};\n"
    f"export const LOOKS = {dump([l.__dict__ for l in LOOKS])};\n"
    f"export const FOCUS = {dump([None, 0, 1, 2, 9])};\n"
    f"export const FACES = {dump(faces)};\n"
    f"export const TRIMS = {dump(trims)};\n"
    f"export const WARNS = {dump(warns)};\n"
    f"export const QUIET = {dump(quiet)};\n"
    f"export const WHOLE = {dump(whole)};\n"
)
print(len(faces), "faces", file=sys.stderr)
