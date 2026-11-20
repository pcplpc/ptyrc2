// SPDX-License-Identifier: GPL-3.0-or-later
//! The faces: pure, a time in, a content out. `cargo test` tests them.

use ptyrc_app::deck::Content;
use ptyrc_app::{Canvas, Cell, Color, Run, attr};

/// Block digits, three cells by three rows.
const DIGITS: [[&str; 3]; 10] = [
    ["█▀█", "█ █", "▀▀▀"],
    [" █ ", " █ ", " ▀ "],
    ["▀▀█", "█▀▀", "▀▀▀"],
    ["▀▀█", " ▀█", "▀▀▀"],
    ["█ █", "▀▀█", "  ▀"],
    ["█▀▀", "▀▀█", "▀▀▀"],
    ["█▀▀", "█▀█", "▀▀▀"],
    ["▀▀█", "  █", "  ▀"],
    ["█▀█", "█▀█", "▀▀▀"],
    ["█▀█", "▀▀█", "▀▀▀"],
];

/// The sizes, largest first: the driver shows the largest that fits.
pub const SIZES: [(&str, u32, u32); 2] = [("big", 17, 3), ("line", 5, 1)];

/// `MM:SS` of `ms`; past an hour, the minutes go on counting.
pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// The face of `ms` in `w` by `h` cells: one row, or block digits on
/// `canvas` (made, or remade at another size).
pub fn face(ms: u64, w: u32, h: u32, paused: bool, canvas: &mut Option<Canvas>) -> Content {
    let text = clock(ms);
    let fg = Color::Palette(if paused { 8 } else { 14 });
    let ink = if paused { 0 } else { attr::BOLD };
    if h < 3 {
        return Content::from(vec![Run::new(text).fg(fg).attrs(ink)]);
    }
    if canvas
        .as_ref()
        .is_none_or(|c| (c.width(), c.height()) != (w, h))
    {
        *canvas = Canvas::new(w, h);
    }
    let Some(c) = canvas.as_mut() else {
        return Content::from(text);
    };
    c.clear();
    let style = Cell {
        fg,
        attrs: ink,
        ..Cell::default()
    };
    let mut x = 0;
    for ch in text.chars() {
        if ch == ':' {
            c.set(
                x,
                1,
                Cell {
                    ch: Some('▪'),
                    ..style
                },
            );
            x += 2;
            continue;
        }
        let rows = DIGITS[ch.to_digit(10).unwrap_or(0) as usize];
        for (y, row) in rows.iter().enumerate() {
            for (i, g) in row.chars().enumerate() {
                c.set(
                    x + i as u32,
                    y as u32,
                    Cell {
                        ch: Some(g),
                        ..style
                    },
                );
            }
        }
        x += 4;
    }
    Content::from(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_text() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(61_999), "01:01");
        assert_eq!(clock(3_600_000), "60:00");
    }

    #[test]
    fn line_face() {
        let row = vec![Run::new("00:05").fg(Color::Palette(14)).attrs(attr::BOLD)];
        assert_eq!(
            face(5000, 5, 1, false, &mut None),
            Content::Cells(vec![row])
        );
    }

    #[test]
    fn big_face() {
        let Content::Cells(rows) = face(72_000, 17, 3, false, &mut None) else {
            panic!("cells")
        };
        let text: Vec<String> = rows
            .iter()
            .map(|r| r.iter().map(|r| r.t.as_str()).collect())
            .collect();
        assert_eq!(
            text,
            [
                "█▀█  █     █  ▀▀█",
                "█ █  █  ▪  █  █▀▀",
                "▀▀▀  ▀     ▀  ▀▀▀"
            ]
        );
        assert_eq!(
            rows[0][0],
            Run::new("█▀█").fg(Color::Palette(14)).attrs(attr::BOLD)
        );
    }
}
