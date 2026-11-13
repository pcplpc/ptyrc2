// SPDX-License-Identifier: GPL-3.0-or-later
//! The hotkey: ctrl+t twice in a row on the host keyboard. The driver
//! runs a command on it (the manager), or ends it when it runs.

use crate::grab;
use std::time::Duration;

/// The second ctrl+t comes within this of the first.
const WINDOW: Duration = Duration::from_millis(350);
const KEY: &[u8] = b"\x14";

#[derive(Default)]
pub(crate) struct Hotkey {
    /// A first ctrl+t, as typed, and when it stops waiting.
    held: Option<(Vec<u8>, Duration)>,
}

impl Hotkey {
    /// When the held key is due.
    pub fn due(&self) -> Option<Duration> {
        self.held.as_ref().map(|(_, at)| *at)
    }

    /// The held key, given up.
    pub fn flush(&mut self) -> Vec<u8> {
        self.held.take().map(|(key, _)| key).unwrap_or_default()
    }

    /// Sort keyboard bytes: what goes on, and whether ctrl+t came twice
    /// in a row. A first ctrl+t is held, until the next key or `due`;
    /// the pair goes nowhere. Not `armed`: nothing is held.
    pub fn sort(&mut self, bytes: &[u8], now: Duration, armed: bool) -> (Vec<u8>, bool) {
        let mut forward = self.flush();
        if !armed {
            forward.extend_from_slice(bytes);
            return (forward, false);
        }
        let mut held = (!forward.is_empty()).then(|| std::mem::take(&mut forward));
        let mut twice = false;
        let mut rest = bytes;
        while !rest.is_empty() {
            let (key, tail) = rest.split_at(grab::key_len(rest));
            rest = tail;
            let is_key = grab::legacy(key).as_deref().unwrap_or(key) == KEY;
            match (held.take(), is_key) {
                (Some(_), true) => twice = true,
                (None, true) => held = Some(key.to_vec()),
                // The release of the first press, between the two.
                (Some(first), false) if grab::is_release(key) => {
                    forward.extend_from_slice(key);
                    held = Some(first);
                }
                (first, false) => {
                    forward.extend(first.unwrap_or_default());
                    forward.extend_from_slice(key);
                }
            }
        }
        self.held = held.map(|key| (key, now + WINDOW));
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
    fn kitty_keys() {
        let mut d = demo();
        let (forward, twice) = d.sort(b"\x1b[116;5u\x1b[116;5:3u\x1b[116;5u", T0, true);
        assert_eq!((&forward[..], twice), (&b"\x1b[116;5:3u"[..], true));
    }
}
