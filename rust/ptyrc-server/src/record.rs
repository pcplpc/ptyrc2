// SPDX-License-Identifier: GPL-3.0-or-later
//! The tape: a session as a log of bursts with their times.
//!
//! UTF-8 JSON, one object per line. The first line is the header:
//!
//! ```text
//! {"ptyrc":"tape","version":1,"size":[80,24],"argv":["less","f"]}
//! ```
//!
//! Then one record per line, `t` in microseconds since the driver started,
//! never decreasing:
//!
//! ```text
//! {"t":1520,"o":"text"}       a burst of app output, valid UTF-8
//! {"t":1520,"ob":"base64"}    a burst that is not valid UTF-8
//! {"t":9000,"i":"j"}          bytes typed into the app (`ib`: base64)
//! {"t":9100,"size":[100,30]}  a resize
//! {"t":9900,"exit":0}         the app exited
//! ```
//!
//! Unknown members and unknown records are ignored by a reader.

use crate::keys::{base64_decode, base64_encode};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

/// Version of the format written.
pub const VERSION: u64 = 1;

/// Writes a tape. A write that fails ends the recording, not the session.
pub struct Recorder {
    file: Option<File>,
    input: bool,
}

impl Recorder {
    /// Create `path` and write the header. `input`: typed bytes are
    /// recorded too.
    pub fn create(
        path: &Path,
        size: (u16, u16),
        argv: &[String],
        input: bool,
    ) -> io::Result<Recorder> {
        let mut recorder = Recorder {
            file: Some(File::create(path)?),
            input,
        };
        recorder.line(&json!({
            "ptyrc": "tape", "version": VERSION, "size": [size.0, size.1], "argv": argv,
        }));
        Ok(recorder)
    }

    fn line(&mut self, value: &Value) {
        if let Some(file) = &mut self.file {
            let mut line = value.to_string();
            line.push('\n');
            if file.write_all(line.as_bytes()).is_err() {
                self.file = None;
            }
        }
    }

    fn bytes(&mut self, t: Duration, key: &str, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let t = t.as_micros() as u64;
        let value = match std::str::from_utf8(bytes) {
            Ok(text) => json!({"t": t, key: text}),
            Err(_) => json!({"t": t, format!("{key}b"): base64_encode(bytes)}),
        };
        self.line(&value);
    }

    /// A burst of app output.
    pub fn output(&mut self, t: Duration, bytes: &[u8]) {
        self.bytes(t, "o", bytes);
    }

    /// Bytes typed into the app.
    pub fn input(&mut self, t: Duration, bytes: &[u8]) {
        if self.input {
            self.bytes(t, "i", bytes);
        }
    }

    pub fn resize(&mut self, t: Duration, cols: u16, rows: u16) {
        self.line(&json!({"t": t.as_micros() as u64, "size": [cols, rows]}));
    }

    pub fn exit(&mut self, t: Duration, status: i32) {
        self.line(&json!({"t": t.as_micros() as u64, "exit": status}));
    }
}

/// One record of a tape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    Output(Vec<u8>),
    Input(Vec<u8>),
    Resize(u16, u16),
    Exit(i32),
}

/// A tape read whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tape {
    /// `(cols, rows)` at the start.
    pub size: (u16, u16),
    pub argv: Vec<String>,
    pub records: Vec<(Duration, Record)>,
}

fn size_of(value: &Value) -> Option<(u16, u16)> {
    let size = value.as_array().filter(|s| s.len() == 2)?;
    let dim = |i: usize| size[i].as_u64().filter(|n| (1..=4096).contains(n));
    Some((dim(0)? as u16, dim(1)? as u16))
}

impl Tape {
    /// Read a tape. The error says the line and what is wrong with it.
    pub fn load(path: &Path) -> Result<Tape, String> {
        let text = std::fs::read_to_string(path).map_err(|e| crate::start::describe(&e))?;
        Tape::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Tape, String> {
        let mut lines = text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let not_a_tape = "not a tape: the first line is not {\"ptyrc\":\"tape\",...}";
        let (_, first) = lines.next().ok_or("the tape is empty")?;
        let header: Value = serde_json::from_str(first).map_err(|_| not_a_tape)?;
        if header["ptyrc"] != "tape" {
            return Err(not_a_tape.into());
        }
        match header["version"].as_u64() {
            Some(VERSION) => {}
            Some(v) => return Err(format!("tape version {v}: this ptyrc reads {VERSION}")),
            None => return Err("the header has no version".into()),
        }
        let size = size_of(&header["size"]).ok_or("the header has no size: [cols, rows]")?;
        let argv = header["argv"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut records = Vec::new();
        let mut last = Duration::ZERO;
        for (n, line) in lines {
            let bad = |what: &str| format!("line {}: {what}", n + 1);
            let value: Value = serde_json::from_str(line).map_err(|e| bad(&e.to_string()))?;
            let t = value["t"].as_u64().ok_or_else(|| bad("no time `t`"))?;
            let t = Duration::from_micros(t);
            if t < last {
                return Err(bad("time goes backwards"));
            }
            last = t;
            let text = |key: &str| value[key].as_str().map(|s| s.as_bytes().to_vec());
            let b64 = |key: &str| -> Result<Option<Vec<u8>>, String> {
                match value[key].as_str() {
                    Some(s) => base64_decode(s)
                        .map(Some)
                        .ok_or_else(|| bad(&format!("`{key}` is not base64"))),
                    None => Ok(None),
                }
            };
            let record = if let Some(bytes) = text("o").or(b64("ob")?) {
                Record::Output(bytes)
            } else if let Some(bytes) = text("i").or(b64("ib")?) {
                Record::Input(bytes)
            } else if !value["size"].is_null() {
                let (cols, rows) = size_of(&value["size"]).ok_or_else(|| bad("size: [c, r]"))?;
                Record::Resize(cols, rows)
            } else if let Some(status) = value["exit"].as_i64() {
                Record::Exit(status as i32)
            } else {
                continue;
            };
            records.push((t, record));
        }
        Ok(Tape {
            size,
            argv,
            records,
        })
    }

    /// Time of the last record.
    pub fn end(&self) -> Duration {
        self.records.last().map_or(Duration::ZERO, |(t, _)| *t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let path = std::env::temp_dir().join(format!("ptyrc-tape-{}", std::process::id()));
        let mut rec = Recorder::create(&path, (80, 24), &["sh".into()], true).unwrap();
        rec.output(Duration::from_micros(10), b"a\x1b[1mb\n");
        rec.output(Duration::from_micros(20), b"\xe2\x94");
        rec.input(Duration::from_micros(30), b"j");
        rec.resize(Duration::from_micros(40), 100, 30);
        rec.exit(Duration::from_micros(50), 3);
        drop(rec);
        let tape = Tape::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(tape.size, (80, 24));
        assert_eq!(tape.argv, ["sh"]);
        let records: Vec<Record> = tape.records.iter().map(|(_, r)| r.clone()).collect();
        assert_eq!(
            records,
            [
                Record::Output(b"a\x1b[1mb\n".to_vec()),
                Record::Output(b"\xe2\x94".to_vec()),
                Record::Input(b"j".to_vec()),
                Record::Resize(100, 30),
                Record::Exit(3),
            ]
        );
        assert_eq!(tape.end(), Duration::from_micros(50));
    }

    #[test]
    fn refusals() {
        assert_eq!(Tape::parse("").unwrap_err(), "the tape is empty");
        assert!(
            Tape::parse("hello\n")
                .unwrap_err()
                .starts_with("not a tape")
        );
        let head = r#"{"ptyrc":"tape","version":1,"size":[80,24]}"#;
        assert!(Tape::parse(head).unwrap().records.is_empty());
        let e = Tape::parse(&format!(
            "{head}\n{{\"t\":5,\"o\":\"a\"}}\n{{\"t\":4,\"o\":\"b\"}}"
        ));
        assert_eq!(e.unwrap_err(), "line 3: time goes backwards");
        let e = Tape::parse(r#"{"ptyrc":"tape","version":9,"size":[80,24]}"#);
        assert_eq!(e.unwrap_err(), "tape version 9: this ptyrc reads 1");
        // An unknown record is skipped.
        let tape = Tape::parse(&format!("{head}\n{{\"t\":5,\"mark\":\"x\"}}")).unwrap();
        assert!(tape.records.is_empty());
    }
}
