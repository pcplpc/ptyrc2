// SPDX-License-Identifier: GPL-3.0-or-later
//! The hotkey: ctrl+t twice in a row on the host keyboard. The driver
//! runs a command on it (the manager), or ends it when it runs. The
//! key that hides all: ctrl+t, then `h`.

use crate::grab;
use std::time::Duration;

/// The second ctrl+t comes within this of the first.
const WINDOW: Duration = Duration::from_millis(350);
const KEY: &[u8] = b"\x14";

/// What becomes of keyboard bytes: keys that go on, or the pair.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Keys(Vec<u8>),
    /// ctrl+t came twice in a row: it reaches nobody.
    Pair,
    /// ctrl+t, then the key of `hide`: it reaches nobody.
    Hide,
}

#[derive(Default)]
pub(crate) struct Hotkey {
    /// A first ctrl+t, as typed, and when it stops waiting.
    held: Option<(Vec<u8>, Duration)>,
    /// A pair ended the last read: the release of its second key, if
    /// it comes first, goes with it.
    paired: bool,
    /// The key after ctrl+t that hides all, as typed without the
    /// kitty protocol; `None`: no such key.
    hide: Option<u8>,
}

impl Hotkey {
    pub fn new(hide: Option<u8>) -> Hotkey {
        Hotkey {
            hide,
            ..Hotkey::default()
        }
    }

    /// When the held key is due.
    pub fn due(&self) -> Option<Duration> {
        self.held.as_ref().map(|(_, at)| *at)
    }

    /// The held key, given up.
    pub fn flush(&mut self) -> Vec<u8> {
        self.held.take().map(|(key, _)| key).unwrap_or_default()
    }

    /// Sort keyboard bytes, in order: what goes on, each pair of ctrl+t
    /// in a row, each ctrl+t and `hide`. A first ctrl+t is held, until
    /// the next key or `due`. The release event of the second key
    /// (kitty keyboard protocol) goes with it. Not `armed`: no pair;
    /// not `hides` either: nothing is held.
    pub fn parts(&mut self, bytes: &[u8], now: Duration, armed: bool, hides: bool) -> Vec<Part> {
        let mut forward = self.flush();
        let mut rest = bytes;
        if std::mem::take(&mut self.paired) {
            rest = grab::skip_release(rest);
        }
        let hide = self.hide.filter(|_| hides);
        if !armed && hide.is_none() {
            forward.extend_from_slice(rest);
            return vec![Part::Keys(forward)];
        }
        let mut parts = Vec::new();
        let mut held = (!forward.is_empty()).then(|| std::mem::take(&mut forward));
        while !rest.is_empty() {
            let (key, tail) = rest.split_at(grab::key_len(rest));
            rest = tail;
            let plain = grab::legacy(key);
            let plain = plain.as_deref().unwrap_or(key);
            let is_key = plain == KEY;
            match (held.take(), is_key) {
                (Some(first), true) if !armed => {
                    forward.extend(first);
                    held = Some(key.to_vec());
                }
                (Some(_), true) => {
                    if !forward.is_empty() {
                        parts.push(Part::Keys(std::mem::take(&mut forward)));
                    }
                    parts.push(Part::Pair);
                    self.paired = rest.is_empty();
                    rest = grab::skip_release(rest);
                }
                (None, true) => held = Some(key.to_vec()),
                // The release of the first press, between the two.
                (Some(first), false) if grab::is_release(key) => {
                    forward.extend_from_slice(key);
                    held = Some(first);
                }
                (Some(_), false) if hide.is_some_and(|h| plain == [h]) => {
                    if !forward.is_empty() {
                        parts.push(Part::Keys(std::mem::take(&mut forward)));
                    }
                    parts.push(Part::Hide);
                    self.paired = rest.is_empty();
                    rest = grab::skip_release(rest);
                }
                (first, false) => {
                    forward.extend(first.unwrap_or_default());
                    forward.extend_from_slice(key);
                }
            }
        }
        if !forward.is_empty() {
            parts.push(Part::Keys(forward));
        }
        self.held = held.map(|key| (key, now + WINDOW));
        parts
    }

    #[cfg(test)]
    fn parts_of(&mut self, bytes: &[u8], now: Duration, armed: bool) -> Vec<Part> {
        self.parts(bytes, now, armed, false)
    }

    /// `parts` as one: what goes on, and whether a pair came.
    #[cfg(test)]
    fn sort(&mut self, bytes: &[u8], now: Duration, armed: bool) -> (Vec<u8>, bool) {
        let mut forward = Vec::new();
        let mut twice = false;
        for part in self.parts(bytes, now, armed, false) {
            match part {
                Part::Keys(keys) => forward.extend(keys),
                Part::Pair => twice = true,
                Part::Hide => unreachable!("not asked"),
            }
        }
        (forward, twice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Hotkey {
        Hotkey::default()
    }

    const T0: Duration = Duration::ZERO;

    #[test]
    fn twice_in_a_row() {
        let mut d = demo();
        assert_eq!(d.sort(b"a\x14\x14b", T0, true), (b"ab".to_vec(), true));
        assert_eq!(d.due(), None);
        // Over two reads.
        assert_eq!(d.sort(b"\x14", T0, true), (Vec::new(), false));
        assert_eq!(d.due(), Some(WINDOW));
        assert_eq!(d.sort(b"\x14", T0, true), (Vec::new(), true));
        assert_eq!(d.due(), None);
        // Three: a pair, and one held.
        assert_eq!(d.sort(b"\x14\x14\x14", T0, true), (Vec::new(), true));
        assert_eq!(d.flush(), b"\x14");
    }

    #[test]
    fn once_goes_on() {
        let mut d = demo();
        // Another key between the two.
        assert_eq!(d.sort(b"\x14a\x14", T0, true), (b"\x14a".to_vec(), false));
        assert_eq!(d.sort(b"x", T0, true), (b"\x14x".to_vec(), false));
        // Too late: given up as typed.
        assert_eq!(d.sort(b"\x1b[116;5u", T0, true), (Vec::new(), false));
        assert_eq!(d.flush(), b"\x1b[116;5u");
        assert_eq!(d.flush(), b"");
        // Not armed: untouched, and what was held goes first.
        assert_eq!(d.sort(b"\x14", T0, true), (Vec::new(), false));
        assert_eq!(
            d.sort(b"\x14\x14", T0, false),
            (b"\x14\x14\x14".to_vec(), false)
        );
    }

    #[test]
    fn pairs_in_order() {
        let mut d = demo();
        let keys = |text: &str| Part::Keys(text.as_bytes().to_vec());
        let parts = d.parts_of(b"a\x14\x14b\x14\x14\x14\x14c", T0, true);
        let expected = [
            keys("a"),
            Part::Pair,
            keys("b"),
            Part::Pair,
            Part::Pair,
            keys("c"),
        ];
        assert_eq!(parts, expected);
        // The release of the second ctrl+t goes with the pair.
        let parts = d.parts_of(b"\x1b[116;5u\x1b[116;5u\x1b[116;5:3ux", T0, true);
        assert_eq!(parts, [Part::Pair, keys("x")]);
        // In the next read too.
        assert_eq!(
            d.parts_of(b"\x1b[116;5u\x1b[116;5u", T0, true),
            [Part::Pair]
        );
        assert_eq!(d.parts_of(b"\x1b[116;5:3uy", T0, true), [keys("y")]);
    }

    #[test]
    fn ctrl_t_then_h_hides() {
        let mut d = Hotkey::new(Some(b'h'));
        let keys = |text: &[u8]| Part::Keys(text.to_vec());
        let parts = d.parts(b"a\x14hb\x14\x14\x14h", T0, true, true);
        assert_eq!(
            parts,
            [keys(b"a"), Part::Hide, keys(b"b"), Part::Pair, Part::Hide]
        );
        assert_eq!(d.parts(b"\x14h", T0, false, true), [Part::Hide]);
        assert_eq!(d.parts(b"\x14h", T0, false, true), [Part::Hide]);
        // Not asked: the app's.
        assert_eq!(d.parts(b"\x14h", T0, true, false), [keys(b"\x14h")]);
        // Under a grab: no pair, the key that hides all the same.
        let parts = d.parts(b"\x14\x14h\x14", T0, false, true);
        assert_eq!(parts, [keys(b"\x14"), Part::Hide]);
        assert_eq!(d.flush(), b"\x14");
        // The kitty forms, the release of `h` with it.
        let parts = d.parts(b"\x1b[116;5u\x1b[104u\x1b[104;1:3ux", T0, true, true);
        assert_eq!(parts, [Part::Hide, keys(b"x")]);
        // No such key: ctrl+t, h, typed.
        let mut d = Hotkey::new(None);
        assert_eq!(d.parts(b"\x14h", T0, false, true), [keys(b"\x14h")]);
    }

    #[test]
    fn kitty_keys() {
        let mut d = demo();
        let (forward, twice) = d.sort(b"\x1b[116;5u\x1b[116;5:3u\x1b[116;5u", T0, true);
        assert_eq!((&forward[..], twice), (&b"\x1b[116;5:3u"[..], true));
    }
}
