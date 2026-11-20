// SPDX-License-Identifier: GPL-3.0-or-later
//! Parsers of `/proc`, utmp, `/run/systemd/sessions`, `/etc/passwd`;
//! shares and rates from two samples. No I/O, no clock.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Bytes. `available`: what can be had without swapping.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

/// Of `/proc/<pid>/stat`. `tty`: the device number of its terminal, 0
/// for none; `tpgid`: the foreground process group of that terminal;
/// `ticks`: user and system time; `rss`: pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidStat {
    pub pid: i64,
    pub name: String,
    pub tty: i64,
    pub tpgid: i64,
    pub ticks: u64,
    pub rss: u64,
}

/// A process of the top. `cpu`: share of one core; `mem`: bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: i64,
    pub name: String,
    pub cpu: f64,
    pub mem: u64,
}

/// The counters at one time. `cpu`: `(idle, total)` ticks, all cores
/// first, then each core; `net`: received and sent bytes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub at: f64,
    pub cpu: Vec<(u64, u64)>,
    pub net: (u64, u64),
    pub procs: BTreeMap<i64, PidStat>,
    pub memory: Memory,
    pub load: [f64; 3],
    pub uptime: f64,
}

/// `cpu`, `cores`: shares in [0, 1]; `rx`, `tx`: bytes a second.
#[derive(Debug, Clone, PartialEq)]
pub struct Glance {
    pub cpu: f64,
    pub cores: Vec<f64>,
    pub load: [f64; 3],
    pub memory: Memory,
    pub uptime: f64,
    pub rx: f64,
    pub tx: f64,
    pub top: Vec<Proc>,
}

fn int(s: &str) -> u64 {
    s.parse().unwrap_or(0)
}

/// `(idle, total)` ticks of the `cpu` lines of `/proc/stat`. Idle counts
/// iowait; the total is the first eight fields (guest time is in user
/// time already).
pub fn parse_stat(text: &str) -> Vec<(u64, u64)> {
    text.lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            let f: Vec<u64> = l.split_whitespace().skip(1).take(8).map(int).collect();
            (f.iter().skip(3).take(2).sum(), f.iter().sum())
        })
        .collect()
}

pub fn parse_meminfo(text: &str) -> Memory {
    let mut kib = HashMap::new();
    for line in text.lines() {
        if let Some((key, rest)) = line.split_once(':')
            && let Some(v) = rest.split_whitespace().next()
        {
            kib.insert(key, int(v) * 1024);
        }
    }
    let get = |k: &str| kib.get(k).copied().unwrap_or(0);
    // Kernels before 3.14 have no `MemAvailable`.
    let available = kib
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| get("MemFree") + get("Buffers") + get("Cached"));
    Memory {
        total: get("MemTotal"),
        available,
        swap_total: get("SwapTotal"),
        swap_free: get("SwapFree"),
    }
}

pub fn parse_loadavg(text: &str) -> [f64; 3] {
    let f: Vec<f64> = text
        .split_whitespace()
        .take(3)
        .map(|v| v.parse().unwrap_or(0.0))
        .collect();
    match f[..] {
        [a, b, c] => [a, b, c],
        _ => [0.0; 3],
    }
}

pub fn parse_uptime(text: &str) -> f64 {
    text.split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

/// Received and sent bytes of each interface of `/proc/net/dev`.
pub fn parse_netdev(text: &str) -> BTreeMap<String, (u64, u64)> {
    let mut out = BTreeMap::new();
    for line in text.lines().skip(2) {
        if let Some((name, data)) = line.split_once(':') {
            let f: Vec<&str> = data.split_whitespace().collect();
            if f.len() >= 9 {
                out.insert(name.trim().to_string(), (int(f[0]), int(f[8])));
            }
        }
    }
    out
}

/// `/proc/<pid>/stat`. The name is between the first `(` and the last
/// `)`: it may hold both, and spaces.
pub fn parse_pidstat(text: &str) -> Option<PidStat> {
    let (left, right) = (text.find('(')?, text.rfind(')')?);
    if right < left {
        return None;
    }
    let rest: Vec<&str> = text.get(right + 2..)?.split_whitespace().collect();
    if rest.len() < 22 {
        return None;
    }
    // From field 3 on: state ppid pgrp session tty_nr tpgid ... utime stime.
    Some(PidStat {
        pid: text[..left].trim().parse().ok()?,
        name: text[left + 1..right].to_string(),
        tty: rest[4].parse().ok()?,
        tpgid: rest[5].parse().ok()?,
        ticks: int(rest[11]) + int(rest[12]),
        rss: int(rest[21]),
    })
}

fn share(before: (u64, u64), after: (u64, u64)) -> f64 {
    let idle = after.0 as f64 - before.0 as f64;
    let total = after.1 as f64 - before.1 as f64;
    if total <= 0.0 {
        return 0.0;
    }
    (1.0 - idle / total).clamp(0.0, 1.0)
}

/// What changed from `before` to `after`, with the state at `after`;
/// `top` processes by share, those of `before` only.
pub fn glance(before: &Sample, after: &Sample, hz: u64, page: u64, top: usize) -> Glance {
    let seconds = (after.at - before.at).max(1e-6);
    let shares: Vec<f64> = before
        .cpu
        .iter()
        .zip(&after.cpu)
        .map(|(b, a)| share(*b, *a))
        .collect();
    let mut procs: Vec<Proc> = after
        .procs
        .iter()
        .filter_map(|(pid, now)| {
            let then = before.procs.get(pid)?;
            Some(Proc {
                pid: *pid,
                name: now.name.clone(),
                cpu: now.ticks.saturating_sub(then.ticks) as f64 / hz.max(1) as f64 / seconds,
                mem: now.rss * page,
            })
        })
        .collect();
    procs.sort_by(|a, b| {
        b.cpu
            .total_cmp(&a.cpu)
            .then(b.mem.cmp(&a.mem))
            .then(a.pid.cmp(&b.pid))
    });
    procs.truncate(top);
    Glance {
        cpu: shares.first().copied().unwrap_or(0.0),
        cores: shares.iter().skip(1).copied().collect(),
        load: after.load,
        memory: after.memory,
        uptime: after.uptime,
        rx: after.net.0.saturating_sub(before.net.0) as f64 / seconds,
        tx: after.net.1.saturating_sub(before.net.1) as f64 / seconds,
        top: procs,
    }
}

// Logins

/// A glibc `struct utmp` of 64-bit Linux, in bytes; `ut_tv` is two
/// int32 there too.
pub const UTMP: usize = 384;
const USER_PROCESS: i16 = 7;

/// A login. `host`: as the login program wrote it (`tmux(1).%3`, `:0`,
/// a name, an address, or empty); `login`: epoch seconds; `idle`:
/// seconds since the tty was read; `what`: the foreground process of
/// the tty.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub user: String,
    pub tty: String,
    pub host: String,
    pub login: f64,
    pub pid: i64,
    pub idle: Option<f64>,
    pub what: Option<String>,
}

impl Session {
    pub fn new(user: &str, tty: &str, host: &str, login: f64, pid: i64) -> Session {
        Session {
            user: user.into(),
            tty: tty.into(),
            host: host.into(),
            login,
            pid,
            idle: None,
            what: None,
        }
    }
}

/// The sessions of one user from one origin. `login`: the earliest;
/// `idle`, `what`: of the least idle.
#[derive(Debug, Clone, PartialEq)]
pub struct Seat {
    pub user: String,
    pub origin: String,
    pub ttys: Vec<String>,
    pub login: f64,
    pub idle: Option<f64>,
    pub what: Option<String>,
}

fn cstr(raw: &[u8]) -> String {
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// The `USER_PROCESS` records of a utmp file; a torn last record is
/// left.
pub fn parse_utmp(data: &[u8]) -> Vec<Session> {
    let mut out = Vec::new();
    for r in data.chunks_exact(UTMP) {
        let kind = i16::from_le_bytes([r[0], r[1]]);
        let user = cstr(&r[44..76]);
        if kind == USER_PROCESS && !user.is_empty() {
            let login = i32_at(r, 340) as f64 + i32_at(r, 344) as f64 / 1e6;
            out.push(Session::new(
                &user,
                &cstr(&r[8..40]),
                &cstr(&r[76..332]),
                login,
                i32_at(r, 4) as i64,
            ));
        }
    }
    out
}

/// A file of `/run/systemd/sessions/`: sessions of class `user` only
/// (not the service manager of a user).
pub fn parse_session(text: &str) -> Option<Session> {
    let kv: HashMap<&str, &str> = text.lines().filter_map(|l| l.split_once('=')).collect();
    let user = kv.get("USER").copied().unwrap_or("");
    if kv.get("CLASS") != Some(&"user") || user.is_empty() {
        return None;
    }
    let get = |k: &str| kv.get(k).copied().unwrap_or("");
    Some(Session::new(
        user,
        get("TTY"),
        get("REMOTE_HOST"),
        int(get("REALTIME")) as f64 / 1e6,
        get("LEADER").parse().unwrap_or(0),
    ))
}

/// Where a session comes from, in short: `tmux`, `screen`, `local` (a
/// console or a display), else the host as given.
pub fn origin(host: &str) -> String {
    if host.is_empty() || host.starts_with(':') {
        return "local".into();
    }
    for word in ["tmux", "screen"] {
        if let Some(rest) = host.strip_prefix(word)
            && !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_')
        {
            return word.into();
        }
    }
    host.into()
}

/// Sessions grouped by user and origin, the most recent login first.
pub fn seats(sessions: &[Session]) -> Vec<Seat> {
    let mut groups: Vec<((String, String), Vec<&Session>)> = Vec::new();
    for one in sessions {
        let key = (one.user.clone(), origin(&one.host));
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(one),
            None => groups.push((key, vec![one])),
        }
    }
    let mut out: Vec<(f64, Seat)> = groups
        .into_iter()
        .map(|((user, origin), group)| {
            let known = group.iter().filter(|s| s.idle.is_some());
            let least = known
                .min_by(|a, b| a.idle.unwrap_or(0.0).total_cmp(&b.idle.unwrap_or(0.0)))
                .or(group.last())
                .copied();
            let latest = group.iter().map(|s| s.login).fold(f64::MIN, f64::max);
            let seat = Seat {
                user,
                origin,
                ttys: group.iter().map(|s| s.tty.clone()).collect(),
                login: group.iter().map(|s| s.login).fold(f64::MAX, f64::min),
                idle: least.and_then(|s| s.idle),
                what: least.and_then(|s| s.what.clone()),
            };
            (latest, seat)
        })
        .collect();
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.into_iter().map(|(_, s)| s).collect()
}

// Listeners

const LISTEN: &str = "0A";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socket {
    pub addr: IpAddr,
    pub port: u16,
    pub uid: u32,
    pub inode: u64,
}

/// `addr`: `*` for both wildcards, else as written; `process`: none when
/// its socket is in no readable `/proc/<pid>/fd`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listener {
    pub addr: String,
    pub port: u16,
    pub user: String,
    pub process: Option<String>,
}

/// An address of `/proc/net/tcp*`: 32-bit words in host order.
fn address(hex: &str) -> Option<IpAddr> {
    let word = |i: usize| {
        let w = u32::from_str_radix(hex.get(i * 8..i * 8 + 8)?, 16).ok()?;
        Some(w.to_ne_bytes())
    };
    match hex.len() {
        8 => Some(IpAddr::V4(Ipv4Addr::from(word(0)?))),
        32 => {
            let mut b = [0u8; 16];
            for i in 0..4 {
                b[i * 4..i * 4 + 4].copy_from_slice(&word(i)?);
            }
            Some(IpAddr::V6(Ipv6Addr::from(b)))
        }
        _ => None,
    }
}

/// The listening sockets of `/proc/net/tcp` or `/proc/net/tcp6`.
pub fn parse_tcp(text: &str) -> Vec<Socket> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 || f[3] != LISTEN {
            continue;
        }
        let Some((host, port)) = f[1].split_once(':') else {
            continue;
        };
        if let (Some(addr), Ok(port)) = (address(host), u16::from_str_radix(port, 16)) {
            out.push(Socket {
                addr,
                port,
                uid: f[7].parse().unwrap_or(0),
                inode: int(f[9]),
            });
        }
    }
    out
}

/// Reachable from another host: not loopback, mapped IPv4 included.
pub fn outside(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => !v4.is_loopback(),
            None => !v6.is_loopback(),
        },
        IpAddr::V4(v4) => !v4.is_loopback(),
    }
}

/// Sockets reachable from outside, one line each; the two wildcards of
/// one port and one owner make one line, `*`. `owners`: inode to
/// process name.
pub fn listeners(
    sockets: &[Socket],
    user: impl Fn(u32) -> String,
    owners: &HashMap<u64, String>,
) -> Vec<Listener> {
    let mut groups: BTreeMap<(u16, u32, Option<String>), BTreeSet<String>> = BTreeMap::new();
    for s in sockets.iter().filter(|s| outside(s.addr)) {
        let key = (s.port, s.uid, owners.get(&s.inode).cloned());
        groups.entry(key).or_default().insert(s.addr.to_string());
    }
    let mut out = BTreeSet::new();
    for ((port, uid, process), mut addrs) in groups {
        if addrs.contains("0.0.0.0") && addrs.contains("::") {
            addrs.remove("0.0.0.0");
            addrs.remove("::");
            addrs.insert("*".into());
        }
        for addr in addrs {
            out.insert(Listener {
                addr,
                port,
                user: user(uid),
                process: process.clone(),
            });
        }
    }
    let mut out: Vec<Listener> = out.into_iter().collect();
    out.sort_by(|a, b| {
        (a.port, a.addr != "*", &a.addr, &a.user).cmp(&(b.port, b.addr != "*", &b.addr, &b.user))
    });
    out
}

/// Uid to name, of `/etc/passwd`.
pub fn parse_passwd(text: &str) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.splitn(4, ':').collect();
        if let [name, _, uid, _] = f[..]
            && let Ok(uid) = uid.parse()
        {
            out.entry(uid).or_insert_with(|| name.to_string());
        }
    }
    out
}

// Disks

/// Read-only images.
const IMAGES: [&str; 3] = ["squashfs", "iso9660", "udf"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub device: String,
    pub point: String,
    pub fstype: String,
}

/// Bytes. `free`: what an unprivileged user can still write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub point: String,
    pub device: String,
    pub size: u64,
    pub used: u64,
    pub free: u64,
}

/// `\NNN` octal escapes of `/proc/mounts`.
fn unescape(field: &str) -> String {
    let b = field.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let octal = |j: usize| b.get(j).is_some_and(|c| (b'0'..=b'7').contains(c));
        if b[i] == b'\\' && octal(i + 1) && octal(i + 2) && octal(i + 3) {
            let n = (b[i + 1] - b'0') as u32 * 64
                + (b[i + 2] - b'0') as u32 * 8
                + (b[i + 3] - b'0') as u32;
            out.push(n as u8);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Filesystems of block devices of `/proc/mounts` (not loop devices,
/// not images); one a device, at its shortest mount point.
pub fn parse_mounts(text: &str) -> Vec<Mount> {
    let mut best: BTreeMap<String, Mount> = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<String> = line.split_whitespace().take(3).map(unescape).collect();
        let [device, point, fstype] = &f[..] else {
            continue;
        };
        if !device.starts_with("/dev/") || device.starts_with("/dev/loop") {
            continue;
        }
        if IMAGES.contains(&fstype.as_str()) {
            continue;
        }
        let shorter = best.get(device).is_none_or(|m| point.len() < m.point.len());
        if shorter {
            let m = Mount {
                device: device.clone(),
                point: point.clone(),
                fstype: fstype.clone(),
            };
            best.insert(device.clone(), m);
        }
    }
    let mut out: Vec<Mount> = best.into_values().collect();
    out.sort_by(|a, b| a.point.cmp(&b.point));
    out
}

/// Of `statvfs`: `frsize` (or `bsize` when 0), `blocks`, `bfree`,
/// `bavail`.
pub fn disk(mount: &Mount, bsize: u64, frsize: u64, blocks: u64, bfree: u64, bavail: u64) -> Disk {
    let unit = if frsize > 0 { frsize } else { bsize };
    Disk {
        point: mount.point.clone(),
        device: mount.device.clone(),
        size: blocks * unit,
        used: blocks.saturating_sub(bfree) * unit,
        free: bavail * unit,
    }
}
