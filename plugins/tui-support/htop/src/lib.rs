// SPDX-License-Identifier: GPL-3.0-or-later
//! Preset for htop.
//!
//! Recognised on its main screen from the function bar (`F1Help F2Setup`)
//! under a table whose header is a row of one coloured backdrop. Meters
//! above the header, processes below; the selected process is an entry
//! of the table, not a bar. The two panes are keyed: a refresh that
//! rewrites them whole leaves their frames in place.

use ptyrc_core::plugin::{Chrome, PaneView, TuiSupport, Tuning};
use ptyrc_vt::{Color, Grid, Rect};

pub struct Htop;

/// The main screen of htop in an area.
struct Look {
    area: Rect,
    /// Screen row of the table header.
    header: u16,
}

fn text(grid: &Grid, area: Rect, y: u16) -> String {
    (area.x..area.right())
        .map(|x| grid.cell(x, y))
        .filter(|c| c.width != 0)
        .map(|c| c.text.as_str())
        .collect()
}

/// A row of the area with no cell on the default background.
fn coloured(grid: &Grid, area: Rect, y: u16) -> bool {
    (area.x..area.right()).all(|x| grid.cell(x, y).backdrop() != Color::Default)
}

impl Look {
    fn of(grid: &Grid, area: Rect) -> Option<Look> {
        if area.h < 5 || area.w < 20 || area.bottom() > grid.rows() || area.right() > grid.cols() {
            return None;
        }
        let bar = text(grid, area, area.bottom() - 1);
        let rest = bar.strip_prefix("F1Help")?;
        if !rest.trim_start().starts_with("F2Setup") {
            return None;
        }
        // The header is the first such row: the selection is below it.
        let header = (area.y..area.bottom() - 2).find(|y| coloured(grid, area, *y))?;
        Some(Look { area, header })
    }

    fn meters(&self) -> Rect {
        Rect::new(
            self.area.x,
            self.area.y,
            self.area.w,
            self.header - self.area.y,
        )
    }

    fn table(&self) -> Rect {
        let y = self.header + 1;
        Rect::new(self.area.x, y, self.area.w, self.area.bottom() - 1 - y)
    }
}

enum Part {
    Whole,
    Meters,
    Table,
}

fn part(pane: &PaneView) -> Option<(Look, Part)> {
    if let Some(look) = Look::of(pane.grid, pane.rect) {
        return Some((look, Part::Whole));
    }
    let look = Look::of(pane.grid, pane.grid.rect())?;
    let part = if pane.rect == look.table() {
        Part::Table
    } else if pane.rect == look.meters() {
        Part::Meters
    } else {
        return None;
    };
    Some((look, part))
}

impl TuiSupport for Htop {
    fn name(&self) -> &'static str {
        "htop"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if part(pane).is_some() { 0.9 } else { 0.0 }
    }

    /// Not split (inside another app's pane): all but the table. A pane
    /// of the split has none: no row of the table is learned as chrome.
    fn chrome(&self, pane: &PaneView) -> Option<Chrome> {
        let (look, part) = part(pane)?;
        Some(match part {
            Part::Whole => Chrome {
                top: look.header + 1 - look.area.y,
                bottom: 1,
                ..Chrome::default()
            },
            _ => Chrome::default(),
        })
    }

    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        match part(pane)?.1 {
            Part::Whole if pane.rect == pane.grid.rect() => None,
            Part::Whole | Part::Table => Some("htop:processes".into()),
            Part::Meters => Some("htop:meters".into()),
        }
    }

    /// Meters, header, table, function bar.
    /// Meters and table are repainted in place.
    fn tuning(&self) -> Tuning {
        Tuning {
            monitor: true,
            ..Tuning::default()
        }
    }

    fn segment(&self, screen: &PaneView) -> Option<Vec<Rect>> {
        let look = Look::of(screen.grid, screen.rect)?;
        let a = look.area;
        Some(
            [
                look.meters(),
                Rect::new(a.x, look.header, a.w, 1),
                look.table(),
                Rect::new(a.x, a.bottom() - 1, a.w, 1),
            ]
            .into_iter()
            .filter(|r| !r.is_empty())
            .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_vt::Vt;

    const COLS: u16 = 60;

    /// htop 3.4, reduced: meters, tabs, header, selection, function bar.
    fn screen(bar: &str) -> Grid {
        let rows = [
            "",
            "    0[|||          3.8%]   2[||           3.2%]",
            "    1[|            0.6%]   3[|            0.6%]",
            "  Mem[||||||||10.7G/14.4G] Tasks: 40, 45 thr; 2 running",
            "",
            "  \x1b[32;42m[\x1b[30;42mMain\x1b[32;42m]\x1b[0m \x1b[34;44m[\x1b[30;44mI/O\x1b[34;44m]\x1b[0m",
            "\x1b[30;42m    PID USER       PRI  NI S \x1b[30;46m CPU%\x1b[30;42m MEM%   TIME+  Command\x1b[K\x1b[0m",
            "\x1b[30;46m    101 user        20   0 S  13.3  0.1  7h25:55 htop\x1b[K\x1b[0m",
            "    102 user        20   0 R   5.7  0.1  0:00.32 sh",
            "    103 user        20   0 S   4.4  0.1  0:00.38 vim",
            "    104 root        20   0 S   0.6  0.1  0:00.03 init",
            bar,
        ];
        let mut vt = Vt::new(COLS, rows.len() as u16);
        vt.feed(rows.join("\r\n").as_bytes());
        vt.grid().clone()
    }

    const BAR: &str = "F1\x1b[30;46mHelp  \x1b[0mF2\x1b[30;46mSetup \x1b[0mF3\x1b[30;46mSearch\x1b[0mF10\x1b[30;46mQuit\x1b[K\x1b[0m";

    fn pane(grid: &Grid, rect: Rect) -> PaneView<'_> {
        PaneView {
            grid,
            rect,
            cursor: None,
            title: "",
            alt_screen: true,
        }
    }

    #[test]
    fn main_screen() {
        let g = screen(BAR);
        let whole = pane(&g, g.rect());
        assert!(Htop.detect(&whole) >= 0.5);
        let (meters, table) = (Rect::new(0, 0, COLS, 6), Rect::new(0, 7, COLS, 4));
        assert_eq!(
            Htop.segment(&whole).unwrap(),
            [
                meters,
                Rect::new(0, 6, COLS, 1),
                table,
                Rect::new(0, 11, COLS, 1)
            ]
        );
        assert_eq!(Htop.frame_key(&whole), None);
        assert_eq!(Htop.frame_key(&pane(&g, meters)).unwrap(), "htop:meters");
        assert_eq!(Htop.frame_key(&pane(&g, table)).unwrap(), "htop:processes");
        assert_eq!(Htop.chrome(&pane(&g, table)), Some(Chrome::default()));
        assert_eq!(Htop.detect(&pane(&g, Rect::new(0, 8, COLS, 3))), 0.0);
    }

    /// A menu (sort, setup) has another function bar: left to the core.
    #[test]
    fn other_screens() {
        let g = screen("Enter\x1b[30;46mSort  \x1b[0mEsc\x1b[30;46mCancel\x1b[K\x1b[0m");
        assert_eq!(Htop.detect(&pane(&g, g.rect())), 0.0);
        assert_eq!(Htop.segment(&pane(&g, g.rect())), None);
    }

    /// In a pane of another app: known, not split, the table scrolls.
    #[test]
    fn nested() {
        let inner = screen(BAR);
        let mut g = Grid::new(COLS, inner.rows() + 1);
        for y in 0..inner.rows() {
            for x in 0..COLS {
                g.put(x, y, inner.cell(x, y).clone());
            }
        }
        assert_eq!(Htop.detect(&pane(&g, g.rect())), 0.0);
        let p = pane(&g, Rect::new(0, 0, COLS, inner.rows()));
        assert!(Htop.detect(&p) >= 0.5);
        assert_eq!(
            Htop.chrome(&p).unwrap(),
            Chrome {
                top: 7,
                bottom: 1,
                left: 0,
                right: 0
            }
        );
        assert_eq!(Htop.frame_key(&p).unwrap(), "htop:processes");
    }
}
