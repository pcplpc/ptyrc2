# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty, this script the host terminal (probes-git's
live.py). `live.py D`, env.sh sourced. A shell: `ptyrc app
ptyrc/monitor --monitor glance,tcp-listen --every 1`; the shell opens a
port on every address (python3 -m http.server), the widget lists it;
the server stopped, it goes."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
work = d
env = os.environ | {"PS1": "$ "}
PORT = "48765"
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
sh("app", "ptyrc/monitor", "--monitor", "glance,tcp-listen", "--every", "1")
pump(5)
print("widgets:", widgets())
shot()
typed(f"python3 -m http.server {PORT} --bind 0.0.0.0 >/dev/null 2>&1 & echo started", 0.5)
until("tcp-listen: the port opened", seen(rf"│ *{PORT} "), 12)
shot()
typed("kill %1", 0.5)
until("tcp-listen: the port gone", lambda: not seen(rf"│ *{PORT} ")(), 12)
shot()
print("widgets:", widgets())
sh("manage", "ls")
os.kill(pid, 15)
pump(1)
