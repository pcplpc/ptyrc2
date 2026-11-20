// SPDX-License-Identifier: GPL-3.0-or-later
//! `probe.system`: the processor, memory, network, logins, listening
//! ports and disks, as JSON. Linux; elsewhere each part is empty and
//! `"unsupported": true`. Blocking; for the driver's worker.

pub mod parse;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use parse::{Glance, PidStat, Sample};

/// Processes of `top` by default, and at most.
pub const TOP: usize = 5;
pub const TOP_MAX: usize = 16;
/// Seconds a sample is kept: a sooner `glance` is the last one again.
pub const SAMPLE: f64 = 1.0;
/// Seconds the owners of the same listening sockets are kept.
pub const OWNED: f64 = 5.0;
/// Bytes read of one file at most.
const READ: u64 = 16 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Glance,
    Logins,
    Listeners,
    Disks,
}

impl Part {
    pub const ALL: [Part; 4] = [Part::Glance, Part::Logins, Part::Listeners, Part::Disks];

    pub fn name(self) -> &'static str {
        match self {
            Part::Glance => "glance",
            Part::Logins => "logins",
            Part::Listeners => "listeners",
            Part::Disks => "disks",
        }
    }

    pub fn parse(name: &str) -> Option<Part> {
        Part::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The part when nothing can be told.
    fn empty(self) -> Value {
        match self {
            Part::Glance | Part::Logins => json!({}),
            Part::Listeners | Part::Disks => json!([]),
        }
    }
}

/// What is asked. `top`: clamped to `TOP_MAX`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub parts: Vec<Part>,
    pub top: usize,
}

impl Default for Ask {
    fn default() -> Ask {
        Ask {
            parts: Part::ALL.to_vec(),
            top: TOP,
        }
    }
}

impl Ask {
    /// From the params of `probe.system`. Err: the rule broken, for
    /// `bad-params`.
    pub fn from_json(params: &Value) -> Result<Ask, String> {
        let mut ask = Ask::default();
        let Some(params) = params.as_object() else {
            return if params.is_null() {
                Ok(ask)
            } else {
                Err("params: an object".into())
            };
        };
        if let Some(parts) = params.get("parts") {
            let list = parts.as_array().ok_or("parts: a list of names")?;
            ask.parts.clear();
            for p in list {
                let part = p
                    .as_str()
                    .and_then(Part::parse)
                    .ok_or("parts: of glance, logins, listeners, disks")?;
                if !ask.parts.contains(&part) {
                    ask.parts.push(part);
                }
            }
        }
        if let Some(top) = params.get("top") {
            let n = top.as_u64().ok_or("top: a count, 0 to 16")?;
            ask.top = n.min(TOP_MAX as u64) as usize;
        }
        Ok(ask)
    }
}

/// Every part asked, empty, and `extra`.
fn empty(ask: &Ask, extra: (&str, Value)) -> Value {
    let mut out: Map<String, Value> = ask
        .parts
        .iter()
        .map(|p| (p.name().into(), p.empty()))
        .collect();
    out.insert(extra.0.into(), extra.1);
    out.into()
}

/// The answer when `probe-system` is not held.
pub fn not_held(ask: &Ask) -> Value {
    empty(ask, ("degree", "none".into()))
}

/// Where the files are read; `Paths::default()` the machine's.
#[derive(Debug, Clone)]
pub struct Paths {
    pub proc: PathBuf,
    pub sys: PathBuf,
    pub dev: PathBuf,
    pub utmp: PathBuf,
    pub sessions: PathBuf,
    pub passwd: PathBuf,
}

impl Default for Paths {
    fn default() -> Paths {
        Paths {
            proc: "/proc".into(),
            sys: "/sys".into(),
            dev: "/dev".into(),
            utmp: "/run/utmp".into(),
            sessions: "/run/systemd/sessions".into(),
            passwd: "/etc/passwd".into(),
        }
    }
}

/// The state kept between two answers to one app: the last two
/// samples, the last scan of the processes and of the socket owners.
#[derive(Debug, Default)]
pub struct Sampler {
    pub paths: Paths,
    samples: Option<(Option<Sample>, Sample)>,
    procs: Option<(f64, std::collections::BTreeMap<i64, PidStat>)>,
    owned: Option<(f64, BTreeSet<u64>, HashMap<u64, String>)>,
}

impl Sampler {
    pub fn new() -> Sampler {
        Sampler::default()
    }

    pub fn with_paths(paths: Paths) -> Sampler {
        Sampler {
            paths,
            ..Sampler::default()
        }
    }

    /// The answer to `probe.system` at `now` (epoch seconds). Shares
    /// are between this sample and the last; the first has zeros.
    pub fn probe(&mut self, ask: &Ask, now: f64) -> Value {
        if !cfg!(target_os = "linux") {
            return empty(ask, ("unsupported", true.into()));
        }
        let mut out = Map::new();
        for part in &ask.parts {
            let v = match part {
                Part::Glance => glance_json(&self.glance(now, ask.top)),
                Part::Logins => self.logins(now),
                Part::Listeners => self.listeners(now),
                Part::Disks => self.disks(),
            };
            out.insert(part.name().into(), v);
        }
        out.into()
    }

    fn read(&self, path: impl AsRef<std::path::Path>) -> String {
        read_text(path.as_ref())
    }

    /// Every process; the last scan when no older than `age` seconds.
    fn procs(&mut self, now: f64, age: f64) -> std::collections::BTreeMap<i64, PidStat> {
        if let Some((at, procs)) = &self.procs
            && (0.0..age).contains(&(now - at))
        {
            return procs.clone();
        }
        let mut out = std::collections::BTreeMap::new();
        for pid in pids(&self.paths.proc) {
            if let Some(stat) =
                parse::parse_pidstat(&self.read(self.paths.proc.join(pid.to_string()).join("stat")))
            {
                out.insert(stat.pid, stat);
            }
        }
        self.procs = Some((now, out.clone()));
        out
    }

    /// The counters now. Network: interfaces that are not virtual
    /// (`lo`, bridges, veth, tunnels carry traffic counted elsewhere).
    fn sample(&mut self, now: f64) -> Sample {
        let mut virtual_: BTreeSet<String> =
            std::fs::read_dir(self.paths.sys.join("devices/virtual/net"))
                .map(|d| {
                    d.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
        virtual_.insert("lo".into());
        let counters = parse::parse_netdev(&self.read(self.paths.proc.join("net/dev")));
        let net = counters
            .iter()
            .filter(|(k, _)| !virtual_.contains(*k))
            .fold((0, 0), |(rx, tx), (_, v)| (rx + v.0, tx + v.1));
        Sample {
            at: now,
            cpu: parse::parse_stat(&self.read(self.paths.proc.join("stat"))),
            net,
            procs: self.procs(now, 0.0),
            memory: parse::parse_meminfo(&self.read(self.paths.proc.join("meminfo"))),
            load: parse::parse_loadavg(&self.read(self.paths.proc.join("loadavg"))),
            uptime: parse::parse_uptime(&self.read(self.paths.proc.join("uptime"))),
        }
    }

    /// Between the last two samples; a new one when the last is
    /// `SAMPLE` seconds old, or from another time line (`now` before it).
    pub fn glance(&mut self, now: f64, top: usize) -> Glance {
        let fresh = match &self.samples {
            Some((_, last)) => !(0.0..SAMPLE).contains(&(now - last.at)),
            None => true,
        };
        if fresh {
            let next = self.sample(now);
            let prev = match self.samples.take() {
                Some((_, last)) if last.at < now => Some(last),
                _ => None,
            };
            self.samples = Some((prev, next));
        }
        let (prev, last) = self.samples.as_ref().expect("sampled");
        let (hz, page) = (
            rustix::param::clock_ticks_per_second(),
            rustix::param::page_size() as u64,
        );
        match prev {
            Some(prev) => parse::glance(prev, last, hz, page, top),
            None => {
                let first = Sample {
                    procs: Default::default(),
                    ..last.clone()
                };
                parse::glance(&first, last, hz, page, top)
            }
        }
    }

    /// Live logins: utmp records whose process exists; without utmp,
    /// the files of logind.
    pub fn logins(&mut self, now: f64) -> Value {
        let mut found = match std::fs::read(&self.paths.utmp) {
            Ok(data) => parse::parse_utmp(&data),
            Err(_) => {
                let mut names: Vec<String> = std::fs::read_dir(&self.paths.sessions)
                    .map(|d| {
                        d.flatten()
                            .map(|e| e.file_name().to_string_lossy().into_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                names
                    .iter()
                    .filter(|n| !n.ends_with(".ref"))
                    .filter_map(|n| parse::parse_session(&self.read(self.paths.sessions.join(n))))
                    .collect()
            }
        };
        found.retain(|s| s.pid <= 0 || self.paths.proc.join(s.pid.to_string()).exists());
        // `tty_nr` and `st_rdev` agree for the majors of terminals
        // (< 4096). The scan of `glance` will do.
        let groups: HashMap<u64, i64> = self
            .procs(now, 1.0)
            .values()
            .filter(|p| p.tty != 0)
            .map(|p| (p.tty as u64, p.tpgid))
            .collect();
        for one in &mut found {
            (one.idle, one.what) = self.tty(&one.tty, now, &groups);
        }
        found.sort_by(|a, b| a.login.total_cmp(&b.login));
        let seats: Vec<Value> = parse::seats(&found)
            .iter()
            .map(|s| {
                json!({"user": s.user, "origin": s.origin, "ttys": s.ttys, "login": s.login,
                       "idle": s.idle, "what": s.what})
            })
            .collect();
        json!({"now": now, "seats": seats})
    }

    /// Idle seconds of `/dev/<tty>` (its last read) and the name of its
    /// foreground process.
    fn tty(
        &self,
        tty: &str,
        now: f64,
        groups: &HashMap<u64, i64>,
    ) -> (Option<f64>, Option<String>) {
        use std::os::unix::fs::MetadataExt;
        let plain = !tty.is_empty()
            && !tty.starts_with('/')
            && !tty.split('/').any(|c| c == ".." || c.is_empty());
        let Some(st) = plain
            .then(|| std::fs::metadata(self.paths.dev.join(tty)).ok())
            .flatten()
        else {
            return (None, None);
        };
        let atime = st.atime() as f64 + st.atime_nsec() as f64 / 1e9;
        let group = groups.get(&st.rdev()).copied().unwrap_or(0);
        let what = if group > 0 {
            self.read(self.paths.proc.join(group.to_string()).join("comm"))
                .trim()
                .to_string()
        } else {
            String::new()
        };
        (
            Some((now - atime).max(0.0)),
            (!what.is_empty()).then_some(what),
        )
    }

    /// The owners are looked for when the sockets changed, or every
    /// `OWNED` seconds: a scan of every `/proc/<pid>/fd`.
    pub fn listeners(&mut self, now: f64) -> Value {
        let mut sockets = parse::parse_tcp(&self.read(self.paths.proc.join("net/tcp")));
        sockets.extend(parse::parse_tcp(
            &self.read(self.paths.proc.join("net/tcp6")),
        ));
        let wanted: BTreeSet<u64> = sockets
            .iter()
            .filter(|s| parse::outside(s.addr) && s.inode != 0)
            .map(|s| s.inode)
            .collect();
        let stale = match &self.owned {
            Some((at, seen, _)) => *seen != wanted || !(0.0..OWNED).contains(&(now - at)),
            None => true,
        };
        if stale {
            let owners = self.owners(&wanted);
            self.owned = Some((now, wanted, owners));
        }
        let owners = &self.owned.as_ref().expect("scanned").2;
        let users = parse::parse_passwd(&self.read(&self.paths.passwd));
        let user = |uid: u32| users.get(&uid).cloned().unwrap_or_else(|| uid.to_string());
        let list: Vec<Value> = parse::listeners(&sockets, user, owners)
            .iter()
            .map(|l| json!({"addr": l.addr, "port": l.port, "user": l.user, "process": l.process}))
            .collect();
        list.into()
    }

    /// Inode to process name, for the sockets found in the readable
    /// `/proc/<pid>/fd`: the user's own processes, all for root.
    fn owners(&self, inodes: &BTreeSet<u64>) -> HashMap<u64, String> {
        let mut out = HashMap::new();
        for pid in pids(&self.paths.proc) {
            if out.len() == inodes.len() {
                break;
            }
            let base = self.paths.proc.join(pid.to_string());
            let Ok(fds) = std::fs::read_dir(base.join("fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                let Ok(link) = std::fs::read_link(fd.path()) else {
                    continue;
                };
                let link = link.to_string_lossy();
                let inode = link
                    .strip_prefix("socket:[")
                    .and_then(|l| l.strip_suffix(']'));
                if let Some(n) = inode.and_then(|n| n.parse::<u64>().ok())
                    && inodes.contains(&n)
                {
                    out.entry(n).or_insert_with(|| {
                        let name = self.read(base.join("comm")).trim().to_string();
                        if name.is_empty() {
                            pid.to_string()
                        } else {
                            name
                        }
                    });
                }
            }
        }
        out
    }

    /// One line a filesystem: two mounts of one (a bind mount, a device
    /// under two names) are one, its first by mount point.
    pub fn disks(&self) -> Value {
        use std::os::unix::fs::MetadataExt;
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for m in parse::parse_mounts(&self.read(self.paths.proc.join("mounts"))) {
            let Ok(dev) = std::fs::metadata(&m.point).map(|s| s.dev()) else {
                continue;
            };
            let Ok(vfs) = rustix::fs::statvfs(m.point.as_str()) else {
                continue;
            };
            if vfs.f_blocks > 0 && seen.insert(dev) {
                let d = parse::disk(
                    &m,
                    vfs.f_bsize,
                    vfs.f_frsize,
                    vfs.f_blocks,
                    vfs.f_bfree,
                    vfs.f_bavail,
                );
                out.push(json!({"point": d.point, "device": d.device, "size": d.size,
                                "used": d.used, "free": d.free}));
            }
        }
        out.into()
    }
}

pub fn glance_json(g: &Glance) -> Value {
    let m = &g.memory;
    let top: Vec<Value> = g
        .top
        .iter()
        .map(|p| json!({"pid": p.pid, "name": p.name, "cpu": p.cpu, "mem": p.mem}))
        .collect();
    json!({
        "cpu": g.cpu, "cores": g.cores, "load": g.load,
        "memory": {"total": m.total, "available": m.available,
                   "swap_total": m.swap_total, "swap_free": m.swap_free},
        "uptime": g.uptime, "rx": g.rx, "tx": g.tx, "top": top,
    })
}

/// The text of `path`, `READ` bytes at most; empty when it cannot be
/// read.
fn read_text(path: &std::path::Path) -> String {
    use std::io::Read;
    let mut out = Vec::new();
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.take(READ).read_to_end(&mut out);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The process ids of `proc`.
fn pids(proc: &std::path::Path) -> Vec<i64> {
    std::fs::read_dir(proc)
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().to_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}
