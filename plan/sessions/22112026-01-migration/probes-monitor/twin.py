# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's looks as fixtures of monitor/twin.js: the data of
test_monitor.py as `probe.system` answers, and for each state, monitor
and size the FNV-1a hash of the rows normalised (runs of one style
merged, colours as numbers); `trimmed`, `merged_sizes` whole. Also
writes the states as `--probe` fixtures of the cases
(monitor/tests/*.json). Run, in plugins/client-apps/python:
  TZ=UTC uv run python ../../../plan/sessions/22112026-01-migration/probes-monitor/twin.py \
    ../../store-apps/monitor"""

import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path("tests").resolve()))

from ptyrc_apps import _probe as probe  # noqa: E402
from ptyrc_apps import monitor  # noqa: E402
from test_monitor import DATA, DISKS, EMPTY, LOGINS, NOW  # noqa: E402

time.tzset()

COLOURS = {
    "bright-cyan": 14,
    "bright-magenta": 13,
    "bright-yellow": 11,
    "bright-green": 10,
    "bright-black": 8,
    "red": 1,
    "yellow": 3,
}
PARTS = {"glance": "glance", "connected": "logins", "tcp-listen": "listeners", "disk-space": "disks"}

LOADED = probe.Glance(0.95, (0.1, 0.97, 0.0, 0.5), (9.0, 6.0, 1.5), probe.Memory(8 << 30, 1 << 29, 0, 0), 59, 0, 1e6, ())
WARM = probe.Glance(0.3, (0.3,), (1.5, 1.0, 1.0), probe.Memory(1 << 30, 1 << 29, 1 << 30, 1 << 29), 86400 * 40, 512, 2.5, DATA["glance"].top[:2])
STATES = {
    "full": DATA,
    "empty": EMPTY,
    "few": {"glance": WARM, "connected": probe.Logins(NOW, LOGINS.sessions[2:3]), "tcp-listen": DATA["tcp-listen"][:1], "disk-space": DISKS[:1]},
    "loaded": {**DATA, "glance": LOADED, "tcp-listen": [x for x in DATA["tcp-listen"] if not x.process] or DATA["tcp-listen"][:1]},
}


def part(name, d):
    if name == "glance":
        m = d.memory
        return {
            "cpu": d.cpu, "cores": list(d.cores), "load": list(d.load),
            "memory": {"total": m.total, "available": m.available, "swap_total": m.swap_total, "swap_free": m.swap_free},
            "uptime": d.uptime, "rx": d.rx, "tx": d.tx,
            "top": [{"pid": p.pid, "name": p.name, "cpu": p.cpu, "mem": p.mem} for p in d.top],
        }
    if name == "connected":
        seats = [{"user": s.user, "origin": s.origin, "ttys": list(s.ttys), "login": s.login, "idle": s.idle, "what": s.what} for s in probe.seats(d.sessions)]
        return {"now": d.now, "seats": seats}
    if name == "tcp-listen":
        return [{"addr": x.addr, "port": x.port, "user": x.user, "process": x.process} for x in d]
    return [{"point": x.point, "device": x.device, "size": x.size, "used": x.used, "free": x.free} for x in d]


def answer(data):
    return {PARTS[n]: part(n, data[n]) for n in monitor.MONITORS}


def norm(rows):
    out = []
    for row in rows:
        runs = []
        for r in row:
            t = str(r["t"])
            if not t:
                continue
            fg = r.get("fg")
            style = [COLOURS.get(fg, fg), bool(r.get("bold"))]
            if runs and runs[-1][1:] == style:
                runs[-1][0] += t
            else:
                runs.append([t, *style])
        out.append(runs)
    return json.dumps(out, ensure_ascii=False, separators=(",", ":"))


def fnv(text):
    h = 0x811C9DC5
    for c in text:
        h = ((h ^ ord(c)) * 0x01000193) & 0xFFFFFFFF
    return h


NAMES = list(monitor.MONITORS)
EXTRA = [(1, 1), (5, 2), (12, 3), (25, 7), (80, 20), (200, 50), (64, 2), (19, 9), (20, 13)]
GRID = [(w, h) for w in range(2, 62, 4) for h in range(1, 15)]
faces, trims, whole = [], [], []
for si, (sn, data) in enumerate(STATES.items()):
    for mi, name in enumerate(NAMES):
        m = monitor.MONITORS[name]
        trims.append([si, mi, [list(s) for s in monitor.trimmed(m, data[name])]])
        for w, h in dict.fromkeys([*m.sizes, *EXTRA, *GRID]):
            rows = m.render(data[name], w, h)
            faces.append([si, mi, w, h, fnv(norm(rows))])
            if (w, h) in m.sizes:
                whole.append([si, mi, w, h, json.loads(norm(rows))])
merged = [[list(s) for s in monitor.merged_sizes(NAMES[:k] if k > 0 else NAMES[k:])] for k in (1, 2, 3, 4, -2)]

out = Path(sys.argv[1])
for sn, data in STATES.items():
    (out / "tests" / f"{sn}.json").write_text(json.dumps(answer(data), ensure_ascii=False, indent=1) + "\n")
dump = lambda v: json.dumps(v, ensure_ascii=False, separators=(",", ":"))  # noqa: E731
(out / "twin.js").write_text(
    "// SPDX-License-Identifier: GPL-3.0-or-later\n"
    "// The twin's looks (plan/sessions/22112026-01-migration/probes-monitor/twin.py), TZ=UTC:\n"
    "// STATES the probe.system answers; FACES [state, monitor, w, h, hash]; TRIMS\n"
    "// [state, monitor, sizes]; WHOLE [state, monitor, w, h, runs]; MERGED the sizes of\n"
    "// the first 1, 2, 3, 4 and the last 2 monitors.\n"
    f"export const STATES = {dump({k: answer(v) for k, v in STATES.items()})};\n"
    f"export const FACES = {dump(faces)};\n"
    f"export const TRIMS = {dump(trims)};\n"
    f"export const WHOLE = {dump(whole)};\n"
    f"export const MERGED = {dump(merged)};\n"
)
print(len(faces), "faces", file=sys.stderr)
