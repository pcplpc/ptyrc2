# SPDX-License-Identifier: GPL-3.0-or-later
"""A live run on a pty: this script is the host terminal, 80x24; it
starts the app (`live.py D ptyrc/flock -- --at 0.4`), shoots,
moves the host mouse (SGR motion reports) into the thick of the flock (the 9x5 box with the most dots of the birds' colours, rows 0 to 18), shoots twice, stops. D: the driver's directory, `D/w.txt` the text shown,
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
    """The shot: per row, per cell, a glyph and its colour (256-colour, or None)."""
    p = subprocess.run(["ptyrc", "-s", "h", "shot", "--ansi", "--sprites", "--no-cursor"], capture_output=True, text=True)
    rows, fg = [], None
    for line in p.stdout.split("\n"):
        row = []
        for part in re.split(r"(\x1b\[[0-9;?]*[A-Za-z])", line):
            if part.startswith("\x1b["):
                if part.endswith("m"):
                    n = part[2:-1].split(";")
                    for i, v in enumerate(n):
                        if v in ("", "0", "39"):
                            fg = None
                        elif v == "38" and i + 2 < len(n) and n[i + 1] == "5":
                            fg = int(n[i + 2])
            else:
                row += [(c, fg) for c in part]
        rows.append(row)
    return rows


BIRDS = (243, 247, 251, 255)


def birds_in(rows, x, y):
    """Bird dots in the 9x5 box about (x, y)."""
    return sum(bin(ord(c) - 0x2800).count("1") for r in rows[max(0, y - 2) : y + 3] for c, ink in r[max(0, x - 4) : x + 5] if ink in BIRDS and 0x2800 < ord(c) < 0x2900)


rows = shot()
# The thick of the flock.
def thick(rows):
    return max((birds_in(rows, x, y), x, y) for y in range(2, 17) for x in range(4, 76))


birds = thick(rows)
print("thick (dots, x, y):", birds)
_, x, y = birds
move(x, y)
pump(0.35)
rows = shot()
print("dots in that box after 0.35 s:", birds_in(rows, x, y))
sh("shot", "--ansi", "--sprites", "--no-cursor")
pump(1.5)
print("dots in that box after 1.85 s:", birds_in(shot(), x, y))
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
