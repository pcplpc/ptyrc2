// SPDX-License-Identifier: GPL-3.0-or-later
//! Columns of a scalar value, coarsely: the runner has the last word
//! (a control character, a glyph of no width: a space).

/// 0: joins the glyph before (dropped by `text`); 2: wide; else 1.
pub fn of(scalar: u32) -> i32 {
    const ZERO: &[(u32, u32)] = &[
        (0x0300, 0x036f),
        (0x200b, 0x200f),
        (0x202a, 0x202e),
        (0x2060, 0x2064),
        (0xfe00, 0xfe0f),
        (0xfeff, 0xfeff),
        (0xe0000, 0xe0fff),
    ];
    const WIDE: &[(u32, u32)] = &[
        (0x1100, 0x115f),
        (0x2e80, 0x303e),
        (0x3041, 0x33ff),
        (0x3400, 0x4dbf),
        (0x4e00, 0x9fff),
        (0xa000, 0xa4cf),
        (0xac00, 0xd7a3),
        (0xf900, 0xfaff),
        (0xfe30, 0xfe4f),
        (0xff00, 0xff60),
        (0xffe0, 0xffe6),
        (0x1f300, 0x1f64f),
        (0x1f900, 0x1f9ff),
        (0x20000, 0x3fffd),
    ];
    let within = |table: &[(u32, u32)]| table.iter().any(|&(a, b)| (a..=b).contains(&scalar));
    if scalar < 0x300 {
        1
    } else if within(ZERO) {
        0
    } else if within(WIDE) {
        2
    } else {
        1
    }
}
