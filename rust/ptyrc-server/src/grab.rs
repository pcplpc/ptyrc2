// SPDX-License-Identifier: GPL-3.0-or-later
//! Keys taken from the host keyboard by a client, before the app.

use crate::engine::ConnId;
use crate::keys;
use std::collections::HashSet;

/// The key that releases a grab when none is given.
pub(crate) const ESCAPE: &str = "ctrl+]";

/// One client holds the keyboard, or some keys of it.
pub(crate) struct Grab {
    pub conn: ConnId,
    /// Names of the keys taken; `None`: all.
    keys: Option<HashSet<String>>,
    /// Always releases; never reaches the client nor the app.
    escape: Vec<u8>,
}

/// What becomes of keyboard bytes under a grab.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Sorted {
    /// For the app.
    pub forward: Vec<u8>,
    /// For the client: name (or `None`) and bytes of each key.
    pub taken: Vec<(Option<String>, Vec<u8>)>,
    /// The escape key was pressed: what follows it is in `forward`.
    pub released: bool,
}

impl Grab {
    pub fn new(conn: ConnId, keys: Option<Vec<String>>, escape: &str) -> Result<Grab, String> {
        let escape = match keys::encode_key(escape, false) {
            Ok(bytes) if name(&bytes).is_some_and(|n| n.len() > 1) => bytes,
            _ => return Err(format!("escape: not a key: {escape}")),
        };
        Ok(Grab {
            conn,
            keys: keys.map(|k| k.into_iter().collect()),
            escape,
        })
    }

    pub fn sort(&self, bytes: &[u8]) -> Sorted {
        let mut sorted = Sorted::default();
        let mut rest = bytes;
        while !rest.is_empty() {
            let (key, tail) = rest.split_at(key_len(rest));
            rest = tail;
            if key == self.escape {
                sorted.released = true;
                sorted.forward.extend_from_slice(rest);
                break;
            }
            let name = name(key);
            let taken = match (&self.keys, &name) {
                (None, _) => true,
                (Some(set), Some(name)) => set.contains(name),
                (Some(_), None) => false,
            };
            if taken {
                sorted.taken.push((name, key.to_vec()));
            } else {
                sorted.forward.extend_from_slice(key);
            }
        }
        sorted
    }
}

/// Length of the first key of `bytes`: a control sequence, an escaped
/// character, a UTF-8 character, a byte.
fn key_len(bytes: &[u8]) -> usize {
    let utf8 = |b: u8| match b {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    };
    let len = match bytes {
        [0x1b, b'[', rest @ ..] => {
            // X10 mouse: three bytes after `M`.
            if rest.first() == Some(&b'M') {
                6
            } else {
                let body = rest
                    .iter()
                    .take_while(|b| (0x20..0x40).contains(*b))
                    .count();
                3 + body
            }
        }
        [0x1b, b'O', _, ..] => 3,
        [0x1b, b, ..] if *b != 0x1b => 1 + utf8(*b),
        [b, ..] => utf8(*b),
        [] => 0,
    };
    len.min(bytes.len())
}

/// Name of a key, as `input.send` takes it; a printable character is its
/// own name.
pub(crate) fn name(key: &[u8]) -> Option<String> {
    const NAMES: [&str; 27] = [
        "enter",
        "tab",
        "esc",
        "backspace",
        "delete",
        "insert",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "shift+tab",
        "f1",
        "f2",
        "f3",
        "f4",
        "f5",
        "f6",
        "f7",
        "f8",
        "f9",
        "f10",
        "f11",
        "f12",
    ];
    for name in NAMES {
        for app_cursor in [false, true] {
            if keys::encode_key(name, app_cursor).is_ok_and(|b| b == key) {
                return Some(name.to_string());
            }
        }
    }
    match key {
        [b' '] => Some("space".into()),
        [0] => Some("ctrl+space".into()),
        [b @ 1..=26] => Some(format!("ctrl+{}", (b'a' + b - 1) as char)),
        [b @ 28..=31] => Some(format!("ctrl+{}", (b'@' + b) as char)),
        [0x1b, rest @ ..] if !rest.is_empty() && rest[0] != b'[' && rest[0] != b'O' => {
            name(rest).map(|n| format!("alt+{n}"))
        }
        _ => std::str::from_utf8(key)
            .ok()
            .filter(|s| s.chars().count() == 1 && !s.chars().any(char::is_control))
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for (bytes, expected) in [
            (&b"\x1b[A"[..], "up"),
            (b"\x1bOA", "up"),
            (b"\r", "enter"),
            (b"\x1b", "esc"),
            (b"\x1b[5~", "pageup"),
            (b"\x1b[Z", "shift+tab"),
            (b"\x03", "ctrl+c"),
            (b"\x1d", "ctrl+]"),
            (b"a", "a"),
            (b" ", "space"),
            ("é".as_bytes(), "é"),
            (b"\x1bx", "alt+x"),
            (b"\x1b\r", "alt+enter"),
        ] {
            assert_eq!(name(bytes).as_deref(), Some(expected), "{bytes:?}");
            // A name goes back to the same bytes.
            let back = keys::encode_key(expected, bytes.starts_with(b"\x1bO")).unwrap();
            assert_eq!(back, bytes, "{expected}");
        }
        assert_eq!(name(b"\x1b[<0;3;4M"), None);
    }

    #[test]
    fn a_set_of_keys_is_taken_the_rest_goes_on() {
        let keys = Some(vec!["up".into(), "down".into()]);
        let grab = Grab::new(ConnId(1), keys, ESCAPE).unwrap();
        let sorted = grab.sort(b"a\x1b[Ab\x1bOB\x1b[1;5C");
        assert_eq!(sorted.forward, b"ab\x1b[1;5C");
        let names: Vec<_> = sorted
            .taken
            .iter()
            .map(|(n, _)| n.clone().unwrap())
            .collect();
        assert_eq!(names, ["up", "down"]);
        assert!(!sorted.released);
    }

    #[test]
    fn everything_is_taken_until_the_escape() {
        let grab = Grab::new(ConnId(1), None, ESCAPE).unwrap();
        let sorted = grab.sort("xé\x1b[M !!\x1dyz".as_bytes());
        assert_eq!(sorted.taken.len(), 3);
        assert_eq!(sorted.taken[2], (None, b"\x1b[M !!".to_vec()));
        assert!(sorted.released);
        assert_eq!(sorted.forward, b"yz");
        assert!(Grab::new(ConnId(1), None, "q").is_err());
        assert!(Grab::new(ConnId(1), None, "ctrl+xx").is_err());
        assert!(Grab::new(ConnId(1), None, "f2").is_ok());
    }
}
