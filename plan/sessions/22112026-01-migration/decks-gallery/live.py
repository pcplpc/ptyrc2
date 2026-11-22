# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal (decks-ask's
live.py). `live.py D PIC`, the guide's environment exported. Under
`cat`: `ptyrc app ptyrc/gallery` (a shipped picture), then with
`--file PIC -o` (a file named): nothing asked; the widget, its plate,
its eyes under the host mouse (SGR motion reports)."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d, pic = sys.argv[1], sys.argv[2]
pid, fd = pty.fork()
if pid == 0:
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "cat"])
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
    p = subprocess.run(["ptyrc", "-s", "h", *argv], capture_output=True, text=True, timeout=60)
    if not quiet:
        print("$ ptyrc -s h", *argv, f"(status {p.returncode})")
        print(re.sub(r"\x1b\[[0-9;]*m", "", p.stdout).rstrip())
        if p.stderr.strip():
            print(p.stderr.rstrip())
    return p.stdout


def widgets():
    found = json.loads(sh("call", "widget.list", quiet=True) or "{}").get("widgets", [])
    return [{k: w.get(k) for k in ("tag", "state", "variant", "rect", "title")} for w in found]


def mouse(x, y):
    """The host mouse moved to the cell (x, y): SGR 1006, motion."""
    os.write(fd, f"\x1b[<35;{x + 1};{y + 1}M".encode())
    pump(0.6)


pump(4)
# 1. Bare: a picture of the ship list, nothing asked.
sh("app", "ptyrc/gallery", "--every", "4")
pump(3)
print("widgets:", widgets())
sh("shot", "--sprites", "--no-cursor")
pump(4.5)
print("after --every 4:", widgets())
sh("manage", "ls")
for line in sh("manage", "ls", quiet=True).splitlines():
    m = re.match(r"\s*(\d+)\s", line)
    if m and "gallery" in line:
        sh("manage", "stop", m[1])
pump(1.5)
print("stopped:", widgets())
# 2. A file named, -o: the eyes follow the host mouse.
sh("app", "ptyrc/gallery", "--file", pic, "-o")
pump(3)
ws = widgets()
print("widgets:", ws)
sh("shot", "--sprites", "--no-cursor", "--ansi")
if ws and ws[0]["rect"]:
    x, y, w, h = ws[0]["rect"]
    mouse(x + 1, y + h // 2)
    sh("shot", "--sprites", "--no-cursor")
    print("hovered:", widgets())
    mouse(x + w - 2, y + 1)
    sh("shot", "--sprites", "--no-cursor")
pump(2.5)
print("after the hover:", widgets())
sh("manage", "ls")
os.kill(pid, 15)
pump(1)
