// SPDX-License-Identifier: GPL-3.0-or-later
//! Throughput of `feed`. `cargo test --release -p ptyrc-vt --test
//! throughput -- --ignored --nocapture`.

use ptyrc_vt::Vt;
use std::time::Instant;

fn rate(name: &str, cols: u16, rows: u16, data: &[u8], rounds: usize) {
    let mut vt = Vt::new(cols, rows);
    let start = Instant::now();
    for _ in 0..rounds {
        vt.feed(data);
        vt.take_events();
    }
    let secs = start.elapsed().as_secs_f64();
    let mb = (data.len() * rounds) as f64 / 1e6;
    println!("{name}: {:.0} MB/s", mb / secs);
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
    rate(
        "vim replay",
        60,
        16,
        include_bytes!("fixtures/vim.raw"),
        2000,
    );
}
