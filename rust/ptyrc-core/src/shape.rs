// SPDX-License-Identifier: GPL-3.0-or-later
//! The shape of a screen: which cells are inked, and the cells of a few
//! words. See `plan/design.md`, `screen.shape`.
//!
//! One character per cell: string index == column. A blank cell is a
//! space, an inked cell `#`, a wide glyph `##`. A cell of a match of a
//! word is shown as on screen. Trailing spaces are trimmed.

use ptyrc_vt::{Cell, Grid, Rect, char_width};
use std::fmt;

/// Characters of a word, inclusive.
pub const WORD_LEN: (usize, usize) = (3, 32);
/// Words of a list.
pub const MAX_WORDS: usize = 32;

/// A rule of `Words::new` broken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WordError {
    /// More than `MAX_WORDS`; the count.
    TooMany(usize),
    /// Fewer characters than `WORD_LEN.0`.
    TooShort(String),
    /// More characters than `WORD_LEN.1`.
    TooLong(String),
    /// A character not alphanumeric.
    NotAlphanumeric(String, char),
    /// A character not one cell wide.
    NotNarrow(String, char),
    /// A character whose lowercase is not one character.
    Lowercase(String, char),
}

impl fmt::Display for WordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (min, max) = WORD_LEN;
        match self {
            WordError::TooMany(n) => write!(f, "words: {n}, at most {MAX_WORDS}"),
            WordError::TooShort(w) => {
                write!(
                    f,
                    "word {w:?}: {} characters, at least {min}",
                    w.chars().count()
                )
            }
            WordError::TooLong(w) => {
                write!(
                    f,
                    "word {w:?}: {} characters, at most {max}",
                    w.chars().count()
                )
            }
            WordError::NotAlphanumeric(w, c) => {
                write!(f, "word {w:?}: {c:?} is not a letter or a digit")
            }
            WordError::NotNarrow(w, c) => write!(f, "word {w:?}: {c:?} is not one cell wide"),
            WordError::Lowercase(w, c) => {
                write!(f, "word {w:?}: {c:?} changes length in lowercase")
            }
        }
    }
}

impl std::error::Error for WordError {}

/// A checked list of words, lowercased, without duplicates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Words(Vec<Vec<char>>);

impl Words {
    pub fn new<S: AsRef<str>>(words: &[S]) -> Result<Words, WordError> {
        if words.len() > MAX_WORDS {
            return Err(WordError::TooMany(words.len()));
        }
        let mut out: Vec<Vec<char>> = Vec::new();
        for word in words {
            let word = word.as_ref();
            let n = word.chars().count();
            if n < WORD_LEN.0 {
                return Err(WordError::TooShort(word.into()));
            }
            if n > WORD_LEN.1 {
                return Err(WordError::TooLong(word.into()));
            }
            let mut low = Vec::with_capacity(n);
            for c in word.chars() {
                if !c.is_alphanumeric() {
                    return Err(WordError::NotAlphanumeric(word.into(), c));
                }
                if char_width(c) != 1 {
                    return Err(WordError::NotNarrow(word.into(), c));
                }
                low.push(lower(c).ok_or_else(|| WordError::Lowercase(word.into(), c))?);
            }
            if !out.contains(&low) {
                out.push(low);
            }
        }
        Ok(Words(out))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The lowercase of `c` when it is one character.
fn lower(c: char) -> Option<char> {
    let mut low = c.to_lowercase();
    match (low.next(), low.next()) {
        (Some(l), None) => Some(l),
        _ => None,
    }
}

fn blank(cell: &Cell) -> bool {
    cell.width != 0 && cell.text.chars().all(char::is_whitespace)
}

/// The shape of `rect` of `grid` (clipped to the grid), one string a row.
pub fn shape(grid: &Grid, rect: Rect, words: &Words) -> Vec<String> {
    let rect = rect.intersect(&grid.rect());
    (rect.y..rect.bottom())
        .map(|row| {
            let cells = grid.row(row);
            let shown = matches(cells, words);
            let line: String = (rect.x..rect.right())
                .map(|col| {
                    let cell = &cells[col as usize];
                    if shown[col as usize] {
                        cell.text.chars().next().unwrap_or(' ')
                    } else if blank(cell) {
                        ' '
                    } else {
                        '#'
                    }
                })
                .collect();
            line.trim_end_matches(' ').to_string()
        })
        .collect()
}

/// Per cell of a full row: part of a match of a word.
fn matches(cells: &[Cell], words: &Words) -> Vec<bool> {
    let mut shown = vec![false; cells.len()];
    if words.is_empty() {
        return shown;
    }
    // A cell is alphanumeric by its glyph; a continuation by its lead.
    let alnum = |i: usize| {
        let lead = if cells[i].width == 0 && i > 0 {
            &cells[i - 1]
        } else {
            &cells[i]
        };
        lead.text.chars().next().is_some_and(char::is_alphanumeric)
    };
    // A narrow cell of one character, lowercased.
    let low = |cell: &Cell| {
        let mut chars = cell.text.chars();
        match (cell.width, chars.next(), chars.next()) {
            (1, Some(c), None) => lower(c),
            _ => None,
        }
    };
    // A whole word is a maximal run of alphanumeric cells.
    let mut start = 0;
    while start < cells.len() {
        if !alnum(start) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < cells.len() && alnum(end) {
            end += 1;
        }
        let run = &cells[start..end];
        let hit = words.0.iter().any(|w| {
            w.len() == run.len() && w.iter().zip(run).all(|(c, cell)| low(cell) == Some(*c))
        });
        if hit {
            shown[start..end].fill(true);
        }
        start = end;
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::Style;

    fn words(list: &[&str]) -> Words {
        Words::new(list).unwrap()
    }

    fn all(grid: &Grid, list: &[&str]) -> Vec<String> {
        shape(grid, grid.rect(), &words(list))
    }

    #[test]
    fn blanks_and_ink() {
        let g = Grid::from_lines(12, &["", "ab  c", "│ x─┐  ", "\t. "]);
        assert_eq!(all(&g, &[]), ["", "##  #", "# ###", " #"]);
    }

    #[test]
    fn wide_glyphs_take_two_cells() {
        let mut g = Grid::new(8, 1);
        g.put(0, 0, Cell::narrow("a", Style::default()));
        g.put(1, 0, Cell::wide("世", Style::default()));
        g.put(4, 0, Cell::narrow("b", Style::default()));
        let lines = all(&g, &[]);
        assert_eq!(lines, ["### #"]);
        // String index == column.
        assert_eq!(lines[0].chars().nth(4), Some('#'));
    }

    #[test]
    fn words_keep_their_case() {
        let g = Grid::from_lines(20, &["a Fish, FISH fish."]);
        assert_eq!(all(&g, &["fish"]), ["# Fish# FISH fish#"]);
        assert_eq!(all(&g, &["FiSh"]), ["# Fish# FISH fish#"]);
    }

    #[test]
    fn whole_words_only() {
        let g = Grid::from_lines(30, &["fish_food fishing toys toy"]);
        assert_eq!(
            all(&g, &["fish", "food", "toy"]),
            ["fish#food ####### #### toy"]
        );
    }

    #[test]
    fn the_edge_of_the_row_is_a_boundary() {
        let g = Grid::from_lines(8, &["cat  dog"]);
        assert_eq!(all(&g, &["cat", "dog"]), ["cat  dog"]);
    }

    #[test]
    fn a_rect_judges_boundaries_on_the_full_row() {
        let g = Grid::from_lines(20, &["xx fishing fish"]);
        let w = words(&["fish"]);
        // `fishing` cut to `fish` does not match.
        assert_eq!(shape(&g, Rect::new(3, 0, 4, 1), &w), ["####"]);
        // A cut match shows its cells inside the rect.
        assert_eq!(shape(&g, Rect::new(13, 0, 5, 1), &w), ["sh"]);
        // A rect past the grid is clipped.
        assert_eq!(shape(&g, Rect::new(11, 0, 40, 5), &w), ["fish"]);
    }

    #[test]
    fn wide_cells_never_match() {
        let mut g = Grid::from_lines(10, &["ab"]);
        g.put(2, 0, Cell::wide("ｃ", Style::default()));
        assert_eq!(all(&g, &["abc"]), ["####"]);
        // A wide letter next to a word is part of its run.
        let mut g = Grid::from_lines(10, &["  cat"]);
        g.put(0, 0, Cell::wide("世", Style::default()));
        assert_eq!(all(&g, &["cat"]), ["#####"]);
        let mut g = Grid::from_lines(10, &["   cat"]);
        g.put(0, 0, Cell::wide("世", Style::default()));
        assert_eq!(all(&g, &["cat"]), ["## cat"]);
    }

    #[test]
    fn word_rules() {
        let long = "a".repeat(33);
        let many: Vec<String> = (0..33).map(|i| format!("w{i:02}")).collect();
        let cases: [(&[&str], WordError); 6] = [
            (&["fi"], WordError::TooShort("fi".into())),
            (&[&long], WordError::TooLong(long.clone())),
            (
                &["fish-food"],
                WordError::NotAlphanumeric("fish-food".into(), '-'),
            ),
            (&["İzmir"], WordError::Lowercase("İzmir".into(), 'İ')),
            (&["世界地"], WordError::NotNarrow("世界地".into(), '世')),
            (&["cat", ""], WordError::TooShort("".into())),
        ];
        for (list, error) in cases {
            assert_eq!(Words::new(list), Err(error));
        }
        assert_eq!(Words::new(&many), Err(WordError::TooMany(33)));
        assert!(Words::new(&many[..32]).is_ok());
        assert!(Words::new(&["a".repeat(32), "abc".into(), "Éte9".into()]).is_ok());
        assert_eq!(words(&["Cat", "cat"]), words(&["cat"]));
        let texts: Vec<String> = [
            WordError::TooMany(33),
            WordError::TooShort("fi".into()),
            WordError::TooLong(long),
            WordError::NotAlphanumeric("fish-food".into(), '-'),
            WordError::NotNarrow("世界地".into(), '世'),
            WordError::Lowercase("İzmir".into(), 'İ'),
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            texts,
            [
                "words: 33, at most 32",
                "word \"fi\": 2 characters, at least 3",
                "word \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\": 33 characters, at most 32",
                "word \"fish-food\": '-' is not a letter or a digit",
                "word \"世界地\": '世' is not one cell wide",
                "word \"İzmir\": 'İ' changes length in lowercase",
            ]
        );
    }
}
