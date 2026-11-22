# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal (games-snake's
live.py). `live.py D`: `ptyrc/timer 3m --click`, the host mouse pressed
on the widget, twice; then `ptyrc/timer 5s -a --hold 3`, its end. The
guide's environment exported (XDG_*, PTYRC_STORE_APPS, PTYRC_BOX,
PTYRC_JS, PATH with target/debug)."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "sh", "-c", f"cat {d}/w.txt; exec cat"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 70, 0, 0))
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


def press(x, y):
    """A left press and release of the host mouse on the screen cell."""
    os.write(fd, f"\x1b[<0;{x + 1};{y + 1}M".encode())
    pump(0.1)
    os.write(fd, f"\x1b[<0;{x + 1};{y + 1}m".encode())


pump(4)
app = subprocess.Popen(["ptyrc", "-s", "h", "app", "ptyrc/timer", "3m", "--click"])
pump(5)
widgets = json.loads(sh("call", "widget.list", quiet=True) or "{}")
print("widgets:", json.dumps(widgets)[:600])
sh("shot", "--sprites", "--no-cursor", "--ansi")
mine = [w for w in widgets.get("widgets", []) if w.get("tag") == "timer"]
if mine:
    x, y, w, h = mine[0]["rect"]
    press(x + w // 2, y + h // 2)
    pump(3)
    sh("shot", "--sprites", "--no-cursor", "--ansi")
    pump(2)
    sh("shot", "--sprites", "--no-cursor", "--ansi")
    press(x + w // 2, y + h // 2)
    pump(2.5)
    sh("shot", "--sprites", "--no-cursor", "--ansi")
sh("manage", "ls")
app.wait(timeout=10)
ids = sh("manage", "ls", quiet=True)
for line in ids.splitlines():
    if "timer" in line:
        sh("manage", "stop", line.split()[0])
pump(2)
app = subprocess.Popen(["ptyrc", "-s", "h", "app", "ptyrc/timer", "5s", "-a", "--hold", "3"])
pump(7.2)
sh("shot", "--sprites", "--no-cursor", "--ansi")
pump(4)
sh("shot", "--sprites", "--no-cursor", "--ansi")
sh("manage", "ls")
os.kill(pid, 15)
pump(1)
