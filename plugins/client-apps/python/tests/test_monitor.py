# SPDX-License-Identifier: GPL-3.0-or-later
"""`monitor`: the probes on fixtures, every variant in its box, options."""

from __future__ import annotations

import os
import struct
import unicodedata

import pytest
from ptyrc_apps import _probe as probe
from ptyrc_apps import monitor

STAT = """cpu  100 0 100 800 0 0 0 0 0 0
cpu0 50 0 50 400 0 0 0 0 0 0
cpu1 50 0 50 400 0 0 0 0 0 0
intr 1 2 3
"""
STAT2 = """cpu  250 0 150 900 100 0 0 0 0 0
cpu0 175 0 75 450 50 0 0 0 0 0
cpu1 75 0 75 450 50 0 0 0 0 0
"""
MEMINFO = """MemTotal:       16000000 kB
MemFree:         2000000 kB
MemAvailable:    6000000 kB
SwapTotal:       8000000 kB
SwapFree:        6000000 kB
"""
NETDEV = """Inter-|   Receive                            |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes ...
    lo: 5000 10 0 0 0 0 0 0 5000 10 0 0 0 0 0 0
  eth0: 1000 10 0 0 0 0 0 0 2000 10 0 0 0 0 0 0
"""


def pidstat(pid: int, name: str, ticks: int, rss: int, tty: int = 0) -> str:
    # pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt
    # majflt cmajflt utime stime cutime cstime prio nice threads it start
    # vsize rss ...
    return (
        f"{pid} ({name}) S 1 {pid} {pid} {tty} {pid + 1} 0 0 0 0 0 "
        f"{ticks} 0 0 0 20 0 1 0 100 1000 {rss} 0 0"
    )


def sample(at: float, stat: str, rx: int, procs: dict[int, int]) -> probe.Sample:
    stats = (
        probe.parse_pidstat(pidstat(p, f"p{p}", t, 1000)) for p, t in procs.items()
    )
    return probe.Sample(
        at=at,
        cpu=probe.parse_stat(stat),
        net=(rx, rx // 2),
        procs={s.pid: s for s in stats if s is not None},
        memory=probe.parse_meminfo(MEMINFO),
        load=probe.parse_loadavg("0.52 0.48 0.40 1/200 300"),
        uptime=probe.parse_uptime("277000.5 1000.0"),
    )


GLANCE = probe.glance(
    sample(10.0, STAT, 1000, {1: 0, 2: 0, 3: 10, 5: 0, 6: 0, 7: 0}),
    sample(12.0, STAT2, 841000, {1: 300, 2: 20, 3: 10, 4: 99, 5: 4, 6: 2, 7: 1}),
    hz=100,
    page=4096,
)


def utmp(kind: int, pid: int, line: str, user: str, host: str, sec: int) -> bytes:
    """A glibc `struct utmp` of 64-bit Linux: type, padding, pid, line, id,
    user, host, exit status, session, `tv_sec`, `tv_usec`, address, unused."""
    return struct.pack(
        "<hhi32s4s32s256shhiii16s20s",
        kind,
        0,
        pid,
        line.encode(),
        b"",
        user.encode(),
        host.encode(),
        0,
        0,
        0,
        sec,
        500000,
        b"",
        b"",
    )


NOW = 1_800_000_000.0
SESSIONS = (
    probe.Session("alice", "pts/1", "tmux(99).%1", NOW - 7200, 99, 3.0, "vim"),
    probe.Session("alice", "pts/2", "tmux(99).%2", NOW - 3600, 99, 400.0, "zsh"),
    probe.Session("bob", "pts/5", "203.0.113.70", NOW - 600, 7, 30.0, "htop"),
    probe.Session(
        "carol-the-admin", "pts/9", "a-very-long-host.example.org", NOW - 9e5, 8
    ),
    probe.Session("root", "tty1", "", NOW - 86400 * 3, 9, 86400 * 2, "login"),
)
LOGINS = probe.Logins(NOW, SESSIONS)

TCP = """  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 101 1
   1: 0100007F:0277 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 102 1
   2: 0A01A8C0:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 103 1
   3: 0A01A8C0:0016 0B01A8C0:C350 01 00000000:00000000 00:00000000 00000000     0        0 104 1
   4: 00000000:1F40 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 105 1
"""
TCP6 = """  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 201 1
   1: 00000000000000000000000001000000:0277 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 202 1
   2: 0000000000000000FFFF00000100007F:1A0A 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 203 1
   3: B80D0120000000000000000001000000:01BB 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000    33        0 204 1
"""
USERS = {0: "root", 33: "www-data", 1000: "alice"}
OWNERS = {101: "sshd", 201: "sshd", 103: "python3", 204: "nginx"}
LISTENERS = probe.listeners(
    probe.parse_tcp(TCP) + probe.parse_tcp(TCP6), USERS.__getitem__, OWNERS
)

MOUNTS = r"""/dev/sda2 / ext4 rw,relatime 0 0
proc /proc proc rw 0 0
tmpfs /tmp tmpfs rw 0 0
/dev/sda2 /var/lib/docker/btrfs ext4 rw 0 0
overlay /var/lib/docker/overlay2/x/merged overlay rw 0 0
/dev/loop3 /snap/core/1 squashfs ro 0 0
/dev/sda1 /boot vfat rw 0 0
/dev/sdb1 /mnt/my\040disk ext4 rw 0 0
/dev/sr0 /media/cd iso9660 ro 0 0
"""
G = 1 << 30
DISKS = [
    probe.Disk("/", "/dev/sda2", 100 * G, 93 * G, 2 * G),
    probe.Disk("/boot", "/dev/sda1", G // 2, G // 8, 3 * G // 8),
    probe.Disk("/mnt/my disk", "/dev/sdb1", 4000 * G, 1200 * G, 2800 * G),
    probe.Disk("/run/media/alice/A_RATHER_LONG_LABEL", "/dev/sdc1", 64 * G, 0, 64 * G),
]

DATA = {
    "glance": GLANCE,
    "connected": LOGINS,
    "tcp-listen": LISTENERS,
    "disk-space": DISKS,
}
EMPTY = {
    "glance": probe.glance(probe.Sample(0.0), probe.Sample(1.0)),
    "connected": probe.Logins(NOW, ()),
    "tcp-listen": [],
    "disk-space": [],
}


def text(rows: monitor.Rows) -> list[str]:
    return ["".join(run["t"] for run in row) for row in rows]


def check(rows: monitor.Rows, w: int, h: int) -> None:
    assert len(rows) <= h
    for row in rows:
        assert monitor.width(row) <= w, text([row])
        for run in row:
            assert set(run) <= {"t", "fg", "bold", "dim"}
            for glyph in run["t"]:
                assert glyph.isprintable()
                assert unicodedata.east_asian_width(glyph) not in "WF"
                assert not unicodedata.combining(glyph)


# glance


def test_parse_stat() -> None:
    assert probe.parse_stat(STAT) == ((800, 1000), (400, 500), (400, 500))


def test_parse_meminfo() -> None:
    mem = probe.parse_meminfo(MEMINFO)
    assert mem.total == 16000000 * 1024 and mem.used == 10000000 * 1024
    assert mem.swap_used == 2000000 * 1024
    old = probe.parse_meminfo("MemTotal: 10 kB\nMemFree: 2 kB\nCached: 3 kB\n")
    assert old.available == 5 * 1024


def test_parse_pidstat() -> None:
    stat = probe.parse_pidstat(pidstat(42, "Web (Content) 2", 70, 9, tty=34817))
    assert stat == probe.PidStat(42, "Web (Content) 2", 34817, 43, 70, 9)
    assert probe.parse_pidstat("garbage") is None


def test_parse_small_files() -> None:
    assert probe.parse_loadavg("1.5 0.5 0.25 2/100 9") == (1.5, 0.5, 0.25)
    assert probe.parse_loadavg("") == (0.0, 0.0, 0.0)
    assert probe.parse_uptime("12.5 3.0") == 12.5
    assert probe.parse_netdev(NETDEV) == {"lo": (5000, 5000), "eth0": (1000, 2000)}


def test_glance() -> None:
    g = GLANCE
    assert g.cpu == pytest.approx(1 - 200 / 400)
    assert g.cores == pytest.approx((1 - 100 / 250, 1 - 100 / 150))
    assert g.rx == pytest.approx(420000) and g.tx == pytest.approx(210000)
    # Shares of one core over the 2 s; a process born meanwhile is not counted.
    top = [(p.pid, p.cpu) for p in g.top]
    assert top == [(1, 1.5), (2, 0.1), (5, 0.02), (6, 0.01), (7, 0.005)]
    assert g.top[0].mem == 1000 * 4096


def test_glance_reader() -> None:
    reader = probe.GlanceReader(settle=0.01)
    for g in (reader.read(), reader.read()):
        assert 0 <= g.cpu <= 1 and g.memory.total > 0 and g.uptime > 0
        assert len(g.top) <= probe.TOP


# connected


def test_parse_utmp() -> None:
    data = b"".join(
        [
            utmp(2, 0, "~", "reboot", "6.1.0", 100),  # BOOT_TIME
            utmp(7, 10, "pts/0", "alice", "192.0.2.1", 1000),
            utmp(8, 11, "pts/1", "", "", 1001),  # DEAD_PROCESS
            utmp(7, 12, "tty1", "bob", "", 2000),
            b"\0" * 100,  # a torn record
        ]
    )
    assert probe.UTMP.size == 384
    assert probe.parse_utmp(data) == [
        probe.Session("alice", "pts/0", "192.0.2.1", 1000.5, 10),
        probe.Session("bob", "tty1", "", 2000.5, 12),
    ]
    assert probe.parse_utmp(b"") == []


def test_parse_session() -> None:
    text = "# private\nUSER=alice\nCLASS=user\nTTY=pts/3\nREMOTE_HOST=h.example\n"
    text += "REALTIME=1700000000500000\nLEADER=77\n"
    assert probe.parse_session(text) == probe.Session(
        "alice", "pts/3", "h.example", 1700000000.5, 77
    )
    assert probe.parse_session(text.replace("CLASS=user", "CLASS=manager")) is None
    assert probe.parse_session("") is None


def test_seats() -> None:
    assert [probe.origin(h) for h in ("", ":0", "tmux(9).%3", "screen", "h")] == [
        "local",
        "local",
        "tmux",
        "screen",
        "h",
    ]
    seats = probe.seats(SESSIONS)
    assert [(s.user, s.origin) for s in seats] == [
        ("bob", "203.0.113.70"),
        ("alice", "tmux"),
        ("root", "local"),
        ("carol-the-admin", "a-very-long-host.example.org"),
    ]
    alice = seats[1]
    assert alice.ttys == ("pts/1", "pts/2") and alice.login == NOW - 7200
    assert (alice.idle, alice.what) == (3.0, "vim")


def test_read_logins(tmp_path: os.PathLike[str]) -> None:
    real = probe.read_logins()
    assert all(s.user for s in real.sessions)
    # No utmp: the files of logind; `.ref` files are not sessions.
    sessions = os.path.join(tmp_path, "sessions")
    os.mkdir(sessions)
    with open(os.path.join(sessions, "3"), "w") as f:
        f.write("USER=alice\nCLASS=user\nREALTIME=1000000\n")
    with open(os.path.join(sessions, "3.ref"), "w") as f:
        f.write("USER=x\nCLASS=user\n")
    got = probe.read_logins(os.path.join(tmp_path, "none"), sessions)
    assert [(s.user, s.login, s.idle) for s in got.sessions] == [("alice", 1.0, None)]
    nothing = probe.read_logins(os.path.join(tmp_path, "none"), "/nonexistent")
    assert nothing.sessions == ()


# tcp-listen


def test_parse_tcp() -> None:
    socks = probe.parse_tcp(TCP)
    assert [(s.addr, s.port, s.uid, s.inode) for s in socks] == [
        ("0.0.0.0", 22, 0, 101),
        ("127.0.0.1", 631, 0, 102),
        ("192.168.1.10", 8080, 1000, 103),
        ("0.0.0.0", 8000, 1000, 105),
    ]
    six = [(s.addr, s.port) for s in probe.parse_tcp(TCP6)]
    assert six == [
        ("::", 22),
        ("::1", 631),
        ("::ffff:127.0.0.1", 6666),
        ("2001:db8::1", 443),
    ]


def test_listeners() -> None:
    assert not probe.outside("127.0.0.1") and not probe.outside("::1")
    assert not probe.outside("::ffff:127.0.0.1") and not probe.outside("127.0.1.1")
    assert (
        probe.outside("0.0.0.0") and probe.outside("::") and probe.outside("10.0.0.1")
    )
    # Loopback out; the two wildcards of sshd on 22 are one line.
    assert LISTENERS == [
        probe.Listener("*", 22, "root", "sshd"),
        probe.Listener("2001:db8::1", 443, "www-data", "nginx"),
        probe.Listener("0.0.0.0", 8000, "alice", None),
        probe.Listener("192.168.1.10", 8080, "alice", "python3"),
    ]
    # Not merged: two owners, or one wildcard alone.
    socks = [probe.Socket("0.0.0.0", 80, 0, 1), probe.Socket("::", 80, 0, 2)]
    split = probe.listeners(socks, str, {1: "a", 2: "b"})
    assert [(x.addr, x.process) for x in split] == [("0.0.0.0", "a"), ("::", "b")]


def test_read_listeners() -> None:
    for x in probe.read_listeners():
        assert probe.outside(x.addr) if x.addr != "*" else True


# disk-space


def test_parse_mounts() -> None:
    assert probe.parse_mounts(MOUNTS) == [
        probe.Mount("/dev/sda2", "/", "ext4"),
        probe.Mount("/dev/sda1", "/boot", "vfat"),
        probe.Mount("/dev/sdb1", "/mnt/my disk", "ext4"),
    ]


def test_disk() -> None:
    # bsize frsize blocks bfree bavail files ffree favail flag namemax
    vfs = os.statvfs_result((4096, 4096, 1000, 300, 250, 0, 0, 0, 0, 255))
    d = probe.disk(probe.Mount("/dev/sda2", "/", "ext4"), vfs)
    assert (d.size, d.used, d.free) == (4096000, 700 * 4096, 250 * 4096)
    assert d.share == pytest.approx(700 / 950)
    assert probe.Disk("/", "x", 0, 0, 0).share == 0.0


def test_read_disks() -> None:
    disks = probe.read_disks()
    assert len({d.point for d in disks}) == len(disks)
    assert all(0 <= d.share <= 1 for d in disks)


# Rendering


def test_units() -> None:
    assert [monitor.size(n) for n in (0, 512, 1000, 3.2 * G, 120 * G, 1.5 * 2**40)] == [
        "0B",
        "512B",
        "1.0K",
        "3.2G",
        "120G",
        "1.5T",
    ]
    assert monitor.span(277000) == "3d 4h" and monitor.span(4000) == "1h 06m"
    assert monitor.ago(None) == "" and monitor.ago(40) == "40s"
    assert monitor.ago(7200) == "2h"


def test_bar_and_fit() -> None:
    assert text([monitor.bar(0.5, 4, "red")]) == ["━━──"]
    assert text([monitor.bar(0.3, 5, "red")]) == ["━╾───"]
    assert text([monitor.bar(0.001, 3, "red")]) == ["╾──"]
    assert text([monitor.bar(2.0, 3, "red")]) == ["━━━"]
    row = [{"t": "alpha "}, {"t": "beta", "fg": "red"}, {"t": "   "}]
    assert text([monitor.fit(row, 20)]) == ["alpha beta"]
    assert text([monitor.fit(row, 8)]) == ["alpha b…"]
    assert text([monitor.fit(row, 6)]) == ["alpha…"]


@pytest.mark.parametrize("name", list(monitor.MONITORS))
def test_every_variant(name: str) -> None:
    m = monitor.MONITORS[name]
    assert 3 <= len(m.sizes) <= 4 and m.sizes == tuple(sorted(m.sizes))
    assert all(w <= 56 and h <= 14 for w, h in m.sizes)
    sizes = [*m.sizes, (1, 1), (5, 2), (12, 3), (25, 7), (80, 20)]
    for data in (DATA[name], EMPTY[name]):
        for w, h in sizes:
            rows = m.render(data, w, h)
            check(rows, w, h)
            assert rows and all(rows[0])  # never blank on top
        for w, h in monitor.trimmed(m, data):
            assert m.render(data, w, h)


def test_tiny_variants() -> None:
    small = {n: monitor.MONITORS[n].sizes[0] for n in DATA}
    shown = {n: text(monitor.MONITORS[n].render(DATA[n], *small[n])) for n in DATA}
    assert shown["glance"] == ["cpu 50%  mem 62%"]
    assert shown["connected"] == ["bob 1  alice 2  +2"]
    assert shown["tcp-listen"] == ["22 443 8000 8080"]
    # The fullest first; what does not fit is counted.
    assert shown["disk-space"] == ["/ 98%  /boot 25%  +2"]


def test_glance_large() -> None:
    w, h = monitor.MONITORS["glance"].sizes[-1]
    rows = text(monitor.render_glance(GLANCE, w, h))
    assert len(rows) == h
    assert rows[0].startswith("cpu  ━") and rows[0].endswith("50%")
    assert rows[1] == "core ▅▃"
    assert rows[4].startswith("load 0.52 0.48 0.40") and rows[4].endswith("up 3d 4h")
    assert rows[5] == "net  ↓ 410K/s   ↑ 205K/s"
    assert rows[7].split() == ["pid", "process", "cpu", "mem"]
    assert rows[8].split() == ["1", "p1", "150%", "3.9M"]


def test_warnings() -> None:
    full = monitor.render_disks(DISKS, 44, 5)
    percent = [run for run in full[1] if run["t"] == "98%"]
    assert percent and percent[0]["fg"] == monitor.ALERT
    loaded = probe.Glance(
        0.1, (0.1, 0.1), (5.0, 1.0, 1.0), probe.Memory(1, 1), 0, 0, 0, ()
    )
    runs = [r for row in monitor.render_glance(loaded, 30, 4) for r in row]
    assert any(r["t"] == "5.00" and r["fg"] == monitor.ALERT for r in runs)
    calm = [r for row in monitor.render_glance(GLANCE, 30, 4) for r in row]
    assert not any(r.get("fg") in (monitor.ALERT, monitor.WARN) for r in calm)


def test_tables() -> None:
    rows = text(monitor.render_listen(LISTENERS, 56, 10))
    assert rows[0].split() == ["port", "address", "user", "process"]
    assert rows[1].split() == ["22", "*", "root", "sshd"]
    assert rows[3].split() == ["8000", "0.0.0.0", "alice", "?"]
    # Too many for the box: the last row counts the rest.
    rows = text(monitor.render_connected(LOGINS, 32, 3))
    assert rows[-1] == "+2 more" and len(rows) == 3
    rows = text(monitor.render_connected(LOGINS, 56, 10))
    assert rows[0].split() == ["user", "from", "tty", "since", "idle", "what"]
    assert rows[2].split()[:3] == ["alice", "tmux", "2"]
    # Aligned: a column starts where its head does; numbers end with it.
    assert rows[1].index("pts/5") == rows[0].index("tty")
    assert rows[1].index("30s") + 3 == rows[0].index("idle") + 4
    long = next(r for r in rows if r.startswith("carol"))
    assert "…" in long


def test_empty_states() -> None:
    assert text(monitor.render_listen([], 30, 3)) == ["nothing listens outside"]
    assert text(monitor.render_listen([], 18, 1)) == ["none outside"]
    assert text(monitor.render_connected(EMPTY["connected"], 20, 1)) == [
        "no one logged in"
    ]
    assert text(monitor.render_disks([], 18, 1)) == ["no local disks"]


def test_trimmed() -> None:
    m = monitor.MONITORS["tcp-listen"]
    assert monitor.trimmed(m, []) == [(w, 1) for w, _ in m.sizes]
    # One disk: a row, and the head when the box had room for it.
    m = monitor.MONITORS["disk-space"]
    assert [h for _, h in monitor.trimmed(m, DISKS[:1])] == [1, 2, 2, 2]
    for (w, h), size in zip(m.sizes, monitor.trimmed(m, DISKS[:1]), strict=True):
        assert m.render(DISKS[:1], *size) == m.render(DISKS[:1], w, h)


def test_merged() -> None:
    names = ["glance", "disk-space", "tcp-listen"]
    sizes = monitor.merged_sizes(names)
    assert len(sizes) == 4 and sizes == sorted(sizes)
    for n in names:
        for (w, h), (mw, mh) in zip(monitor.MONITORS[n].sizes, sizes, strict=True):
            assert w <= mw and h <= mh
    titles = [monitor.merged(DATA, names, turn, 44, 9)[0] for turn in range(4)]
    assert titles == ["glance", "disks", "listening", "glance"]
    _, rows = monitor.merged(DATA, names, 1, 44, 9)
    assert rows == monitor.render_disks(DISKS, 44, 9)


def test_box() -> None:
    rows = [[{"t": "hi", "fg": "red", "bold": True}]]
    assert monitor.box("glance", rows, 10, 2) == [
        "┌ glance ──┐",
        "│hi        │",
        "│          │",
        "└──────────┘",
    ]
    assert monitor.box("a long title", [], 6, 1)[0] == "┌ a l… ┐"
    assert monitor.box("t", [], 2, 1, border="round")[0] == "╭──╮"
    coloured = monitor.box("x", rows, 4, 1, colour=True)
    assert "\x1b[0;1;31mhi\x1b[0m" in coloured[1]
    assert monitor.sgr({"fg": "bright-cyan"}) == "\x1b[0;96m"
    assert monitor.sgr({"fg": 208, "dim": True}) == "\x1b[0;2;38;5;208m"
    assert monitor.sgr({"fg": "#ff8000"}) == "\x1b[0;38;2;255;128;0m"


# Options


def test_options(capsys: pytest.CaptureFixture[str]) -> None:
    args = monitor.options([])
    assert args.monitor == list(monitor.MONITORS) and not args.merged
    assert (args.every, args.cycle, args.border, args.print) == (
        2.0,
        8.0,
        "plain",
        None,
    )
    args = monitor.options(["--monitor", "disk-space,glance", "--merged", "--print"])
    assert args.monitor == ["disk-space", "glance"] and args.merged and args.print == ()
    assert monitor.options(["--print", "30x4"]).print == (30, 4)
    for bad in (["--monitor", "glance,cpu"], ["--every", "0"], ["--print", "30"]):
        with pytest.raises(SystemExit) as exc:
            monitor.options(bad)
        assert exc.value.code == 2
    err = capsys.readouterr().err
    assert (
        "unknown monitor cpu" in err
        and "glance, connected, tcp-listen, disk-space" in err
    )


def test_main(
    capsys: pytest.CaptureFixture[str], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        monitor,
        "MONITORS",
        {
            n: monitor.Monitor(m.name, m.title, m.sizes, lambda n=n: DATA[n], m.render)
            for n, m in monitor.MONITORS.items()
        },
    )
    assert monitor.main(["--print", "--monitor", "tcp-listen"]) == 0
    out = capsys.readouterr().out
    assert "┌ listening ─" in out and "sshd" in out and "\x1b[" not in out
    assert monitor.main(["--print", "20x2", "--merged"]) == 0
    out = capsys.readouterr().out
    assert out.count("┌ ") == 4 and "┌ disks ─" in out


def test_real_readers() -> None:
    for m in monitor.MONITORS.values():
        data = m.read()
        for w, h in m.sizes:
            check(m.render(data, w, h), w, h)
