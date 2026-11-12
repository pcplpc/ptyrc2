// SPDX-License-Identifier: GPL-3.0-or-later
//! The repair property: after `repair`, the host shows the desired grid
//! and is in the state of the app.

use ptyrc_vt::{Attrs, Cell, Color, Grid, Style, Vt, repair};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

const TEXT: &[&str] = &[
    "a", "b", "Z", " ", "_", "q", "x", "~", "0", "é", "世", "界", "🙂", "e\u{301}", "│", "─",
    "\u{308}", "hello ", "world", "    ",
];

fn sgr(rng: &mut Rng) -> String {
    let codes = [
        "0",
        "1",
        "2",
        "3",
        "4",
        "4:3",
        "4:0",
        "21",
        "5",
        "7",
        "8",
        "9",
        "22",
        "23",
        "24",
        "27",
        "53",
        "55",
        "31",
        "42",
        "93",
        "104",
        "39",
        "49",
        "38;5;123",
        "48;2;10;20;30",
        "58:5:9",
        "58:2::1:2:3",
        "59",
        "38:2:3:4:5",
        "",
    ];
    let n = 1 + rng.below(3);
    let parts: Vec<&str> = (0..n).map(|_| rng.pick(&codes)).collect();
    format!("\x1b[{}m", parts.join(";"))
}

/// One valid sequence, or text.
fn piece(rng: &mut Rng, cols: u16, rows: u16) -> String {
    let n = 1 + rng.below(4);
    let r = 1 + rng.below(rows as u64 + 1);
    let c = 1 + rng.below(cols as u64 + 1);
    match rng.below(64) {
        0..=17 => (0..1 + rng.below(6)).map(|_| rng.pick(TEXT)).collect(),
        18..=21 => sgr(rng),
        22..=24 => format!("\x1b[{r};{c}H"),
        25 => format!("\x1b[{n}A"),
        26 => format!("\x1b[{n}B"),
        27 => format!("\x1b[{n}C"),
        28 => format!("\x1b[{n}D"),
        29 => "\r".into(),
        30..=31 => "\n".into(),
        32 => "\r\n".into(),
        33 => "\x08".into(),
        34 => "\t".into(),
        35 => format!("\x1b[{}J", rng.below(3)),
        36 => format!("\x1b[{}K", rng.below(3)),
        37 => format!("\x1b[{n}L"),
        38 => format!("\x1b[{n}M"),
        39 => format!("\x1b[{n}@"),
        40 => format!("\x1b[{n}P"),
        41 => format!("\x1b[{n}X"),
        42 => format!("\x1b[{n}S"),
        43 => format!("\x1b[{n}T"),
        44 => {
            let top = 1 + rng.below(rows as u64);
            let bottom = top + rng.below(rows as u64);
            format!("\x1b[{top};{bottom}r")
        }
        45 => "\x1b[r".into(),
        46 => rng.pick(&["\x1b7", "\x1b8", "\x1b[s", "\x1b[u"]).into(),
        47 => rng.pick(&["\x1bM", "\x1bD", "\x1bE"]).into(),
        48 => rng
            .pick(&["\x1b[?1049h", "\x1b[?1049l", "\x1b[?47h", "\x1b[?47l"])
            .into(),
        49 => rng.pick(&["\x1b[?6h", "\x1b[?6l"]).into(),
        50 => rng.pick(&["\x1b[?7h", "\x1b[?7l", "\x1b[?7h"]).into(),
        51 => rng.pick(&["\x1b[4h", "\x1b[4l", "\x1b[4l"]).into(),
        52 => rng
            .pick(&["\x1b(0", "\x1b(B", "\x1b)0", "\x1b)B", "\x0e", "\x0f"])
            .into(),
        53 => rng.pick(&["\x1b[?2026h", "\x1b[?2026l"]).into(),
        54 => rng
            .pick(&[
                "\x1b]8;;http://a/\x1b\\",
                "\x1b]8;id=x;http://b/\x07",
                "\x1b]8;;\x1b\\",
            ])
            .into(),
        55 => rng
            .pick(&[
                "\x1b[?25l",
                "\x1b[?25h",
                "\x1b[?1h",
                "\x1b[?1l",
                "\x1b[?2004h",
            ])
            .into(),
        56 => format!("{}\x1b[{n}b", rng.pick(&["a", "世", "xy", "é"])),
        57 => "\x1b]2;title\x07".into(),
        58 => format!("\x1b[{c}G"),
        59 => format!("\x1b[{r}d"),
        60 => rng
            .pick(&["\x1b[!p", "\x1bH", "\x1b[3g", "\x1b[Z", "\x1b#8"])
            .into(),
        61 => {
            // Fill a row to the last column: pending wrap.
            let fill: String = (0..cols).map(|_| rng.pick(&["x", "y", "世"])).collect();
            format!("\r{fill}")
        }
        62 => "\x1b[20h\x1b[20l".into(),
        _ => "\x1bc".into(),
    }
}

fn style(rng: &mut Rng, link_max: u32) -> Style {
    let mut s = Style::default();
    if rng.chance(50) {
        s.fg = Color::Indexed(rng.below(256) as u8);
    }
    if rng.chance(30) {
        s.bg = Color::Rgb(rng.below(256) as u8, 7, 9);
    }
    if rng.chance(20) {
        s.attrs.insert(Attrs::BOLD);
    }
    if rng.chance(10) {
        s.attrs.insert(Attrs::UNDERLINE | Attrs::UL_CURLY);
        s.ul = Color::Indexed(3);
    }
    if rng.chance(10) {
        s.attrs.insert(Attrs::INVERSE | Attrs::ITALIC);
    }
    if link_max > 0 && rng.chance(10) {
        s.link = 1 + rng.below(link_max as u64) as u32;
    }
    s
}

/// The app grid with random sprite cells on top.
fn desired(rng: &mut Rng, app: &Vt) -> Grid {
    let mut grid = app.grid().clone();
    if rng.chance(15) {
        return grid;
    }
    let links = (1..4).take_while(|&i| app.link(i).is_some()).count() as u32;
    for _ in 0..rng.below(4) {
        let w = 1 + rng.below(8) as u16;
        let h = 1 + rng.below(3) as u16;
        let x = rng.below(grid.cols() as u64) as u16;
        let y = rng.below(grid.rows() as u64) as u16;
        let st = style(rng, links);
        for row in y..(y + h).min(grid.rows()) {
            let mut col = x;
            while col < (x + w).min(grid.cols()) {
                let cell = match rng.below(8) {
                    0 => Cell::wide("世", st),
                    1 => Cell::narrow("e\u{301}", st),
                    2 => Cell::blank(st),
                    3 => Cell::narrow("│", st),
                    4 => Cell::wide("🙂", st),
                    _ => Cell::narrow("*", st),
                };
                let width = cell.width as u16;
                if !grid.put(col, row, cell) {
                    grid.put(col, row, Cell::narrow("#", st));
                    col += 1;
                } else {
                    col += width;
                }
            }
        }
    }
    grid
}

fn run(seed: u64, cols: u16, rows: u16, steps: usize) {
    let mut rng = Rng(seed | 1);
    let mut app = Vt::new(cols, rows);
    let mut host = Vt::new(cols, rows);
    let mut held: Vec<u8> = Vec::new();
    for step in 0..steps {
        let mut chunk = std::mem::take(&mut held);
        for _ in 0..1 + rng.below(12) {
            chunk.extend_from_slice(piece(&mut rng, cols, rows).as_bytes());
        }
        // Sometimes cut the chunk anywhere: the tail comes with the next.
        // Not before a REP: what it repeats after a repair is the host's
        // business (see `repair_then_rep`).
        if rng.chance(25) && chunk.len() > 2 && !chunk.ends_with(b"b") {
            let cut = 1 + rng.below(chunk.len() as u64 - 1) as usize;
            held = chunk.split_off(cut);
        }
        app.feed(&chunk);
        host.feed(&chunk);
        assert!(
            host.same_state(&app),
            "seed {seed} step {step}: fed alike, states differ\napp  {:?} {:?} {:?}\nhost {:?} {:?} {:?}\nchunk {:?}",
            app.cursor(),
            app.pen(),
            app.modes(),
            host.cursor(),
            host.pen(),
            host.modes(),
            String::from_utf8_lossy(&chunk)
        );
        if !app.in_ground() {
            continue;
        }
        let want = desired(&mut rng, &app);
        let mut out = Vec::new();
        repair(&mut host, &want, &mut out);
        assert!(
            host.grid() == &want,
            "seed {seed} step {step}: grid differs\nwant {:?}\nhost {:?}\nout {:?}",
            want.text(),
            host.grid().text(),
            String::from_utf8_lossy(&out)
        );
        assert!(
            host.same_state(&app),
            "seed {seed} step {step}: state differs\napp  {:?} {:?} {:?}\nhost {:?} {:?} {:?}\nout {:?}",
            app.cursor(),
            app.pen(),
            app.modes(),
            host.cursor(),
            host.pen(),
            host.modes(),
            String::from_utf8_lossy(&out)
        );
        assert!(host.in_ground());
        // Idempotent.
        let mut again = Vec::new();
        repair(&mut host, &want, &mut again);
        assert!(
            again.is_empty(),
            "seed {seed} step {step}: second repair wrote"
        );
    }
}

#[test]
fn repair_property() {
    for seed in 1..=300u64 {
        let cols = [1, 2, 3, 5, 8, 20, 40, 80][(seed % 8) as usize];
        let rows = [1, 2, 3, 6, 24][(seed % 5) as usize];
        run(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15), cols, rows, 150);
    }
}

#[test]
fn repair_is_silent_when_equal() {
    let mut host = Vt::new(10, 3);
    host.feed(b"hello\x1b[31m");
    let want = host.grid().clone();
    let mut out = Vec::new();
    repair(&mut host, &want, &mut out);
    assert!(out.is_empty());
    // Wrong size: nothing.
    repair(&mut host, &Grid::new(4, 4), &mut out);
    assert!(out.is_empty());
}

#[test]
fn repair_restores_pending_wrap() {
    let mut app = Vt::new(5, 2);
    app.feed(b"\x1b[32mabcde");
    let mut host = app.clone();
    let mut want = app.grid().clone();
    want.put(1, 1, Cell::narrow("*", Style::default()));
    let mut out = Vec::new();
    repair(&mut host, &want, &mut out);
    assert!(host.cursor().pending_wrap);
    assert!(host.same_state(&app));
    // What the app prints next lands where it would have.
    app.feed(b"f");
    host.feed(b"f");
    assert_eq!(host.grid().text()[1], "f*");
    assert!(host.same_state(&app));
}

#[test]
fn repair_unpaints() {
    let mut app = Vt::new(10, 2);
    app.feed(b"0123456789\r\nabc");
    let mut host = app.clone();
    let mut want = app.grid().clone();
    for col in 2..6 {
        want.put(
            col,
            0,
            Cell::narrow(
                "#",
                Style {
                    fg: Color::Indexed(1),
                    ..Style::default()
                },
            ),
        );
    }
    let mut out = Vec::new();
    repair(&mut host, &want, &mut out);
    assert_eq!(host.grid().text()[0], "01####6789");
    let text = String::from_utf8_lossy(&out).into_owned();
    assert!(
        text.starts_with("\x1b[?2026h") && text.ends_with("\x1b[?2026l"),
        "{text:?}"
    );
    assert!(!text.contains("\x1b7") && !text.contains("\x1b8"));
    let base = app.grid().clone();
    repair(&mut host, &base, &mut out);
    assert_eq!(host.grid(), app.grid());
    assert!(host.same_state(&app));
}

#[test]
fn repair_inside_app_sync_update() {
    let mut host = Vt::new(10, 2);
    host.feed(b"\x1b[?2026habc");
    let mut want = host.grid().clone();
    want.put(5, 1, Cell::narrow("*", Style::default()));
    let mut out = Vec::new();
    repair(&mut host, &want, &mut out);
    assert!(!String::from_utf8_lossy(&out).contains("2026"));
    assert!(host.modes().sync_update);
}

#[test]
fn repair_then_rep_and_combining() {
    // The app's last glyph is not under a sprite: REP and a combining mark
    // that come after a repair act as without it.
    for tail in ["\x1b[3b", "\u{301}"] {
        let mut app = Vt::new(12, 2);
        app.feed("ab世".as_bytes());
        let mut host = app.clone();
        let mut want = app.grid().clone();
        want.put(
            8,
            1,
            Cell::narrow(
                "*",
                Style {
                    fg: Color::Indexed(2),
                    ..Style::default()
                },
            ),
        );
        let mut out = Vec::new();
        repair(&mut host, &want, &mut out);
        assert!(host.same_state(&app));
        app.feed(tail.as_bytes());
        host.feed(tail.as_bytes());
        assert!(host.same_state(&app), "{tail:?}");
        assert_eq!(host.grid().row(0), app.grid().row(0), "{tail:?}");
    }
}
