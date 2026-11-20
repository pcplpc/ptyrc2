// SPDX-License-Identifier: GPL-3.0-or-later
//! The host functions (`PROTOCOL.md`, Methods): the wall clock, storage,
//! assets, files named by the user, the result; base64.

use crate::json::Value;
use crate::{Error, call, json};

/// Pieces of a read: the host answers 1 MiB at most.
const PIECE: u64 = 1 << 20;

fn bad(method: &str, what: &str) -> Error {
    Error {
        code: "bad-answer".into(),
        message: format!("{method}: no {what}"),
        hint: None,
    }
}

fn names(list: &Value, key: &str) -> Vec<String> {
    let all = list.as_array().into_iter().flatten();
    all.filter_map(|e| e[key].as_str().map(str::to_string))
        .collect()
}

/// Base64 of RFC 4648, the standard alphabet, padded.
pub mod base64 {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for c in bytes.chunks(3) {
            let n = (u32::from(c[0]) << 16)
                | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                | u32::from(*c.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= c.len() {
                    out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn value(b: u8) -> Option<u32> {
        Some(u32::from(match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    }

    /// `None`: a byte outside the alphabet, or a bad length or padding.
    /// The padding may be left out.
    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let t = text.as_bytes();
        let t = t.strip_suffix(b"==").or(t.strip_suffix(b"=")).unwrap_or(t);
        if t.len() % 4 == 1 || (t.len() < text.len() && text.len() % 4 != 0) {
            return None;
        }
        let mut out = Vec::with_capacity(t.len() * 3 / 4);
        for c in t.chunks(4) {
            let mut n = 0u32;
            for (i, b) in c.iter().enumerate() {
                let v = value(*b)?;
                n |= v << (18 - 6 * i);
            }
            let bytes = n.to_be_bytes();
            out.extend_from_slice(&bytes[1..c.len()]);
            // Bits past the last byte are zero.
            if c.len() < 4 && bytes[c.len()..].iter().any(|b| *b != 0) {
                return None;
            }
        }
        Some(out)
    }
}

/// The wall clock and the zones (`clock.now`, `clock.zone`). Without
/// `clock` held: session time as the epoch, UTC, `degree` `"none"`.
pub mod clock {
    use super::{Error, Value, bad, call, json};

    /// The wall clock now, in the local zone.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Wall {
        pub unix_ms: i64,
        /// An IANA name, or `UTC`.
        pub zone: String,
        pub offset_s: i32,
        pub abbr: String,
        /// When the offset changes next; `None`: never known.
        pub next: Option<i64>,
        /// `Some("none")`: `clock` not held.
        pub degree: Option<String>,
    }

    /// The offset of a zone at an instant.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Offset {
        pub at: i64,
        pub offset_s: i32,
        pub abbr: String,
        pub next: Option<i64>,
    }

    /// A date and time of day: what `civil` gives.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Civil {
        pub year: i64,
        /// 1..=12.
        pub month: u32,
        /// 1..=31.
        pub day: u32,
        pub hour: u32,
        pub minute: u32,
        pub second: u32,
        /// 0: Sunday, ..., 6: Saturday.
        pub weekday: u32,
    }

    fn offset_of(v: &Value) -> Option<Offset> {
        Some(Offset {
            at: v["at"].as_i64()?,
            offset_s: v["offset_s"].as_i64()? as i32,
            abbr: v["abbr"].as_str()?.to_string(),
            next: v["next"].as_i64(),
        })
    }

    /// Of an answer of `clock.now`.
    pub fn wall_of(v: &Value) -> Option<Wall> {
        Some(Wall {
            unix_ms: v["unix_ms"].as_i64()?,
            zone: v["zone"].as_str()?.to_string(),
            offset_s: v["offset_s"].as_i64()? as i32,
            abbr: v["abbr"].as_str()?.to_string(),
            next: v["next"].as_i64(),
            degree: v["degree"].as_str().map(str::to_string),
        })
    }

    /// `clock.now`.
    pub fn now() -> Result<Wall, Error> {
        wall_of(&call("clock.now", Value::Null)?).ok_or_else(|| bad("clock.now", "unix_ms"))
    }

    /// `clock.zone`: the offsets of `zone` (an IANA name, `"local"`) at
    /// each instant of `at` (ms, 1 to 64); `at` empty: now.
    pub fn zone(zone: &str, at: &[i64]) -> Result<Vec<Offset>, Error> {
        let mut params = json!({ "zone": zone });
        if !at.is_empty() {
            params["at"] = json!(at);
        }
        let answer = call("clock.zone", params)?;
        let list = answer["offsets"].as_array();
        let offsets: Option<Vec<Offset>> = list.into_iter().flatten().map(offset_of).collect();
        offsets.ok_or_else(|| bad("clock.zone", "offsets"))
    }

    /// The date and time of `unix_ms` at `offset_s` east of UTC
    /// (proleptic Gregorian).
    pub fn civil(unix_ms: i64, offset_s: i32) -> Civil {
        let s = unix_ms.div_euclid(1000) + i64::from(offset_s);
        let (days, secs) = (s.div_euclid(86400), s.rem_euclid(86400));
        // Days to a date: H. Hinnant, `civil_from_days`.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        Civil {
            year,
            month,
            day,
            hour: (secs / 3600) as u32,
            minute: (secs / 60 % 60) as u32,
            second: (secs % 60) as u32,
            weekday: (days + 4).rem_euclid(7) as u32,
        }
    }
}

/// The app's named values (`storage.*`): strings, 256 KiB each, 64
/// keys, 1 MiB in all. Needs `storage`, else `denied`.
pub mod storage {
    use super::{Error, Value, call, json, names};

    /// `storage.get`; `None`: no such key.
    pub fn get(key: &str) -> Result<Option<String>, Error> {
        let answer = call("storage.get", json!({ "key": key }))?;
        Ok(answer["value"].as_str().map(str::to_string))
    }

    /// `storage.set`; the bytes used after it.
    pub fn set(key: &str, value: &str) -> Result<u64, Error> {
        let answer = call("storage.set", json!({ "key": key, "value": value }))?;
        Ok(answer["used"].as_u64().unwrap_or(0))
    }

    /// `storage.remove`.
    pub fn remove(key: &str) -> Result<(), Error> {
        call("storage.remove", json!({ "key": key })).map(drop)
    }

    /// The keys held (`storage.list`), sorted.
    pub fn keys() -> Result<Vec<String>, Error> {
        Ok(names(&call("storage.list", Value::Null)?["keys"], "key"))
    }
}

/// Reads `method` whole, `params` and `offset`, `len`, base64 by pieces.
fn read_all(method: &str, params: &Value) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    loop {
        let mut p = params.clone();
        p["as"] = json!("base64");
        p["offset"] = json!(out.len());
        p["len"] = json!(PIECE);
        let answer = call(method, p)?;
        let piece = answer["base64"].as_str().and_then(base64::decode);
        let piece = piece.ok_or_else(|| bad(method, "base64"))?;
        out.extend_from_slice(&piece);
        if answer["eof"] != false || piece.is_empty() {
            return Ok(out);
        }
    }
}

/// Reads as text: whole in one answer, or by bytes.
fn read_text(method: &str, params: &Value) -> Result<String, Error> {
    let mut p = params.clone();
    p["as"] = json!("text");
    let answer = call(method, p)?;
    match answer["text"].as_str() {
        Some(t) if answer["eof"] != false => Ok(t.to_string()),
        Some(_) => Ok(String::from_utf8_lossy(&read_all(method, params)?).into_owned()),
        None => Err(bad(method, "text")),
    }
}

/// The files of the app's `[app] assets` tree (`asset.*`); a name:
/// `pictures/ship.pic`.
pub mod asset {
    use super::{Error, Value, call, json, names, read_all, read_text};

    /// The asset as text (UTF-8, invalid bytes replaced).
    pub fn text(name: &str) -> Result<String, Error> {
        read_text("asset.read", &json!({ "name": name }))
    }

    /// The asset's bytes.
    pub fn bytes(name: &str) -> Result<Vec<u8>, Error> {
        read_all("asset.read", &json!({ "name": name }))
    }

    /// The names of the assets.
    pub fn list() -> Result<Vec<String>, Error> {
        Ok(names(&call("asset.list", Value::Null)?["assets"], "name"))
    }
}

/// What `File::stat` tells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stat {
    pub exists: bool,
    pub size: u64,
    pub mtime_ms: i64,
}

/// A file or a directory named by the user at the start (`file.*`): a
/// handle, never a path. `init` gets `{"file": "f0", "name": "TODO.md"}`
/// in place of the option's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub handle: String,
    /// The last component, as the user typed it.
    pub name: String,
    /// A file of the directory `handle`: `child`.
    pub child: Option<String>,
}

impl File {
    /// A handle: `"f0"`.
    pub fn new(handle: &str) -> File {
        File {
            handle: handle.into(),
            name: String::new(),
            child: None,
        }
    }

    /// Of a value of `init`'s `args`: `{"file", "name"}`; a list of them:
    /// the first.
    pub fn of(arg: &Value) -> Option<File> {
        let arg = arg.get(0).unwrap_or(arg);
        Some(File {
            handle: arg["file"].as_str()?.into(),
            name: arg["name"].as_str().unwrap_or_default().into(),
            child: None,
        })
    }

    /// Each of a list (an option given more than once), or the one.
    pub fn all(arg: &Value) -> Vec<File> {
        match arg.as_array() {
            Some(list) => list.iter().filter_map(File::of).collect(),
            None => File::of(arg).into_iter().collect(),
        }
    }

    /// The file `name` of this directory.
    pub fn child(&self, name: &str) -> File {
        File {
            child: Some(name.into()),
            name: name.into(),
            ..self.clone()
        }
    }

    fn params(&self) -> Value {
        let mut p = json!({ "file": (self.handle.as_str()) });
        if let Some(c) = &self.child {
            p["name"] = json!(c.as_str());
        }
        p
    }

    /// The content as text (UTF-8, invalid bytes replaced).
    pub fn text(&self) -> Result<String, Error> {
        read_text("file.read", &self.params())
    }

    /// The content.
    pub fn bytes(&self) -> Result<Vec<u8>, Error> {
        read_all("file.read", &self.params())
    }

    /// `file.stat`: a file absent is `exists: false`.
    pub fn stat(&self) -> Result<Stat, Error> {
        let a = call("file.stat", self.params())?;
        Ok(Stat {
            exists: a["exists"] == true,
            size: a["size"].as_u64().unwrap_or(0),
            mtime_ms: a["mtime_ms"].as_i64().unwrap_or(0),
        })
    }

    /// Writes the whole content (`file.write`, 1 MiB at most): needs the
    /// manifest's `write = true` and `files-write`, else `denied`.
    /// `if_mtime_ms`: refused (`not-usable`) if the file changed since.
    /// Its stat after.
    pub fn write(&self, text: &str, if_mtime_ms: Option<i64>) -> Result<Stat, Error> {
        self.put(json!(text), "text", if_mtime_ms)
    }

    /// `write` of bytes.
    pub fn write_bytes(&self, bytes: &[u8], if_mtime_ms: Option<i64>) -> Result<Stat, Error> {
        self.put(json!(base64::encode(bytes)), "base64", if_mtime_ms)
    }

    fn put(&self, content: Value, key: &str, if_mtime_ms: Option<i64>) -> Result<Stat, Error> {
        let mut p = self.params();
        p[key] = content;
        if let Some(m) = if_mtime_ms {
            p["if_mtime_ms"] = json!(m as f64);
        }
        let a = call("file.write", p)?;
        Ok(Stat {
            exists: true,
            size: a["size"].as_u64().unwrap_or(0),
            mtime_ms: a["mtime_ms"].as_i64().unwrap_or(0),
        })
    }

    /// The names of the regular files of a directory (`file.list`).
    pub fn list(&self) -> Result<Vec<String>, Error> {
        Ok(names(&call("file.list", self.params())?["entries"], "name"))
    }
}

/// `app.result`: what `ptyrc app ID --wait` prints, and its exit
/// status (0..=255). The last one holds; then `exit`.
pub fn result(value: Value, status: u8) -> Result<(), Error> {
    call("app.result", json!({ "value": value, "status": status })).map(drop)
}

#[cfg(test)]
mod tests {
    use super::clock::{Civil, civil};
    use super::*;
    use crate::native;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn base64_both_ways() {
        for (raw, text) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
        ] {
            assert_eq!(base64::encode(raw.as_bytes()), text);
            assert_eq!(base64::decode(text).as_deref(), Some(raw.as_bytes()));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64::decode(&base64::encode(&all)), Some(all));
        assert_eq!(base64::decode("Zg").as_deref(), Some(&b"f"[..]));
        for bad in ["Z", "Zh==", "Zg=", "Z!==", "Zm9v=", "Zg==Zg=="] {
            assert_eq!(base64::decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn civil_dates() {
        let at = |y, mo, d, h, mi, s, wd| Civil {
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            second: s,
            weekday: wd,
        };
        assert_eq!(civil(0, 0), at(1970, 1, 1, 0, 0, 0, 4));
        // 2026-11-20T12:00:00Z, a Friday; in Paris (+1 h).
        assert_eq!(
            civil(1_795_176_000_123, 3600),
            at(2026, 11, 20, 13, 0, 0, 5)
        );
        assert_eq!(civil(-1, 0), at(1969, 12, 31, 23, 59, 59, 3));
        assert_eq!(civil(951_782_400_000, 0), at(2000, 2, 29, 0, 0, 0, 2));
        assert_eq!(
            civil(1_795_176_000_000, -13 * 3600),
            at(2026, 11, 19, 23, 0, 0, 4)
        );
    }

    type Log = Rc<RefCell<Vec<(String, Value)>>>;

    /// Answers as the driver does (`ptyrc-box/src/hostfn.rs`); a file of
    /// 3 MiB to read by pieces.
    fn host() -> Log {
        let log: Log = Rc::default();
        let l = log.clone();
        let big: Vec<u8> = (0..3 << 20).map(|i| (i % 251) as u8).collect();
        native::serve(move |method, p| {
            l.borrow_mut().push((method.to_string(), p.clone()));
            Ok(match method {
                "clock.now" => json!({"unix_ms": 1_795_867_200_123i64, "zone": "Europe/Paris",
                    "offset_s": 3600, "abbr": "CET", "next": 1_806_195_600_000i64}),
                "clock.zone" => json!({"zone": "UTC", "offsets": [
                    {"at": 0, "offset_s": 0, "abbr": "UTC", "next": null}], "degree": "none"}),
                "storage.get" => {
                    json!({ "value": (if p["key"] == "a" { Some("1") } else { None }) })
                }
                "storage.set" => json!({"used": 5, "quota": 1_048_576}),
                "storage.list" => json!({"keys": [{"key": "a", "size": 1}], "used": 1, "quota": 9}),
                "asset.list" => json!({"assets": [{"name": "p/ship.pic", "size": 3}]}),
                "file.list" => json!({"entries": [{"name": "x.txt", "size": 3}]}),
                "file.stat" => json!({"exists": true, "size": 3, "mtime_ms": 7}),
                "file.write" => json!({"size": 2, "mtime_ms": 8}),
                "file.read" | "asset.read" if p["as"] == "text" => {
                    json!({"text": "héllo", "size": 6, "eof": p["file"] != "f1"})
                }
                "file.read" | "asset.read" => {
                    let at = p["offset"].as_u64().unwrap() as usize;
                    let end = (at + p["len"].as_u64().unwrap() as usize).min(big.len());
                    json!({"base64": (base64::encode(&big[at..end])), "size": (big.len()),
                           "eof": (end == big.len())})
                }
                _ => json!({}),
            })
        });
        log
    }

    #[test]
    fn requests_and_answers() {
        let log = host();
        let wall = clock::now().unwrap();
        assert_eq!(
            (wall.unix_ms, wall.offset_s, wall.abbr.as_str()),
            (1_795_867_200_123, 3600, "CET")
        );
        assert_eq!((wall.next, wall.degree), (Some(1_806_195_600_000), None));
        let z = clock::zone("Asia/Tokyo", &[0, 1]).unwrap();
        assert_eq!((z.len(), z[0].next), (1, None));
        assert_eq!(storage::get("a").unwrap().as_deref(), Some("1"));
        assert_eq!(storage::get("b").unwrap(), None);
        assert_eq!(storage::set("a", "xy").unwrap(), 5);
        storage::remove("a").unwrap();
        assert_eq!(storage::keys().unwrap(), ["a"]);
        assert_eq!(asset::list().unwrap(), ["p/ship.pic"]);
        assert_eq!(asset::text("p/ship.pic").unwrap(), "héllo");
        result(json!(["done"]), 3).unwrap();
        let sent: Vec<_> = log.borrow_mut().drain(..).collect();
        assert_eq!(sent[1].1, json!({"zone": "Asia/Tokyo", "at": [0, 1]}));
        assert_eq!(sent[4].1, json!({"key": "a", "value": "xy"}));
        assert_eq!(sent[8].1, json!({"name": "p/ship.pic", "as": "text"}));
        assert_eq!(
            sent[9],
            ("app.result".into(), json!({"value": ["done"], "status": 3}))
        );

        let arg = json!({"file": "f0", "name": "TODO.md"});
        let f = File::of(&arg).unwrap();
        assert_eq!((f.handle.as_str(), f.name.as_str()), ("f0", "TODO.md"));
        assert_eq!(
            File::all(&json!([arg, {"file": "f1", "name": "b"}])).len(),
            2
        );
        assert_eq!(File::of(&json!("TODO.md")), None);
        assert_eq!(
            f.stat().unwrap(),
            Stat {
                exists: true,
                size: 3,
                mtime_ms: 7
            }
        );
        assert_eq!(f.write("hi", Some(7)).unwrap().mtime_ms, 8);
        f.write_bytes(&[0, 255], None).unwrap();
        let d = File::new("d0");
        assert_eq!(d.list().unwrap(), ["x.txt"]);
        d.child("x.txt").stat().unwrap();
        let sent: Vec<_> = log.borrow_mut().drain(..).collect();
        assert_eq!(
            sent[1].1,
            json!({"file": "f0", "text": "hi", "if_mtime_ms": 7})
        );
        assert_eq!(sent[2].1, json!({"file": "f0", "base64": "AP8="}));
        assert_eq!(sent[4].1, json!({"file": "d0", "name": "x.txt"}));

        // 3 MiB: three pieces of 1 MiB; text not whole in one answer: by bytes.
        let b = File::new("f1").bytes().unwrap();
        assert_eq!((b.len(), b[300]), (3 << 20, (300 % 251) as u8));
        assert_eq!(log.borrow_mut().drain(..).count(), 3);
        assert_eq!(File::new("f1").text().unwrap(), String::from_utf8_lossy(&b));
    }

    #[test]
    fn no_host_natively() {
        let e = storage::get("a").unwrap_err();
        assert_eq!(e.code, "native");
    }
}
