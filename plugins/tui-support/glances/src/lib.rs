// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset for glances.
//!
//! Recognised from a full-width rule (`─`) over the row that counts the
//! tasks (`TASKS 666 (5215 thr), 2 run`). The summary lies between the
//! rules; under them the plugins fill a sidebar, left of the column where
//! `TASKS` starts, and the processes the rest. No border between the two.
//! Every part is repainted in place at each refresh.

use ptyrc_core::plugin::{Chrome, PaneView, TuiSupport, Tuning};
use ptyrc_vt::{Grid, Rect};

pub struct Glances;

/// Narrowest sidebar kept as a part.
const MIN_SIDE: u16 = 8;

/// Glances in an area.
struct Look {
    area: Rect,
    /// Screen rows of the rules above the tasks, top to bottom.
    rules: Vec<u16>,
    /// Screen row and column of `TASKS`.
    tasks: (u16, u16),
}

fn rule(grid: &Grid, area: Rect, y: u16) -> bool {
    (area.x..area.right()).all(|x| grid.cell(x, y).text == "─")
}

/// Column where `TASKS <n> (<n> thr)` starts on a row.
fn tasks(grid: &Grid, area: Rect, y: u16) -> Option<u16> {
    let cells: Vec<(u16, &str)> = (area.x..area.right())
        .map(|x| (x, grid.cell(x, y)))
        .filter(|(_, c)| c.width != 0)
        .map(|(x, c)| (x, c.text.as_str()))
        .collect();
    let row: String = cells.iter().map(|c| c.1).collect();
    let at = row.find("TASKS ")?;
    let rest = &row[at + 6..];
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || !rest[digits..].starts_with(" (") || !rest.contains(" thr)") {
        return None;
    }
    // Cells hold one glyph each: the byte offset is counted in cells.
    let mut bytes = 0;
    cells.iter().find_map(|(x, t)| {
        let here = bytes == at;
        bytes += t.len().max(1);
        here.then_some(*x)
    })
}

impl Look {
    fn of(grid: &Grid, area: Rect) -> Option<Look> {
        if area.h < 6 || area.w < 40 || area.bottom() > grid.rows() || area.right() > grid.cols() {
            return None;
        }
        let mut rules = Vec::new();
        for y in area.y..area.bottom() {
            if rule(grid, area, y) {
                rules.push(y);
            } else if let Some(x) = tasks(grid, area, y) {
                return (!rules.is_empty()).then_some(Look {
                    area,
                    rules,
                    tasks: (y, x),
                });
            }
        }
        None
    }

    /// Rows between the last two rules: the summary.
    fn summary(&self) -> Option<Rect> {
        let [.., a, b] = self.rules[..] else {
            return None;
        };
        (b > a + 1).then(|| Rect::new(self.area.x, a + 1, self.area.w, b - a - 1))
    }

    /// First row under the last rule.
    fn top(&self) -> u16 {
        self.rules.last().unwrap() + 1
    }

    fn sidebar(&self) -> Option<Rect> {
        let a = self.area;
        let w = self.tasks.1.checked_sub(a.x + 1)?;
        (w >= MIN_SIDE).then(|| Rect::new(a.x, self.top(), w, a.bottom() - self.top()))
    }

    fn processes(&self) -> Rect {
        let a = self.area;
        let x = if self.sidebar().is_some() {
            self.tasks.1
        } else {
            a.x
        };
        Rect::new(x, self.top(), a.right() - x, a.bottom() - self.top())
    }
}

enum Part {
    Whole,
    Summary,
    Sidebar,
    Processes,
}

fn part(pane: &PaneView) -> Option<(Look, Part)> {
    if let Some(look) = Look::of(pane.grid, pane.rect) {
        return Some((look, Part::Whole));
    }
    let look = Look::of(pane.grid, pane.area)?;
    let part = if pane.rect == look.processes() {
        Part::Processes
    } else if Some(pane.rect) == look.sidebar() {
        Part::Sidebar
    } else if Some(pane.rect) == look.summary() {
        Part::Summary
    } else {
        return None;
    };
    Some((look, part))
}

impl TuiSupport for Glances {
    fn name(&self) -> &'static str {
        "glances"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if part(pane).is_some() { 0.9 } else { 0.0 }
    }

    /// Not split: all above the last rule.
    fn chrome(&self, pane: &PaneView) -> Option<Chrome> {
        let (look, part) = part(pane)?;
        Some(match part {
            Part::Whole => Chrome {
                top: look.top() - look.area.y,
                ..Chrome::default()
            },
            _ => Chrome::default(),
        })
    }

    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        Some(
            match part(pane)?.1 {
                Part::Whole => return None,
                Part::Summary => "glances:summary",
                Part::Sidebar => "glances:sidebar",
                Part::Processes => "glances:processes",
            }
            .into(),
        )
    }

    fn tuning(&self, pane: &PaneView) -> Tuning {
        Tuning {
            monitor: part(pane).is_some(),
            ..Tuning::default()
        }
    }

    /// Title, rules, summary, sidebar, processes.
    fn segment(&self, area: &PaneView) -> Option<Vec<Rect>> {
        let look = Look::of(area.grid, area.rect)?;
        let a = look.area;
        let mut rects = Vec::new();
        if look.rules[0] > a.y {
            rects.push(Rect::new(a.x, a.y, a.w, look.rules[0] - a.y));
        }
        rects.extend(look.rules.iter().map(|y| Rect::new(a.x, *y, a.w, 1)));
        rects.extend(look.summary());
        rects.extend(look.sidebar());
        rects.push(look.processes());
        Some(rects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLS: u16 = 60;

    /// glances 4.5, reduced.
    fn screen() -> Grid {
        let rule = "─".repeat(COLS as usize);
        Grid::from_lines(
            COLS,
            &[
                "host Arch Linux 64bit                    Uptime: 4 days",
                &rule,
                "CPU  [      3.9%]   user      1.8%   MEM     75.1%",
                "MEM  [▪▪▪  75.1%]   system    1.7%   total   14.4G",
                &rule,
                "NETWORK   Rx/s   Tx/s   TASKS 666 (5215 thr), 2 run, 516 slp",
                "wlan0    118Kb  190Kb",
                "                        CPU%   MEM%     PID USER   Command",
                "DISK I/O   R/s    W/s   >0.0   5.6  1667733 user   cargo",
                "dm-0      338K  30.0M    0.9   5.0    39753 user   vim",
                "2026-11-12 21:28:27     No warning or critical alert",
            ],
        )
    }

    fn pane(grid: &Grid, rect: Rect) -> PaneView<'_> {
        PaneView {
            grid,
            rect,
            area: grid.rect(),
            cursor: None,
            title: "",
            alt_screen: true,
        }
    }

    #[test]
    fn main_screen() {
        let g = screen();
        let whole = pane(&g, g.rect());
        assert!(Glances.detect(&whole) >= 0.5);
        let summary = Rect::new(0, 2, COLS, 2);
        let sidebar = Rect::new(0, 5, 23, 6);
        let processes = Rect::new(24, 5, 36, 6);
        assert_eq!(
            Glances.segment(&whole).unwrap(),
            [
                Rect::new(0, 0, COLS, 1),
                Rect::new(0, 1, COLS, 1),
                Rect::new(0, 4, COLS, 1),
                summary,
                sidebar,
                processes,
            ]
        );
        assert_eq!(Glances.frame_key(&whole), None);
        for (rect, key) in [
            (summary, "glances:summary"),
            (sidebar, "glances:sidebar"),
            (processes, "glances:processes"),
        ] {
            let p = pane(&g, rect);
            assert_eq!(Glances.frame_key(&p).unwrap(), key);
            assert!(Glances.tuning(&p).monitor);
        }
        assert_eq!(Glances.detect(&pane(&g, Rect::new(0, 6, COLS, 5))), 0.0);
    }

    #[test]
    fn not_glances() {
        let rule = "─".repeat(COLS as usize);
        let g = Grid::from_lines(
            COLS,
            &["title", &rule, "a", "b", "TASKS to do today", "c", "d"],
        );
        assert_eq!(Glances.detect(&pane(&g, g.rect())), 0.0);
        // No rule above the tasks.
        let g = Grid::from_lines(
            COLS,
            &["a", "b", "c", "TASKS 12 (40 thr), 1 run", "d", "e", "f"],
        );
        assert_eq!(Glances.detect(&pane(&g, g.rect())), 0.0);
    }

    /// Without a sidebar (`glances -2`): the processes take the width.
    #[test]
    fn no_sidebar() {
        let rule = "─".repeat(COLS as usize);
        let g = Grid::from_lines(
            COLS,
            &[
                &rule,
                "CPU  [      3.9%]",
                &rule,
                "TASKS 666 (5215 thr), 2 run",
                "",
                "CPU%   MEM%     PID USER   Command",
                ">0.0   5.6  1667733 user   cargo",
            ],
        );
        let whole = pane(&g, g.rect());
        assert_eq!(
            Glances.segment(&whole).unwrap(),
            [
                Rect::new(0, 0, COLS, 1),
                Rect::new(0, 2, COLS, 1),
                Rect::new(0, 1, COLS, 1),
                Rect::new(0, 3, COLS, 4),
            ]
        );
    }
}
