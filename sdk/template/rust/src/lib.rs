// SPDX-License-Identifier: GPL-3.0-or-later
//! A clock in the top right corner. The API: vendor/ptyrc-app/README.md.
//! The rules: AGENTS.md.

use ptyrc_app::{App, Canvas, Cell, Color, Value, attr, json, period};

const WIDTH: u32 = 16;

struct Clock {
    canvas: Canvas,
    label: String,
}

/// What is shown at `now_ms`. Pure: `cargo test` runs its test natively.
fn shown(label: &str, now_ms: u64) -> String {
    format!("{label} = {:.1} s", now_ms as f64 / 1000.0)
}

impl Clock {
    fn draw(&mut self, now_ms: u64) {
        let text = shown(&self.label, now_ms);
        self.canvas.fill(Cell {
            fg: Color::Palette(11),
            attrs: attr::BOLD,
            ..Cell::default()
        });
        // Right-aligned: the last cell of the canvas, at -1, the last column.
        let len = text.chars().count() as u32;
        self.canvas.text(WIDTH.saturating_sub(len), 0, &text);
        // Behind the text, as an app that declares nothing draws.
        self.canvas
            .present(&json!({"anchor": {"at": [-1, 0], "space": "view"}}));
    }
}

impl App for Clock {
    fn init(args: Value) -> Clock {
        let label = args["label"].as_str().unwrap_or("t").to_string();
        let canvas = Canvas::new(WIDTH, 1).expect("a canvas of 16 cells");
        period(100);
        let mut clock = Clock { canvas, label };
        clock.draw(0);
        clock
    }

    fn tick(&mut self, now_ms: u64) {
        self.draw(now_ms);
    }
}

ptyrc_app::app!(Clock);

#[cfg(test)]
mod tests {
    #[test]
    fn shown() {
        assert_eq!(super::shown("t", 1250), "t = 1.2 s");
    }
}
