// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use ptyrc_core::frames::HeatView;
use ptyrc_vt::Grid;
use serde_json::json;

fn make(params: serde_json::Value) -> Snow {
    Snow::new(serde_json::from_value(params).unwrap())
}

/// A view of `rows`, 40 columns, at the frame row `oy`; `hot` cells.
fn look(
    snow: &mut Snow,
    rows: &[&str],
    oy: i64,
    hot: &[(usize, usize)],
    cursor: Option<(u16, u16)>,
) {
    let grid = Grid::from_lines(40, rows);
    let n = 40 * rows.len();
    let mut heat = HeatView {
        cols: 40,
        rows: rows.len() as u16,
        safe: vec![1.0; n],
        activity: vec![0.0; n],
        occupancy: vec![0.0; n],
        still: vec![60.0; n],
        class: vec![Default::default(); n],
    };
    for (x, y) in hot {
        heat.class[y * 40 + x] = HeatClass::Hot;
    }
    let mut under = Under::of(&grid).with_heat(Some(&heat));
    under.origin = (0, oy);
    under.cursor = cursor;
    snow.look(&under);
}

/// Render every frame of `[from, to]` on a canvas of 40 by `h`.
fn run(snow: &mut Snow, h: u16, from: u64, to: u64) -> Canvas {
    let mut canvas = Canvas::new(40, h);
    for t in (from..=to).step_by(FRAME_MS as usize) {
        canvas.clear();
        assert!(snow.render(Duration::from_millis(t), &mut canvas));
    }
    canvas
}

/// Eighths of snow in the cells `[x0, x1)` of row `y`.
fn row(snow: &Snow, y: usize, x0: usize, x1: usize) -> f32 {
    (x0..x1).map(|x| snow.depth(x, y)).sum()
}

const TEXT: [&str; 8] = ["", "", "", "     a line of text here", "", "", "", ""];

#[test]
#[ignore = "prints the snow: cargo test -p ptyrc-draw-snow show -- --ignored --nocapture"]
fn show() {
    let mut snow = make(json!({"rate": 60}));
    look(&mut snow, &TEXT, 0, &[], None);
    for t in (0..=40_000u64).step_by(FRAME_MS as usize) {
        let mut canvas = Canvas::new(40, 8);
        snow.render(Duration::from_millis(t), &mut canvas);
        if t % 10_000 != 0 {
            continue;
        }
        println!(
            "{t} ms, {} flakes, {} eighths",
            snow.flakes.len(),
            snow.lying()
        );
        for y in 0..8 {
            let line: String = (0..40)
                .map(|x| match snow.depth(x, y).ceil() as usize {
                    _ if snow.wall(x, y) => '#',
                    0 => match snow
                        .flakes
                        .iter()
                        .any(|f| f.col(t as f64 / 1000.0) == x as i64 && f.y as usize == y)
                    {
                        true => '*',
                        false => ' ',
                    },
                    n => DRIFT[n.min(8) - 1].chars().next().unwrap(),
                })
                .collect();
            println!("|{line}|");
        }
    }
}

#[test]
fn snow_settles_on_lines_and_on_the_floor() {
    let mut snow = make(json!({"rate": 60, "seed": 3}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 30_000);
    // On the line: the row above its text, nowhere under its glyphs.
    assert!(row(&snow, 2, 5, 24) > 30.0, "{}", row(&snow, 2, 5, 24));
    assert_eq!(row(&snow, 3, 5, 6) + row(&snow, 3, 7, 11), 0.0);
    // The blank between two words holds it too; the end of the line
    // does not.
    assert!(snow.depth(6, 2) > 0.0);
    assert_eq!(snow.depth(4, 2) + snow.depth(24, 2), 0.0);
    // On the floor, under the line too: half the flakes pass behind it.
    assert!(row(&snow, 7, 5, 24) > 20.0);
    assert!(row(&snow, 7, 26, 40) > 30.0);
    // Nothing hangs in the air.
    for y in [0, 4, 5] {
        assert_eq!(row(&snow, y, 0, 40), 0.0, "row {y}");
    }
    // Drifts are smooth: no cell stands far over its neighbour.
    for x in 26..39 {
        let step = (snow.depth(x, 7) - snow.depth(x + 1, 7)).abs();
        assert!(step <= 2.0 * GAIN, "{x}: {step}");
    }
}

#[test]
fn snow_is_drawn_in_eighths_and_far_flakes_are_faint() {
    let mut snow = make(json!({"rate": 0}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 0);
    // A profile within `SLOPE`: it does not slump.
    snow.put(8, 2, 2.0);
    snow.put(9, 2, 3.5);
    snow.put(10, 2, 2.0);
    snow.flakes.push(Flake {
        x: 30.5,
        y: 1.0,
        v: 0.0,
        drift: 0.0,
        phase: 0.0,
        turn: 0.0,
        far: true,
    });
    let canvas = run(&mut snow, 8, 80, 80);
    let mut want = Canvas::new(40, 8);
    want.put_faded(30, 1, SPECKS[0], &StyleSpec::fg(PALE), FAINT);
    want.put(8, 2, "▂", &StyleSpec::fg(WHITE));
    want.put(9, 2, "▄", &StyleSpec::fg(WHITE));
    want.put(10, 2, "▂", &StyleSpec::fg(WHITE));
    assert_eq!(canvas, want);
}

#[test]
fn a_cap_on_text_a_drift_on_the_floor_and_no_more() {
    let mut snow = make(json!({"rate": 600, "seed": 5, "wind": 0}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 60_000);
    // Full, but for what thawed a moment ago.
    let nearly = |got: f32, all: f32| got > 0.9 * all && got <= all;
    assert!(nearly(row(&snow, 2, 5, 24), 19.0 * CAP));
    assert_eq!(row(&snow, 1, 0, 40), 0.0);
    assert!(nearly(row(&snow, 7, 0, 40), 40.0 * FULL));
    assert!(nearly(row(&snow, 6, 0, 40), 40.0 * CAP));
    assert!((0..40).all(|x| snow.depth(x, 6) <= CAP));
    assert_eq!(row(&snow, 5, 0, 40), 0.0);
}

#[test]
fn snow_melts_where_the_screen_is_warm() {
    let mut snow = make(json!({"rate": 0}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 0);
    for x in 8..24 {
        snow.put(x, 2, 6.0);
    }
    // The text under 8..12 changes; the cursor is on the line at 20.
    let hot: Vec<_> = (8..12).map(|x| (x, 3)).collect();
    look(&mut snow, &TEXT, 0, &hot, Some((20, 3)));
    run(&mut snow, 8, 80, 800);
    let part = snow.depth(9, 2);
    assert!(part > 0.0 && part < 6.0, "{part}");
    run(&mut snow, 8, 880, 2000);
    assert_eq!(row(&snow, 2, 8, 12), 0.0);
    assert_eq!(row(&snow, 2, 17, 24), 0.0);
    // The rest stays, but for a little thaw.
    assert!(row(&snow, 2, 12, 17) >= 27.0);
}

#[test]
fn snow_written_over_is_gone_and_snow_let_go_falls() {
    let mut snow = make(json!({"rate": 0}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 0);
    for x in 8..16 {
        snow.put(x, 2, 8.0);
    }
    // Text on a part of it.
    let mut rows = TEXT;
    rows[2] = "        xxxx";
    look(&mut snow, &rows, 0, &[], None);
    run(&mut snow, 8, 80, 80);
    assert_eq!(row(&snow, 2, 8, 12), 0.0);
    assert_eq!(row(&snow, 2, 12, 40), 4.0 * 8.0);
    // The line under the rest goes: it falls, and lies on the floor.
    rows[3] = "";
    look(&mut snow, &rows, 0, &[], None);
    run(&mut snow, 8, 160, 160);
    assert_eq!(row(&snow, 2, 0, 40), 0.0);
    assert!(snow.flakes.len() >= 11, "{}", snow.flakes.len());
    run(&mut snow, 8, 240, 6000);
    assert!(snow.flakes.is_empty());
    assert!(
        snow.lying() >= 28.0 && snow.lying() <= 32.0,
        "{}",
        snow.lying()
    );
    assert_eq!(row(&snow, 7, 0, 40) + row(&snow, 6, 0, 40), snow.lying());
}

#[test]
fn the_snow_scrolls_with_the_text() {
    let mut snow = make(json!({"rate": 0}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 0);
    for x in 8..16 {
        snow.put(x, 2, 5.0);
        snow.put(x, 7, 5.0);
    }
    // Two rows up: the line and its snow; the snow of the floor is in
    // the air and falls.
    let mut rows = [""; 8];
    rows[1] = TEXT[3];
    look(&mut snow, &rows, 2, &[], None);
    run(&mut snow, 8, 80, 80);
    assert_eq!(row(&snow, 0, 0, 40), 8.0 * 5.0);
    assert_eq!(row(&snow, 5, 8, 16), 0.0);
    assert!(!snow.flakes.is_empty());
    // Away and back: the snow of the line is where it was.
    look(&mut snow, &[""; 8], 500, &[], None);
    run(&mut snow, 8, 160, 160);
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 240, 240);
    assert_eq!(row(&snow, 2, 0, 40), 8.0 * 5.0);
    // Far away: forgotten.
    look(&mut snow, &[""; 8], 100_000, &[], None);
    assert!(snow.lying.is_empty());
}

#[test]
fn same_seed_same_snow() {
    let fall = |seed: u64| {
        let mut snow = make(json!({"seed": seed, "rate": 80}));
        look(&mut snow, &TEXT, 0, &[], None);
        run(&mut snow, 8, 0, 5000);
        (snow.lying.clone(), snow.flakes.len())
    };
    assert_eq!(fall(7), fall(7));
    assert_ne!(fall(7), fall(8));
}

#[test]
fn tuned_checked_and_ended() {
    let mut snow = make(json!({"rate": 80}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 3000);
    let flakes = snow.flakes.len();
    assert!(flakes > 0);
    assert!(snow.tune(&json!({"rate": -1})).is_err());
    assert!(snow.tune(&json!({"storm": 1})).is_err());
    snow.tune(&json!({"rate": 0, "wind": -2})).unwrap();
    assert_eq!((snow.flakes.len(), snow.params.wind), (flakes, -2.0));
    run(&mut snow, 8, 3080, 12_000);
    assert!(snow.flakes.is_empty());
    assert!(super::snow(&serde_json::Value::Null).is_ok());
    assert!(super::snow(&json!({"wind": 99})).is_err());
    assert!(super::snow(&json!({"rate": "much"})).is_err());
    let mut done = make(json!({"duration_ms": 100}));
    let mut canvas = Canvas::new(10, 5);
    assert!(done.render(Duration::from_millis(50), &mut canvas));
    assert!(!done.render(Duration::from_millis(100), &mut canvas));
    // A late render does not flood.
    let mut late = make(json!({"rate": 100}));
    assert!(late.render(Duration::from_millis(0), &mut canvas));
    assert!(late.render(Duration::from_secs(3600), &mut canvas));
    assert!(late.flakes.len() <= 10);
}

#[test]
fn a_shift_moves_the_snow_of_its_rows() {
    let mut snow = make(json!({"rate": 0}));
    look(&mut snow, &TEXT, 0, &[], None);
    run(&mut snow, 8, 0, 0);
    for x in 8..16 {
        snow.put(x, 2, 5.0);
    }
    // Two rows are spliced in above the line: it is two rows lower.
    snow.shift(1, 100, 2);
    let mut rows = [""; 8];
    rows[5] = TEXT[3];
    look(&mut snow, &rows, 0, &[], None);
    run(&mut snow, 8, 80, 80);
    assert_eq!(row(&snow, 4, 0, 40), 8.0 * 5.0);
    assert_eq!(row(&snow, 2, 8, 16), 0.0);
}
