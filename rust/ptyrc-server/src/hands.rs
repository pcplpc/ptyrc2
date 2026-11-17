// SPDX-License-Identifier: GPL-3.0-or-later
//! The hands of the user on the host keyboard: what of a read is typed
//! (a key, a paste, the mouse), what is an answer of the terminal.

use crate::grab;

/// Sorts the reads of the host keyboard, one after the other.
#[derive(Default)]
pub(crate) struct Hands {
    /// A string answer (OSC, DCS, APC, PM) began in the last read and
    /// did not end there.
    string: bool,
}

impl Hands {
    /// Whether `bytes`, a read of the host keyboard, hold something the
    /// user did. A string answer cut by the read ends in the next one,
    /// or is that whole read.
    pub fn touch(&mut self, bytes: &[u8]) -> bool {
        let mut rest = bytes;
        if std::mem::take(&mut self.string) {
            match string_end(rest) {
                Some(end) => rest = &rest[end..],
                None => return false,
            }
        }
        let mut touched = false;
        while !rest.is_empty() {
            // Alone at the end of a read: alt and the character.
            if let [0x1b, b']' | b'P' | b'_' | b'^', after @ ..] = rest
                && !after.is_empty()
            {
                match string_end(after) {
                    Some(end) => rest = &after[end..],
                    None => {
                        self.string = true;
                        break;
                    }
                }
                continue;
            }
            let len = grab::key_len(rest).max(1);
            touched |= !is_answer(&rest[..len]);
            rest = &rest[len..];
        }
        touched
    }
}

/// Where the string `bytes` are in ends: past its BEL or its ST.
fn string_end(bytes: &[u8]) -> Option<usize> {
    (0..bytes.len()).find_map(|i| match &bytes[i..] {
        [0x07, ..] => Some(i + 1),
        [0x1b, b'\\', ..] => Some(i + 2),
        _ => None,
    })
}

/// A control sequence the terminal sends by itself or when asked: the
/// focus of its window, the cursor position (`CSI r;c R`; shift+f3 is
/// lost to it), the device attributes, a mode, a status, a window
/// report, the keyboard flags. The mouse is the user's.
fn is_answer(key: &[u8]) -> bool {
    let Some(body) = key.strip_prefix(b"\x1b[") else {
        return false;
    };
    let Some((end, params)) = body.split_last() else {
        return false;
    };
    match params.first() {
        Some(b'<') => return false,
        Some(b'?' | b'>' | b'=') => return true,
        _ => {}
    }
    match end {
        b'I' | b'O' => params.is_empty(),
        b'R' => params.contains(&b';'),
        b'c' | b'n' | b't' | b'y' => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_pastes_and_the_mouse_are_the_user() {
        for bytes in [
            &b"a"[..],
            b"\r",
            b"\x1b",
            b"\x1b[A",
            b"\x1bOP",
            b"\x1b[R",
            b"\x1b[97;5u",
            b"\x1b[27;5;99~",
            b"\x1b[<0;3;4M",
            b"\x1b[M !!",
            b"\x1b[200~text\x1b[201~",
            b"\x1b]",
            b"\xc3\xa9",
        ] {
            assert!(Hands::default().touch(bytes), "{bytes:?}");
        }
    }

    #[test]
    fn answers_of_the_terminal_are_not() {
        for bytes in [
            &b""[..],
            b"\x1b[I",
            b"\x1b[O",
            b"\x1b[12;40R",
            b"\x1b[?62;22c",
            b"\x1b[>0;276;0c",
            b"\x1b[?2026;2$y",
            b"\x1b[?1u",
            b"\x1b[0n",
            b"\x1b[8;24;80t",
            b"\x1b]11;rgb:0000/0000/0000\x07",
            b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\\x1b[?1u\x1b[?62c",
            b"\x1bP1+r544e=787465726d\x1b\\",
        ] {
            assert!(!Hands::default().touch(bytes), "{bytes:?}");
        }
    }

    #[test]
    fn a_key_among_answers_is_seen() {
        assert!(Hands::default().touch(b"\x1b[12;40Rx\x1b[?62c"));
        assert!(Hands::default().touch(b"\x1b]11;rgb:0/0/0\x07j"));
    }

    #[test]
    fn a_string_cut_by_a_read_ends_in_the_next() {
        let mut hands = Hands::default();
        assert!(!hands.touch(b"\x1b]11;rgb:00"));
        assert!(!hands.touch(b"00/0000/0000\x07"));
        assert!(hands.touch(b"a"));
        // One that never ends is one read, no more.
        assert!(!hands.touch(b"\x1bPab"));
        assert!(!hands.touch(b"cd"));
        assert!(hands.touch(b"e"));
        // Its end, then a key.
        assert!(!hands.touch(b"\x1b]0;t"));
        assert!(hands.touch(b"\x1b\\k"));
    }
}
