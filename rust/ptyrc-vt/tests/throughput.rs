// SPDX-License-Identifier: GPL-3.0-or-later
//! Throughput of `feed`, cost of a clone and of a scroll. `cargo test --release -p ptyrc-vt --test
//! throughput -- --ignored --nocapture`.

use ptyrc_vt::Vt;
use std::time::Instant;

/// Best of five rounds.
fn rate(name: &str, cols: u16, rows: u16, data: &[u8], rounds: usize) {
    let mut vt = Vt::new(cols, rows);
    let mut best = f64::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..rounds {
            vt.feed(data);
            vt.take_events();
        }
        best = best.min(start.elapsed().as_secs_f64());
    }
    let mb = (data.len() * rounds) as f64 / 1e6;
    println!("{name}: {:.0} MB/s", mb / best);
}

/// A full-screen program redrawing every row in place, as htop does:
/// cursor addressing, a few renditions a row, erase to the end of line.
fn redraw(cols: u16, rows: u16, frames: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for f in 0..frames {
        for row in 0..rows {
            let pid = 1000 + (row as usize * 37 + f * 11) % 9000;
            let cpu = (row as usize * 13 + f * 7) % 1000;
            let line = format!(
                "\x1b[{};1H\x1b[m{pid:5} user  20   0 \x1b[36m{:6}K\x1b[m \x1b[1;32m{:3}.{}\x1b[m \x1b[38;5;{}m{}\x1b[m\x1b[K",
                row + 1,
                pid * 3,
                cpu / 10,
                cpu % 10,
                16 + (row as usize + f) % 216,
                "/usr/bin/program --with some --arguments "
                    .repeat(cols as usize / 41 + 1)
                    .get(..cols as usize - 34)
                    .unwrap_or(""),
            );
            out.extend_from_slice(line.as_bytes());
        }
    }
    out
}

/// Cost of one call, in microseconds: best of five rounds.
fn cost(name: &str, rounds: usize, mut f: impl FnMut()) {
    let mut best = f64::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..rounds {
            f();
        }
        best = best.min(start.elapsed().as_secs_f64());
    }
    println!("{name}: {:.1} us", best * 1e6 / rounds as f64);
}

#[test]
#[ignore]
fn throughput() {
    let plain: Vec<u8> = (0..2000)
        .flat_map(|i| format!("{i}: the quick brown fox jumps over the lazy dog\r\n").into_bytes())
        .collect();
    rate("plain 80x24", 80, 24, &plain, 200);
    rate("plain 300x100", 300, 100, &plain, 200);
    let colour: Vec<u8> = (0..2000)
        .flat_map(|i| {
            format!(
                "\x1b[38;5;{}m{i}\x1b[0m \x1b[1;32mok\x1b[m \x1b[48;2;1;2;3mworld 世界\x1b[K\r\n",
                i % 256
            )
            .into_bytes()
        })
        .collect();
    rate("colour 80x24", 80, 24, &colour, 200);
    let sgr: Vec<u8> = (0..2000)
        .flat_map(|i| {
            (0..40)
                .flat_map(|j| {
                    format!(
                        "\x1b[38;2;{};{};{};48;5;{}m\u{2580}",
                        i % 256,
                        j * 6,
                        255 - j,
                        j
                    )
                    .into_bytes()
                })
                .chain(*b"\x1b[m\r\n")
                .collect::<Vec<u8>>()
        })
        .collect();
    rate("sgr a cell 80x24", 80, 24, &sgr, 50);
    rate("redraw 200x60", 200, 60, &redraw(200, 60, 20), 50);
    let mut vt = Vt::new(200, 60);
    vt.feed(&redraw(200, 60, 1));
    cost("clone 200x60", 200, || {
        std::hint::black_box(vt.clone());
    });
    let mut other = vt.clone();
    cost("clone_from 200x60", 200, || other.clone_from(&vt));
    cost("grid eq 200x60", 200, || {
        std::hint::black_box(vt.grid() == other.grid());
    });
    cost("scroll a line 200x60", 2000, || vt.feed(b"\x1b[60;1H\n"));
    // A full-view sprite of braille runs redrawn: every other cell
    // differs, a colour a run.
    let frame = |k: u32| {
        let mut vt = Vt::new(200, 60);
        for row in 0..60u32 {
            let mut line = format!("\x1b[{};1H", row + 1);
            let mut col = 0;
            while col < 200 {
                let v = row * 31 + col * 7 + k * 13;
                let n = (1 + v % 8).min(200 - col);
                line += &format!("\x1b[38;5;{}m", 16 + v % 216);
                line.extend((0..n).filter_map(|i| char::from_u32(0x2801 + (v + i) % 255)));
                col += n;
                let gap = (v % 11).min(200 - col);
                line += &format!("\x1b[{}C", gap.max(1));
                col += gap;
            }
            vt.feed(line.as_bytes());
        }
        vt.grid().clone()
    };
    let frames = [frame(0), frame(1)];
    let mut host = Vt::new(200, 60);
    let (mut k, mut sent) = (0, 0);
    cost("repair a sprite frame 200x60", 100, || {
        k += 1;
        let mut out = Vec::new();
        ptyrc_vt::repair(&mut host, &frames[k % 2], &mut out);
        sent = out.len();
    });
    println!("  {sent} B sent");
    cost("repair nothing 200x60", 100, || {
        let mut out = Vec::new();
        ptyrc_vt::repair(&mut host, &frames[k % 2], &mut out);
        assert!(out.is_empty());
    });
    rate(
        "vim replay",
        60,
        16,
        include_bytes!("fixtures/vim.raw"),
        2000,
    );
}

/// Cost of one call, in microseconds: the least of `rounds` calls, the
/// measure least disturbed by other load.
fn best(name: &str, rounds: usize, mut f: impl FnMut()) {
    let mut least = f64::MAX;
    for _ in 0..rounds {
        let start = Instant::now();
        f();
        least = least.min(start.elapsed().as_secs_f64());
    }
    println!("{name}: {:.1} us", least * 1e6);
}

/// A 200x60 frame, every cell its own colour: `ptyrc-box`'s live test.
fn rainbow(k: u32) -> ptyrc_vt::Grid {
    use ptyrc_vt::{Cell, Color, Grid, Style};
    let mut grid = Grid::new(200, 60);
    let c = char::from(b'A' + (k % 26) as u8);
    for row in 0..60u16 {
        for col in 0..200u16 {
            let v = u32::from(row) * 200 + u32::from(col) + k;
            let style = Style {
                fg: Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
                ..Style::default()
            };
            grid.put(col, row, Cell::glyph(c, 1, style));
        }
    }
    grid
}

/// The repair of a colour a cell, and the same bytes parsed whole by a
/// copy of the host. `cargo test --release -p ptyrc-vt --test throughput
/// repaint -- --ignored --nocapture`.
#[test]
#[ignore]
fn repaint() {
    let frames = [rainbow(0), rainbow(1)];
    let mut host = Vt::new(200, 60);
    let (mut k, mut sent) = (0, 0);
    best("repair a colour a cell 200x60", 400, || {
        k += 1;
        let mut out = Vec::new();
        ptyrc_vt::repair(&mut host, &frames[k % 2], &mut out);
        sent = out.len();
    });
    println!("  {sent} B sent");
    let before = host.clone();
    let mut out = Vec::new();
    ptyrc_vt::repair(&mut host, &frames[(k + 1) % 2], &mut out);
    let mut fed = before.clone();
    best("clone_from 200x60", 100, || fed.clone_from(&before));
    best("clone_from, feed the repair 200x60", 100, || {
        fed.clone_from(&before);
        fed.feed(&out);
    });
    assert_eq!(fed.grid(), host.grid());
}
