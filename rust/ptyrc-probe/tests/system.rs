// SPDX-License-Identifier: GPL-3.0-or-later
//! The system parsers on the fixtures of `test_monitor.py`; the sampler
//! on a `/proc` of files, and on the machine.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use ptyrc_probe::system::parse::*;
use ptyrc_probe::system::{self, Ask, Part, Paths, Sampler};
use serde_json::{Value, json};

const STAT: &str = "cpu  100 0 100 800 0 0 0 0 0 0
cpu0 50 0 50 400 0 0 0 0 0 0
cpu1 50 0 50 400 0 0 0 0 0 0
intr 1 2 3
";
const STAT2: &str = "cpu  250 0 150 900 100 0 0 0 0 0
cpu0 175 0 75 450 50 0 0 0 0 0
cpu1 75 0 75 450 50 0 0 0 0 0
";
const MEMINFO: &str = "MemTotal:       16000000 kB
MemFree:         2000000 kB
MemAvailable:    6000000 kB
SwapTotal:       8000000 kB
SwapFree:        6000000 kB
";
const NETDEV: &str = "Inter-|   Receive                            |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes ...
    lo: 5000 10 0 0 0 0 0 0 5000 10 0 0 0 0 0 0
  eth0: 1000 10 0 0 0 0 0 0 2000 10 0 0 0 0 0 0
";

fn pidstat(pid: i64, name: &str, ticks: u64, rss: u64, tty: i64) -> String {
    // pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt
    // majflt cmajflt utime stime cutime cstime prio nice threads it start
    // vsize rss ...
    format!(
        "{pid} ({name}) S 1 {pid} {pid} {tty} {} 0 0 0 0 0 {ticks} 0 0 0 20 0 1 0 100 1000 {rss} 0 0",
        pid + 1
    )
}

fn sample(at: f64, stat: &str, rx: u64, procs: &[(i64, u64)]) -> Sample {
    Sample {
        at,
        cpu: parse_stat(stat),
        net: (rx, rx / 2),
        procs: procs
            .iter()
            .map(|(p, t)| {
                (
                    *p,
                    parse_pidstat(&pidstat(*p, &format!("p{p}"), *t, 1000, 0)).unwrap(),
                )
            })
            .collect(),
        memory: parse_meminfo(MEMINFO),
        load: parse_loadavg("0.52 0.48 0.40 1/200 300"),
        uptime: parse_uptime("277000.5 1000.0"),
    }
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9 * b.abs().max(1.0)
}

#[test]
fn parse_small_files() {
    assert_eq!(parse_stat(STAT), [(800, 1000), (400, 500), (400, 500)]);
    let mem = parse_meminfo(MEMINFO);
    assert_eq!(mem.total, 16000000 * 1024);
    assert_eq!(mem.total - mem.available, 10000000 * 1024);
    assert_eq!(mem.swap_total - mem.swap_free, 2000000 * 1024);
    let old = parse_meminfo("MemTotal: 10 kB\nMemFree: 2 kB\nCached: 3 kB\n");
    assert_eq!(old.available, 5 * 1024);
    assert_eq!(parse_loadavg("1.5 0.5 0.25 2/100 9"), [1.5, 0.5, 0.25]);
    assert_eq!(parse_loadavg(""), [0.0; 3]);
    assert_eq!(parse_uptime("12.5 3.0"), 12.5);
    let net = parse_netdev(NETDEV);
    assert_eq!(net.len(), 2);
    assert_eq!((net["lo"], net["eth0"]), ((5000, 5000), (1000, 2000)));
}

#[test]
fn parse_pidstat_fixture() {
    let stat = parse_pidstat(&pidstat(42, "Web (Content) 2", 70, 9, 34817)).unwrap();
    assert_eq!(
        stat,
        PidStat {
            pid: 42,
            name: "Web (Content) 2".into(),
            tty: 34817,
            tpgid: 43,
            ticks: 70,
            rss: 9
        }
    );
    assert_eq!(parse_pidstat("garbage"), None);
    assert_eq!(parse_pidstat("1 (x) S 1"), None);
}

#[test]
fn glance_fixture() {
    let g = glance(
        &sample(
            10.0,
            STAT,
            1000,
            &[(1, 0), (2, 0), (3, 10), (5, 0), (6, 0), (7, 0)],
        ),
        &sample(
            12.0,
            STAT2,
            841000,
            &[(1, 300), (2, 20), (3, 10), (4, 99), (5, 4), (6, 2), (7, 1)],
        ),
        100,
        4096,
        5,
    );
    assert!(approx(g.cpu, 1.0 - 200.0 / 400.0));
    assert!(approx(g.cores[0], 1.0 - 100.0 / 250.0) && approx(g.cores[1], 1.0 - 100.0 / 150.0));
    assert!(approx(g.rx, 420000.0) && approx(g.tx, 210000.0));
    // Shares of one core over the 2 s; a process born meanwhile is not counted.
    let top: Vec<_> = g.top.iter().map(|p| (p.pid, p.cpu)).collect();
    let want = [(1, 1.5), (2, 0.1), (5, 0.02), (6, 0.01), (7, 0.005)];
    assert_eq!(top.len(), 5);
    for ((p, c), (wp, wc)) in top.iter().zip(want) {
        assert!(*p == wp && approx(*c, wc), "{top:?}");
    }
    assert_eq!(g.top[0].mem, 1000 * 4096);
    let empty = glance(
        &Sample::default(),
        &Sample {
            at: 1.0,
            ..Sample::default()
        },
        100,
        4096,
        5,
    );
    assert_eq!((empty.cpu, empty.cores.len(), empty.top.len()), (0.0, 0, 0));
}

fn utmp(kind: i16, pid: i32, line: &str, user: &str, host: &str, sec: i32) -> Vec<u8> {
    let mut r = vec![0u8; UTMP];
    r[0..2].copy_from_slice(&kind.to_le_bytes());
    r[4..8].copy_from_slice(&pid.to_le_bytes());
    r[8..8 + line.len()].copy_from_slice(line.as_bytes());
    r[44..44 + user.len()].copy_from_slice(user.as_bytes());
    r[76..76 + host.len()].copy_from_slice(host.as_bytes());
    r[340..344].copy_from_slice(&sec.to_le_bytes());
    r[344..348].copy_from_slice(&500000i32.to_le_bytes());
    r
}

#[test]
fn parse_utmp_fixture() {
    let data = [
        utmp(2, 0, "~", "reboot", "6.1.0", 100), // BOOT_TIME
        utmp(7, 10, "pts/0", "alice", "192.0.2.1", 1000),
        utmp(8, 11, "pts/1", "", "", 1001), // DEAD_PROCESS
        utmp(7, 12, "tty1", "bob", "", 2000),
        vec![0; 100], // a torn record
    ]
    .concat();
    assert_eq!(
        parse_utmp(&data),
        [
            Session::new("alice", "pts/0", "192.0.2.1", 1000.5, 10),
            Session::new("bob", "tty1", "", 2000.5, 12)
        ]
    );
    assert!(parse_utmp(b"").is_empty());
}

#[test]
fn parse_session_fixture() {
    let text = "# private\nUSER=alice\nCLASS=user\nTTY=pts/3\nREMOTE_HOST=h.example\n\
                REALTIME=1700000000500000\nLEADER=77\n";
    assert_eq!(
        parse_session(text),
        Some(Session::new(
            "alice",
            "pts/3",
            "h.example",
            1700000000.5,
            77
        ))
    );
    assert_eq!(
        parse_session(&text.replace("CLASS=user", "CLASS=manager")),
        None
    );
    assert_eq!(parse_session(""), None);
}

const NOW: f64 = 1_800_000_000.0;

fn session(
    user: &str,
    tty: &str,
    host: &str,
    login: f64,
    pid: i64,
    idle: Option<f64>,
    what: Option<&str>,
) -> Session {
    Session {
        idle,
        what: what.map(String::from),
        ..Session::new(user, tty, host, login, pid)
    }
}

#[test]
fn seats_fixture() {
    let origins: Vec<_> = ["", ":0", "tmux(9).%3", "screen", "h", "tmuxy"]
        .iter()
        .map(|h| origin(h))
        .collect();
    assert_eq!(origins, ["local", "local", "tmux", "screen", "h", "tmuxy"]);
    let sessions = [
        session(
            "alice",
            "pts/1",
            "tmux(99).%1",
            NOW - 7200.0,
            99,
            Some(3.0),
            Some("vim"),
        ),
        session(
            "alice",
            "pts/2",
            "tmux(99).%2",
            NOW - 3600.0,
            99,
            Some(400.0),
            Some("zsh"),
        ),
        session(
            "bob",
            "pts/5",
            "203.0.113.70",
            NOW - 600.0,
            7,
            Some(30.0),
            Some("htop"),
        ),
        session(
            "carol-the-admin",
            "pts/9",
            "a-very-long-host.example.org",
            NOW - 9e5,
            8,
            None,
            None,
        ),
        session(
            "root",
            "tty1",
            "",
            NOW - 86400.0 * 3.0,
            9,
            Some(86400.0 * 2.0),
            Some("login"),
        ),
    ];
    let seats = seats(&sessions);
    let got: Vec<_> = seats
        .iter()
        .map(|s| (s.user.as_str(), s.origin.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("bob", "203.0.113.70"),
            ("alice", "tmux"),
            ("root", "local"),
            ("carol-the-admin", "a-very-long-host.example.org"),
        ]
    );
    let alice = &seats[1];
    assert_eq!(
        (alice.ttys.as_slice(), alice.login),
        (
            &["pts/1".to_string(), "pts/2".to_string()][..],
            NOW - 7200.0
        )
    );
    assert_eq!(
        (alice.idle, alice.what.as_deref()),
        (Some(3.0), Some("vim"))
    );
}

const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 101 1
   1: 0100007F:0277 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 102 1
   2: 0A01A8C0:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 103 1
   3: 0A01A8C0:0016 0B01A8C0:C350 01 00000000:00000000 00:00000000 00000000     0        0 104 1
   4: 00000000:1F40 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 105 1
";
const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 201 1
   1: 00000000000000000000000001000000:0277 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 202 1
   2: 0000000000000000FFFF00000100007F:1A0A 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 203 1
   3: B80D0120000000000000000001000000:01BB 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000    33        0 204 1
";

fn users(uid: u32) -> String {
    HashMap::from([(0, "root"), (33, "www-data"), (1000, "alice")])[&uid].to_string()
}

fn owners() -> HashMap<u64, String> {
    [
        (101, "sshd"),
        (201, "sshd"),
        (103, "python3"),
        (204, "nginx"),
    ]
    .into_iter()
    .map(|(i, n)| (i, n.to_string()))
    .collect()
}

fn listener(addr: &str, port: u16, user: &str, process: Option<&str>) -> Listener {
    Listener {
        addr: addr.into(),
        port,
        user: user.into(),
        process: process.map(String::from),
    }
}

#[test]
fn parse_tcp_fixture() {
    let socks: Vec<_> = parse_tcp(TCP)
        .iter()
        .map(|s| (s.addr.to_string(), s.port, s.uid, s.inode))
        .collect();
    assert_eq!(
        socks,
        [
            ("0.0.0.0".to_string(), 22, 0, 101),
            ("127.0.0.1".into(), 631, 0, 102),
            ("192.168.1.10".into(), 8080, 1000, 103),
            ("0.0.0.0".into(), 8000, 1000, 105),
        ]
    );
    let six: Vec<_> = parse_tcp(TCP6)
        .iter()
        .map(|s| (s.addr.to_string(), s.port))
        .collect();
    assert_eq!(
        six,
        [
            ("::".to_string(), 22),
            ("::1".into(), 631),
            ("::ffff:127.0.0.1".into(), 6666),
            ("2001:db8::1".into(), 443)
        ]
    );
}

#[test]
fn listeners_fixture() {
    let ip = |s: &str| s.parse::<IpAddr>().unwrap();
    for a in ["127.0.0.1", "::1", "::ffff:127.0.0.1", "127.0.1.1"] {
        assert!(!outside(ip(a)), "{a}");
    }
    for a in ["0.0.0.0", "::", "10.0.0.1"] {
        assert!(outside(ip(a)), "{a}");
    }
    let socks = [parse_tcp(TCP), parse_tcp(TCP6)].concat();
    // Loopback out; the two wildcards of sshd on 22 are one line.
    assert_eq!(
        listeners(&socks, users, &owners()),
        [
            listener("*", 22, "root", Some("sshd")),
            listener("2001:db8::1", 443, "www-data", Some("nginx")),
            listener("0.0.0.0", 8000, "alice", None),
            listener("192.168.1.10", 8080, "alice", Some("python3")),
        ]
    );
    // Not merged: two owners, or one wildcard alone.
    let socks = [
        Socket {
            addr: ip("0.0.0.0"),
            port: 80,
            uid: 0,
            inode: 1,
        },
        Socket {
            addr: ip("::"),
            port: 80,
            uid: 0,
            inode: 2,
        },
    ];
    let names = HashMap::from([(1, "a".to_string()), (2, "b".to_string())]);
    let split: Vec<_> = listeners(&socks, |u| u.to_string(), &names)
        .into_iter()
        .map(|l| (l.addr, l.process.unwrap()))
        .collect();
    assert_eq!(
        split,
        [
            ("0.0.0.0".to_string(), "a".to_string()),
            ("::".into(), "b".into())
        ]
    );
    let passwd = parse_passwd(
        "root:x:0:0::/root:/bin/sh\n# x\nalice:x:1000:1000::/home/alice:/bin/sh\nbad\n",
    );
    assert_eq!(
        (passwd[&0].as_str(), passwd[&1000].as_str(), passwd.len()),
        ("root", "alice", 2)
    );
}

const MOUNTS: &str = r"/dev/sda2 / ext4 rw,relatime 0 0
proc /proc proc rw 0 0
tmpfs /tmp tmpfs rw 0 0
/dev/sda2 /var/lib/docker/btrfs ext4 rw 0 0
overlay /var/lib/docker/overlay2/x/merged overlay rw 0 0
/dev/loop3 /snap/core/1 squashfs ro 0 0
/dev/sda1 /boot vfat rw 0 0
/dev/sdb1 /mnt/my\040disk ext4 rw 0 0
/dev/sr0 /media/cd iso9660 ro 0 0
";

#[test]
fn mounts_fixture() {
    let m = |d: &str, p: &str, f: &str| Mount {
        device: d.into(),
        point: p.into(),
        fstype: f.into(),
    };
    assert_eq!(
        parse_mounts(MOUNTS),
        [
            m("/dev/sda2", "/", "ext4"),
            m("/dev/sda1", "/boot", "vfat"),
            m("/dev/sdb1", "/mnt/my disk", "ext4")
        ]
    );
    let d = disk(&m("/dev/sda2", "/", "ext4"), 4096, 4096, 1000, 300, 250);
    assert_eq!((d.size, d.used, d.free), (4096000, 700 * 4096, 250 * 4096));
    assert_eq!(disk(&m("x", "/", "ext4"), 512, 0, 2, 1, 1).size, 1024);
}

#[test]
fn ask_from_json() {
    assert_eq!(Ask::from_json(&Value::Null), Ok(Ask::default()));
    let ask = Ask::from_json(&json!({"parts": ["disks", "glance"], "top": 99})).unwrap();
    assert_eq!(
        (ask.parts.as_slice(), ask.top),
        (&[Part::Disks, Part::Glance][..], 16)
    );
    assert!(Ask::from_json(&json!({"parts": ["cpu"]})).is_err());
    assert!(Ask::from_json(&json!({"top": -1})).is_err());
    assert_eq!(
        system::not_held(&Ask::default()),
        json!({"glance": {}, "logins": {}, "listeners": [], "disks": [], "degree": "none"})
    );
}

/// A `/proc` of files, and the rest, under `CARGO_TARGET_TMPDIR`.
fn fake(name: &str) -> (PathBuf, Paths) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("probe-system")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["proc/net", "sys/devices/virtual/net/lo", "dev", "sessions"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let paths = Paths {
        proc: dir.join("proc"),
        sys: dir.join("sys"),
        dev: dir.join("dev"),
        utmp: dir.join("none"),
        sessions: dir.join("sessions"),
        passwd: dir.join("passwd"),
    };
    (dir, paths)
}

fn put(path: PathBuf, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn sampler_glance() {
    let (dir, paths) = fake("glance");
    let p = &paths.proc;
    put(p.join("stat"), STAT);
    put(p.join("meminfo"), MEMINFO);
    put(p.join("net/dev"), NETDEV);
    put(p.join("loadavg"), "0.52 0.48 0.40 1/200 300");
    put(p.join("uptime"), "277000.5 1000.0");
    let hz = rustix::param::clock_ticks_per_second();
    put(p.join("1/stat"), &pidstat(1, "init", 0, 10, 0));
    let mut s = Sampler::with_paths(paths.clone());
    let ask = Ask {
        parts: vec![Part::Glance],
        top: 5,
    };
    let first = s.probe(&ask, NOW);
    let g = &first["glance"];
    assert_eq!(
        (g["cpu"].as_f64(), g["cores"].clone(), g["top"].clone()),
        (Some(0.0), json!([0.0, 0.0]), json!([]))
    );
    assert_eq!(g["memory"]["total"], 16000000u64 * 1024);
    assert_eq!(g["load"], json!([0.52, 0.48, 0.40]));
    put(p.join("stat"), STAT2);
    put(
        p.join("net/dev"),
        &NETDEV.replace("eth0: 1000", "eth0: 841000"),
    );
    put(p.join("1/stat"), &pidstat(1, "init", 3 * hz, 10, 0));
    // Within a second: the last answer again.
    assert_eq!(s.probe(&ask, NOW + 0.5), first);
    let second = s.probe(&ask, NOW + 2.0);
    let g = &second["glance"];
    assert!(approx(g["cpu"].as_f64().unwrap(), 0.5));
    assert!(
        approx(g["rx"].as_f64().unwrap(), 420000.0),
        "lo not counted"
    );
    assert_eq!(g["top"][0]["name"], "init");
    assert!(approx(g["top"][0]["cpu"].as_f64().unwrap(), 1.5));
    assert_eq!(
        s.probe(
            &Ask {
                top: 0,
                ..ask.clone()
            },
            NOW + 2.5
        )["glance"]["top"],
        json!([])
    );
    // A clock gone back: zeros again.
    assert_eq!(s.probe(&ask, NOW - 10.0)["glance"]["cpu"], 0.0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sampler_logins() {
    let (dir, paths) = fake("logins");
    // No utmp: the files of logind; `.ref` files are not sessions.
    put(
        paths.sessions.join("3"),
        "USER=alice\nCLASS=user\nREALTIME=1000000\n",
    );
    put(paths.sessions.join("3.ref"), "USER=x\nCLASS=user\n");
    put(
        paths.sessions.join("4"),
        "USER=bob\nCLASS=user\nLEADER=4242\nREALTIME=2000000\n",
    );
    put(
        paths.sessions.join("5"),
        "USER=eve\nCLASS=user\nTTY=../../etc/passwd\nREALTIME=3000000\n",
    );
    let mut s = Sampler::with_paths(paths.clone());
    let got = s.logins(NOW);
    assert_eq!(
        got,
        json!({"now": NOW, "seats": [
            {"user": "eve", "origin": "local", "ttys": ["../../etc/passwd"], "login": 3.0, "idle": null, "what": null},
            {"user": "alice", "origin": "local", "ttys": [""], "login": 1.0, "idle": null, "what": null},
        ]}),
        "bob's leader is gone; eve's tty not followed"
    );
    // utmp first; a tty's idle time and foreground process.
    let tty = paths.dev.join("pts/1");
    put(tty.clone(), "");
    let f = std::fs::File::options().write(true).open(&tty).unwrap();
    f.set_times(
        std::fs::FileTimes::new()
            .set_accessed(std::time::UNIX_EPOCH + std::time::Duration::from_secs(NOW as u64 - 30)),
    )
    .unwrap();
    put(paths.proc.join("77/stat"), &pidstat(77, "zsh", 0, 1, 0));
    std::fs::write(
        dir.join("utmp"),
        [utmp(7, 77, "pts/1", "carol", "tmux(1).%0", 5)].concat(),
    )
    .unwrap();
    let mut s = Sampler::with_paths(Paths {
        utmp: dir.join("utmp"),
        ..paths.clone()
    });
    let got = s.logins(NOW);
    assert_eq!(got["seats"][0]["user"], "carol");
    assert_eq!(got["seats"][0]["origin"], "tmux");
    assert!(approx(got["seats"][0]["idle"].as_f64().unwrap(), 30.0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sampler_listeners() {
    let (dir, paths) = fake("listeners");
    put(paths.proc.join("net/tcp"), TCP);
    put(
        paths.passwd.clone(),
        "root:x:0:0::/:/bin/sh\nalice:x:1000:1000::/:/bin/sh\n",
    );
    std::fs::create_dir_all(paths.proc.join("50/fd")).unwrap();
    std::os::unix::fs::symlink("socket:[101]", paths.proc.join("50/fd/3")).unwrap();
    std::os::unix::fs::symlink("/dev/null", paths.proc.join("50/fd/0")).unwrap();
    put(paths.proc.join("50/comm"), "sshd\n");
    let mut s = Sampler::with_paths(paths.clone());
    let names = |v: Value| -> Vec<(u64, Value)> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|l| (l["port"].as_u64().unwrap(), l["process"].clone()))
            .collect()
    };
    let first = s.listeners(NOW);
    assert_eq!(
        names(first.clone()),
        [
            (22, json!("sshd")),
            (8000, Value::Null),
            (8080, Value::Null)
        ]
    );
    assert_eq!(
        first[0],
        json!({"addr": "0.0.0.0", "port": 22, "user": "root", "process": "sshd"})
    );
    // The same sockets: the owners kept for `OWNED` seconds.
    put(paths.proc.join("50/comm"), "other\n");
    assert_eq!(s.listeners(NOW + 1.0), first);
    assert_eq!(names(s.listeners(NOW + system::OWNED))[0].1, "other");
    // Other sockets: scanned again at once.
    put(paths.proc.join("50/comm"), "third\n");
    put(paths.proc.join("net/tcp"), &TCP.replace(" 105 1", " 106 1"));
    assert_eq!(names(s.listeners(NOW + system::OWNED + 1.0))[0].1, "third");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(target_os = "linux")]
#[test]
fn the_machine() {
    let mut s = Sampler::new();
    let got = s.probe(&Ask::default(), NOW);
    let g = &got["glance"];
    assert!(g["memory"]["total"].as_u64().unwrap() > 0 && g["uptime"].as_f64().unwrap() > 0.0);
    assert!(!g["cores"].as_array().unwrap().is_empty());
    let got = s.probe(&Ask::default(), NOW + 1.5);
    let g = &got["glance"];
    assert!((0.0..=1.0).contains(&g["cpu"].as_f64().unwrap()));
    assert!(g["top"].as_array().unwrap().len() <= system::TOP);
    let disks = got["disks"].as_array().unwrap();
    let points: std::collections::BTreeSet<_> =
        disks.iter().map(|d| d["point"].as_str().unwrap()).collect();
    assert_eq!(points.len(), disks.len());
    for d in disks {
        assert!(d["used"].as_u64().unwrap() <= d["size"].as_u64().unwrap());
    }
    for l in got["listeners"].as_array().unwrap() {
        let a = l["addr"].as_str().unwrap();
        assert!(a == "*" || outside(a.parse().unwrap()), "{a}");
    }
    for seat in got["logins"]["seats"].as_array().unwrap() {
        assert!(!seat["user"].as_str().unwrap().is_empty());
    }
}
