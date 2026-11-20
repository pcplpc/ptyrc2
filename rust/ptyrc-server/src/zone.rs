// SPDX-License-Identifier: GPL-3.0-or-later
//! Time zones: a reader of the system's TZif files (RFC 8536, version 2
//! and later) and of the POSIX TZ rule of their footer. Instants are
//! Unix seconds; an offset is east of UTC, in seconds.

use std::fs::File;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// A TZif file larger than this is refused.
const MAX_FILE: u64 = 256 * 1024;
/// Components of a zone name, at most.
const MAX_DEPTH: usize = 3;

/// The offset in force at an instant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offset {
    pub offset_s: i32,
    pub abbr: String,
    /// When the offset or the abbreviation changes next.
    pub next: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Kind {
    offset: i32,
    abbr: String,
}

/// `std offset [dst [offset] [,start[/time],end[/time]]]`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rule {
    std: Kind,
    dst: Option<(Kind, Date, i32, Date, i32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Date {
    /// `Jn`: 1..=365, February 29 never counted.
    Julian(u16),
    /// `n`: 0..=365, February 29 counted.
    Day(u16),
    /// `Mm.w.d`: day `d` (0: Sunday) of week `w` (5: the last) of month `m`.
    Month(u8, u8, u8),
}

/// A zone: its transitions, then its rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Zone {
    pub name: String,
    times: Vec<i64>,
    /// The kind in force from `times[i]` on.
    kinds_at: Vec<usize>,
    kinds: Vec<Kind>,
    rule: Option<Rule>,
}

/// Why a zone is not had.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Missing {
    /// Not a zone name, or no such zone.
    Unknown,
    /// No database on this machine.
    NoDatabase,
}

impl Zone {
    pub fn utc() -> Zone {
        Zone::fixed("UTC", 0, "UTC")
    }

    fn fixed(name: &str, offset: i32, abbr: &str) -> Zone {
        let kind = Kind {
            offset,
            abbr: abbr.into(),
        };
        Zone {
            name: name.into(),
            times: Vec::new(),
            kinds_at: Vec::new(),
            kinds: vec![kind.clone()],
            rule: Some(Rule {
                std: kind,
                dst: None,
            }),
        }
    }

    /// A TZif file, version 2 or later; a version 1 file is read by its
    /// 32-bit body.
    pub fn parse(name: &str, bytes: &[u8]) -> Option<Zone> {
        let (v1, version) = header(bytes)?;
        let skip = v1.body_len(4);
        let (counts, body, wide) = match version {
            0 => (v1, &bytes[44..], false),
            _ => {
                let rest = bytes.get(44 + skip..)?;
                let (v2, _) = header(rest)?;
                (v2, &rest[44..], true)
            }
        };
        let width = if wide { 8 } else { 4 };
        let mut at = 0;
        let mut take = |n: usize| -> Option<&[u8]> {
            let piece = body.get(at..at + n)?;
            at += n;
            Some(piece)
        };
        let raw_times = take(counts.time * width)?;
        let times: Vec<i64> = raw_times
            .chunks(width)
            .map(|c| match wide {
                true => i64::from_be_bytes(c.try_into().unwrap()),
                false => i32::from_be_bytes(c.try_into().unwrap()) as i64,
            })
            .collect();
        let kinds_at: Vec<usize> = take(counts.time)?.iter().map(|i| *i as usize).collect();
        let raw_kinds = take(counts.kind * 6)?;
        let chars = take(counts.chars)?;
        let mut kinds = Vec::with_capacity(counts.kind);
        for k in raw_kinds.chunks(6) {
            let offset = i32::from_be_bytes(k[..4].try_into().unwrap());
            let abbr = chars.get(k[5] as usize..)?;
            let end = abbr.iter().position(|b| *b == 0).unwrap_or(abbr.len());
            let abbr = String::from_utf8_lossy(&abbr[..end]).into_owned();
            if !(-89_999..=93_599).contains(&offset) {
                return None;
            }
            kinds.push(Kind { offset, abbr });
        }
        if kinds.is_empty() || kinds_at.iter().any(|i| *i >= kinds.len()) {
            return None;
        }
        if times.windows(2).any(|w| w[0] >= w[1]) {
            return None;
        }
        take(counts.leap * (width + 4) + counts.std + counts.ut)?;
        let rule = match wide {
            true => {
                let footer = &body[at..];
                let footer = footer.strip_prefix(b"\n")?;
                let end = footer.iter().position(|b| *b == b'\n')?;
                let text = std::str::from_utf8(&footer[..end]).ok()?;
                match text {
                    "" => None,
                    _ => Some(parse_rule(text)?),
                }
            }
            false => None,
        };
        Some(Zone {
            name: name.into(),
            times,
            kinds_at,
            kinds,
            rule,
        })
    }

    /// A POSIX TZ rule alone (`$TZ` = `CET-1CEST,M3.5.0,M10.5.0/3`).
    pub fn posix(name: &str, text: &str) -> Option<Zone> {
        let rule = parse_rule(text)?;
        Some(Zone {
            name: name.into(),
            times: Vec::new(),
            kinds_at: Vec::new(),
            kinds: vec![rule.std.clone()],
            rule: Some(rule),
        })
    }

    /// The offset in force at `t`, and when it changes next.
    pub fn at(&self, t: i64) -> Offset {
        let (kind, mut from) = self.kind_at(t);
        // The next change of offset or abbreviation: a transition that
        // changes neither is passed over.
        let mut next = None;
        for _ in 0..1024 {
            let Some(at) = self.change_after(from) else {
                break;
            };
            let (after, _) = self.kind_at(at);
            if after != kind {
                next = Some(at);
                break;
            }
            from = at;
        }
        Offset {
            offset_s: kind.offset,
            abbr: kind.abbr.clone(),
            next,
        }
    }

    /// The kind in force at `t`, and an instant at or before `t` from
    /// which to look for the next change.
    fn kind_at(&self, t: i64) -> (&Kind, i64) {
        let last = self.times.last().copied();
        match (&self.rule, last) {
            (Some(rule), None) => return (rule.at(t).0, t),
            (Some(rule), Some(last)) if t >= last => return (rule.at(t).0, t),
            _ => {}
        }
        match self.times.partition_point(|x| *x <= t) {
            // Before the first transition: the first kind (RFC 8536, 3.2).
            0 => (&self.kinds[0], t),
            i => (&self.kinds[self.kinds_at[i - 1]], t),
        }
    }

    /// The first transition, of the table or the rule, after `t`.
    fn change_after(&self, t: i64) -> Option<i64> {
        let i = self.times.partition_point(|x| *x <= t);
        if let Some(at) = self.times.get(i) {
            return Some(*at);
        }
        self.rule.as_ref()?.at(t).1
    }
}

impl Rule {
    /// The kind in force at `t` and the next transition of the rule.
    fn at(&self, t: i64) -> (&Kind, Option<i64>) {
        let Some((dst, start, start_time, end, end_time)) = &self.dst else {
            return (&self.std, None);
        };
        let year = civil_from_days(t.div_euclid(86_400)).0;
        // (instant, the kind from then on), over three years.
        let mut changes = Vec::with_capacity(6);
        for y in year - 1..=year + 1 {
            let on = day_of(y, *start) * 86_400 + *start_time as i64 - self.std.offset as i64;
            let off = day_of(y, *end) * 86_400 + *end_time as i64 - dst.offset as i64;
            changes.push((on, true));
            changes.push((off, false));
        }
        changes.sort_unstable();
        let i = changes.partition_point(|c| c.0 <= t);
        let in_dst = match i {
            0 => !changes[0].1,
            i => changes[i - 1].1,
        };
        let kind = if in_dst { dst } else { &self.std };
        (kind, changes.get(i).map(|c| c.0))
    }
}

struct Counts {
    ut: usize,
    std: usize,
    leap: usize,
    time: usize,
    kind: usize,
    chars: usize,
}

impl Counts {
    fn body_len(&self, width: usize) -> usize {
        self.time * width
            + self.time
            + self.kind * 6
            + self.chars
            + self.leap * (width + 4)
            + self.std
            + self.ut
    }
}

/// The 44 bytes of a header: its counts and the version (0, or 2.. as
/// the digit).
fn header(bytes: &[u8]) -> Option<(Counts, u8)> {
    let head = bytes.get(..44)?;
    if &head[..4] != b"TZif" {
        return None;
    }
    let version = match head[4] {
        0 => 0,
        v @ b'2'..=b'9' => v - b'0',
        _ => return None,
    };
    let n =
        |i: usize| u32::from_be_bytes(head[20 + 4 * i..24 + 4 * i].try_into().unwrap()) as usize;
    let counts = Counts {
        ut: n(0),
        std: n(1),
        leap: n(2),
        time: n(3),
        kind: n(4),
        chars: n(5),
    };
    // Bounds of RFC 8536 that a real file keeps far below.
    let sane = counts.kind <= 256 && counts.chars <= 1024 && counts.time <= 65_536;
    sane.then_some((counts, version))
}

/// A POSIX TZ rule.
fn parse_rule(text: &str) -> Option<Rule> {
    let mut s = text;
    let std_abbr = abbr(&mut s)?;
    let std_offset = -signed_time(&mut s, 24)?;
    let std = Kind {
        offset: std_offset,
        abbr: std_abbr,
    };
    if s.is_empty() {
        return Some(Rule { std, dst: None });
    }
    let dst_abbr = abbr(&mut s)?;
    let dst_offset = match s.starts_with([',', ';']) || s.is_empty() {
        true => std_offset + 3600,
        false => -signed_time(&mut s, 24)?,
    };
    let dst = Kind {
        offset: dst_offset,
        abbr: dst_abbr,
    };
    // No rule: the default of glibc, that of the United States.
    let rules = match s.strip_prefix([',', ';']) {
        Some(rest) => rest,
        None if s.is_empty() => "M3.2.0,M11.1.0",
        None => return None,
    };
    let (first, second) = rules.split_once(',')?;
    let (start, start_time) = date_time(first)?;
    let (end, end_time) = date_time(second)?;
    Some(Rule {
        std,
        dst: Some((dst, start, start_time, end, end_time)),
    })
}

fn abbr(s: &mut &str) -> Option<String> {
    let (name, rest) = match s.strip_prefix('<') {
        Some(quoted) => {
            let end = quoted.find('>')?;
            (&quoted[..end], &quoted[end + 1..])
        }
        None => {
            let end = s
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(s.len());
            (&s[..end], &s[end..])
        }
    };
    let ok = (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-');
    *s = rest;
    ok.then(|| name.to_string())
}

/// `[+-]hh[:mm[:ss]]`, hours below `max_h`; in seconds.
fn signed_time(s: &mut &str, max_h: i32) -> Option<i32> {
    let (sign, rest) = match s.as_bytes().first()? {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, *s),
    };
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != ':')
        .unwrap_or(rest.len());
    let mut parts = rest[..end].split(':');
    let h: i32 = parts.next()?.parse().ok()?;
    let m: i32 = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let sec: i32 = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    if parts.next().is_some() || h > max_h || m > 59 || sec > 59 {
        return None;
    }
    *s = &rest[end..];
    Some(sign * (h * 3600 + m * 60 + sec))
}

/// `date[/time]`; the time 02:00 by default, -167..167 hours.
fn date_time(text: &str) -> Option<(Date, i32)> {
    let (date, time) = match text.split_once('/') {
        Some((date, time)) => {
            let mut t = time;
            let time = signed_time(&mut t, 167)?;
            if !t.is_empty() {
                return None;
            }
            (date, time)
        }
        None => (text, 7200),
    };
    let date = if let Some(n) = date.strip_prefix('J') {
        let n: u16 = n.parse().ok()?;
        (1..=365).contains(&n).then_some(Date::Julian(n))?
    } else if let Some(m) = date.strip_prefix('M') {
        let mut parts = m.split('.').map(|p| p.parse::<u8>().ok());
        let (m, w, d) = (parts.next()??, parts.next()??, parts.next()??);
        let ok = parts.next().is_none() && (1..=12).contains(&m) && (1..=5).contains(&w) && d <= 6;
        ok.then_some(Date::Month(m, w, d))?
    } else {
        let n: u16 = date.parse().ok()?;
        (n <= 365).then_some(Date::Day(n))?
    };
    Some((date, time))
}

fn leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn month_days(y: i64, m: u8) -> i64 {
    match m {
        2 if leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a civil date (H. Hinnant's algorithm).
pub(crate) fn days_from_civil(y: i64, m: u8, d: u8) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a count of days since 1970-01-01.
pub(crate) fn civil_from_days(z: i64) -> (i64, u8, u8) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The day, since 1970-01-01, of `date` in year `y`.
fn day_of(y: i64, date: Date) -> i64 {
    let jan1 = days_from_civil(y, 1, 1);
    match date {
        Date::Julian(n) => {
            let n = n as i64 - 1;
            jan1 + if leap(y) && n >= 59 { n + 1 } else { n }
        }
        Date::Day(n) => jan1 + n as i64,
        Date::Month(m, w, d) => {
            let first = days_from_civil(y, m, 1);
            // 1970-01-01 was a Thursday.
            let weekday = (first + 4).rem_euclid(7);
            let mut day = (d as i64 - weekday).rem_euclid(7) + (w as i64 - 1) * 7;
            if day >= month_days(y, m) {
                day -= 7;
            }
            first + day
        }
    }
}

/// An IANA zone name: components of `[A-Za-z0-9._+-]`, 1 to 64 bytes,
/// none starting with `.`; 3 at most.
pub(crate) fn valid_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    parts.len() <= MAX_DEPTH
        && parts.iter().all(|p| {
            (1..=64).contains(&p.len())
                && !p.starts_with('.')
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
        })
}

/// The database: `$TZDIR`, else `/usr/share/zoneinfo`.
pub(crate) fn database() -> PathBuf {
    match std::env::var_os("TZDIR") {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        _ => PathBuf::from("/usr/share/zoneinfo"),
    }
}

/// The zone `name` of the database `dir`. `UTC` is had without one.
pub(crate) fn load(dir: &Path, name: &str) -> Result<Zone, Missing> {
    if !valid_name(name) {
        return Err(Missing::Unknown);
    }
    let Ok(root) = dir.canonicalize() else {
        return match name {
            "UTC" => Ok(Zone::utc()),
            _ => Err(Missing::NoDatabase),
        };
    };
    // A link of the database is followed, never out of it.
    let path = root
        .join(name)
        .canonicalize()
        .map_err(|_| Missing::Unknown)?;
    if !path.starts_with(&root) {
        return Err(Missing::Unknown);
    }
    let bytes = read_regular(&path).ok_or(Missing::Unknown)?;
    Zone::parse(name, &bytes).ok_or(Missing::Unknown)
}

/// A regular file of `MAX_FILE` bytes at most, its last component not
/// followed.
fn read_regular(path: &Path) -> Option<Vec<u8>> {
    let file = File::options()
        .read(true)
        .custom_flags(
            rustix::fs::OFlags::NOFOLLOW.bits() as i32 | rustix::fs::OFlags::NONBLOCK.bits() as i32,
        )
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_FILE).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// The machine's zone: `$TZ` (a name of the database, an absolute path,
/// or a POSIX rule), else `/etc/localtime`, else UTC. Its name: that of
/// the database when known, else `local`.
pub(crate) fn local(dir: &Path, tz: Option<&str>, localtime: &Path) -> Zone {
    let named = |name: &str, zone: Option<Zone>| {
        zone.map(|mut z| {
            z.name = name.to_string();
            z
        })
    };
    if let Some(tz) = tz
        .map(|t| t.strip_prefix(':').unwrap_or(t))
        .filter(|t| !t.is_empty())
    {
        let zone = if tz.starts_with('/') {
            let bytes = read_regular(Path::new(tz));
            named(&name_in(tz), bytes.and_then(|b| Zone::parse("local", &b)))
        } else if let Ok(zone) = load(dir, tz) {
            Some(zone)
        } else {
            Zone::posix(tz, tz)
        };
        // A `$TZ` not understood: UTC, as the C library takes it.
        return zone.unwrap_or_else(Zone::utc);
    }
    let link = std::fs::read_link(localtime).ok();
    let name = link
        .as_deref()
        .map_or("local".into(), |l| name_in(&l.to_string_lossy()));
    let Ok(file) = File::open(localtime) else {
        return Zone::utc();
    };
    let meta = file
        .metadata()
        .ok()
        .filter(|m| m.is_file() && m.len() <= MAX_FILE);
    let mut bytes = Vec::new();
    let read = meta.is_some() && file.take(MAX_FILE).read_to_end(&mut bytes).is_ok();
    let zone = read.then(|| Zone::parse(&name, &bytes)).flatten();
    zone.unwrap_or_else(Zone::utc)
}

/// The name of a zone file: what follows `zoneinfo/`, else `local`.
fn name_in(path: &str) -> String {
    match path.rsplit_once("zoneinfo/") {
        Some((_, name)) if valid_name(name) => name.to_string(),
        _ => "local".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB: &str = "/usr/share/zoneinfo";

    fn at(y: i64, m: u8, d: u8, h: i64, mi: i64) -> i64 {
        days_from_civil(y, m, d) * 86_400 + h * 3600 + mi * 60
    }

    fn db_zone(name: &str) -> Option<Zone> {
        load(Path::new(DB), name).ok()
    }

    #[test]
    fn the_calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for z in [-800_000, -1, 0, 59, 11_016, 20_000, 2_000_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        // The last Sunday of March 2026: the 29th.
        assert_eq!(
            day_of(2026, Date::Month(3, 5, 0)),
            days_from_civil(2026, 3, 29)
        );
        assert_eq!(
            day_of(2026, Date::Month(11, 1, 0)),
            days_from_civil(2026, 11, 1)
        );
        assert_eq!(day_of(2024, Date::Julian(60)), days_from_civil(2024, 3, 1));
        assert_eq!(day_of(2024, Date::Day(59)), days_from_civil(2024, 2, 29));
    }

    #[test]
    fn a_posix_rule() {
        let paris = Zone::posix("x", "CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        let winter = paris.at(at(2026, 1, 10, 12, 0));
        assert_eq!((winter.offset_s, winter.abbr.as_str()), (3600, "CET"));
        // 2026-03-29 01:00 UTC: CEST.
        assert_eq!(winter.next, Some(at(2026, 3, 29, 1, 0)));
        let summer = paris.at(at(2026, 3, 29, 1, 0));
        assert_eq!((summer.offset_s, summer.abbr.as_str()), (7200, "CEST"));
        assert_eq!(summer.next, Some(at(2026, 10, 25, 1, 0)));
        assert_eq!(paris.at(at(2026, 3, 29, 0, 59)).offset_s, 3600);
        // The south: summer across the new year.
        let sydney = Zone::posix("x", "AEST-10AEDT,M10.1.0,M4.1.0/3").unwrap();
        assert_eq!(sydney.at(at(2026, 1, 1, 0, 0)).offset_s, 39_600);
        assert_eq!(sydney.at(at(2026, 7, 1, 0, 0)).offset_s, 36_000);
        let fixed = Zone::posix("x", "<+0530>-5:30").unwrap().at(0);
        assert_eq!(
            (fixed.offset_s, fixed.abbr.as_str(), fixed.next),
            (19_800, "+0530", None)
        );
        let us = Zone::posix("x", "EST5EDT").unwrap();
        assert_eq!(us.at(at(2026, 7, 1, 0, 0)).offset_s, -4 * 3600);
        for bad in [
            "",
            "C",
            "CET",
            "CET-1CEST,M3.5.0",
            "CET-1CEST,M13.1.0,M1.1.0",
            "CET-99",
        ] {
            assert_eq!(Zone::posix("x", bad), None, "{bad}");
        }
    }

    #[test]
    fn hostile_names() {
        for bad in [
            "../../etc/passwd",
            "/etc/passwd",
            "..",
            ".",
            "",
            "Europe/../../etc/passwd",
            "a//b",
            "Europe/.hidden",
            "a\0b",
            "a/b/c/d",
            "Europe/Paris ",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
            assert_eq!(load(Path::new(DB), bad), Err(Missing::Unknown), "{bad:?}");
        }
        assert!(valid_name("Etc/GMT+5"));
        assert!(valid_name("America/Argentina/Buenos_Aires"));
        // Not a zone: a directory, a file of another form.
        if Path::new(DB).is_dir() {
            assert_eq!(load(Path::new(DB), "Europe"), Err(Missing::Unknown));
            assert_eq!(load(Path::new(DB), "zone.tab"), Err(Missing::Unknown));
        }
        let none = Path::new("/nonexistent/zoneinfo");
        assert_eq!(load(none, "Europe/Paris"), Err(Missing::NoDatabase));
        assert_eq!(load(none, "UTC").unwrap().at(5).offset_s, 0);
    }

    #[test]
    fn the_database_knows_its_transitions() {
        let Some(paris) = db_zone("Europe/Paris") else {
            eprintln!("no zone database: skipped");
            return;
        };
        let before = paris.at(at(2026, 3, 29, 0, 59));
        assert_eq!((before.offset_s, before.abbr.as_str()), (3600, "CET"));
        assert_eq!(before.next, Some(at(2026, 3, 29, 1, 0)));
        assert_eq!(paris.at(at(2026, 3, 29, 1, 0)).offset_s, 7200);
        // Past the table: the footer.
        assert_eq!(paris.at(at(2090, 7, 1, 0, 0)).offset_s, 7200);
        // Before it: 1900, Paris mean time.
        assert_eq!(paris.at(at(1900, 1, 1, 0, 0)).offset_s, 561);
        let tokyo = db_zone("Asia/Tokyo").unwrap();
        let now = tokyo.at(at(2026, 11, 20, 0, 0));
        assert_eq!(
            (now.offset_s, now.abbr.as_str(), now.next),
            (32_400, "JST", None)
        );
        let ny = db_zone("America/New_York").unwrap();
        let o = ny.at(at(2026, 11, 1, 5, 59));
        assert_eq!((o.offset_s, o.abbr.as_str()), (-4 * 3600, "EDT"));
        assert_eq!(o.next, Some(at(2026, 11, 1, 6, 0)));
        let kolkata = db_zone("Asia/Kolkata").unwrap();
        assert_eq!(kolkata.at(at(2026, 1, 1, 0, 0)).offset_s, 19_800);
    }

    /// Against `date`, when both the database and `date` are had.
    #[test]
    fn as_date_tells() {
        let zones = [
            "Europe/Paris",
            "America/Santiago",
            "Australia/Lord_Howe",
            "Africa/Casablanca",
        ];
        for name in zones {
            let Some(zone) = db_zone(name) else {
                return;
            };
            for t in (0..40).map(|i| 1_600_000_000 + i * 9_000_017) {
                let out = std::process::Command::new("date")
                    .env("TZ", name)
                    .args(["-d", &format!("@{t}"), "+%z %Z"])
                    .output();
                let Ok(out) = out else {
                    return;
                };
                let text = String::from_utf8_lossy(&out.stdout);
                let (z, abbr) = text.trim().split_once(' ').unwrap();
                let sign = if z.starts_with('-') { -1 } else { 1 };
                let h: i32 = z[1..3].parse().unwrap();
                let m: i32 = z[3..5].parse().unwrap();
                let o = zone.at(t);
                assert_eq!(o.offset_s, sign * (h * 3600 + m * 60), "{name} @{t}");
                assert_eq!(o.abbr, abbr, "{name} @{t}");
            }
        }
    }

    #[test]
    fn the_local_zone() {
        let none = Path::new("/nonexistent");
        let utc = local(none, None, none);
        assert_eq!((utc.name.as_str(), utc.at(0).offset_s), ("UTC", 0));
        let rule = local(none, Some("CET-1CEST,M3.5.0,M10.5.0/3"), none);
        assert_eq!(rule.at(at(2026, 7, 1, 0, 0)).offset_s, 7200);
        assert_eq!(local(none, Some("nonsense!"), none).at(0).offset_s, 0);
        if db_zone("Asia/Tokyo").is_some() {
            let tokyo = local(Path::new(DB), Some(":Asia/Tokyo"), none);
            assert_eq!(
                (tokyo.name.as_str(), tokyo.at(0).offset_s),
                ("Asia/Tokyo", 32_400)
            );
            let path = format!("{DB}/Asia/Tokyo");
            let by_path = local(none, Some(&path), none);
            assert_eq!(
                (by_path.name.as_str(), by_path.at(0).offset_s),
                ("Asia/Tokyo", 32_400)
            );
        }
    }
}
