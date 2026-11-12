// SPDX-License-Identifier: GPL-3.0-or-later
//! SGR: parameters to `Style`, and `Style` back to bytes.

use crate::grid::{Attrs, Color, Style};
use std::io::Write;

/// CSI parameters. `sub` bit `i`: parameter `i` follows a colon.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Params {
    pub vals: [u16; 32],
    pub len: usize,
    pub sub: u32,
}

impl Params {
    /// Parameter `i`, or `default` when absent or zero.
    pub fn get(&self, i: usize, default: u16) -> u16 {
        if i < self.len && self.vals[i] != 0 {
            self.vals[i]
        } else {
            default
        }
    }

    fn is_sub(&self, i: usize) -> bool {
        i < self.len && self.sub & (1 << i) != 0
    }
}

fn clamp8(v: u16) -> u8 {
    v.min(255) as u8
}

/// Colour of `38`, `48`, `58` in colon form: `vals` are the sub-parameters.
fn color_sub(vals: &[u16]) -> Option<Color> {
    match vals {
        [5, n, ..] => Some(Color::Indexed(clamp8(*n))),
        [2, r, g, b] => Some(Color::Rgb(clamp8(*r), clamp8(*g), clamp8(*b))),
        // With a colour space id.
        [2, _, r, g, b, ..] => Some(Color::Rgb(clamp8(*r), clamp8(*g), clamp8(*b))),
        _ => None,
    }
}

/// Apply an SGR sequence to `pen`. The hyperlink is not touched.
pub(crate) fn apply(pen: &mut Style, p: &Params) {
    if p.len == 0 {
        reset(pen);
        return;
    }
    let mut i = 0;
    while i < p.len {
        let code = p.vals[i];
        // Sub-parameters attached by colons.
        let mut end = i + 1;
        while p.is_sub(end) {
            end += 1;
        }
        if end > i + 1 {
            let subs = &p.vals[i + 1..end];
            match code {
                4 => set_underline(pen, subs[0]),
                38 | 48 | 58 => {
                    if let Some(c) = color_sub(subs) {
                        set_color(pen, code, c);
                    }
                }
                _ => {}
            }
            i = end;
            continue;
        }
        i += 1;
        match code {
            0 => reset(pen),
            1 => pen.attrs.insert(Attrs::BOLD),
            2 => pen.attrs.insert(Attrs::DIM),
            3 => pen.attrs.insert(Attrs::ITALIC),
            4 => set_underline(pen, 1),
            5 | 6 => pen.attrs.insert(Attrs::BLINK),
            7 => pen.attrs.insert(Attrs::INVERSE),
            8 => pen.attrs.insert(Attrs::HIDDEN),
            9 => pen.attrs.insert(Attrs::STRIKE),
            21 => set_underline(pen, 2),
            22 => pen.attrs.remove(Attrs::BOLD | Attrs::DIM),
            23 => pen.attrs.remove(Attrs::ITALIC),
            24 => set_underline(pen, 0),
            25 => pen.attrs.remove(Attrs::BLINK),
            27 => pen.attrs.remove(Attrs::INVERSE),
            28 => pen.attrs.remove(Attrs::HIDDEN),
            29 => pen.attrs.remove(Attrs::STRIKE),
            30..=37 => pen.fg = Color::Indexed((code - 30) as u8),
            39 => pen.fg = Color::Default,
            40..=47 => pen.bg = Color::Indexed((code - 40) as u8),
            49 => pen.bg = Color::Default,
            53 => pen.attrs.insert(Attrs::OVERLINE),
            55 => pen.attrs.remove(Attrs::OVERLINE),
            59 => pen.ul = Color::Default,
            90..=97 => pen.fg = Color::Indexed((code - 90 + 8) as u8),
            100..=107 => pen.bg = Color::Indexed((code - 100 + 8) as u8),
            38 | 48 | 58 => {
                // Semicolon form: 5;n or 2;r;g;b.
                let rest = &p.vals[i..p.len];
                match rest {
                    [5, n, ..] => {
                        set_color(pen, code, Color::Indexed(clamp8(*n)));
                        i += 2;
                    }
                    [2, r, g, b, ..] => {
                        set_color(pen, code, Color::Rgb(clamp8(*r), clamp8(*g), clamp8(*b)));
                        i += 4;
                    }
                    _ => i = p.len,
                }
            }
            _ => {}
        }
    }
}

fn reset(pen: &mut Style) {
    let link = pen.link;
    *pen = Style {
        link,
        ..Style::default()
    };
}

fn set_color(pen: &mut Style, code: u16, c: Color) {
    match code {
        38 => pen.fg = c,
        48 => pen.bg = c,
        _ => pen.ul = c,
    }
}

fn set_underline(pen: &mut Style, shape: u16) {
    pen.attrs.remove(Attrs::UNDERLINE | Attrs::UL_SHAPES);
    match shape {
        0 => {}
        2 => pen.attrs.insert(Attrs::UNDERLINE | Attrs::UL_DOUBLE),
        3 => pen.attrs.insert(Attrs::UNDERLINE | Attrs::UL_CURLY),
        4 => pen.attrs.insert(Attrs::UNDERLINE | Attrs::UL_DOTTED),
        5 => pen.attrs.insert(Attrs::UNDERLINE | Attrs::UL_DASHED),
        _ => pen.attrs.insert(Attrs::UNDERLINE),
    }
}

fn color(out: &mut Vec<u8>, base: u16, c: Color) {
    match c {
        Color::Default => {}
        Color::Indexed(n) if n < 8 => {
            let _ = write!(out, ";{}", base + n as u16);
        }
        Color::Indexed(n) if n < 16 => {
            let _ = write!(out, ";{}", base + 60 + (n - 8) as u16);
        }
        Color::Indexed(n) => {
            let _ = write!(out, ";{};5;{}", base + 8, n);
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(out, ";{};2;{};{};{}", base + 8, r, g, b);
        }
    }
}

/// The SGR sequence that sets the rendition of `style` from any state. The
/// hyperlink is not part of it.
pub fn write_sgr(out: &mut Vec<u8>, style: &Style) {
    out.extend_from_slice(b"\x1b[0");
    let a = style.attrs;
    for (bit, code) in [
        (Attrs::BOLD, "1"),
        (Attrs::DIM, "2"),
        (Attrs::ITALIC, "3"),
        (Attrs::BLINK, "5"),
        (Attrs::INVERSE, "7"),
        (Attrs::HIDDEN, "8"),
        (Attrs::STRIKE, "9"),
        (Attrs::OVERLINE, "53"),
    ] {
        if a.contains(bit) {
            out.push(b';');
            out.extend_from_slice(code.as_bytes());
        }
    }
    if a.contains(Attrs::UNDERLINE) {
        let code = if a.contains(Attrs::UL_DOUBLE) {
            ";4:2"
        } else if a.contains(Attrs::UL_CURLY) {
            ";4:3"
        } else if a.contains(Attrs::UL_DOTTED) {
            ";4:4"
        } else if a.contains(Attrs::UL_DASHED) {
            ";4:5"
        } else {
            ";4"
        };
        out.extend_from_slice(code.as_bytes());
    }
    color(out, 30, style.fg);
    color(out, 40, style.bg);
    // Colon form only: a host that does not know it ignores it whole.
    match style.ul {
        Color::Default => {}
        Color::Indexed(n) => {
            let _ = write!(out, ";58:5:{n}");
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(out, ";58:2::{r}:{g}:{b}");
        }
    }
    out.push(b'm');
}
