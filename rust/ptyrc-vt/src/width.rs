// SPDX-License-Identifier: GPL-3.0-or-later
//! Glyph widths, as the emulator counts them.

use unicode_width::UnicodeWidthChar;

/// Columns a character takes: 1 or 2; 0 for a combining mark, which joins
/// the glyph before it, and for a control character.
pub fn char_width(c: char) -> usize {
    if c.is_ascii() {
        if c.is_ascii_control() { 0 } else { 1 }
    } else {
        c.width().unwrap_or(0)
    }
}

/// Columns of a string printed on one line.
pub fn str_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('世'), 2);
        assert_eq!(char_width('\u{301}'), 0);
        assert_eq!(char_width('\n'), 0);
        assert_eq!(str_width("ae\u{301}世界🌼"), 8);
    }
}
