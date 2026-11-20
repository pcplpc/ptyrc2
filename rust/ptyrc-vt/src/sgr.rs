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
            out.push(b';');
            push_num(out, base + n as u16);
        }
        Color::Indexed(n) if n < 16 => {
            out.push(b';');
            push_num(out, base + 60 + (n - 8) as u16);
        }
        Color::Indexed(n) => {
            out.push(b';');
            push_num(out, base + 8);
            out.extend_from_slice(b";5;");
            push_num(out, n as u16);
        }
        Color::Rgb(r, g, b) => {
            out.push(b';');
            push_num(out, base + 8);
            out.extend_from_slice(b";2");
            for v in [r, g, b] {
                out.push(b';');
                push_num(out, v as u16);
            }
        }
    }
}

/// `n` in decimal. A sequence is written a cell or a run: no formatter.
pub(crate) fn push_num(out: &mut Vec<u8>, n: u16) {
    let mut buf = [0u8; 5];
    let mut i = buf.len();
    let mut n = n;
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[i..]);
}

/// The SGR sequence that turns the rendition `from` into `to`, not the
/// same: the colours that differ when nothing else does, `write_sgr`
/// otherwise.
pub(crate) fn write_sgr_from(out: &mut Vec<u8>, from: &Style, to: &Style) {
    if from.attrs != to.attrs || from.ul != to.ul {
        write_sgr(out, to);
        return;
    }
    let mut seq = Seq {
        bytes: [0; 48],
        len: 0,
    };
    seq.put(b"\x1b[");
    for (base, old, new) in [(30, from.fg, to.fg), (40, from.bg, to.bg)] {
        if old == new {
            continue;
        }
        if seq.len > 2 {
            seq.put(b";");
        }
        match new {
            Color::Default => seq.num(base + 9),
            Color::Indexed(n) if n < 8 => seq.num(base + n),
            Color::Indexed(n) if n < 16 => seq.num(base + 60 + (n - 8)),
            Color::Indexed(n) => {
                seq.num(base + 8);
                seq.put(b";5;");
                seq.num(n);
            }
            Color::Rgb(r, g, b) => {
                seq.num(base + 8);
                seq.put(b";2;");
                seq.num(r);
                seq.put(b";");
                seq.num(g);
                seq.put(b";");
                seq.num(b);
            }
        }
    }
    seq.put(b"m");
    // A copy of fixed size, cut after: no call to copy a few bytes.
    let at = out.len();
    out.extend_from_slice(&seq.bytes);
    out.truncate(at + seq.len);
}

/// A colour change built on the stack, appended at once: at most
/// `ESC [`, two colours of 16 bytes, `;`, `m`; the rest is room for the
/// copies of `num`.
struct Seq {
    bytes: [u8; 48],
    len: usize,
}

/// `n` in decimal: up to three digits, then their count.
static DECIMAL: [[u8; 4]; 256] = {
    let mut out = [[0u8; 4]; 256];
    let mut n = 0;
    while n < 256 {
        let (h, t, o) = ((n / 100) as u8, (n / 10 % 10) as u8, (n % 10) as u8);
        out[n] = if n >= 100 {
            [b'0' + h, b'0' + t, b'0' + o, 3]
        } else if n >= 10 {
            [b'0' + t, b'0' + o, 0, 2]
        } else {
            [b'0' + o, 0, 0, 1]
        };
        n += 1;
    }
    out
};

impl Seq {
    fn put(&mut self, s: &[u8]) {
        self.bytes[self.len..self.len + s.len()].copy_from_slice(s);
        self.len += s.len();
    }

    /// `n` in decimal: four bytes copied, the digits kept.
    fn num(&mut self, n: u8) {
        let d = &DECIMAL[n as usize];
        self.bytes[self.len..self.len + 4].copy_from_slice(d);
        self.len += d[3] as usize;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn colours() -> Vec<Color> {
        let mut all = vec![Color::Default];
        all.extend((0..=255).map(Color::Indexed));
        for v in [0, 1, 9, 10, 99, 100, 199, 255] {
            all.push(Color::Rgb(v, 255 - v, v / 2));
        }
        all
    }

    /// The colours-only form, as `format!` spells it.
    fn reference(from: &Style, to: &Style) -> String {
        let mut params = Vec::new();
        for (base, old, new) in [(30u16, from.fg, to.fg), (40, from.bg, to.bg)] {
            if old == new {
                continue;
            }
            params.push(match new {
                Color::Default => format!("{}", base + 9),
                Color::Indexed(n) if n < 8 => format!("{}", base + n as u16),
                Color::Indexed(n) if n < 16 => format!("{}", base + 60 + (n - 8) as u16),
                Color::Indexed(n) => format!("{};5;{n}", base + 8),
                Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
            });
        }
        format!("\x1b[{}m", params.join(";"))
    }

    /// A change of colours: the bytes of `reference`, and read back,
    /// the rendition wanted.
    #[test]
    fn colours_written_and_read_back() {
        let all = colours();
        let style = |fg, bg| Style {
            fg,
            bg,
            attrs: Attrs(Attrs::BOLD),
            ..Style::default()
        };
        for &a in &all {
            for &b in &all {
                for (old, new) in [
                    (style(a, Color::Default), style(b, Color::Indexed(4))),
                    (style(Color::Indexed(1), a), style(Color::Indexed(1), b)),
                    (style(a, b), style(b, a)),
                ] {
                    if old == new {
                        continue;
                    }
                    let mut out = Vec::new();
                    write_sgr_from(&mut out, &old, &new);
                    assert_eq!(String::from_utf8_lossy(&out), reference(&old, &new));
                    let mut p = Params {
                        vals: [0; 32],
                        len: 0,
                        sub: 0,
                    };
                    for v in out[2..out.len() - 1].split(|&c| c == b';') {
                        p.vals[p.len] = std::str::from_utf8(v).unwrap().parse().unwrap();
                        p.len += 1;
                    }
                    let mut pen = old;
                    apply(&mut pen, &p);
                    assert_eq!(pen, new);
                }
            }
        }
    }
}
