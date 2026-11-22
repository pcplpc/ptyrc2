# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty, this script the host terminal (decks-note's
live.py). `live.py D`, env.sh sourced, `D/work` a repository of two
commits, `/` no repository. A shell in `D/work`: `ptyrc app
ptyrc/git -t -l`; a file changed, added, committed in the shell; the
widgets follow; the shell moves out (`cd /`) and back."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
work = f"{d}/work"
env = os.environ | {
    "GIT_AUTHOR_NAME": "t",
    "GIT_AUTHOR_EMAIL": "t@t",
    "GIT_COMMITTER_NAME": "t",
    "GIT_COMMITTER_EMAIL": "t@t",
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "PS1": "$ ",
}
pid, fd = pty.fork()
if pid == 0:
    os.chdir(work)
    os.execvpe("ptyrc", ["ptyrc", "run", "--name", "h", "--", "sh"], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
out = open(f"{d}/host.out", "wb")
t0 = time.time()


def pump(seconds):
    end = time.time() + seconds
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                out.write(os.read(fd, 65536))
            except OSError:
                return


def sh(*argv, quiet=False):
    p = subprocess.run(["ptyrc", "-s", "h", *argv], capture_output=True, text=True, timeout=60, cwd=work)
    if not quiet:
        print(f"[{time.time() - t0:5.1f} s] $ ptyrc -s h", *argv, f"(status {p.returncode})")
        print(re.sub(r"\x1b\[[0-9;]*m", "", p.stdout).rstrip())
        if p.stderr.strip():
            print(p.stderr.rstrip())
    return p.stdout


def widgets():
    found = json.loads(sh("call", "widget.list", quiet=True) or "{}").get("widgets", [])
    return [(w.get("tag"), w.get("state"), w.get("title"), w.get("size")) for w in found]


def typed(line, wait):
    print(f"[{time.time() - t0:5.1f} s] typed: {line}")
    os.write(fd, line.encode() + b"\r")
    pump(wait)


def until(what, test, limit=8):
    start = time.time()
    while time.time() - start < limit:
        if test():
            print(f"  {what}: after {time.time() - start:.1f} s")
            return
        pump(0.2)
    print(f"  {what}: NOT within {limit} s")


def shown():
    return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", sh("shot", "--sprites", "--ansi", "--no-cursor", quiet=True))


def shot():
    print(f"[{time.time() - t0:5.1f} s] shot:")
    print("\n".join(l.rstrip() for l in shown().splitlines() if l.strip()))


def seen(pattern):
    return lambda: re.search(pattern, shown(), re.M) is not None


pump(3)
sh("app", "ptyrc/git", "-t", "-l")
pump(4)
print("widgets:", widgets())
shot()
typed("echo three >> a.txt", 0.3)
until("head: a.txt changed (the work tree: a read of --every)", seen(r"│\+1 changed"))
typed("git add a.txt", 0.3)
until("head: a.txt staged (the index: the stamp)", seen(r"│●1 staged"))
shot()
typed("git commit -qm 'third, seen by the stamp'", 0.3)
until("log: the commit", seen(r"[0-9a-f]{7} +\d+[smhd] third, seen"))
shot()
typed("cd /", 0.3)
until("out of the repository", seen("not a git repository"))
print("widgets:", widgets())
shot()
typed(f"cd {work}", 0.3)
until("back", seen(r"[0-9a-f]{7} +\d+[smhd] third, seen"))
print("widgets:", widgets())
sh("manage", "ls")
os.kill(pid, 15)
pump(1)
