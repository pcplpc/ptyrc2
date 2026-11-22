# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal (decks-timer's
live.py). `live.py D`, the guide's environment exported. Under `cat`:
`ptyrc app ptyrc/ask` asked thrice; keys typed while the widget is not
focused are cat's; ctrl+t twice gives them to the widget, and none
reaches cat; the answer is the caller's stdout and status."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
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
    p = subprocess.run(["ptyrc", "-s", "h", *argv], capture_output=True, text=True, timeout=60)
    if not quiet:
        print("$ ptyrc -s h", *argv, f"(status {p.returncode})")
        print(re.sub(r"\x1b\[[0-9;]*m", "", p.stdout).rstrip())
        if p.stderr.strip():
            print(p.stderr.rstrip())
    return p.stdout


def keys(data, gap=0.3):
    os.write(fd, data)
    pump(gap)


def ask(*argv, json_out=False):
    pre = ["--json"] if json_out else []
    return subprocess.Popen(["ptyrc", *pre, "-s", "h", "app", "ptyrc/ask", *argv],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def ended(app, what):
    pump(1.5)
    try:
        o, e = app.communicate(timeout=10)
    except subprocess.TimeoutExpired:
        app.kill()
        o, e = app.communicate()
        print(f"{what}: did not end")
    print(f"{what}: status {app.returncode}, stdout {o!r}, stderr {e.strip()!r}")


def widget():
    found = json.loads(sh("call", "widget.list", quiet=True) or "{}").get("widgets", [])
    return [{k: w.get(k) for k in ("tag", "state", "asks", "focused", "title")} for w in found if w.get("tag") == "ask"]


pump(4)
# 1. A choice: keys before the focus are cat's; focused, down and enter.
app = ask("Deploy?", "yes", "no", "later", "--title", "deploy")
pump(5)
print("widget:", widget())
keys(b"before\r", 0.8)
sh("shot", "--sprites", "--no-cursor", "--ansi")
keys(b"\x14\x14", 0.8)
print("focused:", widget())
keys(b"\x1b[B")
keys(b"j")  # down again: `later`
keys(b"k")  # up: `no`
sh("shot", "--sprites", "--no-cursor", "--ansi")
keys(b"\r")
ended(app, "choice")
keys(b"after\r", 0.8)
sh("shot", "--sprites", "--no-cursor", "--ansi")
print("widget:", widget())

# 2. Several, as JSON and as lines.
for as_json in (True, False):
    app = ask("-m", "Which?", "a", "b", "c", json_out=as_json)
    pump(5)
    keys(b"\x14\x14", 0.8)
    keys(b" ")
    keys(b"\x1b[B\x1b[B", 0.4)
    keys(b" ")
    keys(b"\r")
    ended(app, f"multi, json {as_json}")

# 3. A line typed, then closed (ctrl+x): status 1, nothing printed.
app = ask("Name?", "--default", "an")
pump(5)
keys(b"\x14\x14", 0.8)
keys("né".encode())
sh("shot", "--sprites", "--no-cursor", "--ansi")
keys(b"\x18")
ended(app, "closed")
keys(b"done\r", 0.8)
sh("shot", "--sprites", "--no-cursor", "--ansi")
sh("manage", "ls")
os.kill(pid, 15)
pump(1)
