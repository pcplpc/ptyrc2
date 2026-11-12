// SPDX-License-Identifier: GPL-3.0-or-later
//! Mouse reports, as the app asked for them.

/// What the app asked of the mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseMode {
    /// A reporting mode is on (9, 1000, 1002, 1003).
    pub on: bool,
    /// Presses only (mode 9 alone).
    pub press_only: bool,
    /// SGR coordinates (1006).
    pub sgr: bool,
}

pub const BUTTONS: [&str; 5] = ["left", "middle", "right", "wheel-up", "wheel-down"];

/// Bytes of a click at screen cell `(x, y)`: press, then release for a
/// button. `None`: the cell cannot be told in this mode (X10 past 222).
pub fn click(mode: MouseMode, button: &str, x: u16, y: u16) -> Option<Vec<u8>> {
    let code: u8 = match button {
        "left" => 0,
        "middle" => 1,
        "right" => 2,
        "wheel-up" => 64,
        "wheel-down" => 65,
        _ => return None,
    };
    let release = code < 3 && !mode.press_only;
    let mut out = Vec::new();
    if mode.sgr {
        out.extend(format!("\x1b[<{code};{};{}M", x + 1, y + 1).bytes());
        if release {
            out.extend(format!("\x1b[<{code};{};{}m", x + 1, y + 1).bytes());
        }
    } else {
        if x > 222 || y > 222 {
            return None;
        }
        let (cx, cy) = (x as u8 + 33, y as u8 + 33);
        out.extend([0x1b, b'[', b'M', code + 32, cx, cy]);
        if release {
            out.extend([0x1b, b'[', b'M', 3 + 32, cx, cy]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings() {
        let sgr = MouseMode {
            on: true,
            press_only: false,
            sgr: true,
        };
        assert_eq!(
            click(sgr, "left", 4, 2).unwrap(),
            b"\x1b[<0;5;3M\x1b[<0;5;3m"
        );
        assert_eq!(click(sgr, "wheel-down", 0, 0).unwrap(), b"\x1b[<65;1;1M");
        let x10 = MouseMode {
            on: true,
            press_only: false,
            sgr: false,
        };
        assert_eq!(
            click(x10, "right", 0, 1).unwrap(),
            b"\x1b[M\x22!\"\x1b[M#!\""
        );
        let press = MouseMode {
            on: true,
            press_only: true,
            sgr: false,
        };
        assert_eq!(click(press, "left", 0, 0).unwrap(), b"\x1b[M !!");
        assert_eq!(click(x10, "left", 300, 0), None);
        assert_eq!(click(x10, "double", 0, 0), None);
    }
}
