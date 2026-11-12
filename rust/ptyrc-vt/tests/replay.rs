// SPDX-License-Identifier: GPL-3.0-or-later
//! Recorded output of real programs, 60x16. The expected text is what tmux
//! showed for the same bytes.

use ptyrc_vt::{Cell, Style, Vt, repair};

fn check(name: &str, raw: &[u8], want: &str) {
    let want: Vec<&str> = want.lines().collect();
    // Whole.
    let mut vt = Vt::new(60, 16);
    vt.feed(raw);
    assert!(vt.in_ground(), "{name}");
    let got = vt.grid().text();
    for (row, line) in got.iter().enumerate() {
        assert_eq!(
            line,
            want.get(row).copied().unwrap_or(""),
            "{name} row {row}"
        );
    }
    // In chunks of every size, with a repair between them: same screen,
    // same state.
    for size in [1usize, 7, 64, 1000] {
        let mut app = Vt::new(60, 16);
        let mut host = Vt::new(60, 16);
        let mut out = Vec::new();
        for (i, chunk) in raw.chunks(size).enumerate() {
            app.feed(chunk);
            host.feed(chunk);
            if !app.in_ground() {
                continue;
            }
            let mut desired = app.grid().clone();
            let col = (i % 50) as u16;
            for (k, ch) in ["<", "*", ">"].iter().enumerate() {
                desired.put(
                    col + k as u16,
                    (i % 16) as u16,
                    Cell::narrow(ch, Style::default()),
                );
            }
            out.clear();
            repair(&mut host, &desired, &mut out);
            assert!(host.grid() == &desired, "{name} size {size} chunk {i}");
            assert!(host.same_state(&app), "{name} size {size} chunk {i}");
        }
        assert_eq!(app.grid().text(), got, "{name} size {size}");
    }
}

#[test]
fn vim() {
    check(
        "vim",
        include_bytes!("fixtures/vim.raw"),
        include_str!("fixtures/vim.txt"),
    );
}

#[test]
fn less() {
    check(
        "less",
        include_bytes!("fixtures/less.raw"),
        include_str!("fixtures/less.txt"),
    );
}

#[test]
fn tmux() {
    check(
        "tmux",
        include_bytes!("fixtures/tmux.raw"),
        include_str!("fixtures/tmux.txt"),
    );
}
