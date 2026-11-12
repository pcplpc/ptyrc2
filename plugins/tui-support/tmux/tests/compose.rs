// SPDX-License-Identifier: GPL-3.0-or-later
//! Presets compose: vim splits inside a tmux window.

use ptyrc_core::frames::{FrameConfig, FrameInfo, Frames, ScreenInput};
use ptyrc_core::types::{FrameKind, FrameState};
use ptyrc_tui_tmux::Tmux;
use ptyrc_tui_vim::Vim;
use ptyrc_vt::{Attrs, Cursor, Grid};
use std::time::Duration;

const COLS: u16 = 80;
const ROWS: u16 = 16;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Cells shown in inverse video: a status line, a split bar.
fn invert(g: &mut Grid, cols: std::ops::Range<u16>, rows: std::ops::Range<u16>) {
    for row in rows {
        for col in cols.clone() {
            g.cell_mut(col, row).style.attrs.insert(Attrs::INVERSE);
        }
    }
}

fn grid(lines: &[String]) -> Grid {
    let mut g = Grid::from_lines(COLS, &lines.iter().map(String::as_str).collect::<Vec<_>>());
    invert(&mut g, 0..COLS, ROWS - 1..ROWS);
    g
}

/// Window 0: three tmux panes.
fn window0() -> Grid {
    let mut lines = Vec::new();
    for r in 0..ROWS - 1 {
        let right = if r == 7 {
            "─".repeat(39)
        } else {
            format!("{:<39}", format!("$ echo {r}"))
        };
        lines.push(format!("{:<40}│{right}", format!("some text, row {r}")));
    }
    lines.push(format!(
        "{:<60}\"host\" 14:02 12-Nov-26",
        "[0] 0:sh* 1:vim-"
    ));
    grid(&lines)
}

/// Window 1: two vim windows side by side.
fn window1() -> Grid {
    let mut lines = Vec::new();
    for r in 0..ROWS - 3 {
        lines.push(format!(
            "{:<40}|{:<39}",
            format!("line {}: the quick brown fox", r + 1),
            format!("fn f{r}() {{}}")
        ));
    }
    lines.push(format!(
        "{:<41}{:<39}",
        "a.txt               1,1            Top", "b.rs               1,1            Top"
    ));
    lines.push("\"b.rs\" 63L, 3407B".to_string());
    lines.push(format!(
        "{:<60}\"host\" 14:02 12-Nov-26",
        "[0] 0:sh- 1:vim*"
    ));
    let mut g = grid(&lines);
    invert(&mut g, 40..41, 0..ROWS - 3);
    invert(&mut g, 0..COLS, ROWS - 3..ROWS - 2);
    g
}

fn show(f: &mut Frames, g: &Grid, now: Duration) {
    let input = ScreenInput {
        grid: g,
        cursor: Cursor {
            col: 0,
            row: 0,
            visible: true,
            pending_wrap: false,
        },
        alt_screen: true,
        title: "",
        events: &[],
    };
    f.update(&input, now);
}

fn panes(f: &Frames) -> Vec<FrameInfo> {
    f.list()
        .into_iter()
        .filter(|i| i.kind == FrameKind::Pane && i.state != FrameState::Inactive)
        .collect()
}

/// The layout of the new window is committed on a tick, after ticks that
/// came while it was pending (an animation running): the presets asked
/// when the screen changed still answer for its panes.
#[test]
fn vim_splits_after_a_window_switch() {
    let mut f = Frames::new(FrameConfig::default(), vec![Box::new(Vim), Box::new(Tmux)]);
    show(&mut f, &window0(), ms(0));
    for t in (20..3000).step_by(20) {
        f.tick(ms(t));
    }
    assert!(
        panes(&f)
            .iter()
            .all(|p| p.key.as_deref() == Some("tmux:0:0"))
    );

    show(&mut f, &window1(), ms(3000));
    for t in (3020..6000).step_by(20) {
        f.tick(ms(t));
    }
    let panes = panes(&f);
    let keys: Vec<_> = panes.iter().filter_map(|p| p.key.as_deref()).collect();
    assert_eq!(
        keys,
        ["tmux:0:1/vim:a.txt", "tmux:0:1/vim:b.rs"],
        "{panes:#?}"
    );
    assert!(panes.iter().all(|p| p.support.as_deref() == Some("vim")));
}
