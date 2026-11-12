#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Cost of the driver: throughput of a flood and latency of a key echo,
# for no driver, `--transparent`, the screen engine, the engine without
# presets (`--no-presets`), and the engine with sprites up. Then the cost
# of one `Frames::update` at 200x60: screen scan, heat, whole update, tick.
#
#   BENCH_ONLY=latency scripts/bench.sh     skip the flood
#   BENCH_ONLY=frames scripts/bench.sh      the update only
#
#   scripts/bench.sh [MEGABYTES]      default 50
#
# Needs a release build: cargo build --release -p ptyrc-cli

set -eu
cd "$(dirname "$0")/.."
frames() {
    echo "frames, one update at 200x60, best of five rounds"
    cargo test --release -q -p ptyrc-core cost_200x60 -- --ignored --nocapture 2>/dev/null |
        grep '^  '
}
if [ "${BENCH_ONLY:-}" = frames ]; then
    frames
    exit
fi
BIN=${PTYRC_BIN:-$PWD/target/release/ptyrc}
[ -x "$BIN" ] || { echo "no release binary: cargo build --release -p ptyrc-cli" >&2; exit 2; }
MB=${1:-50}
WORK=$PWD/temp/bench.$$
mkdir -p "$WORK"
trap 'rm -rf "$WORK"' EXIT

python3 - "$BIN" "$MB" "$WORK" <<'PY'
import fcntl, json, os, pty, random, select, socket, statistics, struct, subprocess, sys
import termios, time

binary, megabytes, work = sys.argv[1], int(sys.argv[2]), sys.argv[3]
COLS, ROWS = 120, 40

def make_flood(path):
    random.seed(3)
    words = "the quick brown fox jumps over lazy dog 世界 🌼 naïve".split()
    lines = []
    for i in range(4000):
        line = " ".join(random.choice(words) for _ in range(random.randint(3, 18)))
        if i % 4 == 0:
            line = f"\x1b[3{i % 8}m{line}\x1b[0m"
        if i % 50 == 0:
            line += "\x1b[1;4H\x1b[7m status \x1b[0m\x1b[40;1H"
        lines.append(line)
    block = ("\r\n".join(lines) + "\r\n").encode()
    with open(path, "wb") as f:
        size = 0
        while size < megabytes << 20:
            f.write(block)
            size += len(block)
    return size

SPRITES = [
    {"anchor": {"at": [-1, 0], "space": "view"}, "content": {"rows": [" 12:34:56 "], "style": {"bg": "blue"}}},
    {"anchor": {"at": [40, 10], "space": "view"}, "content": {"rows": ["+--------+", "| sprite |", "+--------+"]}},
    {"anchor": {"at": [10, 20], "space": "view"}, "loop": True,
     "frames": [{"rows": ["(o )"], "ms": 80}, {"rows": ["( o)"], "ms": 80}]},
    {"effect": {"name": "confetti", "params": {"duration_ms": 600000}}},
]

def sock_path(name):
    base = os.environ.get("XDG_RUNTIME_DIR")
    base = f"{base}/ptyrc" if base else f"/tmp/ptyrc-{os.getuid()}"
    return f"{base}/{name}.sock"

def add_sprites(name):
    """A connection that keeps its sprites alive while it is open."""
    path = sock_path(name)
    for _ in range(200):
        if os.path.exists(path):
            break
        time.sleep(0.01)
    s = socket.socket(socket.AF_UNIX)
    s.connect(path)
    for i, spec in enumerate(SPRITES):
        s.sendall((json.dumps({"id": i, "method": "draw.put", "params": spec}) + "\n").encode())
    time.sleep(0.4)     # past the settle time of the raw frame
    return s

def spawn(argv):
    """argv on a fresh pty of COLS x ROWS; returns (pid, master)."""
    pid, master = pty.fork()
    if pid == 0:
        os.execvp(argv[0], argv)
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    return pid, master

def variants(app):
    name = f"bench-{os.getpid()}"
    return [
        ("no driver", app, None),
        ("transparent", [binary, "run", "--transparent", "--name", name, "--", *app], None),
        ("engine", [binary, "run", "--name", name, "--", *app], None),
        ("engine, bare", [binary, "run", "--no-presets", "--name", name, "--", *app], None),
        ("engine+sprites", [binary, "run", "--name", name, "--", *app], name),
    ]

def throughput(flood, size):
    print(f"throughput, {size >> 20} MiB of mixed text through a {COLS}x{ROWS} pty")
    app = ["sh", "-c", f"stty raw; sleep 0.8; cat {flood}"]
    for label, argv, sprites in variants(app):
        pid, master = spawn(argv)
        conn = add_sprites(sprites) if sprites else None
        got, start, extra = 0, None, 0
        while True:
            try:
                data = os.read(master, 1 << 20)
            except OSError:
                break
            if not data:
                break
            if start is None and got + len(data) > 4096:
                start = time.perf_counter()
            got += len(data)
        elapsed = time.perf_counter() - (start or time.perf_counter())
        os.waitpid(pid, 0)
        os.close(master)
        if conn:
            conn.close()
        over = f"  +{(got - size) / size * 100:.1f}% bytes" if got > size * 1.001 else ""
        print(f"  {label:<15} {size / (1 << 20) / elapsed:7.1f} MiB/s  {elapsed:6.2f} s{over}")

def latency():
    print("latency, key to echo, 400 keys")
    for label, argv, sprites in variants(["cat"]):
        pid, master = spawn(argv)
        conn = add_sprites(sprites) if sprites else None
        time.sleep(0.5)
        while select.select([master], [], [], 0.05)[0]:
            os.read(master, 65536)
        samples = []
        for i in range(400):
            key = bytes([97 + i % 26])
            t0 = time.perf_counter()
            os.write(master, key)
            seen = b""
            while key not in seen:
                if not select.select([master], [], [], 2.0)[0]:
                    raise SystemExit(f"{label}: no echo")
                seen += os.read(master, 65536)
            samples.append((time.perf_counter() - t0) * 1e6)
            time.sleep(0.002)
        os.write(master, b"\n\x04")
        time.sleep(0.1)
        try:
            os.kill(pid, 15)
        except ProcessLookupError:
            pass
        os.waitpid(pid, 0)
        os.close(master)
        if conn:
            conn.close()
        samples.sort()
        print(f"  {label:<15} median {statistics.median(samples):7.0f} us   "
              f"p99 {samples[int(len(samples) * 0.99)]:7.0f} us")

flood = f"{work}/flood.txt"
size = make_flood(flood)
if os.environ.get("BENCH_ONLY") != "latency":
    throughput(flood, size)
latency()
PY
frames
