# SPDX-License-Identifier: GPL-3.0-or-later
"""Reader of a git repository, for `git`.

Each source has a pure parser of the output of one command, testable
on a fixture, and `read` runs them all. Read only: the commands are
those of `COMMANDS`, run with `GIT_OPTIONAL_LOCKS=0`, so that not even
the index is refreshed (`git diff` would: it is not used). A command that fails gives nothing, never an
error; a directory that is no repository gives `None`.
"""

from __future__ import annotations

import os
import subprocess
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from pathlib import Path

__all__ = [
    "COMMANDS",
    "DAYS",
    "Branch",
    "Change",
    "Commit",
    "Head",
    "Repo",
    "Stash",
    "Watch",
    "Worktree",
    "child_cwd",
    "locate",
    "parse_branches",
    "parse_days",
    "parse_log",
    "parse_numstat",
    "parse_stashes",
    "parse_status",
    "parse_worktrees",
    "progress",
    "read",
    "run",
]

#: The git commands ever run: none takes a lock, none writes.
COMMANDS = (
    "rev-parse",
    "status",
    "diff-files",
    "diff-index",
    "log",
    "stash",
    "for-each-ref",
    "worktree",
)
#: Days of history in `Repo.days`.
DAYS = 112
SEP = "\x1f"

type Run = Callable[[Sequence[str]], str | None]


@dataclass(frozen=True)
class Head:
    """What `HEAD` is. `branch`: none when detached; `gone`: an
    upstream is set and is no more; `initial`: no commit yet."""

    oid: str = ""
    branch: str | None = None
    upstream: str | None = None
    ahead: int = 0
    behind: int = 0
    gone: bool = False
    initial: bool = False


@dataclass(frozen=True)
class Change:
    """A path of the status. `kind`: `staged`, `unstaged`, `untracked`
    or `conflict`; `code`: the letter of git (`M`, `A`, `D`, `R`...,
    `?`, `U`); `added`, `deleted`: lines, when known."""

    kind: str
    code: str
    path: str
    added: int | None = None
    deleted: int | None = None


@dataclass(frozen=True)
class Commit:
    """A line of the log. `graph`: its prefix in `git log --graph`,
    one string a row: the row of the commit first, then those under
    it that hold no commit."""

    oid: str
    at: float
    author: str
    subject: str
    refs: tuple[str, ...] = ()
    graph: tuple[str, ...] = ("*",)


@dataclass(frozen=True)
class Stash:
    n: int
    at: float
    subject: str


@dataclass(frozen=True)
class Branch:
    name: str
    current: bool
    at: float
    upstream: str | None = None
    ahead: int = 0
    behind: int = 0
    gone: bool = False
    subject: str = ""


@dataclass(frozen=True)
class Worktree:
    """`flags`: of `bare`, `detached`, `locked`, `prunable`."""

    path: str
    oid: str = ""
    branch: str | None = None
    flags: tuple[str, ...] = ()
    current: bool = False


@dataclass(frozen=True)
class Repo:
    """One reading. `days`: commits of each of the last `DAYS` days,
    today last; `state`: an operation left open (`rebase 3/7`, `merge`,
    `bisect`...) and `state_since` when it started; `dirty_since`: the
    oldest change of a tracked file not committed."""

    root: Path
    now: float
    head: Head = Head()
    changes: tuple[Change, ...] = ()
    commits: tuple[Commit, ...] = ()
    stashes: tuple[Stash, ...] = ()
    branches: tuple[Branch, ...] = ()
    worktrees: tuple[Worktree, ...] = ()
    days: tuple[int, ...] = ()
    state: str = ""
    state_since: float | None = None
    dirty_since: float | None = None
    counts: dict[str, int] = field(default_factory=dict)

    @property
    def name(self) -> str:
        return self.root.name or str(self.root)

    def count(self, kind: str) -> int:
        """Paths of `kind`; every one, where `changes` is cut."""
        return self.counts.get(kind, sum(c.kind == kind for c in self.changes))

    @property
    def last(self) -> float | None:
        """When `HEAD` was committed."""
        return self.commits[0].at if self.commits else None


# Parsers


def parse_status(text: str) -> tuple[Head, list[Change]]:
    """`git status --porcelain=v2 --branch -z`."""
    head: dict[str, object] = {}
    changes: list[Change] = []
    fields = text.split("\0")
    tracked = False
    while fields:
        entry = fields.pop(0)
        kind, _, rest = entry.partition(" ")
        if kind == "#":
            key, _, value = rest.partition(" ")
            if key == "branch.oid":
                head["initial"] = value == "(initial)"
                head["oid"] = "" if value == "(initial)" else value[:7]
            elif key == "branch.head":
                head["branch"] = None if value == "(detached)" else value
            elif key == "branch.upstream":
                head["upstream"], head["gone"] = value, True
            elif key == "branch.ab":
                ahead, _, behind = value.partition(" ")
                head["ahead"], head["behind"] = int(ahead), -int(behind)
                tracked = True
        elif kind in ("1", "2"):
            parts = rest.split(" ", 8 if kind == "2" else 7)
            xy, path = parts[0], parts[-1]
            if kind == "2" and fields:
                fields.pop(0)  # the path it was renamed from
            if xy[0] != ".":
                changes.append(Change("staged", xy[0], path))
            if xy[1] != ".":
                changes.append(Change("unstaged", xy[1], path))
        elif kind == "u":
            changes.append(Change("conflict", "U", rest.split(" ", 9)[-1]))
        elif kind == "?":
            changes.append(Change("untracked", "?", rest))
    if tracked:
        head["gone"] = False
    return Head(**head), changes  # type: ignore[arg-type]


def parse_numstat(text: str) -> dict[str, tuple[int, int]]:
    """`git diff-files --numstat`: lines added and deleted, by path; a binary
    file counts none."""
    out: dict[str, tuple[int, int]] = {}
    for line in text.splitlines():
        added, _, rest = line.partition("\t")
        deleted, _, path = rest.partition("\t")
        if path:
            out[path] = (
                int(added) if added.isdigit() else 0,
                int(deleted) if deleted.isdigit() else 0,
            )
    return out


#: `--format` of `parse_log`, after `--graph`.
LOG = SEP.join(("", "%h", "%ct", "%an", "%D", "%s"))


def parse_log(text: str) -> list[Commit]:
    """`git log --graph --format=LOG`."""
    out: list[Commit] = []
    for line in text.splitlines():
        graph, sep, rest = line.partition(SEP)
        graph = graph.rstrip()
        if not sep:
            if out and graph:
                last = out[-1]
                out[-1] = Commit(**{**last.__dict__, "graph": (*last.graph, graph)})
            continue
        oid, at, author, refs, subject = (rest.split(SEP, 4) + [""] * 4)[:5]
        names = tuple(r.strip() for r in refs.split(",") if r.strip())
        out.append(Commit(oid, float(at or 0), author, subject, names, (graph or "*",)))
    return out


def parse_stashes(text: str) -> list[Stash]:
    """`git stash list --format=%gd SEP %ct SEP %gs`."""
    out = []
    for line in text.splitlines():
        name, at, subject = (line.split(SEP, 2) + ["", ""])[:3]
        n = name.partition("{")[2].rstrip("}")
        if n.isdigit():
            out.append(Stash(int(n), float(at or 0), subject))
    return out


#: `--format` of `parse_branches`.
BRANCHES = SEP.join(
    (
        "%(refname:short)",
        "%(HEAD)",
        "%(committerdate:unix)",
        "%(upstream:short)",
        "%(upstream:track)",
        "%(contents:subject)",
    )
)


def parse_branches(text: str) -> list[Branch]:
    """`git for-each-ref --format=BRANCHES refs/heads`."""
    out = []
    for line in text.splitlines():
        parts = (line.split(SEP, 5) + [""] * 5)[:6]
        name, mark, at, upstream, track, subject = parts
        counts = {"ahead": 0, "behind": 0}
        for part in track.strip("[]").split(","):
            word, _, n = part.strip().partition(" ")
            if word in counts and n.isdigit():
                counts[word] = int(n)
        out.append(
            Branch(
                name,
                mark == "*",
                float(at or 0),
                upstream or None,
                gone="gone" in track,
                subject=subject,
                **counts,
            )
        )
    return out


def parse_worktrees(text: str) -> list[Worktree]:
    """`git worktree list --porcelain`."""
    out = []
    for block in text.strip().split("\n\n"):
        seen: dict[str, str] = {}
        for line in block.splitlines():
            key, _, value = line.partition(" ")
            seen[key] = value
        if "worktree" not in seen:
            continue
        branch = seen.get("branch", "").removeprefix("refs/heads/") or None
        flags = tuple(
            f for f in ("bare", "detached", "locked", "prunable") if f in seen
        )
        out.append(Worktree(seen["worktree"], seen.get("HEAD", "")[:7], branch, flags))
    return out


def parse_days(text: str, now: float, days: int = DAYS) -> list[int]:
    """Commit times, one a line, as commits a local day: `days` of
    them, today last."""
    out = [0] * days
    t = time.localtime(now)
    midnight = time.mktime((t.tm_year, t.tm_mon, t.tm_mday, 0, 0, 0, 0, 0, -1))
    for line in text.split():
        try:
            back = int((midnight + 86400 - float(line)) // 86400)
        except ValueError:
            continue
        if 0 <= back < days:
            out[days - 1 - back] += 1
    return out


# The repository


def progress(gitdir: Path) -> tuple[str, float | None]:
    """The operation left open in `gitdir`, and since when; empty when
    none."""

    def number(path: Path) -> str:
        try:
            return path.read_text().strip()
        except OSError:
            return "?"

    def since(path: Path) -> float | None:
        try:
            return path.stat().st_mtime
        except OSError:
            return None

    merge, apply = gitdir / "rebase-merge", gitdir / "rebase-apply"
    if merge.is_dir():
        step = f"{number(merge / 'msgnum')}/{number(merge / 'end')}"
        return f"rebase {step}", since(merge / "head-name")
    if apply.is_dir():
        what = "am" if (apply / "applying").exists() else "rebase"
        step = f"{number(apply / 'next')}/{number(apply / 'last')}"
        return f"{what} {step}", since(apply / "head-name") or since(apply)
    for name, what in (
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("BISECT_LOG", "bisect"),
    ):
        if (gitdir / name).exists():
            return what, since(gitdir / name)
    return "", None


def run(root: Path | str) -> Run:
    """A runner of git commands in `root`: their output, or none when
    one fails. No optional lock, no prompt, no pager, paths as they
    are."""
    env = os.environ | {
        "GIT_OPTIONAL_LOCKS": "0",
        "GIT_TERMINAL_PROMPT": "0",
        "LC_ALL": "C",
    }

    def git(args: Sequence[str]) -> str | None:
        if args[0] not in COMMANDS or (
            args[0] in ("stash", "worktree") and args[1] != "list"
        ):
            raise ValueError(f"not a read: git {' '.join(args)}")
        argv = ["git", "--no-optional-locks", "--no-pager", "-C", str(root)]
        argv += ["-c", "core.quotePath=false", "-c", "core.fsmonitor=false", *args]
        try:
            done = subprocess.run(
                argv,
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=10,
                check=False,
            )
        except (OSError, subprocess.SubprocessError):
            return None
        if done.returncode != 0:
            return None
        return done.stdout.decode("utf-8", "replace")

    return git


def locate(path: Path | str, git: Run | None = None) -> tuple[Path, Path, Path] | None:
    """The work tree holding `path`, its git directory and the common
    one (another for a linked work tree); none outside a work tree."""
    git = git or run(path)
    out = git(
        ["rev-parse", "--show-toplevel", "--absolute-git-dir", "--git-common-dir"]
    )
    lines = (out or "").splitlines()
    if len(lines) != 3:
        return None
    root, gitdir = Path(lines[0]), Path(lines[1])
    # The common directory is told from where git ran.
    return root, gitdir, (Path(path) / lines[2]).resolve()


def child_cwd(pid: int | None) -> Path | None:
    """The working directory of process `pid`, when it can be read."""
    if not pid:
        return None
    try:
        return Path(os.readlink(f"/proc/{pid}/cwd"))
    except OSError:
        return None


class Watch:
    """Tells when a repository changed without running git: the mtime
    of `HEAD`, `index`, the reflog and the files of an open
    operation."""

    NAMES = ("HEAD", "index", "logs/HEAD", "MERGE_HEAD", "CHERRY_PICK_HEAD")
    NAMES += ("REVERT_HEAD", "BISECT_LOG", "rebase-merge/msgnum", "rebase-apply/next")
    SHARED = ("refs/stash", "FETCH_HEAD", "packed-refs", "worktrees")

    def __init__(self, gitdir: Path, common: Path) -> None:
        self.paths = [gitdir / n for n in self.NAMES]
        self.paths += [common / n for n in self.SHARED]
        self.seen = self.stamp()

    def stamp(self) -> tuple[int, ...]:
        out = []
        for path in self.paths:
            try:
                out.append(path.stat().st_mtime_ns)
            except OSError:
                out.append(0)
        return tuple(out)

    def changed(self) -> bool:
        """True once after each change."""
        now = self.stamp()
        if now == self.seen:
            return False
        self.seen = now
        return True


#: Paths of the status kept, and commits of the log.
CHANGES, COMMITS = 400, 60


def read(
    path: Path | str, now: float | None = None, git: Run | None = None
) -> Repo | None:
    """The repository holding `path`; none when there is none."""
    git = git or run(path)
    where = locate(path, git)
    if where is None:
        return None
    root, gitdir, _ = where
    now = time.time() if now is None else now
    status = git(["status", "--porcelain=v2", "--branch", "-z"])
    head, changes = parse_status(status or "")
    # Not `git diff`: it refreshes the index, optional locks or not.
    lines = parse_numstat(git(["diff-files", "--numstat"]) or "")
    cached = {}
    if not head.initial:
        cached = parse_numstat(
            git(["diff-index", "--cached", "--numstat", "HEAD"]) or ""
        )
    counts: dict[str, int] = {}
    for c in changes:
        counts[c.kind] = counts.get(c.kind, 0) + 1
    counted = []
    for c in changes[:CHANGES]:
        table = {"staged": cached, "unstaged": lines}.get(c.kind, {})
        added, deleted = table.get(c.path, (None, None))
        counted.append(Change(c.kind, c.code, c.path, added, deleted))
    dirty = None
    for c in counted:
        if c.kind == "untracked":
            continue
        try:
            at = (root / c.path).lstat().st_mtime
        except OSError:
            continue
        dirty = at if dirty is None else min(dirty, at)
    if dirty is None and any(c.kind != "untracked" for c in counted):
        try:
            dirty = (gitdir / "index").stat().st_mtime
        except OSError:
            pass
    commits: list[Commit] = []
    days: list[int] = [0] * DAYS
    if not head.initial:
        log = git(["log", "--graph", f"--format={LOG}", "-n", str(COMMITS)])
        commits = parse_log(log or "")
        times = git(["log", "--format=%ct", f"--since={DAYS + 1}.days"])
        days = parse_days(times or "", now)
    stashes = parse_stashes(
        git(["stash", "list", f"--format=%gd{SEP}%ct{SEP}%gs"]) or ""
    )
    branches = parse_branches(
        git(
            [
                "for-each-ref",
                "--sort=-committerdate",
                f"--format={BRANCHES}",
                "refs/heads",
            ]
        )
        or ""
    )
    worktrees = [
        Worktree(w.path, w.oid, w.branch, w.flags, Path(w.path) == root)
        for w in parse_worktrees(git(["worktree", "list", "--porcelain"]) or "")
    ]
    state, since = progress(gitdir)
    return Repo(
        root=root,
        now=now,
        head=head,
        changes=tuple(counted),
        commits=tuple(commits),
        stashes=tuple(stashes),
        branches=tuple(branches),
        worktrees=tuple(worktrees),
        days=tuple(days),
        state=state,
        state_since=since,
        dirty_since=dirty,
        counts=counts,
    )
