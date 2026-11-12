// SPDX-License-Identifier: GPL-3.0-or-later
//! Colours as RGB: the xterm 256-colour table, the colour specification
//! of an OSC 10 / 11 reply, linear mixes.

use crate::grid::Color;

/// Red, green, blue.
pub type Rgb = [u8; 3];

/// The 16 first colours, as xterm ships them.
const BASE: [Rgb; 16] = [
    [0x00, 0x00, 0x00],
    [0xcd, 0x00, 0x00],
    [0x00, 0xcd, 0x00],
    [0xcd, 0xcd, 0x00],
    [0x00, 0x00, 0xee],
    [0xcd, 0x00, 0xcd],
    [0x00, 0xcd, 0xcd],
    [0xe5, 0xe5, 0xe5],
    [0x7f, 0x7f, 0x7f],
    [0xff, 0x00, 0x00],
    [0x00, 0xff, 0x00],
    [0xff, 0xff, 0x00],
    [0x5c, 0x5c, 0xff],
    [0xff, 0x00, 0xff],
    [0x00, 0xff, 0xff],
    [0xff, 0xff, 0xff],
];

/// Colour `n` of the xterm table: 16 base colours, a 6x6x6 cube, 24 greys.
pub fn xterm(n: u8) -> Rgb {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match n {
        0..16 => BASE[n as usize],
        16..232 => {
            let i = n - 16;
            [
                STEPS[(i / 36) as usize],
                STEPS[(i / 6 % 6) as usize],
                STEPS[(i % 6) as usize],
            ]
        }
        _ => [8 + 10 * (n - 232); 3],
    }
}

/// `color` as RGB; `default` stands for `Color::Default`.
pub fn rgb(color: Color, default: Rgb) -> Rgb {
    match color {
        Color::Default => default,
        Color::Indexed(n) => xterm(n),
        Color::Rgb(r, g, b) => [r, g, b],
    }
}

/// `over` at opacity `alpha` on `under`, channel by channel. `alpha` is
/// clamped to `0..=1`.
pub fn mix(under: Rgb, over: Rgb, alpha: f32) -> Rgb {
    let a = if alpha.is_nan() {
        1.0
    } else {
        alpha.clamp(0.0, 1.0)
    };
    let ch = |u: u8, o: u8| (u as f32 + (o as f32 - u as f32) * a).round() as u8;
    [
        ch(under[0], over[0]),
        ch(under[1], over[1]),
        ch(under[2], over[2]),
    ]
}

/// One channel of `rgb:`: 1 to 4 hex digits, scaled to 8 bits.
fn channel(s: &[u8]) -> Option<u8> {
    if s.is_empty() || s.len() > 4 {
        return None;
    }
    let mut v: u32 = 0;
    for b in s {
        v = v * 16 + (*b as char).to_digit(16)?;
    }
    let max = (1u32 << (4 * s.len())) - 1;
    Some(((v * 255 + max / 2) / max) as u8)
}

/// A colour specification as a terminal reports it: `rgb:RRRR/GGGG/BBBB`
/// (1 to 4 digits a channel), `rgba:...` (alpha dropped), `#RRGGBB`.
pub fn parse_color_spec(spec: &[u8]) -> Option<Rgb> {
    if let Some(hex) = spec.strip_prefix(b"#") {
        if hex.len() != 6 {
            return None;
        }
        return Some([
            channel(&hex[0..2])?,
            channel(&hex[2..4])?,
            channel(&hex[4..6])?,
        ]);
    }
    let body = spec
        .strip_prefix(b"rgba:")
        .or_else(|| spec.strip_prefix(b"rgb:"))?;
    let mut parts = body.split(|b| *b == b'/');
    let out = [
        channel(parts.next()?)?,
        channel(parts.next()?)?,
        channel(parts.next()?)?,
    ];
    // At most the alpha of `rgba:` after the three.
    (parts.count() <= 1).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(xterm(1), [0xcd, 0, 0]);
        assert_eq!(xterm(16), [0, 0, 0]);
        assert_eq!(xterm(17), [0, 0, 95]);
        assert_eq!(xterm(196), [255, 0, 0]);
        assert_eq!(xterm(231), [255, 255, 255]);
        assert_eq!(xterm(232), [8, 8, 8]);
        assert_eq!(xterm(255), [238, 238, 238]);
        assert_eq!(rgb(Color::Default, [1, 2, 3]), [1, 2, 3]);
        assert_eq!(rgb(Color::Rgb(4, 5, 6), [1, 2, 3]), [4, 5, 6]);
    }

    #[test]
    fn mixes() {
        assert_eq!(mix([0, 0, 0], [255, 100, 10], 1.0), [255, 100, 10]);
        assert_eq!(mix([9, 9, 9], [255, 100, 10], 0.0), [9, 9, 9]);
        assert_eq!(mix([0, 0, 0], [255, 100, 10], 0.5), [128, 50, 5]);
        assert_eq!(mix([0, 0, 0], [200, 0, 0], 7.0), [200, 0, 0]);
    }

    #[test]
    fn specs() {
        assert_eq!(parse_color_spec(b"rgb:ffff/8080/0000"), Some([255, 128, 0]));
        assert_eq!(parse_color_spec(b"rgb:ff/80/00"), Some([255, 128, 0]));
        assert_eq!(parse_color_spec(b"rgb:f/8/0"), Some([255, 136, 0]));
        assert_eq!(
            parse_color_spec(b"rgba:1e1e/1e1e/2e2e/ffff"),
            Some([30, 30, 46])
        );
        assert_eq!(parse_color_spec(b"#1e1e2e"), Some([30, 30, 46]));
        assert_eq!(parse_color_spec(b"rgb:ff/80"), None);
        assert_eq!(parse_color_spec(b"rgb:ff/80/zz"), None);
        assert_eq!(parse_color_spec(b"rgb:fffff/0/0"), None);
        assert_eq!(parse_color_spec(b"red"), None);
    }
}
