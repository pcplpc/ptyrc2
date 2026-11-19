// SPDX-License-Identifier: GPL-3.0-or-later
//! Glyph widths, as the emulator counts them.

use std::borrow::Cow;
use unicode_width::UnicodeWidthChar;

/// Characters with no glyph whose effect or width terminals disagree on:
/// the format characters (Cf: bidi embeddings, overrides and isolates,
/// marks, joiners, zero-width space, word joiner, invisible operators,
/// BOM, soft hyphen, tags, prepended marks), the line and paragraph
/// separators, and the variation selectors 15 and 16 (text or emoji
/// presentation, one cell or two).
pub fn is_hidden(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{fe0e}'..='\u{fe0f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

/// A character of untrusted text as it may enter a cell: a control
/// character (C0, DEL, C1) is a space, a hidden one (`is_hidden`) is
/// dropped, any other is itself.
pub fn drawn(c: char) -> Option<char> {
    if c.is_control() {
        Some(' ')
    } else if is_hidden(c) {
        None
    } else {
        Some(c)
    }
}

/// `text` as glyphs only, by `drawn`: no control character, no escape
/// sequence, no hidden character.
pub fn glyphs_only(text: &str) -> Cow<'_, str> {
    if text.chars().all(|c| drawn(c) == Some(c)) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.chars().filter_map(drawn).collect())
    }
}

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

    #[test]
    fn glyphs() {
        assert_eq!(
            glyphs_only("a\x1b]52;c;eA==\x07b\u{9b}31m\u{202e}c\u{200d}\u{fe0f}\t"),
            "a ]52;c;eA== b 31mc "
        );
        assert!(matches!(glyphs_only("é世🌼"), Cow::Borrowed(_)));
        assert!(!is_hidden('\u{301}'));
        assert!(!is_hidden('\u{fe00}'));
    }
}
