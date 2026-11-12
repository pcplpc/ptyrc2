// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset for vim-like editors (vim, neovim, vi).
//!
//! Recognised from the screen alone: the ruler (`12,5  Top`), the mode line
//! (`-- INSERT --`), the `~` filler rows.

use ptyrc_core::plugin::{Chrome, PaneView, TuiSupport, ViewHint};
use regex::Regex;
use std::sync::LazyLock;

static RULER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+),(\d+)(?:-\d+)?\s+(All|Top|Bot|\d+%)\s*$").unwrap());
static MODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^-- (INSERT|VISUAL|VISUAL LINE|VISUAL BLOCK|REPLACE|SELECT|TERMINAL)( --|\b)")
        .unwrap()
});
static GUTTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(\d+) ").unwrap());

pub struct Vim;

/// Row of the pane (relative) holding the ruler, searched from the bottom.
fn ruler_row(pane: &PaneView) -> Option<(u16, String)> {
    let h = pane.rect.h;
    (0..h.min(3)).map(|i| h - 1 - i).find_map(|row| {
        let text = pane.row_text(row);
        RULER.is_match(text.trim_end()).then_some((row, text))
    })
}

fn tilde_rows(pane: &PaneView) -> u16 {
    (0..pane.rect.h)
        .filter(|&r| pane.row_text(r).trim_end() == "~")
        .count() as u16
}

impl TuiSupport for Vim {
    fn name(&self) -> &'static str {
        "vim"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if pane.rect.h < 3 {
            return 0.0;
        }
        let mut score: f32 = 0.0;
        if ruler_row(pane).is_some() {
            score += 0.6;
        }
        let last = pane.row_text(pane.rect.h - 1);
        if MODE.is_match(&last) {
            score += 0.4;
        }
        if tilde_rows(pane) >= 2 {
            score += 0.4;
        }
        let title = pane.title;
        if title.ends_with("VIM") || title.ends_with("NVIM") || title.ends_with("- Nvim") {
            score += 0.3;
        }
        score.min(1.0)
    }

    /// The command line, and the ruler row when it is a separate one.
    fn chrome(&self, pane: &PaneView) -> Option<Chrome> {
        let h = pane.rect.h;
        let bottom = match ruler_row(pane) {
            Some((row, _)) => h - row,
            None => 1,
        };
        Some(Chrome {
            bottom: bottom.min(h.saturating_sub(1)),
            ..Chrome::default()
        })
    }

    /// Absolute row of the first line shown. From the number column when
    /// every row carries consecutive numbers (no wrapped line, no relative
    /// numbers); row 0 when the ruler says `Top` or `All`.
    fn view_hint(&self, pane: &PaneView) -> Option<ViewHint> {
        let chrome = self.chrome(pane).unwrap_or_default();
        let body = pane.rect.h.saturating_sub(chrome.bottom);
        let mut first: Option<i64> = None;
        let mut consecutive = body >= 2;
        for row in 0..body {
            let text = pane.row_text(row);
            if text.trim_end() == "~" {
                break;
            }
            match GUTTER
                .captures(&text)
                .and_then(|c| c[1].parse::<i64>().ok())
            {
                Some(n) => match first {
                    None => first = Some(n),
                    Some(f) if n == f + row as i64 => {}
                    Some(_) => consecutive = false,
                },
                None => consecutive = false,
            }
            if !consecutive {
                break;
            }
        }
        if let (true, Some(n)) = (consecutive, first) {
            return Some(ViewHint {
                row: Some(n - 1),
                col: None,
            });
        }
        let (_, text) = ruler_row(pane)?;
        let caps = RULER.captures(text.trim_end())?;
        matches!(&caps[3], "Top" | "All").then_some(ViewHint {
            row: Some(0),
            col: None,
        })
    }

    /// The file named by the terminal title, for a full-screen editor.
    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        if pane.rect != pane.grid.rect() {
            return None;
        }
        let title = pane.title;
        let name = title
            .strip_suffix(" - VIM")
            .or_else(|| title.strip_suffix(" - NVIM"))
            .or_else(|| title.strip_suffix(" - Nvim"))?;
        Some(format!("vim:{name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::Grid;

    fn pane<'a>(grid: &'a Grid, title: &'a str) -> PaneView<'a> {
        PaneView {
            grid,
            rect: grid.rect(),
            cursor: None,
            title,
            alt_screen: true,
        }
    }

    #[test]
    fn detects_and_hints() {
        let grid = Grid::from_lines(
            40,
            &[
                " 10 fn main() {",
                " 11     let x = 1;",
                " 12 }",
                " 13 ",
                "-- INSERT --                 11,5     Top",
            ],
        );
        let p = pane(&grid, "main.rs (~/src) - VIM");
        assert!(Vim.detect(&p) >= 0.5);
        assert_eq!(Vim.chrome(&p).unwrap().bottom, 1);
        assert_eq!(Vim.view_hint(&p).unwrap().row, Some(9));
        assert_eq!(Vim.frame_key(&p).unwrap(), "vim:main.rs (~/src)");
    }

    #[test]
    fn top_without_numbers() {
        let grid = Grid::from_lines(30, &["hello", "~", "~", "~", "\"x\" 1L, 6B      1,1   All"]);
        let p = pane(&grid, "");
        assert!(Vim.detect(&p) >= 0.5);
        assert_eq!(Vim.view_hint(&p).unwrap().row, Some(0));
    }

    #[test]
    fn plain_text_is_not_vim() {
        let grid = Grid::from_lines(30, &["$ ls", "a  b  c", "$ "]);
        assert!(Vim.detect(&pane(&grid, "zsh")) < 0.5);
    }
}
