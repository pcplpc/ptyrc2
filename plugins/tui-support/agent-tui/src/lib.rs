// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset for coding-agent TUIs: a transcript that scrolls above a prompt
//! box pinned at the bottom.
//!
//! Recognised from the prompt box alone: a rounded box (`╭─╮ │ ╰─╯`) or a
//! prompt line between two horizontal rules, in the lower part of the
//! pane. The box and what is below it are chrome.

use ptyrc_core::plugin::{Chrome, PaneView, TuiSupport};

pub struct AgentTui;

fn is_rule(text: &str, min: usize) -> bool {
    let t = text.trim();
    t.chars().count() >= min && t.chars().all(|c| matches!(c, '─' | '━' | '═'))
}

fn is_box_top(text: &str, min: usize) -> bool {
    let t = text.trim();
    t.chars().count() >= min && t.starts_with(['╭', '┌']) && t.ends_with(['╮', '┐'])
}

fn is_box_bottom(text: &str) -> bool {
    let t = text.trim();
    t.starts_with(['╰', '└']) && t.ends_with(['╯', '┘'])
}

fn is_prompt(text: &str) -> bool {
    let t = text.trim_start_matches(['│', ' ']);
    t.starts_with(['>', '❯', '›'])
}

/// Row (relative) where the prompt area starts: the top of the lowest
/// prompt box.
fn prompt_top(pane: &PaneView) -> Option<u16> {
    let h = pane.rect.h;
    if h < 4 {
        return None;
    }
    let min = (pane.rect.w as usize * 6) / 10;
    let rows: Vec<String> = (0..h).map(|r| pane.row_text(r)).collect();
    // Searched upwards, within the lower two thirds.
    let floor = h / 3;
    for top in (floor..h.saturating_sub(2)).rev() {
        let text = &rows[top as usize];
        let boxed = is_box_top(text, min);
        if !boxed && !is_rule(text, min) {
            continue;
        }
        let inner = &rows[top as usize + 1];
        if !is_prompt(inner) {
            continue;
        }
        let closed = rows[top as usize + 2..].iter().take(12).any(|t| {
            if boxed {
                is_box_bottom(t)
            } else {
                is_rule(t, min)
            }
        });
        if closed {
            return Some(top);
        }
    }
    None
}

impl TuiSupport for AgentTui {
    fn name(&self) -> &'static str {
        "agent-tui"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if prompt_top(pane).is_some() { 0.8 } else { 0.0 }
    }

    fn chrome(&self, pane: &PaneView) -> Option<Chrome> {
        let top = prompt_top(pane)?;
        Some(Chrome {
            bottom: pane.rect.h - top,
            ..Chrome::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::Grid;

    fn pane(grid: &Grid) -> PaneView<'_> {
        PaneView {
            grid,
            rect: grid.rect(),
            cursor: None,
            title: "",
            alt_screen: false,
        }
    }

    #[test]
    fn rounded_box() {
        let grid = Grid::from_lines(
            30,
            &[
                "● Reading the file",
                "  ⎿ 120 lines",
                "",
                "╭────────────────────────────╮",
                "│ > fix the test             │",
                "╰────────────────────────────╯",
                "  ? for shortcuts",
            ],
        );
        let p = pane(&grid);
        assert!(AgentTui.detect(&p) >= 0.5);
        assert_eq!(AgentTui.chrome(&p).unwrap().bottom, 4);
    }

    #[test]
    fn rules() {
        let grid = Grid::from_lines(
            24,
            &[
                "text",
                "more",
                "────────────────────────",
                "❯ hello",
                "────────────────────────",
                " status",
            ],
        );
        assert_eq!(AgentTui.chrome(&pane(&grid)).unwrap().bottom, 4);
    }

    #[test]
    fn shell_is_not() {
        let grid = Grid::from_lines(24, &["$ ls", "a b", "$ ", "", "", ""]);
        assert_eq!(AgentTui.detect(&pane(&grid)), 0.0);
    }
}
