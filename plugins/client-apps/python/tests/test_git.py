# SPDX-License-Identifier: GPL-3.0-or-later
"""`git`: the parsers on fixtures, every variant in its box, a live
repository; nothing is ever written to it."""

from __future__ import annotations

import hashlib
import os
import subprocess
import threading
import time
import unicodedata
from pathlib import Path

import pytest
from ptyrc import Pilot, clock
from ptyrc_apps import _gitprobe as probe
from ptyrc_apps import _widgets, git

NOW = 1_800_000_000.0
S = probe.SEP

STATUS = "\0".join(  # noqa: FLY002
    [
        "# branch.oid 1234567890abcdef1234567890abcdef12345678",
        "# branch.head main",
        "# branch.upstream origin/main",
        "# branch.ab +2 -1",
        "1 M. N... 100644 100644 100644 aaa bbb src/staged.py",
        "1 .M N... 100644 100644 100644 aaa bbb src/changed file.py",
        "1 MM N... 100644 100644 100644 aaa bbb both.py",
        "2 R. N... 100644 100644 100644 aaa bbb R100 new name.py",
        "old name.py",
        "u UU N... 100644 100644 100644 100644 aaa bbb ccc clash.py",
        "? notes/",
        "",
    ]
)
LOG = f"""* {S}aaaaaaa{S}{NOW - 120:.0f}{S}Ada{S}HEAD -> main, tag: v1.0, origin/main{S}Merge side
|\\  
| * {S}bbbbbbb{S}{NOW - 3600:.0f}{S}Bob{S}side{S}a side commit with a subject long enough to wrap twice
|/  
* {S}ccccccc{S}{NOW - 90000:.0f}{S}Ada{S}{S}base
"""
BRANCHES = f"""main{S}*{S}{NOW - 120:.0f}{S}origin/main{S}[ahead 2, behind 1]{S}Merge side
side{S} {S}{NOW - 3600:.0f}{S}{S}{S}a side commit
old{S} {S}{NOW - 900000:.0f}{S}origin/old{S}[gone]{S}forgotten
"""
WORKTREES = """worktree /src/app
HEAD 1234567890abcdef
branch refs/heads/main

worktree /src/app-fix
HEAD abcdef1234567890
detached
locked

worktree /src/gone
HEAD abcdef1234567890
branch refs/heads/side
prunable gitdir file points to non-existent location
"""


def repo(**over: object) -> probe.Repo:
    head, changes = probe.parse_status(STATUS)
    days = [0] * (probe.DAYS - 10) + [3, 0, 1, 7, 0, 0, 2, 12, 1, 4]
    fields: dict[str, object] = {
        "root": Path("/src/app"),
        "now": NOW,
        "head": head,
        "changes": tuple(changes),
        "commits": tuple(probe.parse_log(LOG)),
        "stashes": (
            probe.Stash(0, NOW - 300, "WIP on main: aaaaaaa Merge side"),
            probe.Stash(1, NOW - 86400, "On side: tried another way"),
        ),
        "branches": tuple(probe.parse_branches(BRANCHES)),
        "worktrees": tuple(probe.parse_worktrees(WORKTREES)),
        "days": tuple(days),
    }
    return probe.Repo(**(fields | over))  # type: ignore[arg-type]


class Skip:
    """The session clock, for the app alone: `end` jumps past the
    `seconds` it was given."""

    def __init__(self) -> None:
        self.by = 0.0

    def monotonic(self) -> float:
        return clock.monotonic() + self.by

    def end(self) -> None:
        self.by = 1e6


def text(rows: list[list[dict[str, object]]]) -> list[str]:
    return ["".join(str(run["t"]) for run in row) for row in rows]


# Parsers


def test_parse_status() -> None:
    head, changes = probe.parse_status(STATUS)
    assert head == probe.Head("1234567", "main", "origin/main", 2, 1)
    assert [(c.kind, c.code, c.path) for c in changes] == [
        ("staged", "M", "src/staged.py"),
        ("unstaged", "M", "src/changed file.py"),
        ("staged", "M", "both.py"),
        ("unstaged", "M", "both.py"),
        ("staged", "R", "new name.py"),
        ("conflict", "U", "clash.py"),
        ("untracked", "?", "notes/"),
    ]
    head, changes = probe.parse_status(
        "# branch.oid (initial)\0# branch.head (detached)\0"
    )
    assert head == probe.Head(initial=True) and changes == []
    gone, _ = probe.parse_status("# branch.oid abc\0# branch.upstream o/x\0")
    assert gone.gone and gone.upstream == "o/x"
    assert probe.parse_status("") == (probe.Head(), [])


def test_parse_numstat() -> None:
    assert probe.parse_numstat("3\t1\ta.py\n-\t-\timg.png\n\n12\t0\tb c.py\n") == {
        "a.py": (3, 1),
        "img.png": (0, 0),
        "b c.py": (12, 0),
    }


def test_parse_log() -> None:
    a, b, c = probe.parse_log(LOG)
    assert (a.oid, a.at, a.author, a.subject) == (
        "aaaaaaa",
        NOW - 120,
        "Ada",
        "Merge side",
    )
    assert a.refs == ("HEAD -> main", "tag: v1.0", "origin/main")
    assert a.graph == ("*", "|\\") and b.graph == ("| *", "|/") and c.graph == ("*",)
    assert b.refs == ("side",) and c.refs == ()
    assert probe.parse_log("") == []


def test_parse_lists() -> None:
    stashes = probe.parse_stashes(f"stash@{{0}}{S}100{S}WIP on main: abc x\nnoise\n")
    assert stashes == [probe.Stash(0, 100.0, "WIP on main: abc x")]
    main, side, old = probe.parse_branches(BRANCHES)
    assert (main.current, main.upstream, main.ahead, main.behind) == (
        True,
        "origin/main",
        2,
        1,
    )
    assert not side.current and side.upstream is None
    assert old.gone and old.subject == "forgotten"
    one, two, three = probe.parse_worktrees(WORKTREES)
    assert (one.path, one.oid, one.branch, one.flags) == (
        "/src/app",
        "1234567",
        "main",
        (),
    )
    assert two.branch is None and two.flags == ("detached", "locked")
    assert three.flags == ("prunable",)
    assert probe.parse_worktrees("") == []


def test_parse_days() -> None:
    noon = time.mktime((2026, 11, 13, 12, 0, 0, 0, 0, -1))
    times = f"{noon - 3600}\n{noon - 7200}\n{noon - 86400}\n{noon - 86400 * 9}\nx\n"
    assert probe.parse_days(times, noon, 7) == [0, 0, 0, 0, 0, 1, 2]


def test_progress(tmp_path: Path) -> None:
    assert probe.progress(tmp_path) == ("", None)
    (tmp_path / "MERGE_HEAD").write_text("x")
    state, since = probe.progress(tmp_path)
    assert state == "merge" and since is not None
    merge = tmp_path / "rebase-merge"
    merge.mkdir()
    for name, value in (("msgnum", "3\n"), ("end", "7\n"), ("head-name", "refs/x")):
        (merge / name).write_text(value)
    assert probe.progress(tmp_path)[0] == "rebase 3/7"


# Renders


def test_head() -> None:
    r = repo()
    assert text(git.render_head(r, 24, 1)) == ["main ↑2 ↓1 ✖1 ●3 +2 ?1"]
    assert text(git.render_head(r, 12, 1)) == ["main ↑2 ↓1 *"]
    rows = text(git.render_head(r, 46, 5, git.Look(warn=True)))
    assert rows[0] == "main → origin/main  ↑2 ↓1"
    assert rows[1] == "✖1 conflict  ●3 staged  +2 changed  ?1 new"
    assert rows[2] == "▲ 1 conflict" and rows[3] == "▲ 1 behind origin/main"
    assert rows[4] == "aaaaaaa 2m Merge side"
    busy = repo(state="rebase 3/7")
    assert text(git.render_head(busy, 12, 1)) == ["rebase 3/7 *"]
    assert "rebase 3/7" in text(git.render_head(busy, 34, 3))[1]
    lost = repo(head=probe.Head("abcdef1"), changes=())
    assert text(git.render_head(lost, 24, 1)) == ["@abcdef1 ✓"]
    name = "feature/" + "long-" * 12
    long = repo(head=probe.Head("abcdef1", name), changes=())
    assert text(git.render_head(long, 12, 1)) == ["feature/l… ✓"]
    assert text(git.render_head(long, 34, 3))[0].endswith("…")


def test_status() -> None:
    r = repo()
    lines = {"src/staged.py": (10, 2), "both.py": (1, 1)}
    r = repo(
        changes=tuple(
            probe.Change(c.kind, c.code, c.path, *lines.get(c.path, (None, None)))
            for c in r.changes
        )
    )
    assert text(git.render_status(r, 28, 1)) == ["✖1 ●3 +2 ?1  +12 −4"]
    rows = text(git.render_status(r, 40, 8))
    assert rows[0] == "UU clash.py"
    assert rows[1].startswith("M  src/staged.py  ") and rows[1].endswith("  +10 −2")
    assert len(rows[1]) == 40
    assert rows[2].startswith("MM both.py") and rows[2].endswith(" +2 −2")
    assert rows[-1] == "?? notes/"
    cut = text(git.render_status(r, 16, 3))
    assert cut == ["UU clash.py", "M  src/staged.py", "+4 more"]
    assert text(git.render_status(r, 12, 6))[4] == " M … file.py"
    assert text(git.render_status(repo(changes=()), 30, 4)) == ["working tree clean"]
    many = repo(counts={"untracked": 1000})
    assert text(git.render_status(many, 30, 9))[-1] == "+999 more"


def test_log() -> None:
    r = repo()
    assert text(git.render_log(r, 24, 1)) == ["aaaaaaa 2m Merge side"]
    plain = text(git.render_log(r, 34, 4))
    assert plain[0] == "aaaaaaa  2m Merge side"
    assert plain[1].startswith("bbbbbbb  1h a side commit") and plain[1].endswith("…")
    assert plain[2] == "ccccccc  1d base" and len(plain) == 3
    graph = text(git.render_log(r, 64, 8, git.Look(graph=True)))
    assert graph == [
        "●   aaaaaaa  2m main v1.0 origin/main Merge side",
        "│╲",
        "│ ● bbbbbbb  1h side a side commit with a subject long enough t…",
        "│╱",
        "●   ccccccc  1d base",
    ]
    assert len(git.render_log(r, 64, 2, git.Look(graph=True))) == 2
    assert text(git.render_log(repo(commits=()), 30, 4)) == ["no commit yet"]


def test_hover() -> None:
    r, look = repo(), git.Look()
    hover = git.Hover()
    assert len(hover.drawn(r, 40, 8, look)) == 3 and hover.rows == [0, 1, 2]
    assert not hover.on_leave() and not hover.on_move(3, 9)
    assert hover.on_move(3, 1) and not hover.on_move(9, 1)
    rows = text(hover.drawn(r, 40, 8, look))
    assert rows == [
        "aaaaaaa  2m Merge side",
        "bbbbbbb  1h a side commit with a subject",
        "            long enough to wrap twice",
        time.strftime("            Bob · %b %d %H:%M", time.localtime(NOW - 3600)),
        "            side",
        "ccccccc  1d base",
    ]
    assert hover.rows == [0, 1, 1, 1, 1, 2]
    assert hover.on_move(0, 5) and hover.focus == 2
    assert hover.on_leave() and hover.focus is None
    assert text(hover.drawn(r, 24, 1, look)) == ["aaaaaaa 2m Merge side"]
    assert hover.rows == []


def test_lists() -> None:
    r = repo()
    assert text(git.render_branches(r, 16, 1)) == ["main +2"]
    assert text(git.render_branches(r, 60, 10)) == [
        "● main   2m  ↑2 ↓1  Merge side",
        "  side   1h         a side commit",
        "  old   10d  gone   forgotten",
    ]
    assert text(git.render_branches(r, 30, 2))[1] == "+2 more"
    assert text(git.render_stash(r, 16, 1)) == ["≡2 stashed"]
    assert text(git.render_stash(r, 30, 1)) == ["≡2 stashed  last 5m"]
    assert text(git.render_stash(r, 44, 5)) == [
        "0  5m  Merge side",
        "1  1d  side: tried another way",
    ]
    assert text(git.render_worktrees(r, 14, 1)) == ["3 worktrees"]
    trees = repo(
        worktrees=tuple(
            probe.Worktree(w.path, w.oid, w.branch, w.flags, w.path == "/src/app")
            for w in r.worktrees
        )
    )
    assert text(git.render_worktrees(trees, 44, 5)) == [
        "● app      main",
        "  app-fix  @abcdef1  locked",
        "  gone     side      prunable",
    ]
    assert text(git.render_stash(repo(stashes=()), 30, 3)) == ["nothing stashed"]


def test_pulse() -> None:
    r = repo()
    assert text(git.render_pulse(r, 14, 1)) == ["▁▃▁▂▆▁▁▃█▂▄ 2m"]
    rows = text(git.render_pulse(r, 24, 3))
    assert len(rows) == 3 and rows[2] == "26 this week  last 2m"
    assert all(0x2800 <= ord(c) <= 0x28FF or c == " " for row in rows[:2] for c in row)
    assert rows[1][-5:] == "⣆⣼⣀⣾⣼" and rows[0].strip() == "⢠ ⢸"
    assert text(git.render_pulse(repo(commits=()), 24, 3)) == ["no commit yet"]


@pytest.mark.parametrize("name", git.TAGS)
def test_every_variant(name: str) -> None:
    """In its box: at most `h` rows of at most `w` cells, one cell a
    character."""
    for look in (git.Look(), git.Look(graph=True, warn=True)):
        tag = git.tags(look)[name]
        for data in (repo(), repo(state="merge", state_since=NOW - 7200), None):
            for w, h in [*tag.sizes, (1, 1), (3, 2), (7, 3), (200, 50)]:
                rows = tag.render(data, w, h)
                assert 0 < len(rows) <= h, (name, w, h)
                for row in text(rows):
                    assert len(row) <= w, (name, w, h, row)
                    assert all(unicodedata.east_asian_width(c) != "W" for c in row)
            sizes = git.trimmed(tag, data)
            assert len(git.named(sizes)) == len(tag.sizes) and all(
                h >= 1 for _, h in sizes
            )


def test_warnings() -> None:
    look = git.Look()
    assert (
        git.warnings(None) == []
        and git.warnings(repo(changes=(), head=probe.Head())) == []
    )
    r = repo(state="rebase 3/7", state_since=NOW - 7200, dirty_since=NOW - 5 * 3600)
    assert git.warnings(r, look) == [
        "1 conflict",
        "rebase open for 2h",
        "1 behind origin/main",
        "dirty for 5h",
    ]
    fresh = repo(changes=(), state="merge", state_since=NOW - 60, dirty_since=NOW - 60)
    assert git.warnings(fresh, look) == ["1 behind origin/main"]
    gone = repo(changes=(), head=probe.Head("a", "x", "o/x", gone=True))
    assert git.warnings(gone, look) == ["upstream gone"]
    assert git.chase("round")["base"] == "round" and git.chase("round")["chase"]["len"]


def test_quiet() -> None:
    assert not git.quiet("stash", repo()) and git.quiet("stash", repo(stashes=()))
    assert git.quiet("worktrees", repo(worktrees=repo().worktrees[:1]))
    assert not git.quiet("head", None) and git.quiet("log", None)
    assert not git.quiet("log", repo(commits=()))


def test_options(capsys: pytest.CaptureFixture[str]) -> None:
    args = git.options([])
    assert args.monitor == ["head"] and not args.named and args.look == git.Look()
    assert (args.every, args.border, args.print, args.repo) == (
        5.0,
        "round",
        None,
        None,
    )
    args = git.options(["-l", "-p"])
    assert args.monitor == ["head", "log", "pulse"] and not args.look.graph
    assert git.options(["-g"]).monitor == ["head", "log"]
    args = git.options(["-a", "--dirty", "2"])
    assert args.monitor == list(git.TAGS)
    assert args.look == git.Look(graph=True, warn=True, dirty=7200.0)
    args = git.options(["--monitor", "pulse,log", "--merged", "--print", "30x4"])
    assert args.monitor == ["pulse", "log"] and args.named and args.print == (30, 4)
    for bad in (["--monitor", "head,ci"], ["--every", "0"], ["--print", "30"]):
        with pytest.raises(SystemExit) as exc:
            git.options(bad)
        assert exc.value.code == 2
    assert "unknown tag ci" in capsys.readouterr().err


# A real repository


def sh(cwd: Path, *args: str) -> None:
    env = os.environ | {
        "GIT_AUTHOR_NAME": "t",
        "GIT_AUTHOR_EMAIL": "t@t",
        "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@t",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_SYSTEM": os.devnull,
    }
    subprocess.run(["git", *args], cwd=cwd, env=env, check=True, capture_output=True)


@pytest.fixture
def work(tmp_path: Path) -> Path:
    """A repository: two commits on `main`, a stash, a branch, a file
    staged, one changed, one untracked."""
    root = tmp_path / "work"
    root.mkdir()
    sh(root, "init", "-q", "-b", "main")
    (root / "a.txt").write_text("one\n")
    sh(root, "add", "a.txt")
    sh(root, "commit", "-qm", "first")
    (root / "a.txt").write_text("one\ntwo\n")
    sh(root, "commit", "-qam", "second")
    sh(root, "branch", "side")
    (root / "a.txt").write_text("one\ntwo\nstashed\n")
    sh(root, "stash", "-q")
    (root / "a.txt").write_text("one\nthree\nfour\n")
    (root / "b.txt").write_text("b\n")
    sh(root, "add", "b.txt")
    (root / "c.txt").write_text("c\n")
    return root


def snapshot(root: Path) -> dict[str, tuple[int, str]]:
    """Every file of the repository: its mtime and its content."""
    out = {}
    for path in sorted(root.rglob("*")):
        if path.is_file():
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            out[str(path.relative_to(root))] = (path.stat().st_mtime_ns, digest)
    return out


def test_read(work: Path) -> None:
    r = probe.read(work)
    assert r is not None and r.root == work and r.name == "work"
    assert r.head.branch == "main" and r.head.upstream is None and not r.head.initial
    assert sorted((c.kind, c.code, c.path, c.added, c.deleted) for c in r.changes) == [
        ("staged", "A", "b.txt", 1, 0),
        ("unstaged", "M", "a.txt", 2, 1),
        ("untracked", "?", "c.txt", None, None),
    ]
    assert [c.subject for c in r.commits] == ["second", "first"]
    assert r.commits[0].refs == ("HEAD -> main", "side")
    assert [s.subject for s in r.stashes] == [f"WIP on main: {r.head.oid} second"]
    assert [(b.name, b.current) for b in r.branches] == [
        ("main", True),
        ("side", False),
    ]
    assert [w.current for w in r.worktrees] == [True]
    assert sum(r.days) == 2 and r.days[-1] == 2
    assert r.state == "" and r.dirty_since is not None
    assert text(git.render_head(r, 24, 1)) == ["main ●1 +1 ?1"]


def test_read_awkward(work: Path, tmp_path: Path) -> None:
    assert probe.read(tmp_path) is None and probe.locate(tmp_path) is None
    assert probe.read(tmp_path / "missing") is None
    empty = tmp_path / "empty"
    empty.mkdir()
    sh(empty, "init", "-q", "-b", "trunk")
    r = probe.read(empty)
    assert r is not None and r.head.initial and r.head.branch == "trunk"
    assert r.commits == () and r.last is None and sum(r.days) == 0
    for name in git.TAGS:
        assert git.tags()[name].render(r, 30, 4)
    sh(work, "stash", "-q")
    sh(work, "checkout", "-q", "--detach", "HEAD~1")
    r = probe.read(work)
    assert r is not None and r.head.branch is None and r.head.oid
    assert text(git.render_head(r, 24, 1)) == [f"@{r.head.oid} ?1"]
    # A linked work tree: its own git directory, the shared one found.
    sh(work, "worktree", "add", "-q", str(tmp_path / "linked"), "side")
    linked = probe.read(tmp_path / "linked")
    assert linked is not None and linked.head.branch == "side"
    assert [w.current for w in linked.worktrees] == [False, True]
    _root, gitdir, common = probe.locate(tmp_path / "linked")  # type: ignore[misc]
    assert gitdir != common and common == work / ".git" and (gitdir / "HEAD").is_file()


def test_read_only(work: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """A read leaves every file of the repository as it was: the index
    too, which `git status` would refresh; and no command but a read
    is ever run."""
    os.utime(work / "b.txt", ns=(1, 1))  # the index is stale now
    before = snapshot(work)
    ran: list[list[str]] = []
    real = subprocess.run

    def spy(argv: list[str], **kw: object) -> object:
        ran.append(argv)
        assert kw["env"]["GIT_OPTIONAL_LOCKS"] == "0"  # type: ignore[index]
        return real(argv, **kw)  # type: ignore[call-overload]

    monkeypatch.setattr(probe.subprocess, "run", spy)
    for _ in range(3):
        assert probe.read(work) is not None
    assert snapshot(work) == before
    assert not list(work.rglob("*.lock"))
    verbs = {argv[argv.index("core.fsmonitor=false") + 1] for argv in ran}
    assert verbs == set(probe.COMMANDS)
    assert all("--no-optional-locks" in argv for argv in ran)
    git_ = probe.run(work)
    for bad in (
        ["commit", "-m", "x"],
        ["stash", "pop"],
        ["worktree", "prune"],
        ["fetch"],
    ):
        with pytest.raises(ValueError, match="not a read"):
            git_(bad)
    assert snapshot(work) == before


def test_watch(work: Path) -> None:
    _, gitdir, common = probe.locate(work)  # type: ignore[misc]
    watch = probe.Watch(gitdir, common)
    assert not watch.changed()
    (work / "c.txt").write_text("the work tree is not watched\n")
    assert not watch.changed()
    sh(work, "add", "c.txt")
    assert watch.changed() and not watch.changed()
    sh(work, "commit", "-qm", "third")
    assert watch.changed()
    assert common == work / ".git"
    sh(work, "stash", "drop", "-q")  # no file of the work tree, nor the index
    assert watch.changed()
    assert probe.child_cwd(os.getpid()) == Path.cwd()
    assert probe.child_cwd(None) is None and probe.child_cwd(2**30) is None


def test_main(
    work: Path, capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    assert git.main(["--print", "--repo", str(work)]) == 0
    out = capsys.readouterr().out
    assert out.count("╭") == 4 and "─ work ╯" in out and "\x1b[" not in out
    assert "main → " not in out and "│main  no upstream" in out
    assert git.main(["--print", "-a", "--repo", str(work), "--border", "plain"]) == 0
    out = capsys.readouterr().out
    for title in ("status", "log", "branches", "stash", "pulse"):
        assert f"─ {title} ┘" in out
    assert "worktrees" not in out  # one work tree: nothing to tell
    monkeypatch.chdir(work.parent)
    assert git.main(["--print", "30x2", "--monitor", "worktrees,head"]) == 0
    out = capsys.readouterr().out
    assert "│not a git repository" in out and out.count("┐") == 0


def placed(pilot: Pilot) -> dict[str, dict[str, object]]:
    return {w["tag"]: w for w in pilot.widgets() if w["state"] == "placed"}


def test_live(session: Pilot, work: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """The tags come and go with what they tell; a commit is seen
    without waiting for `--every`."""
    monkeypatch.setattr(git, "clock", skip := Skip())
    seen: dict[str, object] = {}
    argv = ["-s", session.session, "-a", "--repo", str(work), "--every", "60"]
    thread = threading.Thread(
        target=lambda: seen.update(status=git.main(argv, seconds=600))
    )
    thread.start()

    def until(what: str, test: object) -> None:
        deadline = time.monotonic() + 8
        while not test():  # type: ignore[operator]
            assert time.monotonic() < deadline and thread.is_alive(), what
            time.sleep(0.03)

    def shown() -> str:
        return "\n".join(session.dump()["host"])

    try:
        until(
            "tags", lambda: {"head", "status", "log", "stash"} <= set(placed(session))
        )
        assert "worktrees" not in placed(session)
        assert placed(session)["head"]["title"] == "work"
        until("log", lambda: "second" in shown())
        (work / "a.txt").write_text("one\ntwo\n")
        sh(work, "commit", "-qam", "third, seen by the watch")
        until("commit", lambda: "third, seen" in shown())
        sh(work, "stash", "drop", "-q")
        until("no stash", lambda: "stash" not in placed(session))
        sh(work, "worktree", "add", "-q", str(work.parent / "linked"), "side")
        until("worktrees", lambda: "worktrees" in placed(session))
    finally:
        skip.end()
        thread.join()
    assert seen == {"status": 0}


def test_live_follows(session: Pilot, work: Path) -> None:
    """Without `--repo`: the repository the wrapped program is in."""
    session.send(f"cd {work}\n")
    seen: dict[str, object] = {}
    argv = [
        "-s",
        session.session,
        "--merged",
        "--monitor",
        "head,log",
        "--cycle",
        "0.5",
    ]
    thread = threading.Thread(
        target=lambda: seen.update(status=git.main(argv, seconds=5))
    )
    thread.start()
    titles: set[object] = set()
    while thread.is_alive():
        titles.update(w["title"] for w in session.widgets() if w["tag"] == git.MERGED)
        clock.sleep(0.02)
    thread.join()
    assert seen == {"status": 0} and titles == {"work", "log"}


def test_live_hover(session: Pilot, work: Path) -> None:
    """A `hover` event on the log: the commit is told in full; `leave`
    folds it."""
    hover = git.Hover()
    r = probe.read(work)
    deck = _widgets.Deck(session)
    log = deck.add("log", [(40, 6)], lambda w, h: hover.drawn(r, w, h, git.Look()))
    deck.hover_timeout = None
    deck.on_hover(log, hover.on_move, hover.on_leave)
    deadline = time.monotonic() + 5
    while log.refresh().rect is None:
        assert time.monotonic() < deadline
        time.sleep(0.02)
    x, y, _, _ = log.rect  # type: ignore[misc]

    def told() -> bool:
        return any("t · " in line for line in session.dump()["host"])

    deck.hover({"sprite": log.info["sprite"], "x": x + 4, "y": y + 2})
    assert hover.focus == 1 and told()
    deck.leave()
    assert hover.focus is None and not told()
