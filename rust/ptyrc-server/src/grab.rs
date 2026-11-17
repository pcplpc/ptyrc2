// SPDX-License-Identifier: GPL-3.0-or-later
//! Keys taken from the host keyboard by a client, before the app.

use crate::engine::ConnId;
use crate::keys;
use std::collections::HashSet;

/// The key that releases a grab when none is given.
pub(crate) const ESCAPE: &str = "ctrl+t";

/// One client holds the keyboard, or some keys of it.
pub(crate) struct Grab {
    pub conn: ConnId,
    /// Names of the keys taken; `None`: all.
    keys: Option<HashSet<String>>,
    /// Name of the key that always releases; it never reaches the
    /// client nor the app. Empty: none.
    escape: String,
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
    /// The close key was pressed: what follows it is in `forward`.
    pub closed: bool,
}

/// An escape key: a named key that is not text. Its name, as `name`
/// gives it: the key is matched by name, whatever bytes the host sends
/// for it (application cursor keys, CSI u).
pub(crate) fn escape_key(escape: &str) -> Result<String, String> {
    named_key("escape", escape)
}

/// A named key that is not text, as `escape_key`; `member` names it in
/// the error.
pub(crate) fn named_key(member: &str, key: &str) -> Result<String, String> {
    let named = keys::encode_key(key, false)
        .ok()
        .and_then(|bytes| name(&bytes));
    match named {
        Some(name) if name.chars().count() > 1 => Ok(name),
        _ => Err(format!("{member}: not a key: {key}")),
    }
}

/// A report of the host that is no key: the mouse, the focus of its
/// window (1004).
pub(crate) fn is_report(key: &[u8]) -> bool {
    key.starts_with(b"\x1b[<") || key.starts_with(b"\x1b[M") || key == b"\x1b[I" || key == b"\x1b[O"
}

/// `bytes` less a key release at their start (kitty keyboard protocol).
pub(crate) fn skip_release(bytes: &[u8]) -> &[u8] {
    let len = key_len(bytes);
    match is_release(&bytes[..len]) {
        true => &bytes[len..],
        false => bytes,
    }
}

/// Sort keys: those `taken` says, up to the key named `escape` or
/// `close` (empty: none). `reports`: mouse and focus reports are taken
/// as keys; else they go on.
pub(crate) fn sort_keys(
    bytes: &[u8],
    escape: &str,
    close: &str,
    reports: bool,
    taken: impl Fn(Option<&str>) -> bool,
) -> Sorted {
    let mut sorted = Sorted::default();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (key, tail) = rest.split_at(key_len(rest));
        rest = tail;
        if !reports && is_report(key) {
            sorted.forward.extend_from_slice(key);
            continue;
        }
        let name = name(key);
        let is = |stop: &str| !stop.is_empty() && name.as_deref() == Some(stop);
        if is(escape) || is(close) {
            sorted.released = is(escape);
            sorted.closed = !sorted.released;
            // Its release event reaches nobody either.
            sorted.forward.extend_from_slice(skip_release(rest));
            break;
        }
        if taken(name.as_deref()) {
            sorted.taken.push((name, key.to_vec()));
        } else {
            sorted.forward.extend_from_slice(key);
        }
    }
    sorted
}

/// Length of what `bytes` ends with and is not whole: a lone escape,
/// an unfinished control sequence, a cut UTF-8 character. The rest of
/// it may come with the next read.
pub(crate) fn incomplete(bytes: &[u8]) -> usize {
    let mut rest = bytes;
    let mut last: &[u8] = &[];
    while !rest.is_empty() {
        let (key, tail) = rest.split_at(key_len(rest));
        (last, rest) = (key, tail);
    }
    let whole = match last {
        [0x1b] | [0x1b, 0x1b] | [0x1b, b'[' | b'O'] | [0x1b, 0x1b, b'[' | b'O'] => false,
        [0x1b, b'[', b'M', coords @ ..] => coords.len() == 3,
        [0x1b, b'[', .., end] | [0x1b, 0x1b, b'[', .., end] => *end >= 0x40,
        [.., end] if *end >= 0x80 => {
            std::str::from_utf8(last).is_ok()
                || std::str::from_utf8(last).is_err_and(|e| e.error_len().is_some())
        }
        _ => true,
    };
    if whole { 0 } else { last.len() }
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// A piece of the keyboard stream.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    Keys(Vec<u8>),
    /// Pasted text (bracketed paste), a part of it at least; `start`,
    /// `end`: its markers came with it.
    Paste {
        text: Vec<u8>,
        start: bool,
        end: bool,
    },
}

/// Cut keyboard bytes at the markers of a bracketed paste. `pasting`:
/// a paste is under way, from a read to the next.
pub(crate) fn pieces(bytes: &[u8], pasting: &mut bool) -> Vec<Piece> {
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let mut out = Vec::new();
    let mut rest = bytes;
    let mut start = false;
    while !rest.is_empty() {
        if *pasting {
            let end = find(rest, PASTE_END);
            let text = rest[..end.unwrap_or(rest.len())].to_vec();
            rest = &rest[end.map_or(rest.len(), |at| at + PASTE_END.len())..];
            *pasting = end.is_none();
            out.push(Piece::Paste {
                text,
                start,
                end: end.is_some(),
            });
            start = false;
        } else {
            let at = find(rest, PASTE_START);
            if at != Some(0) {
                out.push(Piece::Keys(rest[..at.unwrap_or(rest.len())].to_vec()));
            }
            rest = &rest[at.map_or(rest.len(), |at| at + PASTE_START.len())..];
            (*pasting, start) = (at.is_some(), at.is_some());
        }
    }
    if start {
        // The marker ends the read: the text is to come.
        out.push(Piece::Paste {
            text: Vec::new(),
            start: true,
            end: false,
        });
    }
    out
}

impl Piece {
    /// The bytes as the host sent them.
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Piece::Keys(keys) => keys.clone(),
            Piece::Paste { text, start, end } => {
                let mut out = Vec::new();
                if *start {
                    out.extend_from_slice(PASTE_START);
                }
                out.extend_from_slice(text);
                if *end {
                    out.extend_from_slice(PASTE_END);
                }
                out
            }
        }
    }
}

impl Grab {
    pub fn new(conn: ConnId, keys: Option<Vec<String>>, escape: &str) -> Result<Grab, String> {
        Ok(Grab {
            conn,
            keys: keys.map(|k| k.into_iter().collect()),
            escape: escape_key(escape)?,
        })
    }

    pub fn sort(&self, bytes: &[u8]) -> Sorted {
        sort_keys(bytes, &self.escape, "", true, |name| self.takes(name))
    }

    /// Pasted bytes: no escape key among them.
    pub fn sort_paste(&self, bytes: &[u8]) -> Sorted {
        match &self.keys {
            // Every key: the text, key by key, as ever.
            None => sort_keys(bytes, "", "", true, |_| true),
            // Some keys: a paste is not typed, it is the app's.
            Some(_) => Sorted {
                forward: bytes.to_vec(),
                ..Sorted::default()
            },
        }
    }

    fn takes(&self, name: Option<&str>) -> bool {
        match (&self.keys, name) {
            (None, _) => true,
            (Some(set), Some(name)) => set.contains(name),
            (Some(_), None) => false,
        }
    }
}

/// Length of the first key of `bytes`: a control sequence, an escaped
/// character, a UTF-8 character, a byte.
pub fn key_len(bytes: &[u8]) -> usize {
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
        // Alt and a key of a sequence; else alt+esc.
        [0x1b, 0x1b, b'[' | b'O', _, ..] => 1 + key_len(&bytes[1..]),
        [0x1b, 0x1b, ..] => 2,
        [0x1b, b, ..] => 1 + utf8(*b),
        [b, ..] => utf8(*b),
        [] => 0,
    };
    len.min(bytes.len())
}

/// Legacy bytes of a key sent as `CSI code;mods u` (kitty keyboard
/// protocol) or `CSI 27;mods;code ~` (xterm modifyOtherKeys), when it has
/// some. A release event has none.
pub(crate) fn legacy(key: &[u8]) -> Option<Vec<u8>> {
    let params = std::str::from_utf8(key.strip_prefix(b"\x1b[")?).ok()?;
    let (code, mods): (u32, u32) = if let Some(params) = params.strip_suffix('u') {
        let mut fields = params.split(';');
        let code = fields.next()?.split(':').next()?.parse().ok()?;
        let mut mods = fields.next().unwrap_or("").split(':');
        let level = match mods.next()? {
            "" => 1,
            level => level.parse().ok()?,
        };
        if mods.next() == Some("3") {
            return None;
        }
        (code, level)
    } else {
        let params = params.strip_prefix("27;")?.strip_suffix('~')?;
        let (level, code) = params.split_once(';')?;
        (code.parse().ok()?, level.parse().ok()?)
    };
    // Shift 1, alt 2, ctrl 4; caps lock and num lock are ignored.
    let bits = mods.checked_sub(1)? & !(64 | 128);
    if bits & !7 != 0 {
        return None;
    }
    let (shift, alt, ctrl) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
    let c = char::from_u32(code).filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c))?;
    let mut bytes = Vec::new();
    if alt {
        bytes.push(0x1b);
    }
    match (shift, ctrl) {
        (true, true) => return None,
        (true, false) if c == '\t' => bytes.extend(b"\x1b[Z"),
        (true, false) if c.is_ascii_lowercase() => bytes.push(c.to_ascii_uppercase() as u8),
        (true, false) => return None,
        (false, true) if c == ' ' => bytes.push(0),
        (false, true) => bytes.push(keys::ctrl(c.encode_utf8(&mut [0; 4]))?),
        (false, false) if matches!(c, '\r' | '\t' | '\x1b' | '\x7f') || !c.is_control() => {
            bytes.extend(c.encode_utf8(&mut [0; 4]).as_bytes())
        }
        (false, false) => return None,
    }
    Some(bytes)
}

/// A key release, as the kitty keyboard protocol reports it.
pub(crate) fn is_release(key: &[u8]) -> bool {
    key.starts_with(b"\x1b[") && key.ends_with(b":3u")
}

/// Name of a key, as `input.send` takes it; a printable character is its
/// own name. CSI u and modifyOtherKeys forms are named as their legacy
/// bytes.
pub fn name(key: &[u8]) -> Option<String> {
    if let Some(bytes) = legacy(key) {
        return name(&bytes);
    }
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
    if let Some(name) = modified(key) {
        return Some(name);
    }
    match key {
        [0x1b, rest @ ..] if rest.first() == Some(&0x1b) => name(rest).map(|n| format!("alt+{n}")),
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

/// Name of a special key the host sends with modifiers, as xterm
/// does: `CSI 1 ; m A`, `CSI n ; m ~`: `shift+up`, `ctrl+left`,
/// `ctrl+alt+shift+delete`. `keys::encode_key` writes the same bytes.
fn modified(key: &[u8]) -> Option<String> {
    let body = std::str::from_utf8(key.strip_prefix(b"\x1b[")?).ok()?;
    let (params, end) = body.split_at(body.len().checked_sub(1)?);
    let (code, mods) = params.split_once(';')?;
    let bits = mods.parse::<u8>().ok()?.checked_sub(1)?;
    if bits == 0 || bits > 7 {
        return None;
    }
    let base = keys::MODIFIED
        .iter()
        .find(|(_, c, e)| *c == code && *e == end)
        .map(|(name, _, _)| *name)?;
    let mut name = String::new();
    for (bit, prefix) in [(4, "ctrl+"), (2, "alt+"), (1, "shift+")] {
        if bits & bit != 0 {
            name.push_str(prefix);
        }
    }
    Some(name + base)
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
        let sorted = grab.sort("xé\x1b[M !!\x14yz".as_bytes());
        assert_eq!(sorted.taken.len(), 3);
        assert_eq!(sorted.taken[2], (None, b"\x1b[M !!".to_vec()));
        assert!(sorted.released);
        assert_eq!(sorted.forward, b"yz");
        assert!(Grab::new(ConnId(1), None, "q").is_err());
        assert!(Grab::new(ConnId(1), None, "ctrl+xx").is_err());
        assert!(Grab::new(ConnId(1), None, "f2").is_ok());
    }

    #[test]
    fn csi_u_and_modify_other_keys_are_named() {
        for (bytes, expected) in [
            (&b"\x1b[116;5u"[..], Some("ctrl+t")),
            (b"\x1b[116;5:1u", Some("ctrl+t")),
            (b"\x1b[116;69u", Some("ctrl+t")),
            (b"\x1b[27;5;116~", Some("ctrl+t")),
            (b"\x1b[93;5u", Some("ctrl+]")),
            (b"\x1b[32;5u", Some("ctrl+space")),
            (b"\x1b[120;3u", Some("alt+x")),
            (b"\x1b[97;2u", Some("A")),
            (b"\x1b[13u", Some("enter")),
            (b"\x1b[9;2u", Some("shift+tab")),
            (b"\x1b[27u", Some("esc")),
            (b"\x1b[116;5:3u", None),
            (b"\x1b[116;6u", None),
            (b"\x1b[116;9u", None),
            (b"\x1b[57399u", None),
        ] {
            assert_eq!(name(bytes).as_deref(), expected, "{bytes:?}");
        }
    }

    #[test]
    fn the_escape_releases_in_any_encoding() {
        let grab = Grab::new(ConnId(1), None, ESCAPE).unwrap();
        for escape in [&b"\x1b[116;5u"[..], b"\x1b[116;5:2u", b"\x1b[27;5;116~"] {
            let sorted = grab.sort(&[b"a", escape, b"b"].concat());
            assert!(sorted.released, "{escape:?}");
            assert_eq!(sorted.taken, [(Some("a".into()), b"a".to_vec())]);
            assert_eq!(sorted.forward, b"b");
        }
        // Its release event is not the escape.
        let sorted = grab.sort(b"\x1b[116;5:3u");
        assert!(!sorted.released);
        assert_eq!(sorted.taken, [(None, b"\x1b[116;5:3u".to_vec())]);
    }

    #[test]
    fn modified_special_keys_are_named() {
        for (bytes, expected) in [
            (&b"\x1b[1;2A"[..], Some("shift+up")),
            (b"\x1b[1;5D", Some("ctrl+left")),
            (b"\x1b[1;3H", Some("alt+home")),
            (b"\x1b[6;7~", Some("ctrl+alt+pagedown")),
            (b"\x1b[24;2~", Some("shift+f12")),
            (b"\x1b\x1b[A", Some("alt+up")),
            (b"\x1b\x1bOA", Some("alt+up")),
            (b"\x1b\x1b", Some("alt+esc")),
            (b"\x1b[1;1A", None),
            (b"\x1b[1;9A", None),
            (b"\x1b[7;2~", None),
        ] {
            assert_eq!(name(bytes).as_deref(), expected, "{bytes:?}");
        }
    }

    #[test]
    fn alt_and_escape_are_one_key() {
        assert_eq!(key_len(b"\x1b\x1b"), 2);
        assert_eq!(key_len(b"\x1b\x1b[Ax"), 4);
        assert_eq!(key_len(b"\x1b\x1bx"), 2);
        // Not an escape key and a stray escape for the app.
        let grab = Grab::new(ConnId(1), None, "esc").unwrap();
        let sorted = grab.sort(b"\x1b\x1b");
        assert!(!sorted.released && sorted.forward.is_empty());
        assert_eq!(sorted.taken[0].0.as_deref(), Some("alt+esc"));
    }

    #[test]
    fn the_escape_is_matched_by_name() {
        // Application cursor keys, a modifier: the same key.
        for (escape, typed) in [
            ("up", &b"\x1b[A"[..]),
            ("up", b"\x1bOA"),
            ("home", b"\x1bOH"),
            ("f1", b"\x1bOP"),
            ("shift+up", b"\x1b[1;2A"),
        ] {
            let grab = Grab::new(ConnId(1), None, escape).unwrap();
            let sorted = grab.sort(&[b"a", typed, b"b"].concat());
            assert!(sorted.released, "{escape}");
            assert_eq!(sorted.forward, b"b");
        }
        assert_eq!(escape_key("esc").as_deref(), Ok("esc"));
        assert!(escape_key("x").is_err() && escape_key("é").is_err());
        // The release event of the escape key goes with it.
        let grab = Grab::new(ConnId(1), None, ESCAPE).unwrap();
        let sorted = grab.sort(b"\x1b[116;5u\x1b[116;5:3ub");
        assert!(sorted.released);
        assert_eq!(sorted.forward, b"b");
    }

    #[test]
    fn reports_are_no_keys_of_a_focus() {
        let typed = b"a\x1b[<0;3;4M\x1b[I\x1b[M !!b\x1b[O\x1bc";
        let sorted = sort_keys(typed, "esc", "", false, |_| true);
        let names: Vec<_> = sorted.taken.iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(
            names,
            [Some("a".into()), Some("b".into()), Some("alt+c".into())]
        );
        assert_eq!(sorted.forward, b"\x1b[<0;3;4M\x1b[I\x1b[M !!\x1b[O");
        assert!(!sorted.released);
    }

    #[test]
    fn the_close_key_stops_as_the_escape() {
        // Legacy, kitty, modifyOtherKeys; its release event with it.
        for typed in [
            &b"\x18"[..],
            b"\x1b[120;5u",
            b"\x1b[120;5u\x1b[120;5:3u",
            b"\x1b[27;5;120~",
        ] {
            let sorted = sort_keys(
                &[b"a", typed, b"b"].concat(),
                "esc",
                "ctrl+x",
                false,
                |_| true,
            );
            assert!(sorted.closed && !sorted.released, "{typed:?}");
            assert_eq!(sorted.taken.len(), 1);
            assert_eq!(sorted.forward, b"b");
        }
        let sorted = sort_keys(b"\x1bb", "esc", "ctrl+x", false, |_| true);
        assert!(!sorted.closed && !sorted.released);
        let sorted = sort_keys(b"\x1b[27u\x18", "esc", "ctrl+x", false, |_| true);
        assert!(!sorted.closed && sorted.released);
        assert_eq!(sorted.forward, b"\x18");
        assert_eq!(named_key("close", "ctrl+x").as_deref(), Ok("ctrl+x"));
        assert!(named_key("close", "x").unwrap_err().starts_with("close:"));
    }

    #[test]
    fn unfinished_ends() {
        for (bytes, n) in [
            (&b"ab"[..], 0),
            (b"a\x1b", 1),
            (b"a\x1b\x1b", 2),
            (b"a\x1b[", 2),
            (b"a\x1b[1;5", 5),
            (b"a\x1b[200", 5),
            (b"a\x1bO", 2),
            (b"a\x1b[M !", 5),
            (b"a\x1b[M !!", 0),
            (b"a\x1b[A", 0),
            (b"a\x1b[<0;3;4M", 0),
            (b"a\x1bx", 0),
            (b"a\xc3", 1),
            (b"a\xe2\x82", 2),
            (b"a\xc3\xa9", 0),
            (b"a\xff", 0),
        ] {
            assert_eq!(incomplete(bytes), n, "{bytes:?}");
        }
    }

    #[test]
    fn a_paste_is_cut_out() {
        let keys = |t: &[u8]| Piece::Keys(t.to_vec());
        let paste = |t: &[u8], start, end| Piece::Paste {
            text: t.to_vec(),
            start,
            end,
        };
        let mut pasting = false;
        let got = pieces(b"a\x1b[200~b\x1b\x14\x14c\x1b[201~d", &mut pasting);
        assert_eq!(
            got,
            [keys(b"a"), paste(b"b\x1b\x14\x14c", true, true), keys(b"d")]
        );
        assert!(!pasting);
        assert_eq!(got[1].bytes(), b"\x1b[200~b\x1b\x14\x14c\x1b[201~");
        // Over three reads.
        assert_eq!(
            pieces(b"x\x1b[200~", &mut pasting),
            [keys(b"x"), paste(b"", true, false)]
        );
        assert!(pasting);
        assert_eq!(
            pieces(b"he\x1b", &mut pasting),
            [paste(b"he\x1b", false, false)]
        );
        assert_eq!(
            pieces(b"llo\x1b[201~", &mut pasting),
            [paste(b"llo", false, true)]
        );
        assert!(!pasting);
        // Two in a read.
        let got = pieces(b"\x1b[200~a\x1b[201~\x1b[200~b\x1b[201~", &mut pasting);
        assert_eq!(got, [paste(b"a", true, true), paste(b"b", true, true)]);
        // Under a grab: of every key, the text key by key and no escape;
        // of some keys, the app's.
        let grab = Grab::new(ConnId(1), None, ESCAPE).unwrap();
        let sorted = grab.sort_paste(b"a\x14b");
        assert_eq!((sorted.taken.len(), sorted.released), (3, false));
        let grab = Grab::new(ConnId(1), Some(vec!["up".into()]), ESCAPE).unwrap();
        let sorted = grab.sort_paste(b"\x1b[A\x14");
        assert_eq!(
            (sorted.taken.len(), &sorted.forward[..]),
            (0, &b"\x1b[A\x14"[..])
        );
    }
}
