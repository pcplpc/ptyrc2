// SPDX-License-Identifier: GPL-3.0-or-later
//! Heat: visible change, background against foreground, idle.

use super::*;
use ptyrc_vt::{Attrs, Cell, Color, Style};

const COLS: u16 = 40;
const ROWS: u16 = 12;

struct Sim {
    f: Frames,
    now: Duration,
}

impl Sim {
    fn new() -> Sim {
        Sim {
            f: Frames::new(FrameConfig::default(), Vec::new()),
            now: Duration::ZERO,
        }
    }

    /// Let `dt` ms pass, ticking every 100 ms, then show `g`.
    fn feed(&mut self, g: &Grid, dt: u64) {
        self.wait(dt);
        let input = ScreenInput {
            grid: g,
            cursor: Cursor {
                col: 0,
                row: 0,
                visible: true,
                pending_wrap: false,
            },
            alt_screen: false,
            title: "",
            events: &[],
        };
        self.f.update(&input, self.now);
    }

    fn wait(&mut self, dt: u64) {
        let end = self.now + Duration::from_millis(dt);
        while self.now + Duration::from_millis(100) < end {
            self.now += Duration::from_millis(100);
            self.f.tick(self.now);
        }
        self.now = end;
        self.f.tick(end);
    }

    fn heat(&self) -> HeatView {
        self.f.heat(RAW).unwrap()
    }
}

fn at(h: &HeatView, x: u16, y: u16) -> usize {
    y as usize * h.cols as usize + x as usize
}

fn put(g: &mut Grid, x: u16, y: u16, text: &str) {
    for (i, ch) in text.chars().enumerate() {
        let style = g.cell(x + i as u16, y).style;
        g.put(x + i as u16, y, Cell::narrow(&ch.to_string(), style));
    }
}

fn fill(g: &mut Grid, r: Rect, style: Style) {
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            g.put(x, y, Cell::blank(style));
        }
    }
}

fn grey() -> Style {
    Style {
        bg: Color::Indexed(236),
        ..Style::default()
    }
}

fn page() -> Grid {
    let mut g = Grid::new(COLS, ROWS);
    for y in 0..6 {
        put(&mut g, 0, y, &format!("row {y} of a static page"));
    }
    g
}

#[test]
fn clear_then_identical_redraw_is_no_event() {
    let mut s = Sim::new();
    let g = page();
    s.feed(&g, 0);
    s.wait(20_000);
    // The app clears the screen in one burst and redraws it in the next.
    s.feed(&Grid::new(COLS, ROWS), 0);
    let h = s.heat();
    assert_eq!(h.class[at(&h, 2, 2)], HeatClass::Hot);
    s.feed(&g, 60);
    let h = s.heat();
    let i = at(&h, 2, 2);
    assert_eq!(h.class[i], HeatClass::Text);
    assert!(h.activity[i] < 1e-3, "{}", h.activity[i]);
    assert!(h.still[i] >= 20.0, "{}", h.still[i]);
    assert!(h.idle().idle);
    // A redraw that comes late is a change.
    s.feed(&Grid::new(COLS, ROWS), 1000);
    s.feed(&g, 1000);
    let h = s.heat();
    assert_eq!(h.class[at(&h, 2, 2)], HeatClass::Hot);
    assert_eq!(h.class[at(&h, 30, 10)], HeatClass::Free);
}

#[test]
fn clearing_a_clean_area_is_no_event() {
    let mut s = Sim::new();
    let mut g = page();
    s.feed(&g, 0);
    s.wait(12_000);
    // Erase the empty lower half with another pen, redraw the text bold
    // then as it was: the first changes nothing, the second is undone.
    let pen = Style {
        fg: Color::Indexed(3),
        attrs: Attrs(Attrs::BOLD),
        ..Style::default()
    };
    fill(&mut g, Rect::new(0, 6, COLS, 6), pen);
    s.feed(&g, 100);
    let h = s.heat();
    assert!(h.activity.iter().all(|a| *a < 1e-3));
    assert_eq!(h.idle().score, 1.0);
    // The text of a row redrawn identically.
    put(&mut g, 0, 1, "row 1 of a static page");
    s.feed(&g, 100);
    assert_eq!(s.heat().idle().score, 1.0);
}

#[test]
fn a_panel_is_background_of_another_colour() {
    let mut s = Sim::new();
    let mut g = page();
    let panel = Rect::new(20, 6, 20, 6);
    fill(&mut g, panel, grey());
    put(&mut g, 22, 7, "title");
    s.feed(&g, 0);
    s.wait(12_000);
    let h = s.heat();
    assert_eq!(h.class[at(&h, 30, 10)], HeatClass::Panel);
    assert_eq!(h.class[at(&h, 23, 7)], HeatClass::Text);
    assert_eq!(h.class[at(&h, 5, 10)], HeatClass::Free);
    assert_eq!(h.class[at(&h, 1, 3)], HeatClass::Text);
    // Not occupied: as safe as the empty area next to it.
    assert!(h.occupancy[at(&h, 30, 10)] < 1e-3);
    assert!(h.safe_at(30, 10) > 0.99);
    assert!(h.safe_at(23, 7) < 0.01);
    let c = h.classes();
    assert_eq!(&c[10][18..22], "..pp");
    assert_eq!(&c[7][21..28], "ptttttp");
    // The panel grows over most of the screen: it becomes the backdrop.
    fill(&mut g, Rect::new(0, 0, COLS, ROWS), grey());
    fill(&mut g, Rect::new(0, 0, 10, 2), Style::default());
    s.feed(&g, 100);
    s.wait(12_000);
    let h = s.heat();
    assert_eq!(h.class[at(&h, 30, 10)], HeatClass::Free);
    assert_eq!(h.class[at(&h, 3, 1)], HeatClass::Panel);
}

#[test]
fn idle_after_ten_seconds() {
    let mut s = Sim::new();
    let g = page();
    s.feed(&g, 0);
    assert_eq!(s.heat().idle().score, 0.0);
    s.wait(9_000);
    let i = s.heat().idle();
    assert!(!i.idle && i.score == 0.0, "{i:?}");
    // Time passes without a tick.
    let later = s.now + Duration::from_millis(1_500);
    let i = s.f.idle(RAW, later).unwrap();
    assert!(i.idle && i.score == 1.0, "{i:?}");
    let inked = (6 * 17) as f32 / (COLS * ROWS) as f32;
    assert!((i.free - (1.0 - inked)).abs() < 1e-3, "{i:?}");
    let info = &s.f.list_at(later)[0];
    assert!(info.is_idle && info.idle == 1.0);
    assert!(!s.f.list()[0].is_idle);
    // Typing on one row: the rest stays idle.
    s.wait(2_000);
    let mut g = g;
    put(&mut g, 0, 8, "typed text");
    s.feed(&g, 0);
    let i = s.heat().idle();
    assert!(i.idle && i.score < 1.0, "{i:?}");
    // A full repaint with other content: nothing is idle.
    let mut g2 = Grid::new(COLS, ROWS);
    for y in 0..ROWS {
        put(&mut g2, 0, y, &"x".repeat(COLS as usize));
    }
    s.feed(&g2, 100);
    let i = s.heat().idle();
    assert!(!i.idle && i.score == 0.0, "{i:?}");
    let v = serde_json::to_value(&s.f.list()[0]).unwrap();
    assert_eq!(v["is_idle"], false);
    assert_eq!(v["idle"], 0.0);
}

#[test]
fn a_monitor_is_hot_where_its_digits_change() {
    let mut s = Sim::new();
    let table = |n: usize| {
        let mut g = Grid::new(COLS, ROWS);
        fill(&mut g, Rect::new(0, 0, COLS, 1), grey());
        put(&mut g, 0, 0, "  PID  CPU% COMMAND");
        for y in 1..ROWS {
            let cpu = (n * 7 + y as usize * 13) % 90 + 10;
            put(&mut g, 0, y, &format!("{:5}  {cpu}.0 task-{y}", 100 + y));
        }
        g
    };
    for n in 0..30 {
        // Each refresh clears the screen, then paints every row again.
        s.feed(&Grid::new(COLS, ROWS), 990);
        s.feed(&table(n), 10);
    }
    let h = s.heat();
    for y in 1..ROWS {
        assert_eq!(h.class[at(&h, 7, y)], HeatClass::Hot, "row {y}");
        assert_eq!(h.class[at(&h, 3, y)], HeatClass::Text, "row {y}");
        assert_eq!(h.class[at(&h, 14, y)], HeatClass::Text, "row {y}");
        assert_eq!(h.class[at(&h, 30, y)], HeatClass::Free, "row {y}");
    }
    assert_eq!(h.class[at(&h, 30, 0)], HeatClass::Panel);
    assert!(h.safe_at(30, 5) > 0.99);
    let i = h.idle();
    assert!(i.idle && i.score < 0.96, "{i:?}");
}

#[test]
fn the_idle_name_is_the_calm_pane() {
    let mut s = Sim::new();
    let mut g = Grid::new(COLS, ROWS);
    for y in 0..ROWS {
        put(&mut g, 20, y, "│");
        put(&mut g, 0, y, &format!("left {y}"));
    }
    s.feed(&g, 0);
    for n in 0..150 {
        for y in 0..ROWS {
            put(&mut g, 0, y, &format!("left {}", (n + y as usize) % 10));
        }
        s.feed(&g, 100);
    }
    let panes: Vec<FrameInfo> =
        s.f.list()
            .into_iter()
            .filter(|i| i.kind == FrameKind::Pane)
            .collect();
    assert_eq!(panes.len(), 2, "{panes:#?}");
    let right = panes.iter().find(|p| p.rect[0] > 20).unwrap();
    let left = panes.iter().find(|p| p.rect[0] == 0).unwrap();
    assert!(right.is_idle && right.idle == 1.0, "{right:?}");
    assert!(left.idle < 1.0);
    assert_eq!(s.f.resolve(&FrameSel::Name("idle".into())), Some(right.id));
}

/// Cost of an update at 200x60, printed: the best of five rounds. Run by `scripts/bench.sh`:
/// `cargo test --release -p ptyrc-core cost_200x60 -- --ignored --nocapture`.
#[test]
#[ignore = "timing"]
fn cost_200x60() {
    use super::heat::Heat;
    use super::screen::Screen;
    use std::time::Instant;

    const N: u32 = 300;
    let (cols, rows) = (200u16, 60u16);
    let page = |k: u32| {
        let mut g = Grid::new(cols, rows);
        for y in 0..rows {
            for x in 0..cols {
                let v = x as u32 * 7 + y as u32 * 13 + k * 31;
                if !v.is_multiple_of(5) {
                    let ch = (b'a' + (v % 26) as u8) as char;
                    g.put(x, y, Cell::narrow(&ch.to_string(), Style::default()));
                }
            }
        }
        g
    };
    let pages = [page(0), page(1)];
    let all = Rect::new(0, 0, cols, rows);
    fn input(g: &Grid) -> ScreenInput<'_> {
        ScreenInput {
            grid: g,
            cursor: Cursor {
                col: 0,
                row: 0,
                visible: true,
                pending_wrap: false,
            },
            alt_screen: false,
            title: "",
            events: &[],
        }
    }
    // The app writes on one grid, as a terminal does. 0: every row
    // changes; 1: nothing does; 2: one row does.
    let write = |g: &mut Grid, case: u32, i: u32| match case {
        0 => {
            for y in 0..rows {
                g.row_mut(y).clone_from_slice(pages[i as usize % 2].row(y));
            }
        }
        1 => {}
        _ => put(g, 3, 30, if i.is_multiple_of(2) { "x" } else { "y" }),
    };
    let mut out = [[f64::MAX; 4]; 3];
    for case in (0..5).flat_map(|_| 0..3u32) {
        let mut spent = [Duration::ZERO; 4];
        let mut timed = |k: usize, f: &mut dyn FnMut()| {
            let t = Instant::now();
            f();
            spent[k] += t.elapsed();
        };

        let mut g = pages[0].clone();
        let mut s = Screen::default();
        let mut h = Heat::default();
        for i in 0..N {
            write(&mut g, case, i);
            timed(0, &mut || {
                s.scan(&g);
            });
            timed(1, &mut || {
                h.update(&s, all, Duration::from_millis(20 * i as u64))
            });
            s.settle();
        }

        let mut g = pages[0].clone();
        let mut sim = Sim::new();
        sim.feed(&g, 0);
        sim.wait(2000);
        for i in 0..N {
            sim.now += Duration::from_millis(20);
            timed(3, &mut || {
                sim.f.tick(sim.now);
            });
            write(&mut g, case, i);
            timed(2, &mut || {
                sim.f.update(&input(&g), sim.now);
            });
        }
        for (best, d) in out[case as usize].iter_mut().zip(spent) {
            *best = best.min(d.as_secs_f64() * 1e6 / N as f64);
        }
    }
    for (name, [scan, heat, update, tick]) in ["all rows", "no change", "one row"].iter().zip(out) {
        println!(
            "  {name:<10} scan {scan:6.1} us   heat {heat:6.1} us   update {update:6.1} us   tick {tick:6.1} us"
        );
    }
}
