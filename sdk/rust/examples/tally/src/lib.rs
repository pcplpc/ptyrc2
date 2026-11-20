// SPDX-License-Identifier: GPL-3.0-or-later
//! Tally: counts its runs in storage, shows the local date and time,
//! gives the count as its result (`ptyrc app tally --wait`). The twin
//! of `sdk/js/examples/tally`. Storage denied: the count is not kept;
//! the clock denied: session time from 1970, UTC.

use ptyrc_app::clock::{self, civil};
use ptyrc_app::{App, Canvas, Cell, Color, Value, json, period, result, storage};

struct Tally {
    runs: u64,
    kept: bool,
    canvas: Canvas,
}

impl Tally {
    /// `run N · YYYY-MM-DD HH:MM:SS ZONE`, or the error of the clock.
    fn line(&self) -> String {
        let kept = if self.kept { "" } else { " (not kept)" };
        match clock::now() {
            Ok(w) => {
                let c = civil(w.unix_ms, w.offset_s);
                format!(
                    "run {}{kept} · {}-{:02}-{:02} {:02}:{:02}:{:02} {}",
                    self.runs, c.year, c.month, c.day, c.hour, c.minute, c.second, w.abbr
                )
            }
            Err(e) => format!("run {}{kept} · {e}", self.runs),
        }
    }
}

impl App for Tally {
    fn init(_args: Value) -> Tally {
        let last = storage::get("runs").ok().flatten();
        let runs = last.and_then(|n| n.parse::<u64>().ok()).unwrap_or(0) + 1;
        let kept = storage::set("runs", &runs.to_string()).is_ok();
        let _ = result(json!({ "runs": runs }), 0);
        period(1000);
        let canvas = Canvas::new(48, 1).expect("canvas");
        let mut tally = Tally { runs, kept, canvas };
        tally.tick(0);
        tally
    }

    fn tick(&mut self, _now_ms: u64) {
        let line = self.line();
        self.canvas.fill(Cell { fg: Color::Palette(10), ..Cell::default() });
        self.canvas.text(0, 0, &line);
        self.canvas.present(&json!({"anchor": {"at": [0, 0]}}));
    }
}

ptyrc_app::app!(Tally);
