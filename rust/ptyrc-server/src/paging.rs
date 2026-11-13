// SPDX-License-Identifier: GPL-3.0-or-later
//! Page keys and wheel notches among the bytes typed into the app: hints
//! for the frames.

use crate::mouse;

/// One request to page: `dy` -1 up, 1 down; `at`: the screen cell of
/// the wheel.
pub(crate) type Paged = (i8, Option<(u16, u16)>);

/// `pageup`, `pagedown` (`CSI 5 ~`, `CSI 6 ~`, with no modifier, not a
/// release) and the wheel, up or down, with no modifier.
pub(crate) fn scan(bytes: &[u8]) -> Vec<Paged> {
    let mut found = Vec::new();
    let mut rest = bytes;
    while let Some(start) = rest.windows(2).position(|w| w == b"\x1b[") {
        rest = &rest[start..];
        if let Some((report, len)) = mouse::parse(rest) {
            if report.wheel() && !report.release && report.code & !65 == 0 {
                let dy = if report.code & 1 == 0 { -1 } else { 1 };
                found.push((dy, Some((report.x, report.y))));
            }
            rest = &rest[len..];
            continue;
        }
        if let Some(dy) = page_key(&rest[2..]) {
            found.push((dy, None));
        }
        rest = &rest[2..];
    }
    found
}

/// The key `5[;mods[:event]]~` or `6...~` after `CSI`.
fn page_key(bytes: &[u8]) -> Option<i8> {
    let end = bytes.iter().take(12).position(|b| *b == b'~')?;
    let params = std::str::from_utf8(&bytes[..end]).ok()?;
    let (code, mods) = params.split_once(';').unwrap_or((params, "1"));
    let (mods, event) = mods.split_once(':').unwrap_or((mods, "1"));
    // Caps lock and num lock are no modifiers.
    let held = mods.parse::<u32>().ok()?.checked_sub(1)? & !(64 | 128);
    if held != 0 || event == "3" {
        return None;
    }
    match code {
        "5" => Some(-1),
        "6" => Some(1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_wheel() {
        assert_eq!(scan(b"a\x1b[5~b\x1b[6~"), [(-1, None), (1, None)]);
        assert_eq!(scan(b"\x1b[6;1:2~\x1b[5;129~"), [(1, None), (-1, None)]);
        // Modifiers, a release, other keys.
        assert_eq!(scan(b"\x1b[5;2~\x1b[6;1:3~\x1b[3~\x1b[A\x1b[15~j"), []);
        // SGR and X10 reports; a click, a wheel with ctrl, sideways.
        assert_eq!(
            scan(b"\x1b[<64;4;2M\x1b[<65;10;7M\x1b[<0;1;1M\x1b[<80;1;1M\x1b[<66;1;1M"),
            [(-1, Some((3, 1))), (1, Some((9, 6)))]
        );
        assert_eq!(scan(b"\x1b[M\x61\x25\x23"), [(1, Some((4, 2)))]);
    }
}
