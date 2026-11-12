// SPDX-License-Identifier: GPL-3.0-or-later
//! ranger 1.9.4, 80x16, captured with `ptyrc text --ansi`.

use ptyrc_core::plugin::{PaneView, TuiSupport};
use ptyrc_tui_ranger::Ranger;
use ptyrc_vt::{Grid, Rect, Vt};

const COLS: u16 = 80;
const ROWS: u16 = 16;

fn screen(ansi: &str) -> Grid {
    let mut vt = Vt::new(COLS, ROWS);
    let rows: Vec<&str> = ansi.lines().collect();
    assert_eq!(rows.len(), ROWS as usize);
    vt.feed(rows.join("\r\n").as_bytes());
    vt.grid().clone()
}

fn pane(grid: &Grid, rect: Rect) -> PaneView<'_> {
    PaneView {
        grid,
        rect,
        cursor: None,
        title: "",
        alt_screen: true,
    }
}

fn whole(grid: &Grid) -> PaneView<'_> {
    pane(grid, grid.rect())
}

/// Rectangles of the panes (three rows or more), left to right.
fn panes(grid: &Grid) -> Vec<Rect> {
    let mut rects: Vec<Rect> = Ranger
        .segment(&whole(grid))
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.h >= 3)
        .collect();
    rects.sort_by_key(|r| r.x);
    rects
}

const THREE: [Rect; 3] = [
    Rect {
        x: 0,
        y: 1,
        w: 9,
        h: 14,
    },
    Rect {
        x: 10,
        y: 1,
        w: 29,
        h: 14,
    },
    Rect {
        x: 40,
        y: 1,
        w: 40,
        h: 14,
    },
];

#[test]
fn three_columns() {
    let g = screen(include_str!("screens/home.ans"));
    assert!(Ranger.detect(&whole(&g)) >= 0.5);
    assert_eq!(panes(&g), THREE);
    let rects = Ranger.segment(&whole(&g)).unwrap();
    assert!(rects.contains(&Rect::new(0, 0, 80, 1)));
    assert!(rects.contains(&Rect::new(0, 15, 80, 1)));
    assert_eq!(Ranger.frame_key(&whole(&g)), None);

    let main = pane(&g, THREE[1]);
    assert!(Ranger.detect(&main) >= 0.5);
    assert_eq!(Ranger.frame_key(&main).unwrap(), "ranger:/srv/demo/docs");
    assert_eq!(Ranger.view_hint(&main).unwrap().row, Some(0));
    let parent = pane(&g, THREE[0]);
    assert!(Ranger.detect(&parent) >= 0.5);
    assert_eq!(Ranger.frame_key(&parent), None);
    assert_eq!(Ranger.view_hint(&parent), None);
    assert_eq!(Ranger.frame_key(&pane(&g, THREE[2])), None);
    // Any other rectangle is not ranger's.
    assert_eq!(Ranger.detect(&pane(&g, Rect::new(0, 1, 40, 14))), 0.0);
}

/// The preview of a file has no selected entry: it takes what is left.
#[test]
fn file_preview() {
    let g = screen(include_str!("screens/file.ans"));
    assert_eq!(panes(&g), THREE);
    assert_eq!(Ranger.view_hint(&pane(&g, THREE[1])).unwrap().row, Some(0));
}

#[test]
fn scrolled() {
    let g = screen(include_str!("screens/scrolled.ans"));
    assert_eq!(panes(&g), THREE);
    // Entry 40 of 42 on the twelfth row of the column.
    assert_eq!(Ranger.view_hint(&pane(&g, THREE[1])).unwrap().row, Some(28));
}

/// At the root the parent column is empty.
#[test]
fn root() {
    let g = screen(include_str!("screens/root.ans"));
    assert_eq!(panes(&g), THREE);
    assert_eq!(Ranger.frame_key(&pane(&g, THREE[1])).unwrap(), "ranger:/");
}

/// The console hides the status row: same columns, no hint.
#[test]
fn console() {
    let g = screen(include_str!("screens/console.ans"));
    assert!(Ranger.detect(&whole(&g)) >= 0.5);
    assert_eq!(panes(&g), THREE);
    assert_eq!(Ranger.view_hint(&pane(&g, THREE[1])), None);
}

/// An empty directory: nothing selected in it, nothing previewed.
#[test]
fn empty_directory() {
    let g = screen(include_str!("screens/empty.ans"));
    assert!(Ranger.detect(&whole(&g)) >= 0.5);
    let panes = panes(&g);
    assert_eq!(panes, [THREE[0], Rect::new(10, 1, 70, 14)]);
    assert_eq!(
        Ranger.frame_key(&pane(&g, panes[1])).unwrap(),
        "ranger:/srv/demo/empty"
    );
}

/// ranger in a pane of another app: known, not split.
#[test]
fn nested() {
    let inner = screen(include_str!("screens/home.ans"));
    let mut g = Grid::new(COLS + 10, ROWS + 2);
    for y in 0..ROWS {
        for x in 0..COLS {
            g.put(x + 10, y, inner.cell(x, y).clone());
        }
    }
    assert_eq!(Ranger.detect(&whole(&g)), 0.0);
    let p = pane(&g, Rect::new(10, 0, COLS, ROWS));
    assert!(Ranger.detect(&p) >= 0.5);
    assert_eq!(Ranger.chrome(&p).unwrap().top, 1);
    assert_eq!(Ranger.frame_key(&p).unwrap(), "ranger:/srv/demo/docs");
}

#[test]
fn not_ranger() {
    let g = Grid::from_lines(
        40,
        &[
            "user@host ~ $ ls -l",
            "drwxr-xr-x 2 user user 1 docs",
            "",
            "$ ",
        ],
    );
    assert_eq!(Ranger.detect(&whole(&g)), 0.0);
}
