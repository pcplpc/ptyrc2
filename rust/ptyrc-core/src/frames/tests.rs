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
    /// The app hides the cursor.
    hidden: bool,
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
            hidden: false,
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
            visible: !self.hidden,
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
    // A match ending the text on a wide glyph.
    let mut g = Grid::new(COLS, 3);
    put(&mut g, 0, 0, "ab");
    g.put(2, 0, Cell::wide("世", Style::default()));
    g.put(4, 0, Cell::wide("界", Style::default()));
    s.feed(&g, 10);
    let m = s.f.find(RAW, &lit("b世界"), false);
    assert_eq!((m[0].x, m[0].len), (1, 5));
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
    // Numbers off: the same rows, four columns to the left. Taken at
    // once, and not as a horizontal scroll or a jump.
    s.feed(&nu(120), 300);
    s.feed(&plain(120), 300);
    assert_eq!(
        (s.pane().rect, s.pane().origin),
        ([0, 0, COLS, 20], [0, 120])
    );
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
fn islands_are_collected() {
    // A shell cleared again and again: one island per clear, the oldest
    // forgotten; the extent is the island on screen.
    let mut s = Sim::new();
    let page =
        |k: usize| -> Vec<String> { (0..12).map(|i| format!("run {k:02} out {i:02}")).collect() };
    s.settle(&view(&page(0), 0, 20, &[]));
    let id = s.pane().id;
    for k in 1..=12 {
        s.feed(&Grid::new(COLS, 20), 300);
        s.feed(&view(&page(k), 0, 20, &[]), 300);
        s.idle(300);
    }
    let info = s.pane();
    assert_eq!(info.id, id);
    assert_eq!(info.extent[1] - info.extent[0], 19, "{info:?}");
    assert_eq!(info.extent[0], info.origin[1]);
    assert_eq!(s.f.text(id, info.extent[0], info.extent[0] + 12), page(12));
    // The first runs are gone, the last ones are found again.
    assert_eq!(s.f.text(id, 0, 3), vec!["", "", ""]);
    assert!(s.f.find(id, &lit("run 01 out 03"), true).is_empty());
    assert_eq!(s.f.find(id, &lit("run 09 out 03"), true).len(), 1);
    s.feed(&view(&page(9), 0, 20, &[]), 300);
    let back = s.pane().origin[1];
    assert!(back < info.origin[1] && back >= ext::ISLAND_GAP);
}

#[test]
fn island_merge_moves_the_rows_on_screen() {
    // The merge comes before the rows on screen are remembered: the shift
    // covers them all the same, drawings are fixed on them.
    let d = doc(400);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    s.idle(300);
    s.feed(&view(&d, 30, 20, &[]), 300);
    let island = s.pane().origin[1];
    assert!(island >= ext::ISLAND_GAP);
    let mut merged = None;
    for (k, top) in (5..30).rev().enumerate() {
        let ev = s.feed(&view(&d, top, 20, &[]), 20);
        if let Some(FrameEvent::Shift { from, to, dy, .. }) =
            ev.iter().find(|e| matches!(e, FrameEvent::Shift { .. }))
        {
            merged = Some((*from, *to, *dy, k as i64 + 1));
            break;
        }
    }
    let (from, to, dy, steps) = merged.expect("merge");
    // The view was at `island - steps`, twenty rows.
    assert!(
        from <= island - steps && to >= island - steps + 19,
        "{from} {to}"
    );
    assert_eq!(island + dy, 30);
    s.idle(300);
    assert_eq!(s.f.text(id, 0, 50), d[..50].to_vec());
}

#[test]
fn lines_inserted_away_from_the_cursor_are_an_edit() {
    // A splice is believed at the cursor only. Elsewhere the rows are
    // rewritten in place: nothing moves in the frame, the text is right.
    let d = doc(100);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    s.idle(300);
    let mut e = d.clone();
    e.insert(8, "NEW one".into());
    e.insert(9, "NEW two".into());
    s.cursor = (0, 18);
    let ev = s.feed(&view(&e, 0, 20, &[]), 100);
    assert!(
        !ev.iter()
            .any(|x| matches!(x, FrameEvent::Shift { .. } | FrameEvent::Scroll { .. })),
        "{ev:?}"
    );
    assert_eq!(s.pane().origin, [0, 0]);
    s.idle(300);
    assert_eq!(s.f.text(id, 0, 20), e[..20].to_vec());
}

#[test]
fn narrower_pane_keeps_its_frame() {
    // The terminal loses ten columns; the lines fit: same frame, same rows.
    let d = doc(300);
    let mut s = Sim::new();
    s.settle(&view(&d, 40, 20, &[]));
    let id = s.pane().id;
    for top in 41..=50 {
        s.feed(&view(&d, top, 20, &[]), 100);
    }
    s.idle(500);
    let origin = s.pane().origin;
    let lines: Vec<&str> = d[50..70].iter().map(String::as_str).collect();
    let ev = s.feed(&Grid::from_lines(COLS - 10, &lines), 100);
    assert!(ev.contains(&FrameEvent::Attached { frame: id }), "{ev:?}");
    assert_eq!(
        (s.pane().id, s.pane().origin, s.pane().rect),
        (id, origin, [0, 0, COLS - 10, 20])
    );
    s.idle(1600);
    assert!(s.pane().usable);
    let lines: Vec<&str> = d[55..75].iter().map(String::as_str).collect();
    s.feed(&Grid::from_lines(COLS - 10, &lines), 100);
    assert_eq!(s.pane().origin, [origin[0], origin[1] + 5]);
    assert_eq!(s.f.text(id, 0, 2), d[40..42].to_vec());
}

#[test]
fn layout_flaps_under_load() {
    // A border half redrawn every other burst while a pane scrolls: no
    // frame is allocated, the panes resume, the scroll is followed.
    let (a, b, c) = (doc(300), doc(100), doc(100));
    let mut s = Sim::new();
    let screen = |top: usize| tmux(&a[top..], &b, &c, "status");
    s.settle(&screen(0));
    let ids: Vec<_> = s.panes().iter().map(|p| p.id).collect();
    let all = s.f.list().len();
    let mut events = Vec::new();
    for top in 1..=40 {
        let mut broken = screen(top);
        put(&mut broken, 0, 10, &" ".repeat(40));
        events.extend(s.feed(&broken, 20));
        events.extend(s.feed(&screen(top), 20));
    }
    assert!(
        !events.iter().any(|e| matches!(
            e,
            FrameEvent::Added { .. } | FrameEvent::Dropped { .. } | FrameEvent::Layout { .. }
        )),
        "{events:?}"
    );
    assert_eq!(s.f.list().len(), all);
    assert_eq!(s.panes().iter().map(|p| p.id).collect::<Vec<_>>(), ids);
    assert!(s.panes().iter().all(|p| p.usable));
    assert_eq!(s.panes()[0].origin, [0, 40]);
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

    fn tuning(&self, _pane: &PaneView) -> crate::plugin::Tuning {
        crate::plugin::Tuning {
            settle_ms: Some(1000),
            commit_ms: None,
            monitor: false,
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
fn a_column_of_numbers_is_not_a_gutter() {
    // `seq 1 200`: numbers with nothing beside them.
    let mut s = Sim::new();
    let screen = |top: usize| {
        let lines: Vec<String> = (top..top + 20).map(|i| format!("{i}")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        Grid::from_lines(COLS, &refs)
    };
    s.settle(&screen(1));
    s.feed(&screen(2), 300);
    s.feed(&screen(3), 300);
    assert_eq!(s.pane().rect, [0, 0, COLS, 20]);
    assert_eq!(s.pane().origin, [0, 2]);
}

#[test]
fn scroll_further_than_the_view_by_vt_events() {
    // A long command output in one burst: no row survives, the emulator
    // counted the lines.
    let d = doc(400);
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    let scroll = |n: i16| VtEvent::Scroll {
        region: Rect::new(0, 0, COLS, 20),
        dy: -n,
        scrollback: true,
    };
    let ev = s.feed_ev(&view(&d, 150, 20, &[]), 300, &[scroll(150)]);
    assert!(
        ev.contains(&FrameEvent::Scroll {
            frame: id,
            dx: 0,
            dy: 150
        }),
        "{ev:?}"
    );
    assert_eq!(s.pane().origin, [0, 150]);
    s.idle(300);
    assert_eq!(s.pane().extent, [0, 169]);
    assert_eq!(s.f.text(id, 0, 20), d[..20].to_vec());
}

#[test]
fn output_added_to_a_nearly_empty_pane_is_an_edit() {
    let mut s = Sim::new();
    let mut lines = vec!["$ seq 1 10".to_string()];
    let screen = |lines: &[String]| {
        let mut refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        refs.resize(20, "");
        Grid::from_lines(COLS, &refs)
    };
    s.settle(&screen(&lines));
    let id = s.pane().id;
    lines.extend((1..=10).map(|i| i.to_string()));
    lines.push("$ ".into());
    let ev = s.feed(&screen(&lines), 300);
    assert!(
        !ev.iter().any(|e| matches!(e, FrameEvent::Jump { .. })),
        "{ev:?}"
    );
    assert_eq!((s.pane().id, s.pane().origin), (id, [0, 0]));
}

#[test]
fn the_alternate_screen_has_its_own_frames() {
    let d = doc(100);
    let other: Vec<String> = (0..100).map(|i| format!("editor {i:03}")).collect();
    let mut s = Sim::new();
    s.settle(&view(&d, 0, 20, &[]));
    let shell = s.pane().id;
    s.alt = true;
    s.feed_ev(&view(&other, 0, 20, &[]), 300, &[VtEvent::AltScreen(true)]);
    s.idle(2000);
    let editor = s.pane().id;
    assert_ne!(editor, shell);
    s.feed(&view(&other, 5, 20, &[]), 300);
    s.alt = false;
    s.feed_ev(&view(&d, 0, 20, &[]), 300, &[VtEvent::AltScreen(false)]);
    assert_eq!((s.pane().id, s.pane().origin), (shell, [0, 0]));
    // An empty alternate screen does not take the shell's frame either.
    s.alt = true;
    s.feed_ev(&Grid::new(COLS, 20), 300, &[VtEvent::AltScreen(true)]);
    assert_ne!(s.pane().id, shell);
    s.idle(300);
    assert_eq!(s.f.text(shell, 0, 20), d[..20].to_vec());
    assert_eq!(s.f.text(editor, 0, 25), other[..25].to_vec());
}

/// A pager on the alternate screen, painting as `less` 710 was recorded
/// to: a page down feeds lines at the bottom row, a page up reverse-indexes
/// at the top, a far jump forward feeds a marker and a screenful, a jump
/// to the top clears, a search repaints by cursor addressing.
struct Pager {
    vt: ptyrc_vt::Vt,
    s: Sim,
    doc: Vec<String>,
    top: usize,
    rows: usize,
    /// Standout `line N` prompt, as `man` shows; else `:`.
    status: bool,
}

impl Pager {
    fn open(doc: Vec<String>, rows: usize, status: bool) -> Pager {
        let mut p = Pager {
            vt: ptyrc_vt::Vt::new(COLS, rows as u16),
            s: Sim::new(),
            doc,
            top: 0,
            rows,
            status,
        };
        let mut out = format!("\x1b[?1049h\x1b[{rows};1H\r");
        out += &p.lines(0, rows - 1);
        out += &p.prompt();
        p.step(&out);
        p.s.idle(2000);
        p
    }

    fn lines(&self, from: usize, n: usize) -> String {
        (from..from + n)
            .map(|i| format!("{}\r\n", self.doc.get(i).map_or("~", String::as_str)))
            .collect()
    }

    fn prompt(&self) -> String {
        if self.status {
            format!(
                "\x1b[7m Manual page line {} (press h)\x1b[27m\x1b[K",
                self.top + 1
            )
        } else {
            ":\x1b[K".to_string()
        }
    }

    fn step(&mut self, bytes: &str) -> Vec<FrameEvent> {
        self.step_after(bytes, 300)
    }

    fn step_after(&mut self, bytes: &str, dt: u64) -> Vec<FrameEvent> {
        self.vt.feed(bytes.as_bytes());
        let c = self.vt.cursor();
        self.s.cursor = (c.col, c.row);
        self.s.alt = self.vt.modes().alt_screen;
        let events = self.vt.take_events();
        let grid = self.vt.grid().clone();
        self.s.feed_ev(&grid, dt, &events)
    }

    /// A page down that reaches the driver in two bursts, cut after `cut`
    /// lines.
    fn down_split(&mut self, n: usize, cut: usize) {
        let from = self.top + self.rows - 1;
        self.top += n;
        let first = format!("\r\x1b[K{}", self.lines(from, cut));
        let rest = format!("{}{}", self.lines(from + cut, n - cut), self.prompt());
        self.step(&first);
        self.step_after(&rest, 3);
    }

    /// A page down whose first burst only erases the prompt, as a key
    /// echo goes out alone.
    fn down_late(&mut self, n: usize) {
        let from = self.top + self.rows - 1;
        self.top += n;
        self.step("\r\x1b[K");
        let out = format!("{}{}", self.lines(from, n), self.prompt());
        self.step_after(&out, 3);
    }

    fn down(&mut self, n: usize) -> Vec<FrameEvent> {
        let from = self.top + self.rows - 1;
        self.top += n;
        let out = format!("\r\x1b[K{}{}", self.lines(from, n), self.prompt());
        self.step(&out)
    }

    fn up(&mut self, n: usize) -> Vec<FrameEvent> {
        let mut out = "\r\x1b[K".to_string();
        for i in (self.top - n..self.top).rev() {
            out += &format!("\x1b[H\x1bM{}\r\n", self.doc[i]);
        }
        self.top -= n;
        out += &format!("\x1b[{};1H\r\x1b[K{}", self.rows, self.prompt());
        self.step(&out)
    }

    /// `G`, `40g`: a marker line, then a screenful, all fed at the bottom.
    fn skip_to(&mut self, top: usize) -> Vec<FrameEvent> {
        self.top = top;
        let out = format!(
            "\r\x1b[K...skipping...\r\n{}{}",
            self.lines(top, self.rows - 1),
            self.prompt()
        );
        self.step(&out)
    }

    /// `g`: a clear, then the first screenful reverse-indexed in.
    fn jump_top(&mut self) -> Vec<FrameEvent> {
        self.top = 0;
        let mut out = "\r\x1b[K\x1b[H\x1b[2J".to_string();
        for i in (0..self.rows - 1).rev() {
            out += &format!("\x1b[H\x1bM{}\r\n", self.doc[i]);
        }
        out += &format!("\x1b[{};1H\r\x1b[K{}", self.rows, self.prompt());
        self.step(&out)
    }

    /// A search hit: every row addressed and rewritten.
    fn repaint_at(&mut self, top: usize) -> Vec<FrameEvent> {
        self.top = top;
        let mut out = String::new();
        for r in 0..self.rows - 1 {
            let text = self.doc.get(top + r).map_or("~", String::as_str);
            out += &format!("\x1b[{};1H\x1b[K{text}", r + 1);
        }
        out += &format!("\x1b[{};1H\r\x1b[K{}", self.rows, self.prompt());
        self.step(&out)
    }

    fn origin(&self) -> i64 {
        self.s.pane().origin[1]
    }
}

#[test]
fn a_pager_page_without_overlap_is_a_scroll() {
    // A page down feeds one line less than the screen has rows: every
    // line that went through is seen, the distance is exact.
    let mut p = Pager::open(doc(300), 20, false);
    let id = p.s.pane().id;
    let ev = p.down(19);
    assert!(ev.contains(&FrameEvent::Scroll {
        frame: id,
        dx: 0,
        dy: 19
    }));
    assert_eq!(p.origin(), 19);
    p.down(19);
    p.down(19);
    assert_eq!(p.origin(), 57);
    let ev = p.up(19);
    assert!(ev.contains(&FrameEvent::Scroll {
        frame: id,
        dx: 0,
        dy: -19
    }));
    p.up(19);
    p.up(19);
    assert_eq!((p.s.pane().id, p.origin()), (id, 0));
    p.s.idle(300);
    let info = p.s.pane();
    assert_eq!(info.extent[0], 0);
    assert!(info.extent[1] < 100, "no island: {info:?}");
    assert_eq!(p.s.f.text(id, 0, 76), p.doc[..76].to_vec());
}

#[test]
fn a_pager_page_in_two_bursts_keeps_the_prompt_out() {
    // The last row is blank between the two bursts: it says nothing of
    // the chrome, and the prompt is never remembered.
    for split in 1..=4 {
        for cut in [1, 7, 18] {
            let mut p = Pager::open(doc(300), 20, false);
            let id = p.s.pane().id;
            for page in 1..=4 {
                if page == split {
                    p.down_split(19, cut);
                } else {
                    p.down(19);
                }
            }
            p.s.idle(300);
            let info = p.s.pane();
            assert_eq!(info.origin[1], 76, "split {split} cut {cut}");
            assert_eq!(info.extent, [0, 94], "split {split} cut {cut}");
            assert_eq!(p.s.f.text(id, 0, 95), p.doc[..95].to_vec());
        }
    }
    // The prompt erased alone, then the page; pages up tell the chrome
    // as well.
    for late in 1..=4 {
        let mut p = Pager::open(doc(300), 20, false);
        let id = p.s.pane().id;
        for page in 1..=4 {
            if page == late {
                p.down_late(19);
            } else {
                p.down(19);
            }
        }
        p.s.idle(300);
        assert_eq!(p.s.pane().extent, [0, 94], "late {late}");
        p.up(19);
        p.s.idle(300);
        assert_eq!(p.s.pane().extent, [0, 94], "late {late}");
        assert_eq!(p.s.f.text(id, 0, 95), p.doc[..95].to_vec());
    }
    let mut p = Pager::open(doc(300), 20, false);
    p.down_late(19);
    p.down(19);
    p.up(19);
    p.s.idle(300);
    assert_eq!(p.s.pane().extent, [0, 56]);
}

#[test]
fn pager_pages_in_one_burst_are_a_scroll() {
    // A key held: two or three pages reach the driver at once. Each is
    // the screen less the prompt row; a jump feeds a whole screen.
    let mut p = Pager::open(doc(300), 20, false);
    let id = p.s.pane().id;
    p.down(19);
    p.down(19);
    let from = p.top + p.rows - 1;
    p.top += 38;
    let out = format!(
        "\r\x1b[K{}{}\r\x1b[K{}{}",
        p.lines(from, 19),
        p.prompt(),
        p.lines(from + 19, 19),
        p.prompt()
    );
    let ev = p.step(&out);
    assert!(ev.contains(&FrameEvent::Scroll {
        frame: id,
        dx: 0,
        dy: 38
    }));
    assert_eq!((p.s.pane().id, p.origin()), (id, 76));
    let mut out = String::new();
    for _ in 0..3 {
        out += "\r\x1b[K";
        for i in (p.top - 19..p.top).rev() {
            out += &format!("\x1b[H\x1bM{}\r\n", p.doc[i]);
        }
        p.top -= 19;
        out += &format!("\x1b[{};1H\r\x1b[K{}", p.rows, p.prompt());
    }
    p.step(&out);
    assert_eq!(p.origin(), 19);
    p.s.idle(300);
    let info = p.s.pane();
    assert_eq!(info.extent, [0, 94], "no island: {info:?}");
    // Rows that went through unseen are not known.
    assert_eq!(p.s.f.text(id, 19, 38), p.doc[19..38].to_vec());
    assert_eq!(p.s.f.text(id, 76, 95), p.doc[76..95].to_vec());
    assert!(p.s.f.text(id, 60, 70).iter().all(String::is_empty));
}

#[test]
fn a_pager_jump_is_not_a_scroll() {
    // A far jump feeds a whole screen or more: a line went through unseen.
    // The view goes to an island; a clear and a repaint find the top again.
    let mut p = Pager::open(doc(300), 20, false);
    let id = p.s.pane().id;
    p.down(19);
    p.skip_to(250);
    assert_eq!(p.s.pane().id, id);
    assert!(p.origin() >= ext::ISLAND_GAP);
    p.jump_top();
    assert_eq!(p.origin(), 0);
    p.skip_to(250);
    assert!(p.origin() >= ext::ISLAND_GAP);
    p.repaint_at(10);
    assert_eq!(p.origin(), 10);
}

#[test]
fn a_pager_jump_fed_in_pieces_is_not_a_scroll() {
    // The feed of a whole screen cut in two or three bursts: each piece
    // is shorter than the screen, their sum is not.
    for cuts in [vec![1], vec![7], vec![19], vec![5, 12]] {
        let mut p = Pager::open(doc(300), 20, false);
        let id = p.s.pane().id;
        p.down(19);
        p.top = 250;
        let mut lines: Vec<String> = vec!["...skipping...\r\n".into()];
        lines.extend((250..269).map(|i| format!("{}\r\n", p.doc[i])));
        let mut from = 0;
        for (k, cut) in cuts.iter().chain([&20]).enumerate() {
            let mut out = if k == 0 {
                "\r\x1b[K".to_string()
            } else {
                String::new()
            };
            out += &lines[from..*cut].concat();
            if *cut == 20 {
                out += &p.prompt();
            }
            from = *cut;
            p.step_after(&out, if k == 0 { 300 } else { 2 });
        }
        assert_eq!(p.s.pane().id, id);
        assert!(
            p.origin() >= ext::ISLAND_GAP,
            "cuts {cuts:?}: {}",
            p.origin()
        );
        p.jump_top();
        assert_eq!(p.origin(), 0);
        p.s.idle(300);
        assert_eq!(p.s.f.text(id, 0, 38), p.doc[..38].to_vec(), "cuts {cuts:?}");
    }
    // Lines fed one by one as a key repeats, however many, are scrolls.
    let mut p = Pager::open(doc(300), 20, false);
    for n in 1..=45 {
        p.step_after(&format!("\r\x1b[K{}{}", p.lines(18 + n, 1), p.prompt()), 30);
        p.top += 1;
        assert_eq!(p.origin(), n as i64);
    }
}

#[test]
fn a_clear_and_its_repaint_in_two_bursts() {
    // The clear comes alone; the repaint that follows is fed by reverse
    // indexes, fewer than a screen: not a scroll, the rows are looked up.
    let mut p = Pager::open(doc(300), 20, false);
    let id = p.s.pane().id;
    p.down(19);
    p.down(19);
    p.s.idle(300);
    p.step("\r\x1b[K\x1b[H\x1b[J");
    assert!(p.origin() >= ext::ISLAND_GAP);
    let mut out = String::new();
    for i in (0..19).rev() {
        out += &format!("\x1b[H\x1bM{}\r\n", p.doc[i]);
    }
    out += "\x1b[20;1H\r\x1b[K:\x1b[K";
    p.vt.feed(out.as_bytes());
    let events = p.vt.take_events();
    let grid = p.vt.grid().clone();
    p.s.feed_ev(&grid, 5, &events);
    assert_eq!((p.s.pane().id, p.origin()), (id, 0));
    p.top = 0;
    p.down(19);
    assert_eq!(p.origin(), 19);
    p.s.idle(300);
    assert_eq!(p.s.pane().extent, [0, 56]);
}

#[test]
fn a_status_row_places_a_pager_jump() {
    // `line N` in a standout prompt under the cursor: rows are absolute.
    let mut p = Pager::open(doc(300), 20, true);
    let id = p.s.pane().id;
    p.down(19);
    assert_eq!(p.origin(), 19);
    let ev = p.skip_to(250);
    assert!(ev.contains(&FrameEvent::Jump { frame: id }));
    assert_eq!(p.origin(), 250);
    p.up(19);
    assert_eq!(p.origin(), 231);
    p.repaint_at(120);
    assert_eq!(p.origin(), 120);
    p.jump_top();
    assert_eq!((p.s.pane().id, p.origin()), (id, 0));
    p.s.idle(300);
    assert_eq!(p.s.f.text(id, 231, 269), p.doc[231..269].to_vec());
    assert_eq!(p.s.pane().extent, [0, 268]);
}

#[test]
fn a_pager_jump_fed_in_two_bursts_is_placed_by_the_status_row() {
    // The marker and a few lines, then the rest: two feeds that look like
    // scrolls. The status row says further: a jump.
    let mut p = Pager::open(doc(300), 20, true);
    let id = p.s.pane().id;
    p.down(19);
    let first = format!("\r\x1b[K...skipping...\r\n{}", p.lines(250, 6));
    p.step(&first);
    p.top = 250;
    let rest = format!("{}{}", p.lines(256, 13), p.prompt());
    let ev = p.step(&rest);
    assert!(ev.contains(&FrameEvent::Jump { frame: id }));
    assert_eq!(p.origin(), 250);
}

#[test]
fn a_pager_jump_fed_in_pieces_leaves_no_marker() {
    // With a status row the jump is placed; the marker line that went
    // through between two bursts is not remembered.
    for cut in [1, 7, 19] {
        let mut p = Pager::open(doc(300), 20, true);
        let id = p.s.pane().id;
        p.down(19);
        p.top = 250;
        let mut lines: Vec<String> = vec!["...skipping...\r\n".into()];
        lines.extend((250..269).map(|i| format!("{}\r\n", p.doc[i])));
        p.step(&format!("\r\x1b[K{}", lines[..cut].concat()));
        p.step_after(&format!("{}{}", lines[cut..].concat(), p.prompt()), 2);
        assert_eq!(p.origin(), 250, "cut {cut}");
        p.jump_top();
        p.s.idle(300);
        let known = p.s.f.find(id, &lit("skipping"), true);
        assert!(known.is_empty(), "cut {cut}: {known:?}");
    }
}

#[test]
fn a_status_row_that_counts_wrapped_lines_is_dropped() {
    // The number moves less than the rows do: it counts file lines. It is
    // not read any more; the frame keeps screen rows.
    let mut p = Pager::open(doc(300), 20, true);
    p.down(5);
    assert_eq!(p.origin(), 5);
    p.top -= 2;
    p.down(5);
    assert_eq!(p.origin(), 10);
    p.down(19);
    assert_eq!(p.origin(), 29);
    p.skip_to(250);
    assert!(p.origin() >= ext::ISLAND_GAP);
}

#[test]
fn status_rows_are_parsed() {
    assert_eq!(
        status_line(" Manual page bash(1) line 30 (press h for help)"),
        Some(29)
    );
    assert_eq!(status_line("lines 30-58/300 9%"), Some(29));
    assert_eq!(status_line("long.txt line 6180/6208 (END)"), Some(6179));
    assert_eq!(status_line("outline 3"), None);
    assert_eq!(status_line("line 3rd"), None);
    assert_eq!(status_line("line: 3"), None);
    assert_eq!(status_line("--More--(20%)"), None);
}

#[test]
fn a_monitor_is_rewritten_in_place() {
    // A pane repainted whole every second under a status line that ticks:
    // two frames are spent, then it is one frame, stable.
    let mut s = Sim::new();
    let screen = |n: usize| {
        let lines: Vec<String> = (0..23)
            .map(|i| {
                format!(
                    "{:5} proc-{} {}.{}%",
                    n * 97 + i * 13,
                    (n * 7 + i * 3) % 50,
                    n + i,
                    n % 10
                )
            })
            .collect();
        let mut refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let status = format!("load {n}");
        refs.push(&status);
        let mut g = Grid::from_lines(COLS, &refs);
        bg(&mut g, Rect::new(0, 23, COLS, 1), 2);
        g
    };
    s.settle(&screen(0));
    let mut seen = vec![s.pane().id];
    for n in 1..12 {
        s.feed(&screen(n), 1000);
        seen.push(s.pane().id);
    }
    seen.dedup();
    assert!(seen.len() <= 3, "{seen:?}");
    assert!(s.pane().usable, "stable after the churn");
    // A lone switch later is still a switch.
    s.idle(5000);
    let id = s.pane().id;
    let d = doc(100);
    let mut g = view(&d, 0, 23, &["other window"]);
    bg(&mut g, Rect::new(0, 23, COLS, 1), 2);
    s.feed(&g, 10);
    assert_ne!(s.pane().id, id);
}

#[test]
fn a_slow_monitor_with_a_cursor_spends_two_islands() {
    // A table repainted every 5 s with the cursor shown, left on the same
    // cell: two repaints may be an editor's jumps, the third, as long
    // after the second as the second after the first, is a monitor's.
    let table = |n: usize| {
        let lines: Vec<String> = (0..20)
            .map(|i| {
                format!(
                    "{:5} proc-{} {}.{}%",
                    n * 97 + i * 13,
                    (n * 7 + i * 3) % 50,
                    n + i,
                    n % 10
                )
            })
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        Grid::from_lines(COLS, &refs)
    };
    let mut s = Sim::new();
    s.cursor = (0, 20);
    s.settle(&table(0));
    let id = s.pane().id;
    s.feed(&table(1), 5000);
    s.feed(&table(2), 5050);
    let second = s.pane().origin[1];
    for n in 3..9 {
        s.feed(&table(n), 4950 + 20 * n as u64);
        assert_eq!(
            (s.pane().id, s.pane().origin[1]),
            (id, second),
            "refresh {n}"
        );
    }
    assert!(s.pane().usable);

    // The cursor moves with each repaint, or the repaints come as keys
    // do: jumps in a table, an island each.
    for moving in [true, false] {
        let mut s = Sim::new();
        s.cursor = (0, 3);
        s.settle(&table(0));
        let mut origins = vec![s.pane().origin[1]];
        for (n, dt) in [4000, 9000, 5000, 16000].into_iter().enumerate() {
            if moving {
                s.cursor = (n as u16 + 1, 4 + n as u16);
            }
            s.feed(&table(n + 1), if moving { 5000 } else { dt });
            origins.push(s.pane().origin[1]);
        }
        origins.dedup();
        assert_eq!(origins.len(), 5, "{origins:?}");
    }
}

/// A process monitor: meters, a header bar, a table sorted anew at each
/// refresh `n`, shown from row `first` of its list.
fn monitor(n: usize, first: usize) -> Grid {
    let mut lines: Vec<String> = (0..4)
        .map(|i| {
            let load = (n * 37 + i * 11) % 100;
            format!("  {i}[{:<20}{load:5}.{}%]", "|".repeat(load / 5), n % 10)
        })
        .collect();
    lines.push(format!("  Tasks: {}, {} running", 400 + n, n % 7));
    lines.push("    PID USER   PRI  VIRT   RES  CPU%  TIME+  Command".into());
    for k in first..first + 14 {
        let p = (k * 7 + n * 5) % 23;
        lines.push(format!(
            "{:7} user    20 {:5}M {:4}M {:3}.{} 0:{:02}.{:02} proc-{}",
            1000 + p * 131,
            100 + p * 17,
            10 + p * 3,
            (p * 9 + n) % 100,
            (p + n) % 10,
            (p + n) % 60,
            (p * 3 + n * 7) % 100,
            p % 9
        ));
    }
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut g = Grid::from_lines(COLS, &refs);
    bg(&mut g, Rect::new(0, 5, COLS, 1), 2);
    g
}

#[test]
fn a_monitor_keeps_its_frames_from_the_first_refresh() {
    // No cursor, cells rewritten where they are, the same shape with
    // other values: the panes are repainted, not replaced.
    let mut s = Sim::new();
    s.hidden = true;
    s.feed(&monitor(0, 0), 10);
    let ids: Vec<FrameId> = s.panes().iter().map(|p| p.id).collect();
    let rects = s.rects();
    assert_eq!(rects, vec![[0, 0, COLS, 5], [0, 6, COLS, 14]]);
    for n in 1..8 {
        let ev = s.feed(&monitor(n, 0), 1500);
        assert!(
            !ev.iter().any(|e| matches!(
                e,
                FrameEvent::Added { .. } | FrameEvent::Jump { .. } | FrameEvent::Scroll { .. }
            )),
            "refresh {n}: {ev:?}"
        );
        assert_eq!(s.panes().iter().map(|p| p.id).collect::<Vec<_>>(), ids);
        assert_eq!(s.rects(), rects);
        assert!(s.panes().iter().all(|p| p.usable && p.origin == [0, 0]));
    }
    // What it shows is what is remembered.
    s.idle(500);
    let text = s.f.text(ids[1], 0, 1);
    assert_eq!(text[0], s.f.text(RAW, 6, 7)[0]);
    // Another document in its place is still another frame.
    let d = doc(100);
    s.feed(&view(&d, 0, 20, &[]), 4000);
    s.idle(300);
    assert!(s.panes().iter().all(|p| !ids.contains(&p.id)));
}

#[test]
fn a_monitor_that_scrolls_its_list_keeps_its_rows() {
    // The list follows the selection: a scroll of the table by the
    // emulator, and the rows sorted anew. Rows that did not move are not
    // chrome.
    let mut s = Sim::new();
    s.hidden = true;
    s.feed(&monitor(0, 0), 10);
    let rects = s.rects();
    let table = Rect::new(0, 6, COLS, 14);
    s.feed(&monitor(1, 0), 1500);
    let mut first = 0;
    for n in 2..9 {
        first += 3;
        let ev = [VtEvent::Scroll {
            region: table,
            dy: -3,
            scrollback: false,
        }];
        s.feed_ev(&monitor(n, first), 1500, &ev);
        assert_eq!(s.rects(), rects, "refresh {n}");
        assert!(s.panes().iter().all(|p| p.usable));
    }
}

#[test]
fn rows_the_emulator_scrolled_are_not_chrome() {
    // A scroll by the emulator over the whole pane; the first rows are
    // repainted with what they showed (a sticky header would be outside
    // the region).
    let mut s = Sim::new();
    let d = doc(200);
    let screen = |top: usize| {
        let mut lines: Vec<&str> = vec![&d[0], &d[1]];
        lines.extend(d.iter().skip(top + 2).take(18).map(String::as_str));
        Grid::from_lines(COLS, &lines)
    };
    s.settle(&screen(0));
    let all = Rect::new(0, 0, COLS, 20);
    for top in 1..6 {
        let ev = [VtEvent::Scroll {
            region: all,
            dy: -1,
            scrollback: false,
        }];
        s.feed_ev(&screen(top), 300, &ev);
    }
    assert_eq!(s.pane().rect, [0, 0, COLS, 20]);
    // Without the emulator's word the same rows are chrome.
    let mut s = Sim::new();
    s.settle(&screen(0));
    for top in 1..6 {
        s.feed(&screen(top), 300);
    }
    assert_eq!(s.pane().rect, [0, 2, COLS, 18]);
}

struct Declared;

impl TuiSupport for Declared {
    fn name(&self) -> &'static str {
        "declared"
    }

    fn detect(&self, _pane: &PaneView) -> f32 {
        1.0
    }

    fn tuning(&self, _pane: &PaneView) -> crate::plugin::Tuning {
        crate::plugin::Tuning {
            monitor: true,
            ..Default::default()
        }
    }
}

#[test]
fn a_preset_declares_a_pane_rewritten_in_place() {
    // Whole rewrites of other shapes, under a cursor, some of them
    // shifted: nothing but edits.
    let mut s = Sim::with(FrameConfig::default(), vec![Box::new(Declared)]);
    let d = doc(400);
    s.settle(&view(&d, 0, 20, &[]));
    let id = s.pane().id;
    for top in [200, 3, 120, 121, 123, 300] {
        let ev = s.feed(&view(&d, top, 20, &[]), 700);
        assert!(
            !ev.iter().any(|e| matches!(
                e,
                FrameEvent::Added { .. }
                    | FrameEvent::Jump { .. }
                    | FrameEvent::Scroll { .. }
                    | FrameEvent::Rect { .. }
            )),
            "top {top}: {ev:?}"
        );
        let p = s.pane();
        assert_eq!((p.id, p.origin, p.usable), (id, [0, 0], true));
    }
    s.idle(500);
    assert_eq!(s.f.text(id, 0, 1), vec![d[300].clone()]);
}

#[test]
fn rows_under_an_overlay_do_not_vote() {
    // A palette opens over the only rows that hold text: the pane under
    // it does not jump, and finds its rows again when it closes.
    let mut s = Sim::new();
    let d = doc(40);
    let home = || {
        let mut g = Grid::new(COLS, 24);
        for (i, l) in d.iter().take(6).enumerate() {
            put(&mut g, 4, 8 + i as u16, l);
        }
        g
    };
    let palette = || {
        let mut g = home();
        let r = Rect::new(10, 6, 40, 12);
        for y in r.y..r.bottom() {
            put(&mut g, r.x, y, &" ".repeat(r.w as usize));
            put(&mut g, r.x + 2, y, &format!("command {y}"));
        }
        bg(&mut g, r, 8);
        g
    };
    s.settle(&home());
    let under = s.pane();
    let mut ev = s.feed(&palette(), 1000);
    ev.extend(s.idle(2000));
    assert!(
        !ev.iter().any(|e| matches!(e, FrameEvent::Jump { .. })),
        "{ev:?}"
    );
    let panes = s.panes();
    assert_eq!(panes.len(), 2, "{panes:#?}");
    let still = panes.iter().find(|p| p.id == under.id).expect("kept");
    assert_eq!((still.origin, still.extent), (under.origin, under.extent));
    assert!(panes.iter().any(|p| p.rect == [10, 6, 40, 12]));
    // What the palette hides is still known.
    assert_eq!(s.f.text(under.id, 8, 9)[0].trim(), d[0]);
    let mut ev = s.feed(&home(), 1000);
    ev.extend(s.idle(2000));
    assert!(
        !ev.iter().any(|e| matches!(e, FrameEvent::Jump { .. })),
        "{ev:?}"
    );
    let back = s.pane();
    assert_eq!(
        (back.id, back.origin, back.usable),
        (under.id, under.origin, true)
    );
}

#[test]
fn a_pane_first_seen_under_an_overlay_does_not_take_its_rows() {
    // The app starts with its palette open: the rows it hides are not
    // known, and are not the palette's.
    let mut s = Sim::new();
    let d = doc(40);
    let home = || {
        let mut g = Grid::new(COLS, 24);
        for (i, l) in d.iter().take(14).enumerate() {
            put(&mut g, 4, 2 + i as u16, l);
        }
        g
    };
    let palette = || {
        let mut g = home();
        let r = Rect::new(10, 6, 40, 8);
        for y in r.y..r.bottom() {
            put(&mut g, r.x, y, &" ".repeat(r.w as usize));
            put(&mut g, r.x + 2, y, &format!("command {y}"));
        }
        bg(&mut g, r, 8);
        g
    };
    s.settle(&palette());
    let panes = s.panes();
    assert_eq!(panes.len(), 2, "{panes:#?}");
    let under = panes.iter().find(|p| p.rect[3] == 24).expect("under");
    let text = s.f.text(under.id, 0, 24);
    assert_eq!(text[2].trim(), d[0]);
    assert!(text[6..14].iter().all(String::is_empty), "{text:?}");
    assert!(s.f.find(under.id, &lit("command"), true).is_empty());
    let mut ev = s.feed(&home(), 1000);
    ev.extend(s.idle(2000));
    assert!(
        !ev.iter().any(|e| matches!(e, FrameEvent::Jump { .. })),
        "{ev:?}"
    );
    let back = s.pane();
    assert_eq!((back.id, back.origin), (under.id, under.origin));
    let text = s.f.text(under.id, 0, 24);
    for i in 0..14 {
        assert_eq!(text[2 + i].trim(), d[i]);
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
            "rect": [0, 0, 60, 20], "origin": [0, 0], "extent": [0, 19], "key": null, "support": null,
            "idle": 0.0, "is_idle": false})
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
        // Slower than a monitor repaints: these are switches.
        s.feed(&g, 4000);
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
fn resized_pane_with_little_text_keeps_its_frame() {
    // An empty shell: one prompt row cannot tell the pane by content.
    let shell = |cols: u16, rows: u16| {
        let mut g = Grid::new(cols, rows);
        put(&mut g, 0, 0, "$ ");
        g
    };
    let mut s = Sim::new();
    s.settle(&shell(COLS, 14));
    let id = s.pane().id;
    let ev = s.feed(&shell(COLS - 20, 10), 100);
    assert!(ev.contains(&FrameEvent::Attached { frame: id }), "{ev:?}");
    assert_eq!(s.pane().id, id);
    assert_eq!(s.panes().len(), 1);
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

/// Counts how often it is asked; knows a screen by its first row.
struct Counting(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl TuiSupport for Counting {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if pane.row_text(0).starts_with("APP") {
            1.0
        } else {
            0.0
        }
    }
}

#[test]
fn presets_are_not_looked_for_on_every_key() {
    use std::sync::atomic::Ordering;
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut s = Sim::with(
        FrameConfig::default(),
        vec![Box::new(Counting(calls.clone()))],
    );
    let d = doc(40);
    let screen = |typed: &str| {
        let mut g = view(&d, 0, 19, &[""]);
        put(&mut g, 0, 19, typed);
        g
    };
    s.settle(&screen("$"));
    assert_eq!(s.pane().support, None);
    // Keys echoed on one row, 10 ms apart: asked on the first, the
    // last time being two seconds ago, then not.
    s.feed(&screen("$ w"), 10);
    let before = calls.load(Ordering::Relaxed);
    for n in 1..=20 {
        s.feed(&screen(&format!("$ {}", "x".repeat(n))), 10);
    }
    assert_eq!(calls.load(Ordering::Relaxed), before);
    // A second later one key asks again, once for the screen.
    s.feed(&screen("$ y"), 1100);
    assert_eq!(calls.load(Ordering::Relaxed), before + 1);
    // An app that paints several rows is seen in the same burst.
    let mut g = screen("$ y");
    put(&mut g, 0, 0, "APP header                  ");
    put(&mut g, 0, 1, "second row                  ");
    s.feed(&g, 10);
    assert_eq!(s.pane().support.as_deref(), Some("counting"));
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

/// An app told by `MILLER` on its first row: a title row, a list and a
/// preview, side by side with no rule.
struct Miller;

impl Miller {
    fn area(pane: &PaneView) -> Option<Rect> {
        [pane.rect, pane.area].into_iter().find(|a| {
            let row: String = (a.x..a.right())
                .map(|x| pane.grid.cell(x, a.y).text.as_str())
                .collect();
            row.starts_with("MILLER")
        })
    }

    fn parts(a: Rect) -> [Rect; 3] {
        let w = a.w / 2;
        [
            Rect::new(a.x, a.y, a.w, 1),
            Rect::new(a.x, a.y + 1, w, a.h - 1),
            Rect::new(a.x + w, a.y + 1, a.w - w, a.h - 1),
        ]
    }
}

impl TuiSupport for Miller {
    fn name(&self) -> &'static str {
        "miller"
    }

    fn detect(&self, pane: &PaneView) -> f32 {
        if Miller::area(pane).is_some() {
            1.0
        } else {
            0.0
        }
    }

    fn frame_key(&self, pane: &PaneView) -> Option<String> {
        let a = Miller::area(pane)?;
        (pane.rect == Miller::parts(a)[1]).then(|| "miller:list".into())
    }

    fn segment(&self, area: &PaneView) -> Option<Vec<Rect>> {
        let a = Miller::area(area).filter(|a| *a == area.rect)?;
        Some(Miller::parts(a).to_vec())
    }

    fn tuning(&self, pane: &PaneView) -> crate::plugin::Tuning {
        crate::plugin::Tuning {
            monitor: Miller::area(pane).is_some_and(|a| pane.rect == Miller::parts(a)[2]),
            ..Default::default()
        }
    }
}

/// A rule down column 30; `MILLER` right of it, showing `list` and
/// `preview` from their tops.
fn beside(d: &[String], list: usize, preview: usize) -> Grid {
    let mut g = view(d, 0, 20, &[]);
    for y in 0..20 {
        put(&mut g, 29, y, " │");
        put(&mut g, 31, y, &" ".repeat(29));
    }
    put(&mut g, 31, 0, "MILLER");
    for y in 1..20usize {
        put(&mut g, 31, y as u16, &format!("entry {:03}", list + y));
        put(&mut g, 45, y as u16, &format!("word {:03} x", preview + y));
    }
    g
}

#[test]
fn a_preset_splits_a_pane_of_another_app() {
    let mut s = Sim::with(FrameConfig::default(), vec![Box::new(Miller)]);
    let d = doc(400);
    s.settle(&beside(&d, 0, 0));
    assert_eq!(
        s.rects(),
        vec![[0, 0, 30, 20], [31, 1, 14, 19], [45, 1, 15, 19]]
    );
    let p = s.panes();
    assert_eq!(p[0].support, None);
    assert_eq!(p[1].support.as_deref(), Some("miller"));
    assert_eq!(p[1].key.as_deref(), Some("miller:list"));
    assert_eq!(p[2].key, None);
    // The title row is chrome: one row is below a pane.
    assert!(s.f.list().iter().any(|f| {
        f.kind == FrameKind::Chrome && f.state != FrameState::Inactive && f.rect == [31, 0, 29, 1]
    }));
    // The tuning is the pane's: the preview is rewritten in place, the
    // list scrolls.
    let (list, preview) = (p[1].id, p[2].id);
    for (top, shown) in [(1, 150), (2, 7), (3, 300)] {
        s.feed(&beside(&d, top, shown), 700);
        let p = s.panes();
        assert_eq!((p[1].id, p[1].origin), (list, [0, top as i64]));
        assert_eq!((p[2].id, p[2].origin), (preview, [0, 0]));
    }
    // The app leaves: the pane is whole again.
    let mut g = view(&d, 0, 20, &[]);
    for y in 0..20 {
        put(&mut g, 29, y, " │");
        put(&mut g, 31, y, &" ".repeat(29));
        put(&mut g, 31, y, "$");
    }
    s.settle(&g);
    assert_eq!(s.rects(), vec![[0, 0, 30, 20], [31, 0, 29, 20]]);
}
