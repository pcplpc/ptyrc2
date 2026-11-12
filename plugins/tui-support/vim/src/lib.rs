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

/// Text of a screen row over the columns of the pane.
fn screen_row(pane: &PaneView, y: u16) -> String {
    (pane.rect.x..pane.rect.right())
        .map(|x| pane.grid.cell(x, y))
        .filter(|c| c.width != 0)
        .map(|c| c.text.as_str())
        .collect()
}

/// The pane reaches the bottom of the screen: it holds the command line.
fn at_bottom(pane: &PaneView) -> bool {
    pane.rect.bottom() == pane.grid.rows()
}

/// The status line of a window in a split: the row right below the pane.
fn status_below(pane: &PaneView) -> Option<String> {
    let y = pane.rect.bottom();
    if y >= pane.grid.rows() {
        return None;
    }
    let text = screen_row(pane, y);
    RULER.is_match(text.trim_end()).then_some(text)
}

/// Row of the pane (relative) holding the ruler, searched from the bottom.
fn ruler_row(pane: &PaneView) -> Option<(u16, String)> {
    let h = pane.rect.h;
    (0..h.min(3)).map(|i| h - 1 - i).find_map(|row| {
        let text = pane.row_text(row);
        RULER.is_match(text.trim_end()).then_some((row, text))
    })
}

/// The ruler text, inside the pane or on the status line below it.
fn ruler(pane: &PaneView) -> Option<String> {
    ruler_row(pane)
        .map(|(_, text)| text)
        .or_else(|| status_below(pane))
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
        if ruler(pane).is_some() {
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

    /// The command line, and the ruler row when it is a separate one. A
    /// window of a split has neither inside it.
    fn chrome(&self, pane: &PaneView) -> Option<Chrome> {
        let h = pane.rect.h;
        let bottom = match ruler_row(pane) {
            Some((row, _)) => h - row,
            None if at_bottom(pane) => 1,
            None => 0,
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
        let text = ruler(pane)?;
        let caps = RULER.captures(text.trim_end())?;
        matches!(&caps[3], "Top" | "All").then_some(ViewHint {
            row: Some(0),
            col: None,
        })
    }

    /// The file: named by the status line of a split window, else by the
    /// terminal title for a full-screen editor.
    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        if let Some(status) = status_below(pane) {
            let end = RULER.find(status.trim_end())?.start();
            let name = status[..end]
                .trim()
                .trim_end_matches("[+]")
                .trim_end_matches("[RO]")
                .trim();
            return (!name.is_empty()).then(|| format!("vim:{name}"));
        }
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
    fn window_of_a_split() {
        let grid = Grid::from_lines(
            40,
            &[
                "other 008 sigma mu",
                "other 009 theta",
                "other 010 tau",
                "dir/other.txt [+]         8,1        6%",
                "line 001: brown",
                "line 002: apple",
                "line 003: the",
                "dir/long.txt              1,1       Top",
                ":",
            ],
        );
        let pane = |y, h| PaneView {
            grid: &grid,
            rect: ptyrc_vt::Rect::new(0, y, 40, h),
            cursor: None,
            title: "",
            alt_screen: true,
        };
        let (top, low) = (pane(0, 3), pane(4, 3));
        assert!(Vim.detect(&top) >= 0.5);
        assert_eq!(Vim.chrome(&top).unwrap().bottom, 0);
        assert_eq!(Vim.view_hint(&top), None);
        assert_eq!(Vim.frame_key(&top).unwrap(), "vim:dir/other.txt");
        assert_eq!(Vim.view_hint(&low).unwrap().row, Some(0));
        assert_eq!(Vim.frame_key(&low).unwrap(), "vim:dir/long.txt");
    }

    #[test]
    fn plain_text_is_not_vim() {
        let grid = Grid::from_lines(30, &["$ ls", "a  b  c", "$ "]);
        assert!(Vim.detect(&pane(&grid, "zsh")) < 0.5);
    }
}
