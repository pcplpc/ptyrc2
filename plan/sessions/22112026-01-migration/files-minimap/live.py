# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal (decks-note's
live.py). `live.py D`, env.sh sourced. Under `less` of a file of 400
lines: `ptyrc app ptyrc/minimap -a --mark 'line 3 '`; pages turned
(space), the map grows; `g` back to the top, the band with it; the host
mouse on the map: the line there told beside it."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
work = f"{d}/work"
os.makedirs(work, exist_ok=True)
path = f"{work}/text"
with open(path, "w") as f:
    f.write("".join(f"line {i} {'word ' * (i % 9)}\n" for i in range(400)))
pid, fd = pty.fork()
if pid == 0:
    os.chdir(work)
    os.environ["LESS"] = ""
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "less", "text"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
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


def widget():
    found = json.loads(sh("call", "widget.list", quiet=True) or "{}").get("widgets", [])
    return [{k: w.get(k) for k in ("tag", "state", "rect", "size", "title")} for w in found if w.get("tag") == "minimap"]


def band():
    """The map's rows on the ground of the view (237) or the cursor (60)."""
    w = [x for x in json.loads(sh("call", "widget.list", quiet=True) or "{}").get("widgets", []) if x.get("tag") == "minimap"]
    if not w or not w[0].get("rect"):
        return None
    x, y, ww, hh = w[0]["rect"]
    rows = json.loads(sh("call", "screen.cells", json.dumps({"rect": [x + 1, y + 1, ww - 2, hh - 2], "sprites": True}), quiet=True))["rows"]
    return [i for i, row in enumerate(rows) if any(r.get("bg") in (237, 60) for r in row)]


pump(4)
sh("app", "ptyrc/minimap", "-a", "--mark", "line 3 ")
pump(4)
print("widget:", widget(), "band:", band())
sh("shot", "--sprites", "--ansi", "--no-cursor")
for page in range(4):
    keys(b" ", 0.6)
pump(4)
print("paged:", widget(), "band:", band())
sh("shot", "--sprites", "--ansi", "--no-cursor")
keys(b"g", 1.5)
print("top:", widget(), "band:", band())
sh("shot", "--sprites", "--ansi", "--no-cursor")
x, y, _, _ = widget()[0]["rect"]
keys(f"\x1b[<35;{x + 3};{y + 2}M".encode(), 1.0)
sh("shot", "--sprites", "--ansi", "--no-cursor")
keys(f"\x1b[<35;3;20M".encode(), 1.0)
print("left:", [l for l in sh("shot", "--sprites", "--no-cursor", quiet=True).splitlines() if "line 0" in l and "│" in l])
sh("manage", "ls")
keys(b"q", 1)
os.kill(pid, 15)
pump(1)
