# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal (decks-ask's
live.py). `live.py D`, env.sh sourced, `D/work/TODO.md` written. Under
`cat`: `ptyrc app ptyrc/note -i -c TODO.md`; ctrl+t twice focuses the
note; keys tick, add, move, edit (a paste), delete; the file on disk read
after each; written by another hand, read again; ctrl+x closes, the app
ends."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
work = f"{d}/work"
path = f"{work}/TODO.md"
pid, fd = pty.fork()
if pid == 0:
    os.chdir(work)
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "cat"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 20, 70, 0, 0))
out = open(f"{d}/host.out", "wb")


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
        print("$ ptyrc -s h", *argv, f"(status {p.returncode})")
        print(re.sub(r"\x1b\[[0-9;]*m", "", p.stdout).rstrip())
        if p.stderr.strip():
            print(p.stderr.rstrip())
    return p.stdout


def keys(data, gap=0.5):
    os.write(fd, data)
    pump(gap)


def disk(what):
    print(f"{what}: {open(path).read()!r} mode {oct(os.stat(path).st_mode & 0o777)}")


def widget():
    found = json.loads(sh("call", "widget.list", quiet=True) or "{}").get("widgets", [])
    return [{k: w.get(k) for k in ("tag", "state", "asks", "focused", "title")} for w in found if w.get("tag") == "note"]


pump(4)
disk("before")
sh("app", "ptyrc/note", "-i", "-c", "TODO.md")
pump(5)
print("widget:", widget())
sh("shot", "--sprites", "--no-cursor")
keys(b"\x14\x14", 1)
print("focused:", widget())
keys(b"j ", 1)
disk("ticked")
keys(b"athird\r", 1)
disk("added")
keys(b"\x1b[1;2A", 1)
disk("moved")
keys(b"e\x15\x1b[200~3rd\r\x1b[201~\r", 1)
disk("edited")
keys(b"jdy", 1)
disk("deleted")
sh("shot", "--sprites", "--no-cursor")
with open(path, "a") as f:
    f.write("- [ ] by another hand\n")
pump(2.5)
sh("shot", "--sprites", "--no-cursor")
keys(b"\x18", 1.5)
print("closed:", widget())
sh("manage", "ls")
keys(b"after\r", 0.8)
sh("shot", "--no-cursor")
disk("at the end")
os.kill(pid, 15)
pump(1)
