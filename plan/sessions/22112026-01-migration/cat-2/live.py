# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal, 80x24; it
starts the app (`live.py D ptyrc/cat --hunt --count 2`), shoots,
moves the host mouse (SGR motion reports) over a ledge, shoots twice, stops. D: the driver's directory, `D/w.txt` the text shown,
`D/config/ptyrc/permissions.toml` the answers; the guide's environment
exported (XDG_*, PTYRC_STORE_APPS, PTYRC_BOX, PTYRC_JS, PATH with
target/debug)."""
import fcntl, os, pty, re, select, struct, subprocess, sys, termios, time

d = sys.argv[1]
pid, fd = pty.fork()
if pid == 0:
    os.execvp("ptyrc", ["ptyrc", "run", "--name", "h", "--", "sh", "-c", f"cat {d}/w.txt; exec cat"])
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


def sh(*argv):
    p = subprocess.run(["ptyrc", "-s", "h", *argv], capture_output=True, text=True, timeout=60)
    print("$ ptyrc -s h", *argv, f"(status {p.returncode})")
    print(re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", p.stdout).rstrip())
    print(p.stderr.rstrip(), file=sys.stderr)


def move(x, y):
    """The host pointer to the cell (x, y), 0-based: a motion report."""
    os.write(fd, f"\x1b[<35;{x + 1};{y + 1}M".encode())


pump(3)
p = subprocess.Popen(["ptyrc", "-s", "h", "app", *sys.argv[2:]])
pump(5)
p.wait()
pump(2)
sh("manage", "ls")
sh("shot", "--ansi", "--sprites", "--no-cursor")
def shot():
    p = subprocess.run(["ptyrc", "-s", "h", "shot", "--ansi", "--sprites", "--no-cursor"], capture_output=True, text=True)
    return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", p.stdout).split("\n")


pump(6)
sh("shot", "--ansi", "--sprites", "--no-cursor")
for x in (20, 24, 28, 32):
    move(x, 8)
    pump(0.4)
pump(3)
sh("shot", "--ansi", "--sprites", "--no-cursor")
sh("text")
for line in subprocess.run(["ptyrc", "-s", "h", "manage", "ls"], capture_output=True, text=True).stdout.split("\n")[1:]:
    if line.strip():
        app = line.split()[0]
        sh("manage", "log", app)
        sh("manage", "stop", app)
pump(1)
sh("shot", "--ansi", "--sprites", "--no-cursor")
os.kill(pid, 15)
pump(1)
