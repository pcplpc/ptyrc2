// SPDX-License-Identifier: GPL-3.0-or-later
//! The assets of an app: the tree under `[app] assets`, read with the
//! scripts before the OS profile and answered by the runner
//! (`asset.list`, `asset.read`), live and headless alike.

use crate::core::CallError;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// The tree, bytes in all, at most.
pub const MAX_TREE: u64 = 8 << 20;
/// Content of one answer, at most.
pub const MAX_READ: usize = 1 << 20;
/// Components of a name, at most.
pub const MAX_DEPTH: usize = 4;

pub const NAME_RULE: &str = "components of [A-Za-z0-9._-], 1 to 64 bytes, none starting with \
    `.`, `/` between them, 4 at most";

/// A component of a name: 1 to 64 bytes of `[A-Za-z0-9._-]`, not
/// starting with `.` (so neither `.` nor `..`).
pub fn valid_component(c: &str) -> bool {
    (1..=64).contains(&c.len())
        && !c.starts_with('.')
        && c.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// A name of an asset: components by `valid_component`, `/` between,
/// `MAX_DEPTH` at most.
pub fn valid_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    parts.len() <= MAX_DEPTH && parts.iter().all(|p| valid_component(p))
}

/// The assets, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assets {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Assets {
    /// The regular files under `dir` whose names keep the rule; a link
    /// is not followed, a name outside the rule is skipped. More than
    /// `MAX_TREE` bytes: refused.
    pub fn read(dir: &Path) -> Result<Assets, String> {
        let mut assets = Assets::default();
        let mut total = 0u64;
        walk(dir, "", 0, &mut assets, &mut total)?;
        Ok(assets)
    }

    /// `asset.list`, `asset.read`.
    pub fn call(&self, method: &str, params: &Value) -> Result<Value, CallError> {
        match method {
            "asset.list" => {
                let list: Vec<Value> = (self.files.iter())
                    .map(|(name, bytes)| json!({"name": name, "size": bytes.len()}))
                    .collect();
                Ok(json!({"assets": list}))
            }
            "asset.read" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                if !valid_name(name) {
                    return Err(CallError {
                        hint: Some(format!("a name: {NAME_RULE}")),
                        ..CallError::new("bad-params", "asset.read: name")
                    });
                }
                let Some(bytes) = self.files.get(name) else {
                    let near = ptyrc_proto::suggest::did_you_mean(
                        name,
                        self.files.keys().map(String::as_str),
                    );
                    return Err(CallError {
                        hint: Some(format!("{near}asset.list names them")),
                        ..CallError::new("no-file", format!("no asset {name}"))
                    });
                };
                read_answer(bytes, params, "asset.read")
            }
            _ => Err(CallError::new(
                "unknown-method",
                format!("unknown method: {method}"),
            )),
        }
    }
}

/// `{"text" | "base64", "size", "eof"}` of `bytes` from `offset`, `len`
/// bytes at most (`MAX_READ`); `as`: `text` (default) or `base64`.
pub fn read_answer(bytes: &[u8], params: &Value, method: &str) -> Result<Value, CallError> {
    let num = |k: &str, default: u64| match params.get(k) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_u64().ok_or_else(|| {
            CallError::new("bad-params", format!("{method}: {k}: a count of bytes"))
        }),
    };
    let offset = num("offset", 0)?;
    let len = num("len", MAX_READ as u64)?.min(MAX_READ as u64);
    let start = (offset as usize).min(bytes.len());
    let end = start.saturating_add(len as usize).min(bytes.len());
    let piece = &bytes[start..end];
    let how = match params.get("as") {
        None | Some(Value::Null) => Some("text"),
        Some(v) => v.as_str(),
    };
    let (key, content) = match how {
        Some("text") => ("text", String::from_utf8_lossy(piece).into_owned()),
        Some("base64") => ("base64", base64_encode(piece)),
        _ => {
            return Err(CallError {
                hint: Some("\"text\" (UTF-8, invalid bytes replaced) or \"base64\"".into()),
                ..CallError::new("bad-params", format!("{method}: as"))
            });
        }
    };
    let mut out = json!({"size": bytes.len(), "eof": end == bytes.len()});
    out[key] = json!(content);
    Ok(out)
}

pub fn base64_encode(bytes: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            match i <= chunk.len() {
                true => out.push(B64[(n >> (18 - 6 * i)) as usize & 63] as char),
                false => out.push('='),
            }
        }
    }
    out
}

/// Bytes of a base64 text (padding optional); `None`: not base64.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for b in text.bytes().filter(|b| *b != b'=') {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn walk(
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut Assets,
    total: &mut u64,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !valid_component(&name) {
            continue;
        }
        let full = match prefix.is_empty() {
            true => name,
            false => format!("{prefix}/{name}"),
        };
        // `file_type` does not follow a link.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth + 1 < MAX_DEPTH {
            walk(&entry.path(), &full, depth + 1, out, total)?;
        } else if kind.is_file() {
            let bytes = read_regular(&entry.path())?;
            *total += bytes.len() as u64;
            if *total > MAX_TREE {
                return Err(format!("assets: more than {MAX_TREE} bytes"));
            }
            out.files.insert(full, bytes);
        }
    }
    Ok(())
}

/// A regular file, its last component not followed.
fn read_regular(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    #[cfg(target_os = "linux")]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::File::options()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
    };
    #[cfg(not(target_os = "linux"))]
    let file = std::fs::File::open(path);
    let file = file.map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: not a regular file", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TREE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_by_the_rule() {
        for ok in ["a", "pictures/ship.pic", "a/b/c/d", "x-1_2.txt"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "",
            "../ptyrc-app.toml",
            "/etc/passwd",
            "a//b",
            ".x",
            "a/..",
            "a/b/c/d/e",
            "a b",
            "a\0b",
            &"x".repeat(65),
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn read_and_list() {
        let mut a = Assets::default();
        a.files.insert("p/ship.pic".into(), b"hello\xff".to_vec());
        let l = a.call("asset.list", &json!({})).unwrap();
        assert_eq!(l, json!({"assets": [{"name": "p/ship.pic", "size": 6}]}));
        let r = a
            .call("asset.read", &json!({"name": "p/ship.pic"}))
            .unwrap();
        assert_eq!(r, json!({"text": "hello\u{fffd}", "size": 6, "eof": true}));
        let r = (a.call(
            "asset.read",
            &json!({"name": "p/ship.pic", "as": "base64",
                                              "offset": 1, "len": 2}),
        ))
        .unwrap();
        assert_eq!(r, json!({"base64": "ZWw=", "size": 6, "eof": false}));
        let r = (a.call("asset.read", &json!({"name": "p/ship.pic", "offset": 99}))).unwrap();
        assert_eq!(r, json!({"text": "", "size": 6, "eof": true}));
        let e = a
            .call("asset.read", &json!({"name": "p/shp.pic"}))
            .unwrap_err();
        assert_eq!(e.code, "no-file");
        assert!(e.hint.unwrap().contains("p/ship.pic"));
        for bad in ["../ptyrc-app.toml", "/etc/passwd", "a//b"] {
            let e = a.call("asset.read", &json!({"name": bad})).unwrap_err();
            assert_eq!(e.code, "bad-params", "{bad}");
        }
        let e = (a.call("asset.read", &json!({"name": "p/ship.pic", "as": "hex"}))).unwrap_err();
        assert_eq!(e.code, "bad-params");
        assert_eq!(base64_decode("ZWw=").unwrap(), b"el");
    }

    #[test]
    fn a_tree_read_links_not_followed() {
        let dir = std::env::temp_dir().join(format!("ptyrc-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/c.txt"), "c").unwrap();
        std::fs::write(dir.join(".hidden"), "h").unwrap();
        std::fs::write(dir.join("bad name"), "h").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", dir.join("passwd")).unwrap();
        let a = Assets::read(&dir).unwrap();
        let names: Vec<&str> = a.files.keys().map(String::as_str).collect();
        assert_eq!(names, ["a/b/c.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
