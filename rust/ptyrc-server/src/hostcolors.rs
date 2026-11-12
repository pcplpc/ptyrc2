// SPDX-License-Identifier: GPL-3.0-or-later
//! The default colours of the host terminal: asked with OSC 10 and
//! OSC 11, the replies taken out of the keyboard stream.

use ptyrc_core::scene::HostColors;
use ptyrc_vt::Rgb;
use ptyrc_vt::palette::parse_color_spec;
use serde_json::{Value, json};

/// OSC 10 and OSC 11 with `?`: the host answers with its colours.
pub const QUERY_FG: &[u8] = b"\x1b]10;?\x1b\\";
pub const QUERY_BG: &[u8] = b"\x1b]11;?\x1b\\";

/// Longest reply held while its end is awaited.
const REPLY_MAX: usize = 48;

/// Where a colour comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Nobody said: light grey on black.
    Default,
    /// `--host-fg`, `--host-bg`.
    Option,
    /// The host answered.
    Host,
    /// A client said (`session.colors`).
    Client,
}

impl Source {
    fn name(self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::Option => "option",
            Source::Host => "host",
            Source::Client => "client",
        }
    }
}

/// The colours and where they come from; index 0 is the foreground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Known {
    pub colors: HostColors,
    pub from: [Source; 2],
}

impl Known {
    pub fn new(fg: Option<Rgb>, bg: Option<Rgb>) -> Known {
        let default = HostColors::default();
        let from = |c: Option<Rgb>| c.map_or(Source::Default, |_| Source::Option);
        Known {
            colors: HostColors {
                fg: fg.unwrap_or(default.fg),
                bg: bg.unwrap_or(default.bg),
            },
            from: [from(fg), from(bg)],
        }
    }

    /// The host said `which` is `rgb`. Returns whether it changed.
    pub fn learn(&mut self, which: usize, rgb: Rgb) -> bool {
        self.set(which, rgb, Source::Host)
    }

    /// `which` is `rgb`, from `source`. Returns whether it changed.
    pub fn set(&mut self, which: usize, rgb: Rgb, source: Source) -> bool {
        let slot = if which == 0 {
            &mut self.colors.fg
        } else {
            &mut self.colors.bg
        };
        let changed = *slot != rgb;
        *slot = rgb;
        self.from[which] = source;
        changed
    }

    /// The `host_colors` member of `session.info`.
    pub fn json(&self) -> Value {
        let hex = |[r, g, b]: Rgb| format!("#{r:02x}{g:02x}{b:02x}");
        json!({
            "fg": hex(self.colors.fg),
            "bg": hex(self.colors.bg),
            "from": {"fg": self.from[0].name(), "bg": self.from[1].name()},
        })
    }
}

/// The params of `session.colors`: colours to set (foreground,
/// background), and whether to ask the host for the others.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub colors: [Option<Rgb>; 2],
    pub ask: bool,
}

impl Change {
    /// `{"fg": colour, "bg": colour, "ask": bool}`, all optional. A
    /// colour is `#rrggbb`, a name or `0..255`; not `default`.
    pub fn parse(params: &Value) -> Result<Change, String> {
        let mut change = Change::default();
        let Some(fields) = params.as_object() else {
            return Ok(change);
        };
        for (key, value) in fields {
            match key.as_str() {
                "fg" | "bg" => {
                    let spec: ptyrc_core::scene::ColorSpec =
                        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                    if spec.0 == ptyrc_vt::Color::Default {
                        return Err(format!("{key}: default is what is being set"));
                    }
                    change.colors[(key == "bg") as usize] =
                        Some(ptyrc_vt::palette::rgb(spec.0, [0; 3]));
                }
                "ask" => {
                    change.ask = value.as_bool().ok_or("ask: true or false")?;
                }
                other => return Err(format!("unknown field: {other} (fg, bg, ask)")),
            }
        }
        Ok(change)
    }
}

/// Reads the replies in what the host sends. A reply the driver asked
/// for is taken out; any other one (the app asked) is read and left.
#[derive(Debug, Default)]
pub struct Replies {
    /// Replies still expected: foreground, background.
    want: [bool; 2],
    /// The start of a reply whose end has not come.
    held: Vec<u8>,
}

/// What `Replies::feed` found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Fed {
    /// What goes on to the app.
    pub forward: Vec<u8>,
    /// Colours read: foreground, background.
    pub colors: [Option<Rgb>; 2],
}

impl Replies {
    pub fn expect(&mut self, fg: bool, bg: bool) {
        self.want = [fg, bg];
    }

    /// A reply is still expected.
    pub fn waiting(&self) -> bool {
        self.want != [false, false]
    }

    /// Give up: what was held goes on to the app.
    pub fn give_up(&mut self) -> Vec<u8> {
        self.want = [false, false];
        std::mem::take(&mut self.held)
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Fed {
        let mut fed = Fed::default();
        if !self.waiting() && !bytes.windows(3).any(|w| w == b"\x1b]1") {
            fed.forward.extend_from_slice(bytes);
            return fed;
        }
        let mut data = std::mem::take(&mut self.held);
        data.extend_from_slice(bytes);
        let mut i = 0;
        while i < data.len() {
            let rest = &data[i..];
            if rest[0] != 0x1b {
                fed.forward.push(rest[0]);
                i += 1;
                continue;
            }
            match reply(rest) {
                Parsed::Reply { which, rgb, len } => {
                    fed.colors[which] = Some(rgb);
                    if self.want[which] {
                        self.want[which] = false;
                    } else {
                        fed.forward.extend_from_slice(&rest[..len]);
                    }
                    i += len;
                }
                // Its end may come with the next read.
                Parsed::Partial if self.waiting() && rest.len() >= 2 => {
                    self.held = rest.to_vec();
                    return fed;
                }
                Parsed::Partial | Parsed::Other => {
                    fed.forward.push(rest[0]);
                    i += 1;
                }
            }
        }
        fed
    }
}

enum Parsed {
    /// A whole reply of `len` bytes.
    Reply {
        which: usize,
        rgb: Rgb,
        len: usize,
    },
    /// The start of what may be one.
    Partial,
    Other,
}

/// `bytes` starts with ESC: is it `ESC ] 1 (0|1) ; spec (BEL | ESC \)`?
fn reply(bytes: &[u8]) -> Parsed {
    const HEAD: &[u8] = b"\x1b]1";
    let n = bytes.len().min(HEAD.len());
    if bytes[..n] != HEAD[..n] {
        return Parsed::Other;
    }
    let Some(&digit) = bytes.get(3) else {
        return Parsed::Partial;
    };
    let which = match digit {
        b'0' => 0,
        b'1' => 1,
        _ => return Parsed::Other,
    };
    match bytes.get(4) {
        None => return Parsed::Partial,
        Some(b';') => {}
        Some(_) => return Parsed::Other,
    }
    let body = &bytes[5..];
    for (at, b) in body.iter().enumerate().take(REPLY_MAX) {
        let end = match b {
            0x07 => 1,
            0x1b => match body.get(at + 1) {
                Some(b'\\') => 2,
                None => return Parsed::Partial,
                Some(_) => return Parsed::Other,
            },
            // A colour specification is printable.
            0x20..0x7f => continue,
            _ => return Parsed::Other,
        };
        return match parse_color_spec(&body[..at]) {
            Some(rgb) => Parsed::Reply {
                which,
                rgb,
                len: 5 + at + end,
            },
            None => Parsed::Other,
        };
    }
    if body.len() < REPLY_MAX {
        Parsed::Partial
    } else {
        Parsed::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: &[u8] = b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\";
    const FG: &[u8] = b"\x1b]10;rgb:cdcd/d6d6/f4f4\x07";

    #[test]
    fn a_client_sets_the_colours() {
        let change = Change::parse(&json!({"fg": "#102030", "bg": 4, "ask": true})).unwrap();
        assert_eq!(
            change.colors,
            [Some([0x10, 0x20, 0x30]), Some([0, 0, 0xee])]
        );
        assert!(change.ask);
        assert_eq!(Change::parse(&json!({})).unwrap(), Change::default());
        assert_eq!(Change::parse(&Value::Null).unwrap(), Change::default());
        for bad in [
            json!({"fg": "default"}),
            json!({"bg": "nope"}),
            json!({"ask": 1}),
            json!({"colour": "#000000"}),
        ] {
            assert!(Change::parse(&bad).is_err(), "{bad}");
        }

        let mut known = Known::new(None, Some([1, 2, 3]));
        assert!(known.set(0, [9, 9, 9], Source::Client));
        assert!(!known.set(0, [9, 9, 9], Source::Client));
        assert_eq!(
            known.json()["from"],
            json!({"fg": "client", "bg": "option"})
        );
        assert_eq!(known.json()["fg"], "#090909");
    }

    #[test]
    fn replies_asked_for_are_taken_out() {
        let mut replies = Replies::default();
        replies.expect(true, true);
        let fed = replies.feed(&[b"ab", FG, b"c", BG, b"d\x1b[A"].concat());
        assert_eq!(fed.forward, b"abcd\x1b[A");
        assert_eq!(fed.colors, [Some([205, 214, 244]), Some([30, 30, 46])]);
        assert!(!replies.waiting());
        // The next one answers the app: read, and left in the stream.
        let fed = replies.feed(BG);
        assert_eq!(fed.forward, BG);
        assert_eq!(fed.colors, [None, Some([30, 30, 46])]);
        // Nothing of interest: untouched.
        assert_eq!(
            replies.feed(b"\x1b]52;c;aGk=\x07x").forward,
            b"\x1b]52;c;aGk=\x07x"
        );
    }

    #[test]
    fn a_reply_in_pieces_is_held() {
        for cut in 2..BG.len() {
            let mut replies = Replies::default();
            replies.expect(false, true);
            let first = replies.feed(&[b"x", &BG[..cut]].concat());
            assert_eq!(first.forward, b"x", "cut at {cut}");
            let second = replies.feed(&[&BG[cut..], b"y"].concat());
            assert_eq!(second.forward, b"y", "cut at {cut}");
            assert_eq!(second.colors[1], Some([30, 30, 46]), "cut at {cut}");
        }
        // No end: given back when the wait is over.
        let mut replies = Replies::default();
        replies.expect(true, true);
        assert_eq!(replies.feed(b"\x1b]11;rgb:1e").forward, b"");
        assert_eq!(replies.give_up(), b"\x1b]11;rgb:1e");
        assert!(!replies.waiting());
        assert_eq!(replies.feed(b"1e\x07").forward, b"1e\x07");
    }

    #[test]
    fn keys_pass() {
        let mut replies = Replies::default();
        replies.expect(true, true);
        // Escape alone, an arrow, alt+], another OSC, a held start that
        // turns out to be something else.
        for keys in [
            &b"\x1b"[..],
            b"\x1b[A",
            b"\x1b]",
            b"\x1b]0;title\x07",
            b"\x1b]1;x\x07",
        ] {
            let mut out = replies.feed(keys).forward;
            out.extend(replies.feed(b"z").forward);
            assert_eq!(out, [keys, b"z"].concat());
        }
        assert!(replies.waiting());
        // A reply that is not a colour.
        assert_eq!(replies.feed(b"\x1b]11;?\x07").forward, b"\x1b]11;?\x07");
    }

    #[test]
    fn known_colours() {
        let mut known = Known::new(None, Some([1, 2, 3]));
        assert_eq!(
            known.json(),
            json!({"fg": "#e5e5e5", "bg": "#010203", "from": {"fg": "default", "bg": "option"}})
        );
        assert!(known.learn(0, [255, 255, 255]));
        assert!(!known.learn(1, [1, 2, 3]));
        assert_eq!(known.json()["from"], json!({"fg": "host", "bg": "host"}));
    }
}
