// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset for tmux.
//!
//! Recognised from its status line (`[session] 0:zsh* 1:vim-`), on the
//! last or the first row. The frame key names the session and the current
//! window, so the panes of a window are found again when it comes back.

use ptyrc_core::plugin::{PaneView, TuiSupport};
use regex::Regex;
use std::sync::LazyLock;

static STATUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[([^\]]+)\]\s+((?:\d+:\S+\s*)+)").unwrap());
static CURRENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+):\S*?\*[A-Z]*(?:\s|$)").unwrap());

pub struct Tmux;

/// `(session, current window)` from the status line of the whole screen.
fn status(pane: &PaneView) -> Option<(String, String)> {
    let grid = pane.grid;
    if grid.rows() == 0 {
        return None;
    }
    [grid.rows() - 1, 0].into_iter().find_map(|row| {
        let text = grid.row_text(row);
        let caps = STATUS.captures(&text)?;
        let window = CURRENT.captures(&caps[2]).map(|c| c[1].to_string())?;
        Some((caps[1].to_string(), window))
    })
}

impl TuiSupport for Tmux {
    fn name(&self) -> &'static str {
        "tmux"
    }

    /// tmux fills the screen: it is seen on the whole screen only. What a
    /// pane of it shows is another preset's business.
    fn detect(&self, pane: &PaneView) -> f32 {
        if pane.rect == pane.grid.rect() && status(pane).is_some() {
            0.9
        } else {
            0.0
        }
    }

    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        let (session, window) = status(pane)?;
        Some(format!("tmux:{session}:{window}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::{Grid, Rect};

    #[test]
    fn status_line() {
        let grid = Grid::from_lines(
            60,
            &[
                "$ ls",
                "",
                "[work] 0:zsh- 1:vim* 2:htop           \"host\" 14:02 12-Nov-26",
            ],
        );
        let pane = |rect| PaneView {
            grid: &grid,
            rect,
            area: rect,
            cursor: None,
            title: "",
            alt_screen: true,
        };
        assert!(Tmux.detect(&pane(grid.rect())) >= 0.5);
        assert_eq!(Tmux.detect(&pane(Rect::new(0, 0, 60, 2))), 0.0);
        assert_eq!(Tmux.frame_key(&pane(grid.rect())).unwrap(), "tmux:work:1");
    }

    #[test]
    fn not_tmux() {
        let grid = Grid::from_lines(40, &["[note] this is text", "0:00 elapsed"]);
        let p = PaneView {
            grid: &grid,
            rect: grid.rect(),
            area: grid.rect(),
            cursor: None,
            title: "",
            alt_screen: false,
        };
        assert_eq!(Tmux.detect(&p), 0.0);
    }
}
