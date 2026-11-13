# SPDX-License-Identifier: GPL-3.0-or-later
"""Readers of the local system, Linux, for `monitor`.

Each source has a pure parser of the text of a file, testable on a
fixture, and a thin reader of the real file. Shares and rates come
from two samples; `GlanceReader` keeps the previous one. A file that
cannot be read gives nothing, never an error.
"""

from __future__ import annotations

import dataclasses
import ipaddress
import os
import pwd
import re
import struct
import time
from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass
from pathlib import Path

__all__ = [
    "HZ",
    "PAGE",
    "TOP",
    "UTMP",
    "Disk",
    "Glance",
    "GlanceReader",
    "Listener",
    "Logins",
    "Memory",
    "Mount",
    "PidStat",
    "Proc",
    "Sample",
    "Seat",
    "Session",
    "Socket",
    "disk",
    "glance",
    "listeners",
    "origin",
    "outside",
    "parse_loadavg",
    "parse_meminfo",
    "parse_mounts",
    "parse_netdev",
    "parse_pidstat",
    "parse_session",
    "parse_stat",
    "parse_tcp",
    "parse_uptime",
    "parse_utmp",
    "read_disks",
    "read_listeners",
    "read_logins",
    "sample",
    "seats",
]

PROC = Path("/proc")
HZ = os.sysconf("SC_CLK_TCK")
PAGE = os.sysconf("SC_PAGE_SIZE")
TOP = 5  # processes kept by `glance`


def _read(path: str | Path) -> str:
    try:
        return Path(path).read_text(errors="replace")
    except OSError:
        return ""


# glance


@dataclass(frozen=True)
class Memory:
    """Bytes. `available`: what can be had without swapping."""

    total: int = 0
    available: int = 0
    swap_total: int = 0
    swap_free: int = 0

    @property
    def used(self) -> int:
        return self.total - self.available

    @property
    def swap_used(self) -> int:
        return self.swap_total - self.swap_free


@dataclass(frozen=True)
class PidStat:
    """What `/proc/<pid>/stat` says of a process. `tty`: the device
    number of its terminal, 0 for none; `tpgid`: the foreground process
    group of that terminal; `ticks`: user and system time, in clock
    ticks; `rss`: pages."""

    pid: int
    name: str
    tty: int
    tpgid: int
    ticks: int
    rss: int


@dataclass(frozen=True)
class Proc:
    """A process of the top. `cpu`: share of one core; `mem`: bytes."""

    pid: int
    name: str
    cpu: float
    mem: int


@dataclass(frozen=True)
class Sample:
    """The counters at one time. `cpu`: `(idle, total)` ticks, all cores
    first, then each core; `net`: received and sent bytes."""

    at: float
    cpu: tuple[tuple[int, int], ...] = ()
    net: tuple[int, int] = (0, 0)
    procs: Mapping[int, PidStat] = dataclasses.field(default_factory=dict)
    memory: Memory = Memory()
    load: tuple[float, float, float] = (0.0, 0.0, 0.0)
    uptime: float = 0.0


@dataclass(frozen=True)
class Glance:
    """`cpu` and `cores`: shares in [0, 1]; `rx`, `tx`: bytes a second."""

    cpu: float
    cores: tuple[float, ...]
    load: tuple[float, float, float]
    memory: Memory
    uptime: float
    rx: float
    tx: float
    top: tuple[Proc, ...]


def parse_stat(text: str) -> tuple[tuple[int, int], ...]:
    """`(idle, total)` ticks of the `cpu` lines of `/proc/stat`. Idle
    counts iowait; the total is the first eight fields (guest time is in
    user time already)."""
    out = []
    for line in text.splitlines():
        if not line.startswith("cpu"):
            continue
        fields = [int(v) for v in line.split()[1:9]]
        out.append((sum(fields[3:5]), sum(fields)))
    return tuple(out)


def parse_meminfo(text: str) -> Memory:
    kib = {}
    for line in text.splitlines():
        key, _, rest = line.partition(":")
        if fields := rest.split():
            kib[key] = int(fields[0]) * 1024
    available = kib.get("MemAvailable")
    if available is None:  # kernels before 3.14
        available = sum(kib.get(k, 0) for k in ("MemFree", "Buffers", "Cached"))
    return Memory(
        kib.get("MemTotal", 0),
        available,
        kib.get("SwapTotal", 0),
        kib.get("SwapFree", 0),
    )


def parse_loadavg(text: str) -> tuple[float, float, float]:
    fields = text.split()
    if len(fields) < 3:
        return (0.0, 0.0, 0.0)
    return (float(fields[0]), float(fields[1]), float(fields[2]))


def parse_uptime(text: str) -> float:
    fields = text.split()
    return float(fields[0]) if fields else 0.0


def parse_netdev(text: str) -> dict[str, tuple[int, int]]:
    """Received and sent bytes of each interface of `/proc/net/dev`."""
    out = {}
    for line in text.splitlines()[2:]:
        name, sep, data = line.partition(":")
        fields = data.split()
        if sep and len(fields) >= 9:
            out[name.strip()] = (int(fields[0]), int(fields[8]))
    return out


def parse_pidstat(text: str) -> PidStat | None:
    """`/proc/<pid>/stat`. The name is between the first `(` and the
    last `)`: it may hold both, and spaces."""
    left, right = text.find("("), text.rfind(")")
    if left < 0 or right < left:
        return None
    rest = text[right + 2 :].split()
    if len(rest) < 22:
        return None
    # Fields from 3 on: state ppid pgrp session tty_nr tpgid ... utime stime.
    return PidStat(
        int(text[:left]),
        text[left + 1 : right],
        int(rest[4]),
        int(rest[5]),
        int(rest[11]) + int(rest[12]),
        int(rest[21]),
    )


def _share(before: tuple[int, int], after: tuple[int, int]) -> float:
    idle, total = after[0] - before[0], after[1] - before[1]
    if total <= 0:
        return 0.0
    return min(1.0, max(0.0, 1.0 - idle / total))


def glance(before: Sample, after: Sample, *, hz: int = HZ, page: int = PAGE) -> Glance:
    """What changed from `before` to `after`, with the state at `after`."""
    seconds = max(after.at - before.at, 1e-6)
    shares = [_share(b, a) for b, a in zip(before.cpu, after.cpu, strict=False)]
    top = [
        Proc(pid, now.name, (now.ticks - then.ticks) / hz / seconds, now.rss * page)
        for pid, now in after.procs.items()
        if (then := before.procs.get(pid)) is not None
    ]
    top.sort(key=lambda p: (-p.cpu, -p.mem, p.pid))
    return Glance(
        cpu=shares[0] if shares else 0.0,
        cores=tuple(shares[1:]),
        load=after.load,
        memory=after.memory,
        uptime=after.uptime,
        rx=max(0, after.net[0] - before.net[0]) / seconds,
        tx=max(0, after.net[1] - before.net[1]) / seconds,
        top=tuple(top[:TOP]),
    )


def _procs() -> dict[int, PidStat]:
    out = {}
    try:
        entries = list(os.scandir(PROC))
    except OSError:
        return out
    for entry in entries:
        if entry.name.isdigit():
            stat = parse_pidstat(_read(PROC / entry.name / "stat"))
            if stat is not None:
                out[stat.pid] = stat
    return out


def _virtual() -> set[str]:
    try:
        return set(os.listdir("/sys/devices/virtual/net"))
    except OSError:
        return {"lo"}


def sample() -> Sample:
    """The counters now. Network: interfaces that are not virtual
    (`lo`, bridges, veth, tunnels carry traffic counted elsewhere)."""
    virtual = _virtual() | {"lo"}
    counters = parse_netdev(_read(PROC / "net/dev"))
    net = [v for k, v in counters.items() if k not in virtual]
    return Sample(
        at=time.monotonic(),
        cpu=parse_stat(_read(PROC / "stat")),
        net=(sum(v[0] for v in net), sum(v[1] for v in net)),
        procs=_procs(),
        memory=parse_meminfo(_read(PROC / "meminfo")),
        load=parse_loadavg(_read(PROC / "loadavg")),
        uptime=parse_uptime(_read(PROC / "uptime")),
    )


class GlanceReader:
    """Keeps the last sample. The first `read` takes two, `settle`
    seconds apart."""

    def __init__(self, settle: float = 0.25) -> None:
        self.settle = settle
        self._last: Sample | None = None

    def read(self) -> Glance:
        now = sample()
        if self._last is None:
            time.sleep(self.settle)
            self._last, now = now, sample()
        out = glance(self._last, now)
        self._last = now
        return out


# connected

# glibc `struct utmp`, 384 bytes; `ut_tv` is two int32 on 64-bit too.
UTMP = struct.Struct("<h2xi32s4s32s256shhiii16s20s")
USER_PROCESS = 7


@dataclass(frozen=True)
class Session:
    """A login. `host`: as written by the login program (`tmux(1).%3`,
    `:0`, a name, an address, or empty); `login`, epoch seconds; `idle`:
    seconds since the tty was read, None when unknown; `what`: the
    foreground process of the tty."""

    user: str
    tty: str
    host: str
    login: float
    pid: int = 0
    idle: float | None = None
    what: str | None = None


@dataclass(frozen=True)
class Seat:
    """The sessions of one user from one origin. `login`: the earliest;
    `idle` and `what`: of the least idle."""

    user: str
    origin: str
    ttys: tuple[str, ...]
    login: float
    idle: float | None
    what: str | None


@dataclass(frozen=True)
class Logins:
    now: float
    sessions: tuple[Session, ...]


def _cstr(raw: bytes) -> str:
    return raw.split(b"\0", 1)[0].decode(errors="replace")


def parse_utmp(data: bytes) -> list[Session]:
    """The `USER_PROCESS` records of a utmp file."""
    out = []
    for offset in range(0, len(data) - UTMP.size + 1, UTMP.size):
        kind, pid, line, _, user, host, _, _, _, sec, usec, _, _ = UTMP.unpack_from(
            data, offset
        )
        if kind == USER_PROCESS and (name := _cstr(user)):
            out.append(Session(name, _cstr(line), _cstr(host), sec + usec / 1e6, pid))
    return out


def parse_session(text: str) -> Session | None:
    """A file of `/run/systemd/sessions/`: sessions of class `user` only
    (not the service manager of a user)."""
    kv = dict(line.partition("=")[::2] for line in text.splitlines() if "=" in line)
    if kv.get("CLASS") != "user" or not kv.get("USER"):
        return None
    return Session(
        kv["USER"],
        kv.get("TTY", ""),
        kv.get("REMOTE_HOST", ""),
        int(kv.get("REALTIME", "0") or 0) / 1e6,
        int(kv.get("LEADER", "0") or 0),
    )


def origin(host: str) -> str:
    """Where a session comes from, in short: `tmux`, `screen`, `local`
    (a console or a display), else the host as given."""
    if not host or host.startswith(":"):
        return "local"
    if match := re.fullmatch(r"(tmux|screen)\b.*", host):
        return match[1]
    return host


def seats(sessions: Iterable[Session]) -> list[Seat]:
    """Sessions grouped by user and origin, the most recent login first."""
    groups: dict[tuple[str, str], list[Session]] = {}
    for one in sessions:
        groups.setdefault((one.user, origin(one.host)), []).append(one)
    out = []
    for (user, where), group in groups.items():
        known = [s for s in group if s.idle is not None]
        least = min(known, key=lambda s: s.idle or 0.0) if known else group[-1]
        out.append(
            Seat(
                user,
                where,
                tuple(s.tty for s in group),
                min(s.login for s in group),
                least.idle,
                least.what,
            )
        )
    out.sort(key=lambda s: -max(x.login for x in groups[s.user, s.origin]))
    return out


def _tty(
    tty: str, now: float, groups: Mapping[int, int]
) -> tuple[float | None, str | None]:
    """Idle seconds of `/dev/<tty>` (its last read) and the name of its
    foreground process; None when unknown."""
    try:
        st = os.stat(f"/dev/{tty}") if tty else None
    except OSError:
        st = None
    if st is None:
        return None, None
    group = groups.get(st.st_rdev, 0)
    what = _read(PROC / str(group) / "comm").strip() if group > 0 else ""
    return max(0.0, now - st.st_atime), what or None


def read_logins(
    utmp: str = "/var/run/utmp", sessions: str = "/run/systemd/sessions"
) -> Logins:
    """Live logins: utmp records whose process exists; without utmp, the
    files of logind."""
    now = time.time()
    try:
        found = parse_utmp(Path(utmp).read_bytes())
    except OSError:
        found = []
        try:
            names = sorted(os.listdir(sessions))
        except OSError:
            names = []
        for name in names:
            if not name.endswith(".ref"):
                one = parse_session(_read(Path(sessions) / name))
                found += [one] if one is not None else []
    found = [s for s in found if s.pid <= 0 or (PROC / str(s.pid)).exists()]
    # `tty_nr` and `st_rdev` agree for the majors of terminals (< 4096).
    groups = {p.tty: p.tpgid for p in _procs().values() if p.tty}
    out = []
    for one in found:
        idle, what = _tty(one.tty, now, groups)
        out.append(dataclasses.replace(one, idle=idle, what=what))
    out.sort(key=lambda s: s.login)
    return Logins(now, tuple(out))


# tcp-listen

LISTEN = "0A"
WILD = ("0.0.0.0", "::")


@dataclass(frozen=True)
class Socket:
    addr: str
    port: int
    uid: int
    inode: int


@dataclass(frozen=True)
class Listener:
    """`addr`: `*` for both wildcards, else as written; `process`: None
    when its socket is in no readable `/proc/<pid>/fd`."""

    addr: str
    port: int
    user: str
    process: str | None


def _address(hexa: str) -> str:
    raw = bytes.fromhex(hexa)
    if len(raw) == 4:
        return str(ipaddress.IPv4Address(raw[::-1]))
    # Four 32-bit words, each in host order (little-endian here).
    words = b"".join(raw[i : i + 4][::-1] for i in range(0, 16, 4))
    return str(ipaddress.IPv6Address(words))


def parse_tcp(text: str) -> list[Socket]:
    """The listening sockets of `/proc/net/tcp` or `/proc/net/tcp6`."""
    out = []
    for line in text.splitlines()[1:]:
        fields = line.split()
        if len(fields) < 10 or fields[3] != LISTEN:
            continue
        host, _, port = fields[1].partition(":")
        out.append(
            Socket(_address(host), int(port, 16), int(fields[7]), int(fields[9]))
        )
    return out


def outside(addr: str) -> bool:
    """True when `addr` is reachable from another host: not loopback,
    mapped IPv4 included."""
    ip = ipaddress.ip_address(addr)
    if isinstance(ip, ipaddress.IPv6Address) and ip.ipv4_mapped is not None:
        ip = ip.ipv4_mapped
    return not ip.is_loopback


def listeners(
    sockets: Iterable[Socket],
    user: Callable[[int], str],
    owners: Mapping[int, str],
) -> list[Listener]:
    """Sockets reachable from outside, one line each; the two wildcards
    of one port and one owner make one line, `*`. `owners`: inode to
    process name."""
    groups: dict[tuple[int, int, str | None], set[str]] = {}
    for sock in sockets:
        if outside(sock.addr):
            key = (sock.port, sock.uid, owners.get(sock.inode))
            groups.setdefault(key, set()).add(sock.addr)
    out = set()
    for (port, uid, process), addrs in groups.items():
        if all(w in addrs for w in WILD):
            addrs = (addrs - set(WILD)) | {"*"}
        out |= {Listener(a, port, user(uid), process) for a in addrs}
    return sorted(out, key=lambda x: (x.port, x.addr != "*", x.addr, x.user))


def _user(uid: int) -> str:
    try:
        return pwd.getpwuid(uid).pw_name
    except KeyError:
        return str(uid)


def _owners(inodes: set[int]) -> dict[int, str]:
    """Inode to process name, for the sockets found in the readable
    `/proc/<pid>/fd`: the user's own processes, all of them for root."""
    out: dict[int, str] = {}
    try:
        pids = [e.name for e in os.scandir(PROC) if e.name.isdigit()]
    except OSError:
        return out
    for pid in pids:
        if len(out) == len(inodes):
            break
        try:
            fds = list(os.scandir(PROC / pid / "fd"))
        except OSError:
            continue
        for fd in fds:
            try:
                link = os.readlink(fd.path)
            except OSError:
                continue
            if link.startswith("socket:[") and (n := int(link[8:-1])) in inodes:
                out.setdefault(n, _read(PROC / pid / "comm").strip() or pid)
    return out


def read_listeners() -> list[Listener]:
    sockets = parse_tcp(_read(PROC / "net/tcp")) + parse_tcp(_read(PROC / "net/tcp6"))
    wanted = {s.inode for s in sockets if outside(s.addr) and s.inode}
    return listeners(sockets, _user, _owners(wanted))


# disk-space

PSEUDO = {"squashfs", "iso9660", "udf"}  # read-only images


@dataclass(frozen=True)
class Mount:
    device: str
    point: str
    fstype: str


@dataclass(frozen=True)
class Disk:
    """Bytes. `free`: what an unprivileged user can still write."""

    point: str
    device: str
    size: int
    used: int
    free: int

    @property
    def share(self) -> float:
        """Used, as `df` counts it: of what is used and free to users."""
        room = self.used + self.free
        return self.used / room if room else 0.0


def _unescape(field: str) -> str:
    return re.sub(r"\\([0-7]{3})", lambda m: chr(int(m[1], 8)), field)


def parse_mounts(text: str) -> list[Mount]:
    """Filesystems of block devices of `/proc/mounts` (not loop devices,
    not images); one a device, at its shortest mount point."""
    best: dict[str, Mount] = {}
    for line in text.splitlines():
        fields = line.split()
        if len(fields) < 3:
            continue
        device, point, fstype = (_unescape(f) for f in fields[:3])
        if not device.startswith("/dev/") or device.startswith("/dev/loop"):
            continue
        if fstype in PSEUDO:
            continue
        mount = Mount(device, point, fstype)
        if device not in best or len(point) < len(best[device].point):
            best[device] = mount
    return sorted(best.values(), key=lambda m: m.point)


def disk(mount: Mount, vfs: os.statvfs_result) -> Disk:
    unit = vfs.f_frsize or vfs.f_bsize
    return Disk(
        mount.point,
        mount.device,
        vfs.f_blocks * unit,
        (vfs.f_blocks - vfs.f_bfree) * unit,
        vfs.f_bavail * unit,
    )


def read_disks() -> list[Disk]:
    """One line a filesystem: two mounts of one (a bind mount, a device
    under two names) are one, its first by mount point."""
    out, seen = [], set()
    for mount in parse_mounts(_read(PROC / "mounts")):
        try:
            dev = os.stat(mount.point).st_dev
            vfs = os.statvfs(mount.point)
        except OSError:
            continue
        if dev not in seen and vfs.f_blocks:
            seen.add(dev)
            out.append(disk(mount, vfs))
    return out
