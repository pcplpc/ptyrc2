// SPDX-License-Identifier: GPL-3.0-or-later
//! Sessions simulated as sequences of screens.

use super::*;
use crate::plugin::{Chrome, ViewHint};
use ptyrc_vt::{Attrs, Cell, Color, Style};

const COLS: u16 = 60;

struct Sim {
    f: Frames,
    now: Duration,
    cursor: (u16, u16),
    alt: bool,
    title: String,
}

impl Sim {
    fn new() -> Sim {
        Sim::with(FrameConfig::default(), Vec::new())
    }

    fn with(cfg: FrameConfig, supports: Vec<Box<dyn TuiSupport>>) -> Sim {
        Sim {
            f: Frames::new(cfg, supports),
            now: Duration::ZERO,
            cursor: (0, 0),
            alt: false,
            title: String::new(),
        }
    }

    /// Advance `dt` ms, then show `g`.
    fn feed(&mut self, g: &Grid, dt: u64) -> Vec<FrameEvent> {
        self.feed_ev(g, dt, &[])
    }

    fn feed_ev(&mut self, g: &Grid, dt: u64, events: &[VtEvent]) -> Vec<FrameEvent> {
        let mut ev = self.idle(dt);
        let cursor = Cursor {
            col: self.cursor.0,
            row: self.cursor.1,
            visible: true,
            pending_wrap: false,
        };
        let input = ScreenInput {
            grid: g,
            cursor,
            alt_screen: self.alt,
            title: &self.title,
            events,
        };
        ev.extend(self.f.update(&input, self.now));
        ev
    }

    /// Let `dt` ms pass, ticking when the frames ask.
    fn idle(&mut self, dt: u64) -> Vec<FrameEvent> {
        let end = self.now + Duration::from_millis(dt);
        let mut ev = Vec::new();
        while let Some(w) = self.f.next_wake() {
            if w > end {
                break;
            }
            let at = w.max(self.now);
            let before = self.f.next_wake();
            ev.extend(self.f.tick(at));
            self.now = at;
            if self.f.next_wake() == before {
                break;
            }
        }
        self.now = end;
        ev.extend(self.f.tick(end));
        ev
    }

    /// Show `g` and wait until everything is stable.
    fn settle(&mut self, g: &Grid) {
        self.feed(g, 10);
        self.idle(2000);
    }

    fn on_screen(&self) -> Vec<FrameInfo> {
        self.f
            .list()
            .into_iter()
            .filter(|i| i.state != FrameState::Inactive && i.id != RAW)
            .collect()
    }

    fn panes(&self) -> Vec<FrameInfo> {
        self.on_screen()
            .into_iter()
            .filter(|i| i.kind == FrameKind::Pane)
            .collect()
    }

    fn pane(&self) -> FrameInfo {
        let p = self.panes();
        assert_eq!(p.len(), 1, "one pane expected: {p:#?}");
        p.into_iter().next().unwrap()
    }

    fn rects(&self) -> Vec<[u16; 4]> {
        self.panes().iter().map(|p| p.rect).collect()
    }
}

fn doc(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("line {i:04} {}", "abcdefghij".repeat(1 + i % 3)))
        .collect()
}

/// `rows` lines of `doc` from `top`, then `tail` lines.
fn view(doc: &[String], top: usize, rows: usize, tail: &[&str]) -> Grid {
    let mut lines: Vec<&str> = doc
        .iter()
        .skip(top)
        .take(rows)
        .map(String::as_str)
        .collect();
    lines.resize(rows, "");
    lines.extend_from_slice(tail);
    Grid::from_lines(COLS, &lines)
}

fn paint(g: &mut Grid, r: Rect, f: impl Fn(&mut Style)) {
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            f(&mut g.cell_mut(x, y).style);
        }
    }
}

fn inverse(g: &mut Grid, r: Rect) {
    paint(g, r, |s| s.attrs.insert(Attrs::INVERSE));
}

fn bg(g: &mut Grid, r: Rect, c: u8) {
    paint(g, r, |s| s.bg = Color::Indexed(c));
}

fn put(g: &mut Grid, x: u16, y: u16, text: &str) {
    for (i, ch) in text.chars().enumerate() {
        let style = g.cell(x + i as u16, y).style;
        g.put(x + i as u16, y, Cell::narrow(&ch.to_string(), style));
    }
}

fn lit(p: &str) -> TextQuery {
    TextQuery {
        pattern: p.to_string(),
        regex: false,
        ignore_case: false,
    }
}

// -- raw frame ---------------------------------------------------------

#[test]
fn raw_frame_text_find_cursor() {
    let mut s = Sim::new();
    let g = Grid::from_lines(COLS, &["hello world", "a flower here", "Flower again"]);
    s.cursor = (3, 1);
    s.feed(&g, 0);
    assert_eq!(
        s.f.text(RAW, 0, 4),
        vec!["hello world", "a flower here", "Flower again", ""]
    );
    let m = s.f.find(RAW, &lit("flower"), false);
    assert_eq!(m.len(), 1);
    assert_eq!(
        (m[0].x, m[0].y, m[0].len, m[0].screen),
        (2, 1, 6, Some((2, 1)))
    );
    let q = TextQuery {
        pattern: "f.ower".into(),
        regex: true,
        ignore_case: true,
    };
    assert_eq!(s.f.find(RAW, &q, false).len(), 2);
    assert!(
        s.f.try_find(
            RAW,
            &TextQuery {
                pattern: "(".into(),
                regex: true,
                ignore_case: false
            },
            false
        )
        .is_err()
    );
    assert_eq!(s.f.cursor(), (3, 1));
    s.cursor = (5, 2);
    s.feed(&g, 10);
    let t = s.f.cursor_trail();
    assert_eq!(t.len(), 2);
    assert_eq!(
        (t[1].col, t[1].row, t[1].at),
        (5, 2, Duration::from_millis(10))
    );
    // Raw is usable 250 ms after the first screen (a resize).
    assert!(!s.f.info(RAW).unwrap().usable);
    s.idle(300);
    assert!(s.f.info(RAW).unwrap().usable);
    s.alt = true;
    s.feed(&g, 10);
    assert!(!s.f.info(RAW).unwrap().usable);
    s.idle(300);
    assert!(s.f.info(RAW).unwrap().usable);
}

#[test]
fn wide_glyph_columns() {
    let mut s = Sim::new();
    let mut g = Grid::new(COLS, 3);
    g.put(0, 0, Cell::wide("世", Style::default()));
    g.put(2, 0, Cell::wide("界", Style::default()));
    put(&mut g, 4, 0, " flower");
    s.feed(&g, 0);
    let m = s.f.find(RAW, &lit("flower"), false);
    assert_eq!((m[0].x, m[0].len), (5, 6));
    let m = s.f.find(RAW, &lit("界"), false);
    assert_eq!((m[0].x, m[0].len), (2, 2));
}

// -- extended frame ----------------------------------------------------

#[test]
fn scroll_by_one_and_back() {
    let d = doc(200);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    assert_eq!(s.pane().origin, [0, 0]);
    for top in 1..=30 {
        let ev = s.feed(&view(&d, top, 20, &[]), 30);
        assert!(
            ev.contains(&FrameEvent::Scroll {
                frame: id,
                dx: 0,
                dy: 1
            }),
            "{top}: {ev:?}"
        );
        assert_eq!(s.pane().origin, [0, top as i64]);
    }
    for top in (0..30).rev() {
        s.feed(&view(&d, top, 20, &[]), 30);
        assert_eq!(s.pane().origin, [0, top as i64]);
    }
    assert_eq!(s.pane().id, id);
    assert!(s.pane().usable);
    s.idle(500);
    // Everything seen is remembered.
    let text = s.f.text(id, 0, 50);
    assert_eq!(text, d[..50].to_vec());
    assert_eq!(s.pane().extent, [0, 49]);
}

#[test]
fn half_pages_and_jumps_return_to_the_same_rows() {
    let d = doc(400);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    for top in [10, 20, 30, 40, 30, 20] {
        s.feed(&view(&d, top, 20, &[]), 200);
        assert_eq!(s.pane().origin[1], top as i64);
    }
    s.idle(300);
    // A jump to rows never seen: an island far away.
    let ev = s.feed(&view(&d, 300, 20, &[]), 200);
    assert!(ev.contains(&FrameEvent::Jump { frame: id }), "{ev:?}");
    let far = s.pane().origin[1];
    assert!(far >= ext::ISLAND_GAP);
    s.idle(300);
    // And back: the rows are recognised.
    let ev = s.feed(&view(&d, 25, 20, &[]), 200);
    assert!(ev.contains(&FrameEvent::Jump { frame: id }), "{ev:?}");
    assert_eq!(s.pane().origin[1], 25);
    // Forth again: the island too.
    s.feed(&view(&d, 300, 20, &[]), 200);
    assert_eq!(s.pane().origin[1], far);
    assert_eq!(s.pane().id, id);
    assert!(s.pane().usable, "a jump keeps the frame usable");
}

#[test]
fn frame_positions_follow_the_text() {
    let d = doc(100);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    let m = s.f.find(id, &lit("line 0012"), false);
    let pos = FramePos {
        x: m[0].x,
        y: m[0].y,
    };
    assert_eq!(s.f.info(id).unwrap().to_screen(pos), Some((0, 12)));
    s.feed(&view(&d, 5, 20, &[]), 50);
    assert_eq!(s.f.info(id).unwrap().to_screen(pos), Some((0, 7)));
    s.feed(&view(&d, 13, 20, &[]), 50);
    assert_eq!(s.f.info(id).unwrap().to_screen(pos), None);
    // Remembered, not shown.
    s.idle(300);
    let m = s.f.find(id, &lit("line 0012"), true);
    assert_eq!((m.len(), m[0].y, m[0].screen), (1, 12, None));
    assert!(s.f.find(id, &lit("line 0012"), false).is_empty());
    assert_eq!(s.f.at(3, 2), Some((id, FramePos { x: 3, y: 15 })));
}

#[test]
fn status_bar_stays_and_is_learned_as_chrome() {
    let d = doc(200);
    let mut s = Sim::new();
    // A ruler that changes with the position: not a bar, plain text.
    let ruler = |top: usize| format!("{top},1   {}%", top / 2);
    s.settle(&view(&d, 0, 20, &["", &ruler(0)]));
    let id = s.pane().id;
    assert_eq!(s.pane().rect, [0, 0, COLS, 22]);
    for top in 1..=6 {
        s.feed(&view(&d, top, 20, &["", &ruler(top)]), 50);
        assert_eq!(s.pane().origin[1], top as i64, "top {top}");
    }
    assert_eq!(s.pane().rect, [0, 0, COLS, 21], "the ruler row is chrome");
    for top in (0..6).rev() {
        s.feed(&view(&d, top, 20, &["", &ruler(top)]), 50);
        assert_eq!(s.pane().origin[1], top as i64);
    }
    s.idle(300);
    // The ruler never entered the remembered text.
    assert!(s.f.find(id, &lit("%"), true).is_empty());
    assert_eq!(s.f.text(id, 0, 3), d[..3].to_vec());
}

#[test]
fn top_chrome_shifts_origin_not_content() {
    let d = doc(200);
    let mut s = Sim::new();
    let screen = |top: usize| {
        let mut lines: Vec<&str> = vec!["File  Edit  View"];
        lines.extend(d.iter().skip(top).take(15).map(String::as_str));
        Grid::from_lines(COLS, &lines)
    };
    s.settle(&screen(0));
    let id = s.pane().id;
    let pos = {
        let m = s.f.find(id, &lit("line 0007"), false);
        FramePos {
            x: m[0].x,
            y: m[0].y,
        }
    };
    for top in 1..=5 {
        s.feed(&screen(top), 50);
    }
    assert_eq!(s.pane().rect, [0, 1, COLS, 15]);
    assert_eq!(s.f.info(id).unwrap().to_screen(pos), Some((0, 3)));
    let m = s.f.find(id, &lit("line 0007"), false);
    assert_eq!((m[0].x, m[0].y), (pos.x, pos.y));
}

#[test]
fn horizontal_scroll() {
    let pat = "0123456789abcdefghijklmnopqrstuvwxyz".repeat(6);
    let d: Vec<String> = (0..20)
        .map(|i| format!("{}: {}", (b'a' + i as u8) as char, &pat[i * 3..i * 3 + 140]))
        .collect();
    let mut s = Sim::new();
    let screen = |dx: usize| {
        let lines: Vec<String> = d.iter().map(|l| l.chars().skip(dx).collect()).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        Grid::from_lines(COLS, &refs)
    };
    s.settle(&screen(0));
    let id = s.pane().id;
    let ev = s.feed(&screen(7), 300);
    assert!(
        ev.contains(&FrameEvent::Scroll {
            frame: id,
            dx: 7,
            dy: 0
        }),
        "{ev:?}"
    );
    assert_eq!(s.pane().origin, [7, 0]);
    s.feed(&screen(20), 300);
    assert_eq!(s.pane().origin, [20, 0]);
    s.feed(&screen(3), 300);
    assert_eq!(s.pane().origin, [3, 0]);
    s.idle(300);
    // Both ends of the lines are remembered, at their columns.
    let text = s.f.text(id, 0, 1);
    assert_eq!(text[0], d[0][..80]);
    let m = s.f.find(id, &lit("a: 0123"), true);
    assert_eq!((m[0].x, m[0].screen), (0, None));
}

#[test]
fn splice_insert_and_delete() {
    let d = doc(100);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    // Scroll a little so rows below the view are known.
    s.feed(&view(&d, 5, 20, &[]), 300);
    s.feed(&view(&d, 0, 20, &[]), 300);
    s.idle(300);
    // Two lines inserted after row 7, cursor there.
    let mut e = d.clone();
    e.insert(8, "NEW one".into());
    e.insert(9, "NEW two".into());
    s.cursor = (0, 9);
    let ev = s.feed(&view(&e, 0, 20, &[]), 100);
    assert!(
        ev.contains(&FrameEvent::Shift {
            frame: id,
            from: 8,
            to: i64::MAX - ext::ISLAND_GAP,
            dy: 2
        }),
        "{ev:?}"
    );
    assert_eq!(s.pane().origin, [0, 0]);
    s.idle(300);
    assert_eq!(
        s.f.text(id, 0, 27),
        e[..27].to_vec(),
        "rows below the view moved too"
    );
    // And deleted again.
    s.cursor = (0, 8);
    let ev = s.feed(&view(&d, 0, 20, &[]), 100);
    assert!(
        ev.iter()
            .any(|x| matches!(x, FrameEvent::Shift { dy: -2, .. })),
        "{ev:?}"
    );
    s.idle(300);
    assert_eq!(s.f.text(id, 0, 25), d[..25].to_vec());
    // An edit in place is no splice.
    let mut e = d.clone();
    e[3] = "changed".into();
    e[4] = "changed too".into();
    let ev = s.feed(&view(&e, 0, 20, &[]), 100);
    assert!(ev.is_empty(), "{ev:?}");
    s.idle(300);
    assert_eq!(s.f.text(id, 3, 5), vec!["changed", "changed too"]);
}

#[test]
fn diagonal_scroll() {
    // Down half a page while the app re-centres sideways.
    let mut seed = 7u64;
    let d: Vec<String> = (0..80)
        .map(|i| {
            let text: String = (0..200)
                .map(|_| {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    (b'a' + ((seed >> 33) % 26) as u8) as char
                })
                .collect();
            format!("{i:02}: {text}")
        })
        .collect();
    let mut s = Sim::new();
    let screen = |top: usize, dx: usize| {
        let lines: Vec<String> = d[top..top + 20]
            .iter()
            .map(|l| l.chars().skip(dx).collect())
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        Grid::from_lines(COLS, &refs)
    };
    s.settle(&screen(0, 40));
    let id = s.pane().id;
    s.feed(&screen(0, 60), 300);
    assert_eq!(s.pane().origin, [20, 0]);
    let ev = s.feed(&screen(10, 35), 300);
    assert!(
        ev.contains(&FrameEvent::Scroll {
            frame: id,
            dx: -25,
            dy: 10
        }),
        "{ev:?}"
    );
    assert_eq!(s.pane().origin, [-5, 10]);
    s.feed(&screen(4, 40), 300);
    assert_eq!(s.pane().origin, [0, 4]);
}

#[test]
fn number_column_under_a_plugin() {
    // The plugin gives top and bottom chrome; the numbers are still left
    // chrome, and toggling them is not a horizontal scroll.
    let d = doc(300);
    let mut s = Sim::with(FrameConfig::default(), vec![Box::new(Fake)]);
    let plain = |top: usize| view(&d[top..], 0, 20, &["FAKE a.txt x"]);
    let nu = |top: usize| {
        let mut lines: Vec<String> = (top..top + 20)
            .map(|i| format!("{:>3} {}", i + 1, d[i]))
            .collect();
        lines.push("FAKE a.txt x".into());
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        Grid::from_lines(COLS, &refs)
    };
    s.settle(&plain(0));
    let id = s.pane().id;
    s.feed(&nu(0), 300);
    assert_eq!(
        (s.pane().rect, s.pane().origin),
        ([4, 0, COLS - 4, 20], [0, 0])
    );
    // A jump is placed by the numbers.
    s.feed(&nu(120), 300);
    assert_eq!(s.pane().origin, [0, 120]);
    s.feed(&nu(0), 300);
    // Numbers off: believed at the second screen.
    s.feed(&plain(0), 300);
    s.feed(&plain(1), 300);
    assert_eq!((s.pane().rect, s.pane().origin), ([0, 0, COLS, 20], [0, 1]));
    s.idle(300);
    assert_eq!(s.f.text(id, 0, 21), d[..21].to_vec());
    assert_eq!(s.f.text(id, 120, 140), d[120..140].to_vec());
}

fn numbered(doc: &[String], top: usize, rows: usize) -> Grid {
    let lines: Vec<String> = doc
        .iter()
        .enumerate()
        .skip(top)
        .take(rows)
        .map(|(i, l)| format!("{:>4} {l}", i + 1))
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    Grid::from_lines(COLS, &refs)
}

#[test]
fn gutter_gives_absolute_rows() {
    let d = doc(5000);
    let mut s = Sim::new();
    s.settle(&numbered(&d, 0, 20));
    let id = s.pane().id;
    // The gutter is left chrome.
    assert_eq!(s.pane().rect, [5, 0, COLS - 5, 20]);
    assert_eq!(s.pane().origin, [0, 0]);
    for top in [3000, 17, 4200, 0] {
        let ev = s.feed(&numbered(&d, top, 20), 300);
        assert_eq!(s.pane().origin, [0, top as i64], "{ev:?}");
        assert_eq!(s.pane().id, id);
    }
    s.idle(300);
    assert_eq!(s.f.text(id, 4205, 4207), d[4205..4207].to_vec());
    assert!(s.f.list().iter().all(|i| i.state != FrameState::Inactive));
    // A line inserted above the view: the rows shown are renumbered.
    s.feed(&numbered(&d, 100, 20), 300);
    s.idle(300);
    let mut e = d.clone();
    e.insert(50, "inserted above".into());
    let ev = s.feed(&numbered(&e, 101, 20), 300);
    assert!(
        ev.iter().any(|x| matches!(
            x,
            FrameEvent::Shift {
                from: 100,
                dy: 1,
                ..
            }
        )),
        "{ev:?}"
    );
    assert_eq!(s.pane().origin, [0, 101]);
}

#[test]
fn islands_merge_when_they_touch() {
    let d = doc(400);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    s.idle(300);
    // Jump to 60: unknown, an island.
    s.feed(&view(&d, 60, 20, &[]), 300);
    let island = s.pane().origin[1];
    assert!(island >= ext::ISLAND_GAP);
    let m = s.f.find(id, &lit("line 0070"), false);
    assert_eq!(m[0].y, island + 10);
    s.idle(300);
    // Scroll up towards the start until both overlap.
    let mut merged = None;
    for top in (5..60).rev() {
        let ev = s.feed(&view(&d, top, 20, &[]), 200);
        if let Some(FrameEvent::Shift { from, to, dy, .. }) =
            ev.iter().find(|e| matches!(e, FrameEvent::Shift { .. }))
        {
            merged = Some((*from, *to, *dy, top));
            break;
        }
    }
    let (from, to, dy, top) = merged.expect("merge");
    assert_eq!(s.pane().origin[1], top as i64);
    assert!(from <= island && to >= island + 19, "{from} {to}");
    assert_eq!(island + dy, 60);
    s.idle(300);
    assert_eq!(s.f.text(id, 0, 80), d[..80].to_vec());
}

#[test]
fn walking_into_an_island_brings_it_home() {
    // The main island keeps its rows; the island met on the way joins it.
    let d = doc(400);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    s.feed(&view(&d, 150, 20, &[]), 300);
    assert!(s.pane().origin[1] >= ext::ISLAND_GAP);
    s.feed(&view(&d, 300, 20, &[]), 300);
    assert!(s.pane().origin[1] >= 2 * ext::ISLAND_GAP);
    // Back to the start, found in the buffer.
    s.feed(&view(&d, 0, 20, &[]), 300);
    assert_eq!(s.pane().origin, [0, 0]);
    for top in (10..=380).step_by(10) {
        s.feed(&view(&d, top, 20, &[]), 200);
        assert_eq!(s.pane().origin, [0, top as i64], "at {top}");
    }
    s.idle(300);
    assert_eq!(s.pane().extent, [0, 399]);
    assert_eq!(s.f.text(id, 0, 400), d);
}

#[test]
fn shell_output_scrolls_with_vt_events() {
    // One prompt line moving up: too little to align by content alone.
    let mut s = Sim::new();
    let mut g = Grid::new(COLS, 10);
    put(&mut g, 0, 9, "$ make");
    s.settle(&g);
    let id = s.pane().id;
    let mut g2 = Grid::new(COLS, 10);
    put(&mut g2, 0, 6, "$ make");
    put(&mut g2, 0, 7, "cc -o a a.c");
    put(&mut g2, 0, 9, "$");
    let ev = s.feed_ev(
        &g2,
        100,
        &[VtEvent::Scroll {
            region: Rect::new(0, 0, COLS, 10),
            dy: -3,
            scrollback: true,
        }],
    );
    assert!(
        ev.contains(&FrameEvent::Scroll {
            frame: id,
            dx: 0,
            dy: 3
        }),
        "{ev:?}"
    );
}

#[test]
fn wiped_pane_keeps_what_was_there() {
    let d = doc(50);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    s.idle(300);
    s.feed(&Grid::new(COLS, 20), 100);
    s.idle(1000);
    assert_eq!(s.pane().id, id);
    assert!(s.pane().origin[1] >= ext::ISLAND_GAP);
    assert_eq!(s.f.text(id, 0, 3), d[..3].to_vec());
}

#[test]
fn half_drawn_screens_are_not_remembered() {
    let d = doc(50);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    let mut g = view(&d, 0, 20, &[]);
    put(&mut g, 0, 4, "GARBAGE");
    s.feed(&g, 50);
    s.feed(&view(&d, 0, 20, &[]), 50);
    s.idle(500);
    assert!(s.f.find(id, &lit("GARBAGE"), true).is_empty());
}

// -- dynamic frame -----------------------------------------------------

/// tmux-like: two panes side by side over one, a status bar.
fn tmux(left: &[String], right: &[String], below: &[String], status: &str) -> Grid {
    let mut g = Grid::new(COLS, 24);
    for r in 0..10u16 {
        put(
            &mut g,
            0,
            r,
            &left[r as usize].chars().take(29).collect::<String>(),
        );
        put(&mut g, 29, r, "│");
        put(
            &mut g,
            30,
            r,
            &right[r as usize].chars().take(30).collect::<String>(),
        );
    }
    put(&mut g, 0, 10, &"─".repeat(COLS as usize));
    put(&mut g, 29, 10, "┴");
    for r in 0..12u16 {
        put(&mut g, 0, 11 + r, &below[r as usize]);
    }
    put(&mut g, 0, 23, status);
    bg(&mut g, Rect::new(0, 23, COLS, 1), 2);
    g
}

#[test]
fn tmux_layout() {
    let (a, b, c) = (doc(100), doc(100), doc(100));
    let mut s = Sim::new();
    s.feed(&tmux(&a, &b[20..], &c[40..], "[0] 0:zsh*"), 0);
    assert_eq!(
        s.rects(),
        vec![[0, 0, 29, 10], [30, 0, 30, 10], [0, 11, COLS, 12]]
    );
    let chrome: Vec<_> = s
        .on_screen()
        .into_iter()
        .filter(|i| i.kind == FrameKind::Chrome)
        .collect();
    assert_eq!(chrome.len(), 1);
    assert_eq!(chrome[0].rect, [0, 23, COLS, 1]);
    // Candidates first, stable after the settle time.
    assert!(
        s.panes()
            .iter()
            .all(|p| p.state == FrameState::Candidate && !p.usable)
    );
    s.idle(1400);
    assert!(s.panes().iter().all(|p| !p.usable));
    let ev = s.idle(200);
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, FrameEvent::Stable { .. }))
            .count(),
        4
    );
    assert!(s.panes().iter().all(|p| p.usable));
    // One pane scrolls: the others do not move.
    let ids: Vec<_> = s.panes().iter().map(|p| p.id).collect();
    let ev = s.feed(&tmux(&a, &b[23..], &c[40..], "[0] 0:zsh*"), 100);
    assert_eq!(
        ev,
        vec![FrameEvent::Scroll {
            frame: ids[1],
            dx: 0,
            dy: 3
        }]
    );
    assert_eq!(s.panes()[0].origin, [0, 0]);
    assert_eq!(s.panes()[1].origin, [0, 3]);
    // Names.
    s.cursor = (40, 5);
    s.feed(&tmux(&a, &b[23..], &c[40..], "[0] 0:zsh*"), 100);
    assert_eq!(s.f.resolve(&FrameSel::Name("active".into())), Some(ids[1]));
    assert_eq!(s.f.resolve(&FrameSel::Name("largest".into())), Some(ids[2]));
    assert_eq!(s.f.resolve(&FrameSel::Name("raw".into())), Some(RAW));
    assert_eq!(
        s.f.resolve(&FrameSel::Name(ids[0].to_string())),
        Some(ids[0])
    );
    assert_eq!(s.f.resolve(&FrameSel::Id(999)), None);
    assert_eq!(s.f.resolve(&FrameSel::Name("nope".into())), None);
    s.idle(3000);
    assert_eq!(
        s.f.resolve(&FrameSel::Name("quietest".into())),
        Some(ids[2]),
        "largest of the calm ones"
    );
    assert_eq!(s.f.at(29, 3), Some((RAW, FramePos { x: 29, y: 3 })));
    assert_eq!(s.f.at(31, 3).unwrap().0, ids[1]);
}

#[test]
fn layout_switch_and_back_keeps_identities() {
    let (a, b, c, e) = (doc(100), doc(100), doc(100), doc(300));
    let mut s = Sim::new();
    let one = tmux(&a, &b[20..], &c[40..], "[0] 0:zsh* 1:vim-");
    s.settle(&one);
    let ids: Vec<_> = s.panes().iter().map(|p| p.id).collect();
    let layout = s.f.layout();
    // Window 1: a single pane.
    let mut two = view(&e[200..], 0, 23, &["[0] 0:zsh- 1:vim*"]);
    bg(&mut two, Rect::new(0, 23, COLS, 1), 2);
    let ev = s.feed(&two, 500);
    assert!(ev.is_empty(), "suspended first: {ev:?}");
    assert!(s.panes().iter().all(|p| !p.usable));
    let ev = s.idle(150);
    assert!(
        ev.iter().any(|x| matches!(x, FrameEvent::Layout { .. })),
        "{ev:?}"
    );
    assert_ne!(s.f.layout(), layout);
    let big = s.pane();
    assert!(!ids.contains(&big.id));
    assert_eq!(big.rect, [0, 0, COLS, 23]);
    assert_eq!(
        s.f.list()
            .iter()
            .filter(|i| i.state == FrameState::Inactive)
            .count(),
        3
    );
    s.idle(2000);
    // Back to window 0, the middle pane has scrolled meanwhile.
    s.feed(&tmux(&a, &b[22..], &c[40..], "[0] 0:zsh* 1:vim-"), 500);
    s.idle(150);
    assert_eq!(s.f.layout(), layout);
    assert_eq!(s.panes().iter().map(|p| p.id).collect::<Vec<_>>(), ids);
    assert_eq!(s.panes()[1].origin, [0, 2], "the origin is found again");
    assert!(s.panes().iter().all(|p| !p.usable));
    s.idle(1500);
    assert!(s.panes().iter().all(|p| p.usable));
    // And window 1 again.
    s.feed(&two, 500);
    s.idle(150);
    assert_eq!(s.pane().id, big.id);
}

#[test]
fn same_layout_other_window() {
    // Two windows with the same single pane: told apart by content, the
    // status bar says it is a switch and not a jump.
    let (a, b) = (doc(300), doc(600));
    let win = |d: &[String], status: &str| {
        let mut g = view(d, 0, 23, &[status]);
        bg(&mut g, Rect::new(0, 23, COLS, 1), 2);
        g
    };
    let mut s = Sim::new();
    s.settle(&win(&a, "0:a* 1:b-"));
    let first = s.pane().id;
    let ev = s.feed(&win(&b[300..], "0:a- 1:b*"), 500);
    let second = s.pane().id;
    assert_ne!(first, second);
    assert!(
        ev.contains(&FrameEvent::Inactive { frame: first })
            && ev.contains(&FrameEvent::Added { frame: second })
    );
    assert!(!s.pane().usable);
    s.idle(2000);
    let ev = s.feed(&win(&a, "0:a* 1:b-"), 500);
    assert_eq!(s.pane().id, first);
    assert!(ev.contains(&FrameEvent::Attached { frame: first }));
    s.idle(2000);
    s.feed(&win(&b[300..], "0:a- 1:b*"), 500);
    assert_eq!(s.pane().id, second);
    // Without a chrome change the same whole-pane change is a jump.
    s.idle(2000);
    let ev = s.feed(&win(&b[500..], "0:a- 1:b*"), 500);
    assert_eq!(s.pane().id, second);
    assert!(ev.contains(&FrameEvent::Jump { frame: second }), "{ev:?}");
    assert!(s.pane().usable);
}

#[test]
fn flapping_segmentation_only_suspends() {
    let (a, b, c) = (doc(100), doc(100), doc(100));
    let mut s = Sim::new();
    let g = tmux(&a, &b, &c, "status");
    s.settle(&g);
    let ids: Vec<_> = s.panes().iter().map(|p| p.id).collect();
    // The border is half redrawn for one burst.
    let mut broken = g.clone();
    put(&mut broken, 0, 10, &" ".repeat(40));
    s.feed(&broken, 100);
    assert!(!s.panes()[0].usable);
    let ev = s.feed(&g, 20);
    assert!(ev.is_empty(), "{ev:?}");
    assert!(s.panes().iter().all(|p| p.usable));
    assert_eq!(s.panes().iter().map(|p| p.id).collect::<Vec<_>>(), ids);
}

/// vim-like: two windows side by side with inverse status lines, a
/// command line.
fn vim(left: &[String], right: &[String]) -> Grid {
    let mut g = Grid::new(COLS, 24);
    for r in 0..22u16 {
        put(
            &mut g,
            0,
            r,
            &left[r as usize].chars().take(29).collect::<String>(),
        );
        put(&mut g, 29, r, "|");
        put(
            &mut g,
            30,
            r,
            &right[r as usize].chars().take(30).collect::<String>(),
        );
    }
    inverse(&mut g, Rect::new(29, 0, 1, 22));
    put(&mut g, 0, 22, "left.txt");
    put(&mut g, 30, 22, "right.txt");
    inverse(&mut g, Rect::new(0, 22, COLS, 1));
    paint(&mut g, Rect::new(0, 22, 30, 1), |s| {
        s.attrs.insert(Attrs::BOLD)
    });
    put(&mut g, 0, 23, ":set nu");
    g
}

#[test]
fn vim_splits() {
    let (a, b) = (doc(100), doc(100));
    let mut s = Sim::new();
    s.cursor = (3, 3);
    s.settle(&vim(&a, &b[50..]));
    assert_eq!(s.rects(), vec![[0, 0, 29, 22], [30, 0, 30, 22]]);
    let chrome: Vec<_> = s
        .on_screen()
        .into_iter()
        .filter(|i| i.kind == FrameKind::Chrome)
        .map(|i| i.rect)
        .collect();
    assert_eq!(chrome, vec![[0, 22, COLS, 1], [0, 23, COLS, 1]]);
    // A cursor line highlight is not a bar.
    let mut g = vim(&a, &b[50..]);
    bg(&mut g, Rect::new(0, 3, 29, 1), 8);
    let ev = s.feed(&g, 100);
    assert!(ev.is_empty(), "{ev:?}");
    assert_eq!(s.rects(), vec![[0, 0, 29, 22], [30, 0, 30, 22]]);
}

#[test]
fn boxes() {
    // Stacked boxes on the left, one on the right: guillotine.
    let mut g = Grid::new(COLS, 20);
    let draw = |g: &mut Grid, r: Rect| {
        put(g, r.x, r.y, &format!("┌{}┐", "─".repeat(r.w as usize - 2)));
        put(
            g,
            r.x,
            r.bottom() - 1,
            &format!("└{}┘", "─".repeat(r.w as usize - 2)),
        );
        for y in r.y + 1..r.bottom() - 1 {
            put(g, r.x, y, "│");
            put(g, r.right() - 1, y, "│");
        }
    };
    draw(&mut g, Rect::new(0, 0, 20, 10));
    draw(&mut g, Rect::new(0, 10, 20, 10));
    draw(&mut g, Rect::new(20, 0, 40, 20));
    put(&mut g, 2, 2, "files");
    put(&mut g, 22, 2, "diff");
    let mut s = Sim::new();
    s.settle(&g);
    assert_eq!(
        s.rects(),
        vec![[1, 1, 18, 8], [21, 1, 38, 18], [1, 11, 18, 8]]
    );
    // A box floating over text: a pane inside the pane.
    let d = doc(30);
    let mut g = view(&d, 0, 20, &[]);
    for y in 5..12 {
        put(&mut g, 10, y, &" ".repeat(30));
    }
    draw(&mut g, Rect::new(10, 5, 30, 7));
    put(&mut g, 12, 7, "Really quit?");
    let mut s = Sim::new();
    s.settle(&g);
    assert_eq!(s.rects(), vec![[0, 0, COLS, 20], [11, 6, 28, 5]]);
    let inner = s.panes()[1].id;
    assert_eq!(s.f.at(12, 7).unwrap().0, inner);
    s.cursor = (12, 7);
    s.feed(&g, 10);
    assert_eq!(s.f.resolve(&FrameSel::Name("active".into())), Some(inner));
}

#[test]
fn small_leaves_are_chrome() {
    let d = doc(30);
    let mut g = view(&d, 0, 20, &["", "> prompt", ""]);
    put(&mut g, 0, 20, &"─".repeat(COLS as usize));
    put(&mut g, 0, 22, &"─".repeat(COLS as usize));
    let mut s = Sim::new();
    s.settle(&g);
    assert_eq!(s.rects(), vec![[0, 0, COLS, 20]]);
    let chrome: Vec<_> = s
        .on_screen()
        .into_iter()
        .filter(|i| i.kind == FrameKind::Chrome)
        .map(|i| i.rect)
        .collect();
    assert_eq!(chrome, vec![[0, 21, COLS, 1]]);
}

#[test]
fn separator_that_scrolls_is_content() {
    // A rule inside a transcript.
    let mut d = doc(200);
    d[12] = "─".repeat(COLS as usize);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    assert_eq!(
        s.rects(),
        vec![[0, 0, COLS, 12], [0, 13, COLS, 7]],
        "believed while static"
    );
    s.feed(&view(&d, 1, 20, &[]), 100);
    s.idle(200);
    assert_eq!(
        s.rects(),
        vec![[0, 0, COLS, 20]],
        "demoted once it moved with the text"
    );
    // Other rules in the same place are not believed either.
    d[40] = "─".repeat(COLS as usize);
    s.feed(&view(&d, 30, 20, &[]), 100);
    s.idle(200);
    assert_eq!(s.rects(), vec![[0, 0, COLS, 20]]);
    // A border that moves alone (a resize) is still a border.
    let (a, b, c) = (doc(100), doc(100), doc(100));
    let mut s = Sim::new();
    s.settle(&tmux(&a, &b, &c, "st"));
    let mut g = tmux(&a, &b, &c, "st");
    for r in 10..13u16 {
        put(&mut g, 0, r, &" ".repeat(COLS as usize));
    }
    put(&mut g, 0, 12, &"─".repeat(COLS as usize));
    s.feed(&g, 100);
    s.idle(200);
    assert_eq!(s.rects().len(), 2, "{:?}", s.rects());
}

// -- heat, placement ---------------------------------------------------

#[test]
fn heat_converges() {
    let mut s = Sim::new();
    let mut g = Grid::new(COLS, 20);
    put(&mut g, 0, 0, "static text");
    s.settle(&g);
    let id = s.pane().id;
    // A spinner at (30, 10).
    for i in 0..40 {
        put(&mut g, 30, 10, ["|", "/", "-", "\\"][i % 4]);
        s.feed(&g, 100);
    }
    let h = s.f.heat(id).unwrap();
    assert!(h.safe_at(30, 10) < 0.05);
    assert!(h.safe_at(0, 0) < 0.05, "occupied");
    assert!(h.safe_at(40, 15) > 0.95);
    assert_eq!(h.digits()[15].as_bytes()[40], b'9');
    assert_eq!(h.digits()[10].as_bytes()[30], b'0');
    // It calms down, slowly.
    put(&mut g, 30, 10, " ");
    s.feed(&g, 100);
    s.idle(60_000);
    let mid = s.f.heat(id).unwrap().safe_at(30, 10);
    assert!(mid > 0.2 && mid < 0.8, "{mid}");
    s.idle(300_000);
    assert!(s.f.heat(id).unwrap().safe_at(30, 10) > 0.95);
    assert!(s.f.heat(RAW).unwrap().safe_at(30, 10) > 0.95);
}

#[test]
fn placement_and_hysteresis() {
    let mut s = Sim::new();
    let mut g = Grid::new(COLS, 20);
    for r in 0..10 {
        put(&mut g, 40, r, "busy busy busy busy");
    }
    s.settle(&g);
    let id = s.pane().id;
    assert_eq!(s.f.place(id, 18, 6, Prefer::TopLeft, &[]), Some((0, 0)));
    assert_eq!(
        s.f.place(id, 18, 6, Prefer::BottomRight, &[]),
        Some((42, 14))
    );
    // Top-right is occupied: next to it.
    let (x, y) = s.f.place(id, 18, 6, Prefer::TopRight, &[]).unwrap();
    assert!(x + 18 <= 40 || y >= 10, "{x},{y}");
    let first = (x, y);
    // Another sprite does not get the same spot.
    let other =
        s.f.place(id, 18, 6, Prefer::TopRight, &[Rect::new(x, y, 18, 6)])
            .unwrap();
    assert!(!Rect::new(other.0, other.1, 18, 6).intersects(&Rect::new(x, y, 18, 6)));
    assert_eq!(s.f.place(id, 18, 6, Prefer::TopRight, &[]), Some(first));
    assert_eq!(
        s.f.place(id, 18, 6, Prefer::TopRight, &[Rect::new(x, y, 18, 6)]),
        Some(other)
    );
    // The corner clears: a better spot, taken only after it stayed better.
    s.feed(&Grid::new(COLS, 20), 100);
    s.idle(400_000);
    assert_eq!(s.f.place(id, 18, 6, Prefer::TopRight, &[]), Some(first));
    s.idle(1900);
    assert_eq!(s.f.place(id, 18, 6, Prefer::TopRight, &[]), Some(first));
    s.idle(200);
    assert_eq!(s.f.place(id, 18, 6, Prefer::TopRight, &[]), Some((42, 0)));
    assert_eq!(s.f.place(id, 80, 6, Prefer::Any, &[]), None);
    assert_eq!(s.f.place(77, 8, 6, Prefer::Any, &[]), None);
}

// -- plugins -----------------------------------------------------------

struct Fake;

impl TuiSupport for Fake {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if pane.row_text(pane.rect.h - 1).starts_with("FAKE") {
            1.0
        } else {
            0.0
        }
    }

    fn chrome(&self, _pane: &PaneView) -> Option<Chrome> {
        Some(Chrome {
            top: 0,
            bottom: 1,
            left: 0,
            right: 0,
        })
    }

    fn view_hint(&self, pane: &PaneView) -> Option<ViewHint> {
        let t = pane.row_text(pane.rect.h - 1);
        let row = t.split_whitespace().nth(2)?.parse().ok()?;
        Some(ViewHint {
            row: Some(row),
            col: None,
        })
    }

    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        pane.row_text(pane.rect.h - 1)
            .split_whitespace()
            .nth(1)
            .map(str::to_string)
    }

    fn tuning(&self) -> crate::plugin::Tuning {
        crate::plugin::Tuning {
            settle_ms: Some(1000),
            commit_ms: None,
        }
    }
}

#[test]
fn plugin_chrome_hint_key() {
    let (a, b) = (doc(1000), doc(2000));
    let mut s = Sim::with(FrameConfig::default(), vec![Box::new(Fake)]);
    let screen = |d: &[String], name: &str, top: usize| {
        view(&d[top..], 0, 20, &[&format!("FAKE {name} {top}")])
    };
    s.feed(&screen(&a, "a.txt", 0), 0);
    let first = s.pane();
    assert_eq!(first.support.as_deref(), Some("fake"));
    assert_eq!(first.key.as_deref(), Some("a.txt"));
    assert_eq!(first.rect, [0, 0, COLS, 20]);
    s.idle(900);
    assert!(!s.pane().usable);
    s.idle(200);
    assert!(s.pane().usable, "tuned settle time");
    // The hint places a jump exactly.
    s.feed(&screen(&a, "a.txt", 700), 300);
    assert_eq!((s.pane().id, s.pane().origin), (first.id, [0, 700]));
    // Another key: another frame, though the layout is the same.
    s.feed(&screen(&b[1000..], "b.txt", 0), 300);
    let second = s.pane();
    assert_ne!(second.id, first.id);
    assert_eq!(second.key.as_deref(), Some("b.txt"));
    // Back by key, even on rows never seen.
    s.feed(&screen(&a, "a.txt", 300), 300);
    assert_eq!((s.pane().id, s.pane().origin), (first.id, [0, 300]));
    s.idle(500);
    assert_eq!(s.f.text(first.id, 700, 702), a[700..702].to_vec());
}

// -- wire, cost --------------------------------------------------------

#[test]
fn hinted_view_of_another_document_is_another_frame() {
    // The app says "row 0" for both files (a tab switch at the top of
    // each): the first frame is not overwritten, and comes back.
    let a = doc(300);
    let b: Vec<String> = (0..300).map(|i| format!("other {i:04} zzz")).collect();
    let mut s = Sim::with(FrameConfig::default(), vec![Box::new(Fake)]);
    let screen = |d: &[String], top: usize| view(&d[top..], 0, 20, &[&format!("FAKE x {top}")]);
    s.settle(&screen(&a, 0));
    let first = s.pane().id;
    s.feed(&screen(&a, 10), 300);
    s.feed(&screen(&a, 0), 300);
    s.feed(&screen(&b, 0), 300);
    let second = s.pane().id;
    assert_ne!(second, first);
    s.feed(&screen(&b, 5), 300);
    s.idle(300);
    s.feed(&screen(&a, 0), 300);
    assert_eq!((s.pane().id, s.pane().origin), (first, [0, 0]));
    s.idle(300);
    assert_eq!(s.f.text(first, 0, 30), a[..30].to_vec());
    assert_eq!(s.f.text(second, 0, 25), b[..25].to_vec());
}

#[test]
fn regex_anchors_are_the_ends_of_the_text() {
    let d = doc(40);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 19, &["  indented foo"]));
    let re = |p: &str| TextQuery {
        pattern: p.into(),
        regex: true,
        ignore_case: false,
    };
    for id in [RAW, s.pane().id] {
        let m = s.f.find(id, &re("foo$"), false);
        assert_eq!(m.len(), 1, "frame {id}");
        assert_eq!((m[0].x, m[0].y), (11, 19));
        assert_eq!(s.f.find(id, &re("^line 0005"), false).len(), 1);
        assert_eq!(s.f.find(id, &re("^indented"), false).len(), 0);
    }
}

#[test]
fn wire_forms() {
    let mut s = Sim::new();
    s.settle(&view(&doc(30), 0, 20, &[]));
    let v = serde_json::to_value(s.f.list()).unwrap();
    assert_eq!(
        v[1],
        serde_json::json!({"id": 1, "kind": "pane", "state": "stable", "usable": true,
            "rect": [0, 0, 60, 20], "origin": [0, 0], "extent": [0, 19], "key": null, "support": null})
    );
    assert_eq!(v[0]["kind"], "raw");
    let e = serde_json::to_value(FrameEvent::Scroll {
        frame: 1,
        dx: 0,
        dy: 2,
    })
    .unwrap();
    assert_eq!(
        e,
        serde_json::json!({"type": "scroll", "frame": 1, "dx": 0, "dy": 2})
    );
}

#[test]
fn ids_are_never_reused_and_inactive_frames_are_capped() {
    let cfg = FrameConfig {
        max_inactive: 3,
        ..FrameConfig::default()
    };
    let mut s = Sim::with(cfg, Vec::new());
    let mut seen = Vec::new();
    for i in 0..8 {
        let d: Vec<String> = (0..30).map(|k| format!("window {i} row {k}")).collect();
        let mut g = view(&d, 0, 20, &[&format!("status {i}")]);
        bg(&mut g, Rect::new(0, 20, COLS, 1), 4);
        s.feed(&g, 300);
        let id = s.pane().id;
        assert!(!seen.contains(&id));
        seen.push(id);
    }
    assert_eq!(
        s.f.list()
            .iter()
            .filter(|i| i.state == FrameState::Inactive)
            .count(),
        3
    );
}

/// `cargo test -p ptyrc-core --release frames::tests::bench -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_update_cost() {
    let cols = 200u16;
    let d: Vec<String> = (0..5000)
        .map(|i| format!("{i:05} {}", "lorem ipsum dolor sit amet ".repeat(6)))
        .collect();
    let screens: Vec<Grid> = (0..200)
        .map(|top| {
            let lines: Vec<&str> = d.iter().skip(top).take(60).map(String::as_str).collect();
            Grid::from_lines(cols, &lines)
        })
        .collect();
    let mut f = Frames::new(FrameConfig::default(), Vec::new());
    let cursor = Cursor::default();
    let mut now = Duration::ZERO;
    let start = std::time::Instant::now();
    let rounds = 10;
    for _ in 0..rounds {
        for g in &screens {
            now += Duration::from_millis(16);
            f.update(
                &ScreenInput {
                    grid: g,
                    cursor,
                    alt_screen: false,
                    title: "",
                    events: &[],
                },
                now,
            );
        }
    }
    let scroll = start.elapsed() / (rounds * screens.len() as u32);
    let start = std::time::Instant::now();
    for _ in 0..2000 {
        now += Duration::from_millis(16);
        f.update(
            &ScreenInput {
                grid: &screens[0],
                cursor,
                alt_screen: false,
                title: "",
                events: &[],
            },
            now,
        );
    }
    let idle = start.elapsed() / 2000;
    eprintln!("200x60: full scroll {scroll:?} per update, unchanged {idle:?} per update");
    assert!(scroll < Duration::from_millis(1));
}

// -- identity through changes of shape -----------------------------------

#[test]
fn resized_pane_keeps_its_frame() {
    let d = doc(300);
    let mut s = Sim::new();
    s.settle(&view(&d, 40, 20, &[]));
    let id = s.pane().id;
    for top in 41..=50 {
        s.feed(&view(&d, top, 20, &[]), 100);
    }
    s.idle(500);
    let origin = s.pane().origin;
    // The terminal grows by four rows: more of the same text.
    let ev = s.feed(&view(&d, 50, 24, &[]), 100);
    assert!(ev.contains(&FrameEvent::Attached { frame: id }), "{ev:?}");
    assert_eq!(
        (s.pane().id, s.pane().origin, s.pane().rect),
        (id, origin, [0, 0, COLS, 24])
    );
    assert!(!s.pane().usable);
    s.idle(1600);
    assert!(s.pane().usable);
    assert_eq!(s.f.text(id, 0, 2), d[40..42].to_vec());
}

#[test]
fn transcript_with_prompt_box() {
    // A coding-agent TUI: a transcript that scrolls, a prompt between two
    // rules, a status line.
    let mut d = doc(300);
    let rule = "─".repeat(COLS as usize);
    let screen = |d: &[String], top: usize| {
        view(
            &d[top..],
            0,
            18,
            &[&rule, "> ask anything", &rule, "  ? for shortcuts"],
        )
    };
    let mut s = Sim::new();
    s.settle(&screen(&d, 0));
    let id = s.pane().id;
    assert_eq!(s.pane().rect, [0, 0, COLS, 18]);
    for top in 1..40 {
        let ev = s.feed(&screen(&d, top), 60);
        assert_eq!(
            ev,
            vec![FrameEvent::Scroll {
                frame: id,
                dx: 0,
                dy: 1
            }]
        );
    }
    assert!(s.pane().usable);
    s.idle(300);
    // A rule inside the transcript scrolls by: rules are no longer
    // believed, the frame survives in a larger rectangle.
    d[60] = rule.clone();
    for top in 40..50 {
        s.feed(&screen(&d, top), 60);
    }
    s.idle(2000);
    for top in 50..56 {
        s.feed(&screen(&d, top), 60);
    }
    let p = s.pane();
    assert_eq!(p.id, id);
    assert_eq!(p.origin, [0, 55]);
    assert_eq!(
        p.rect,
        [0, 0, COLS, 18],
        "the prompt box is learned as chrome"
    );
    assert_eq!(s.f.text(id, 3, 5), d[3..5].to_vec());
}

// -- robustness --------------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

#[test]
fn random_screens_do_not_break_anything() {
    let glyphs = [
        "a", "b", " ", " ", " ", "│", "─", "┌", "┐", "└", "┘", "┼", "|", "-", "1", "2", "世",
    ];
    for seed in 0..12u64 {
        let mut rng = Lcg(seed * 7919 + 1);
        let mut s = Sim::new();
        let (mut cols, mut rows) = (1 + rng.next(40) as u16, 1 + rng.next(16) as u16);
        let mut g = Grid::new(cols, rows);
        let mut max_id = 0;
        for _ in 0..120 {
            match rng.next(10) {
                0 => {
                    (cols, rows) = (1 + rng.next(40) as u16, 1 + rng.next(16) as u16);
                    g = Grid::new(cols, rows);
                }
                1 => g = Grid::new(cols, rows),
                2 => {
                    // Scroll up by one.
                    let mut n = Grid::new(cols, rows);
                    for r in 1..rows {
                        for c in 0..cols {
                            *n.cell_mut(c, r - 1) = g.cell(c, r).clone();
                        }
                    }
                    g = n;
                }
                3 => {
                    let r = rng.next(rows as u64) as u16;
                    let c = glyphs[rng.next(glyphs.len() as u64) as usize];
                    for x in 0..cols {
                        g.put(
                            x,
                            r,
                            Cell::narrow(if c == "世" { "x" } else { c }, Style::default()),
                        );
                    }
                    if rng.next(2) == 0 {
                        inverse(&mut g, Rect::new(0, r, cols, 1));
                    }
                }
                4 => {
                    let c = rng.next(cols as u64) as u16;
                    for y in 0..rows {
                        g.put(c, y, Cell::narrow("│", Style::default()));
                    }
                }
                _ => {
                    for _ in 0..1 + rng.next(30) {
                        let (c, r) = (rng.next(cols as u64) as u16, rng.next(rows as u64) as u16);
                        let t = glyphs[rng.next(glyphs.len() as u64) as usize];
                        if t == "世" {
                            g.put(c, r, Cell::wide(t, Style::default()));
                        } else {
                            g.put(c, r, Cell::narrow(t, Style::default()));
                        }
                    }
                }
            }
            s.cursor = (rng.next(cols as u64) as u16, rng.next(rows as u64) as u16);
            s.alt = rng.next(20) == 0;
            let ev = s.feed(&g, rng.next(400));
            for e in &ev {
                if let FrameEvent::Added { frame } = e {
                    assert!(*frame > max_id, "ids grow");
                    max_id = *frame;
                }
            }
            // Every query answers, every frame on screen is inside it.
            for i in s.f.list() {
                let info = s.f.info(i.id).unwrap();
                if i.state != FrameState::Inactive {
                    assert!(
                        info.rect.right() <= cols && info.rect.bottom() <= rows,
                        "{i:?}"
                    );
                    assert!(!info.rect.is_empty() || cols == 0);
                }
                s.f.text(i.id, i.extent[0] - 2, i.extent[0] + 5);
                s.f.find(i.id, &lit("a"), true);
                s.f.heat(i.id).unwrap().digits();
                s.f.place(i.id, 2, 1, Prefer::TopRight, &[]);
            }
            for name in ["active", "largest", "quietest"] {
                assert!(s.f.resolve(&FrameSel::Name(name.into())).is_some());
            }
            s.f.at(s.cursor.0, s.cursor.1).unwrap();
            // The wake time moves on.
            if let Some(w) = s.f.next_wake() {
                assert!(w > s.now, "wake {w:?} at {:?}", s.now);
            }
        }
    }
}

struct Halves;

impl TuiSupport for Halves {
    fn name(&self) -> &'static str {
        "halves"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if pane.title == "halves" { 1.0 } else { 0.0 }
    }

    fn segment(&self, area: &PaneView) -> Option<Vec<Rect>> {
        let w = area.rect.w / 2;
        Some(vec![
            Rect::new(0, 0, w, area.rect.h),
            Rect::new(w, 0, area.rect.w - w, area.rect.h),
        ])
    }
}

#[test]
fn plugin_segmentation_replaces_the_generic_one() {
    let mut s = Sim::with(FrameConfig::default(), vec![Box::new(Halves)]);
    let g = view(&doc(30), 0, 20, &[]);
    s.settle(&g);
    assert_eq!(s.rects(), vec![[0, 0, COLS, 20]]);
    s.title = "halves".into();
    let mut g2 = g.clone();
    put(&mut g2, 0, 0, "x");
    s.feed(&g2, 10);
    s.idle(200);
    assert_eq!(s.rects(), vec![[0, 0, 30, 20], [30, 0, 30, 20]]);
    // Panes born on a tick meet the plugins at the next burst.
    s.feed(&g2, 10);
    assert_eq!(s.panes()[0].support.as_deref(), Some("halves"));
}
