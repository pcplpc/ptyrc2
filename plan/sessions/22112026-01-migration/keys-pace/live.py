# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal (decks-note's
live.py). `live.py D`, env.sh sourced, the grant written. Under `sh`:
`ptyrc app ptyrc/pace -a`; words typed on the host keyboard reach the
shell (echoed, run) and are heard by pace; stopped, its counts kept;
a second run shows them, a word typed adds to them."""
import json, os, re, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
    os.chdir(d)
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "env", "PS1=$ ", "sh"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 90, 0, 0))
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
    p = subprocess.run(["ptyrc", "-s", "h", *argv], capture_output=True, text=True, timeout=60, cwd=d)
    if not quiet:
        print("$ ptyrc -s h", *argv, f"(status {p.returncode})")
        print(re.sub(r"\x1b\[[0-9;]*m", "", p.stdout).rstrip())
        if p.stderr.strip():
            print(p.stderr.rstrip())
    return p.stdout


def type_(text, gap=0.08):
    for c in text:
        os.write(fd, c.encode())
        pump(gap)


def app_id():
    for line in sh("manage", "ls", quiet=True).splitlines():
        if "pace" in line:
            return line.split()[0]
    return None


pump(3)
sh("app", "ptyrc/pace", "-a")
pump(8)  # the interpreter compiled: keys before it are not heard
type_("echo hourglass widget hourglass 9f86d081884cdeadbeef widget\n")
type_("stty -echo\n")
pump(0.5)
type_("hunter secret\n")
type_("stty echo\n")
pump(1.5)
sh("shot", "--sprites", "--no-cursor")
ident = app_id()
sh("manage", "stop", ident)
pump(1.5)
sh("manage", "ls")
store = [os.path.join(r, f) for r, _, fs in os.walk(d) for f in fs if "storage" in r]
for p in store:
    print("storage file:", os.path.relpath(p, d), open(p, errors="replace").read()[:200])
type_("clear\n")
pump(1)
sh("app", "ptyrc/pace", "-a")
pump(8)
type_("echo again widget\n")
pump(1.5)
sh("shot", "--sprites", "--no-cursor")
sh("manage", "stop", app_id())
pump(1)
os.kill(pid, 15)
pump(1)
