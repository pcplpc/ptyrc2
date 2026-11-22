# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal; it starts
the app (`live.py D ptyrc/snake --grab --speed 6`), types the arrows,
then ctrl+t. D: the driver's directory, `D/w.txt` the text shown,
`D/config/ptyrc/permissions.toml` the answers; the guide's
environment exported (XDG_*, PTYRC_STORE_APPS, PTYRC_BOX, PTYRC_JS,
PATH with target/debug)."""
import os, pty, select, subprocess, sys, time, fcntl, termios, struct

d = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "sh", "-c", f"cat {d}/w.txt; exec cat"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 10, 60, 0, 0))
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

def sh(*argv):
    p = subprocess.run(["ptyrc", "-s", "h", *argv], capture_output=True, text=True, timeout=60)
    print("$ ptyrc -s h", *argv, f"(status {p.returncode})")
    print(p.stdout.rstrip()); print(p.stderr.rstrip(), file=sys.stderr)

pump(4)
p = subprocess.Popen(["ptyrc", "-s", "h", "app", *sys.argv[2:]])
pump(6); p.wait()
sh("call", "session.info")
sh("text")
for key, wait in ((b"\x1b[B", 1.5), (b"\x1b[D", 2.5), (b"\x1b[A", 1.5)):
    os.write(fd, key); pump(wait)
    sh("shot", "--sprites", "--no-cursor", "--ansi")
sh("text")
os.write(fd, b"\x14"); pump(3)
sh("manage", "ls")
sh("call", "session.info")
os.write(fd, b"\x1b[B"); pump(1)
sh("text")
os.kill(pid, 15); pump(1)
