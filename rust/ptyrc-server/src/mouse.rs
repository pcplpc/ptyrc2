// SPDX-License-Identifier: GPL-3.0-or-later
//! Mouse reports: read from the host keyboard, written as the app asked
//! for them.

/// What the app asked of the mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseMode {
    /// A reporting mode is on (9, 1000, 1002, 1003).
    pub on: bool,
    /// Presses only (mode 9 alone).
    pub press_only: bool,
    /// SGR coordinates (1006).
    pub sgr: bool,
    /// The modes on: bit 0 is mode 9, bit 1 1000, bit 2 1002, bit 3 1003.
    pub bits: u8,
}

impl MouseMode {
    /// Moves with a button down are reported (1002, 1003).
    pub fn drags(self) -> bool {
        self.bits & 0b1100 != 0
    }

    /// The private modes that put a terminal in this mode.
    pub fn set_sequence(self) -> Vec<u8> {
        let mut out = Vec::new();
        for (bit, mode) in [(1, 9), (2, 1000), (4, 1002), (8, 1003)] {
            if self.bits & bit != 0 {
                out.extend(format!("\x1b[?{mode}h").bytes());
            }
        }
        out.extend(if self.sgr {
            &b"\x1b[?1006h"[..]
        } else {
            b"\x1b[?1006l"
        });
        out
    }
}

/// What the driver asks of the host for a client: presses, releases,
/// drags, in SGR form.
pub const HOST_ON: &[u8] = b"\x1b[?1002h\x1b[?1006h";
/// The same and the moves with no button down, while a client listens
/// to `hover`.
pub const HOST_HOVER: &[u8] = b"\x1b[?1003h\x1b[?1006h";
pub const HOST_OFF: &[u8] = b"\x1b[?1002l\x1b[?1003l\x1b[?1000l\x1b[?9l\x1b[?1006l";

pub const BUTTONS: [&str; 7] = [
    "left",
    "middle",
    "right",
    "wheel-up",
    "wheel-down",
    "wheel-left",
    "wheel-right",
];
pub const MODS: [&str; 3] = ["shift", "alt", "ctrl"];

/// What the driver asks of the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostMouse {
    /// Nothing: the modes of the app.
    #[default]
    Off,
    /// Presses, releases, drags (`mouse`).
    Clicks,
    /// And the moves with no button down (`hover`).
    Hover,
}

impl HostMouse {
    /// What the listeners want: `hover` asks the most.
    pub fn wanted(mouse: bool, hover: bool) -> HostMouse {
        match (mouse, hover) {
            (_, true) => HostMouse::Hover,
            (true, false) => HostMouse::Clicks,
            (false, false) => HostMouse::Off,
        }
    }

    /// The private modes that put the host in this state, whatever the
    /// app set meanwhile. Empty for `Off`: see `leave`.
    pub fn sequence(self) -> &'static [u8] {
        match self {
            HostMouse::Off => b"",
            HostMouse::Clicks => HOST_ON,
            HostMouse::Hover => HOST_HOVER,
        }
    }

    /// From `self` to `next`; `app`: the modes the host gets back when
    /// no client listens.
    pub fn leave(self, next: HostMouse, app: MouseMode) -> Vec<u8> {
        match next {
            HostMouse::Off => {
                let mut out = HOST_OFF.to_vec();
                out.extend(app.set_sequence());
                out
            }
            // 1002 does not end 1003 on every terminal.
            HostMouse::Clicks if self == HostMouse::Hover => {
                [&b"\x1b[?1003l"[..], HOST_ON].concat()
            }
            next => next.sequence().to_vec(),
        }
    }
}

const SHIFT: u8 = 4;
const MOTION: u8 = 32;
const WHEEL: u8 = 64;

/// One report. `code` is xterm's: bits 0-1 the button (3: none), 4
/// shift, 8 alt, 16 ctrl, 32 a move, 64 the wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub code: u8,
    /// Screen cell.
    pub x: u16,
    pub y: u16,
    pub release: bool,
}

impl Report {
    pub fn wheel(self) -> bool {
        self.code & WHEEL != 0
    }

    pub fn motion(self) -> bool {
        self.code & MOTION != 0
    }

    /// A button went down.
    pub fn press(self) -> bool {
        !self.release && !self.motion() && !self.wheel()
    }

    /// A move with no button down: what 1003 adds.
    pub fn hover(self) -> bool {
        self.motion() && !self.wheel() && self.code & 3 == 3
    }

    /// `left`, ..., or `none` (a release in X10 form, a move with no
    /// button down).
    pub fn button(self) -> &'static str {
        match (self.wheel(), self.code & 3) {
            (false, 3) => "none",
            (false, b) => BUTTONS[b as usize],
            (true, b) => BUTTONS[3 + b as usize],
        }
    }

    /// `press`, `release`, `drag`, `move`, `wheel`.
    pub fn action(self) -> &'static str {
        match () {
            _ if self.wheel() => "wheel",
            _ if self.release => "release",
            _ if self.motion() && self.code & 3 == 3 => "move",
            _ if self.motion() => "drag",
            _ => "press",
        }
    }

    pub fn mods(self) -> Vec<&'static str> {
        let held = |i: usize| self.code & (SHIFT << i) != 0;
        (0..3).filter(|i| held(*i)).map(|i| MODS[i]).collect()
    }

    /// The bytes of this report for an app in `mode`. `None`: the app
    /// did not ask for it, or the mode cannot tell it (X10 past 222).
    pub fn encode(self, mode: MouseMode) -> Option<Vec<u8>> {
        if !mode.on
            || (self.motion() && !mode.drags())
            || (self.motion() && self.code & 3 == 3 && mode.bits & 8 == 0)
            || (mode.press_only && !self.press())
        {
            return None;
        }
        let (x, y) = (self.x, self.y);
        if mode.sgr {
            let end = if self.release { 'm' } else { 'M' };
            return Some(format!("\x1b[<{};{};{}{end}", self.code, x + 1, y + 1).into_bytes());
        }
        if x > 222 || y > 222 {
            return None;
        }
        let code = if self.release {
            self.code | 3
        } else {
            self.code
        };
        Some(vec![
            0x1b,
            b'[',
            b'M',
            code + 32,
            x as u8 + 33,
            y as u8 + 33,
        ])
    }
}

/// The report `bytes` starts with, and its length.
pub fn parse(bytes: &[u8]) -> Option<(Report, usize)> {
    match bytes {
        [0x1b, b'[', b'<', rest @ ..] => {
            let end = rest.iter().position(|b| matches!(b, b'M' | b'm'))?;
            let body = std::str::from_utf8(&rest[..end]).ok()?;
            let mut fields = body.split(';').map(|f| f.parse::<u16>().ok());
            let (code, x, y) = (fields.next()??, fields.next()??, fields.next()??);
            if fields.next().is_some() || code > 255 || x == 0 || y == 0 {
                return None;
            }
            let report = Report {
                code: code as u8,
                x: x - 1,
                y: y - 1,
                release: rest[end] == b'm',
            };
            Some((report, 3 + end + 1))
        }
        [0x1b, b'[', b'M', cb, cx, cy, ..] if *cb >= 32 && *cx > 32 && *cy > 32 => {
            let code = cb - 32;
            let release = code & 3 == 3 && code & (MOTION | WHEEL) == 0;
            let report = Report {
                code,
                x: (*cx - 33) as u16,
                y: (*cy - 33) as u16,
                release,
            };
            Some((report, 6))
        }
        _ => None,
    }
}

/// Where the next report may start in `bytes`.
pub fn find(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(3)
        .position(|w| w[0] == 0x1b && w[1] == b'[' && (w[2] == b'<' || w[2] == b'M'))
}

/// A gesture asked by a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gesture<'a> {
    pub button: &'a str,
    /// Bits of the modifiers held, as in `Report::code`.
    pub mods: u8,
    pub at: (u16, u16),
    /// Dragged there before the release.
    pub to: Option<(u16, u16)>,
    /// Clicks: 2 is a double click.
    pub count: u8,
}

/// Bits of the modifier `name`.
pub fn mod_bits(name: &str) -> Option<u8> {
    let index = MODS.iter().position(|m| *m == name)?;
    Some(SHIFT << index)
}

/// Bytes of a gesture: press, a move when dragged and the app follows
/// drags, release; the press alone under mode 9 or for the wheel.
/// `None`: a cell cannot be told in this mode (X10 past 222), or no such
/// button.
pub fn gesture(mode: MouseMode, g: Gesture) -> Option<Vec<u8>> {
    let index = BUTTONS.iter().position(|b| *b == g.button)?;
    let code = if index < 3 {
        index as u8
    } else {
        WHEEL | (index as u8 - 3)
    } | g.mods;
    let report = |code: u8, (x, y): (u16, u16), release: bool| Report {
        code,
        x,
        y,
        release,
    };
    let mut out = Vec::new();
    for _ in 0..g.count.max(1) {
        out.extend(report(code, g.at, false).encode(mode)?);
        if index >= 3 || mode.press_only {
            continue;
        }
        let end = g.to.unwrap_or(g.at);
        if g.to.is_some() && mode.drags() {
            out.extend(report(code | MOTION, end, false).encode(mode)?);
        }
        out.extend(report(code, end, true).encode(mode)?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(bits: u8, sgr: bool) -> MouseMode {
        MouseMode {
            on: bits != 0,
            press_only: bits == 1,
            sgr,
            bits,
        }
    }

    fn click(mode: MouseMode, button: &str, x: u16, y: u16) -> Option<Vec<u8>> {
        let click = Gesture {
            button,
            mods: 0,
            at: (x, y),
            to: None,
            count: 1,
        };
        gesture(mode, click)
    }

    #[test]
    fn encodings() {
        let sgr = mode(2, true);
        assert_eq!(
            click(sgr, "left", 4, 2).unwrap(),
            b"\x1b[<0;5;3M\x1b[<0;5;3m"
        );
        assert_eq!(click(sgr, "wheel-down", 0, 0).unwrap(), b"\x1b[<65;1;1M");
        let x10 = mode(2, false);
        assert_eq!(
            click(x10, "right", 0, 1).unwrap(),
            b"\x1b[M\x22!\"\x1b[M#!\""
        );
        assert_eq!(click(mode(1, false), "left", 0, 0).unwrap(), b"\x1b[M !!");
        assert_eq!(click(x10, "left", 300, 0), None);
        assert_eq!(click(x10, "double", 0, 0), None);
    }

    #[test]
    fn gestures() {
        let drag = Gesture {
            button: "left",
            mods: mod_bits("ctrl").unwrap(),
            at: (1, 1),
            to: Some((5, 2)),
            count: 1,
        };
        // Followed by an app that asked for drags; else press and release.
        assert_eq!(
            gesture(mode(4, true), drag).unwrap(),
            b"\x1b[<16;2;2M\x1b[<48;6;3M\x1b[<16;6;3m"
        );
        assert_eq!(
            gesture(mode(2, true), drag).unwrap(),
            b"\x1b[<16;2;2M\x1b[<16;6;3m"
        );
        let double = Gesture {
            mods: 0,
            to: None,
            count: 2,
            ..drag
        };
        assert_eq!(
            gesture(mode(2, true), double).unwrap(),
            b"\x1b[<0;2;2M\x1b[<0;2;2m\x1b[<0;2;2M\x1b[<0;2;2m"
        );
        assert_eq!(mod_bits("meta"), None);
    }

    #[test]
    fn reports_are_read_and_written_again() {
        let (press, len) = parse(b"\x1b[<4;10;3Mrest").unwrap();
        assert_eq!(len, 10);
        assert_eq!((press.x, press.y, press.release), (9, 2, false));
        assert_eq!(
            (press.button(), press.action(), press.mods()),
            ("left", "press", vec!["shift"])
        );
        let (release, _) = parse(b"\x1b[<0;10;3m").unwrap();
        assert_eq!(release.action(), "release");
        let (drag, _) = parse(b"\x1b[<34;1;1M").unwrap();
        assert_eq!((drag.button(), drag.action()), ("right", "drag"));
        let (wheel, _) = parse(b"\x1b[<64;1;1M").unwrap();
        assert_eq!((wheel.button(), wheel.action()), ("wheel-up", "wheel"));
        // X10: the release names no button.
        let (x10, len) = parse(b"\x1b[M#!\"").unwrap();
        assert_eq!((len, x10.x, x10.y), (6, 0, 1));
        assert_eq!((x10.button(), x10.action()), ("none", "release"));
        assert_eq!(parse(b"\x1b[<0;0;3M"), None);
        assert_eq!(parse(b"\x1b[<0;3M"), None);
        assert_eq!(parse(b"\x1b[A"), None);
        assert_eq!(find(b"ab\x1b[A\x1b[<0;1;1M"), Some(5));

        // SGR from the host, for an app that reads X10, and back.
        assert_eq!(press.encode(mode(2, false)).unwrap(), b"\x1b[M$*#");
        assert_eq!(release.encode(mode(2, false)).unwrap(), b"\x1b[M#*#");
        assert_eq!(x10.encode(mode(2, true)).unwrap(), b"\x1b[<3;1;2m");
        // Not asked for: no mouse, a drag without 1002, a release in mode 9.
        assert_eq!(press.encode(mode(0, true)), None);
        assert_eq!(drag.encode(mode(2, true)), None);
        assert!(drag.encode(mode(4, true)).is_some());
        assert_eq!(release.encode(mode(1, false)), None);
    }

    #[test]
    fn a_move_reaches_only_an_app_that_asked_1003() {
        let (hover, _) = parse(b"\x1b[<35;4;2M").unwrap();
        assert!(hover.hover() && !hover.press());
        assert_eq!((hover.button(), hover.action()), ("none", "move"));
        for bits in [0, 1, 2, 4, 6] {
            assert_eq!(hover.encode(mode(bits, true)), None, "{bits}");
        }
        assert_eq!(hover.encode(mode(8, true)).unwrap(), b"\x1b[<35;4;2M");
        assert_eq!(hover.encode(mode(8, false)).unwrap(), b"\x1b[MC$\"");
        let (drag, _) = parse(b"\x1b[<32;4;2M").unwrap();
        assert!(!drag.hover());
    }

    #[test]
    fn the_host_is_asked_what_the_listeners_want() {
        use HostMouse::{Clicks, Hover, Off};
        assert_eq!(HostMouse::wanted(false, false), Off);
        assert_eq!(HostMouse::wanted(true, false), Clicks);
        assert_eq!(HostMouse::wanted(false, true), Hover);
        assert_eq!(HostMouse::wanted(true, true), Hover);
        let app = mode(2, false);
        assert_eq!(Off.leave(Clicks, app), HOST_ON);
        assert_eq!(Off.leave(Hover, app), b"\x1b[?1003h\x1b[?1006h");
        assert_eq!(Clicks.leave(Hover, app), b"\x1b[?1003h\x1b[?1006h");
        assert_eq!(
            Hover.leave(Clicks, app),
            b"\x1b[?1003l\x1b[?1002h\x1b[?1006h"
        );
        // The last listener gone: 1003 off, the app's own modes on.
        let back = Hover.leave(Off, app);
        assert!(back.starts_with(HOST_OFF) && back.ends_with(b"\x1b[?1000h\x1b[?1006l"));
        assert!(Off.sequence().is_empty());
    }

    #[test]
    fn the_modes_of_the_app_are_set_again() {
        assert_eq!(mode(2, true).set_sequence(), b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(mode(0, false).set_sequence(), b"\x1b[?1006l");
    }
}
