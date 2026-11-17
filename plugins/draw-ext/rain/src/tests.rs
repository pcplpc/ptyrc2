// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use ptyrc_core::frames::HeatView;
use ptyrc_vt::Grid;
use serde_json::json;

fn make(params: serde_json::Value) -> Rain {
    Rain::new(serde_json::from_value(params).unwrap())
}

fn look(rain: &mut Rain, rows: &[&str], cursor: Option<(u16, u16)>) {
    let grid = Grid::from_lines(80, if rows.is_empty() { &[""] } else { rows });
    let mut under = Under::of(&grid);
    under.cursor = cursor;
    rain.look(&under);
}

/// Render every frame of `[from, to]`; `each` sees the rain after each.
fn run(rain: &mut Rain, w: u16, h: u16, from: u64, to: u64, mut each: impl FnMut(&Rain)) {
    let mut canvas = Canvas::new(w, h);
    for t in (from..=to).step_by(FRAME_MS as usize) {
        canvas.clear();
        assert!(rain.render(Duration::from_millis(t), &mut canvas));
        each(rain);
    }
}

#[test]
fn drops_fall_and_end_on_the_floor() {
    let mut rain = make(json!({"rate": 40, "seed": 3}));
    let (mut most, mut floor) = (0, 0);
    run(&mut rain, 40, 12, 0, 4000, |r| {
        most = most.max(r.drops.len());
        assert!(r.drops.iter().all(|d| d.x < 40 && (d.y as usize) < 12));
        assert!(r.splashes.iter().all(|s| s.y == 11));
        floor += r.splashes.len();
    });
    assert!(most > 3, "{most}");
    assert!(floor > 20, "{floor}");
}

#[test]
fn drops_land_on_lines_or_pass_behind_them() {
    let mut rain = make(json!({"rate": 60, "seed": 5}));
    let line = "x".repeat(30);
    look(&mut rain, &["", "", "", "", &line], None);
    let mut rows = [0usize; 12];
    run(&mut rain, 30, 12, 0, 8000, |r| {
        for s in &r.splashes {
            rows[s.y] += 1;
        }
    });
    // On the line (the row above it) and on the floor, nowhere else.
    assert!(rows[3] > 20 && rows[11] > 20, "{rows:?}");
    assert_eq!(rows.iter().filter(|n| **n > 0).count(), 2, "{rows:?}");
}

#[test]
fn a_drop_always_lands_on_the_cursor() {
    let mut rain = make(json!({"rate": 0}));
    rain.resize(10, 10);
    rain.cursor = Some((4, 6));
    for _ in 0..50 {
        assert_eq!(rain.landing(4, 0), 5);
    }
    assert_eq!(rain.landing(5, 0), 9);
    // Text on the last row: nothing to land on in sight.
    look(
        &mut rain,
        &["", "", "", "", "", "", "", "", "", "xxxxxxxxxx"],
        None,
    );
    rain.build();
    rain.cursor = None;
    assert!((0..50).all(|_| [8, 10].contains(&rain.landing(2, 0))));
}

#[test]
fn text_written_under_a_drop_takes_it() {
    let mut rain = make(json!({"rate": 0}));
    rain.resize(10, 10);
    rain.build();
    rain.drops.push(Drop {
        x: 3,
        y: 1.0,
        v: 1.0,
        far: false,
        land: 9,
    });
    // Until one lands on it: the landing is drawn at each change.
    let mut taken = false;
    for i in 0..40 {
        let row = if i % 2 == 0 { "   x" } else { "   y" };
        look(&mut rain, &["", "", "", "", "", row], None);
        rain.build();
        taken |= rain.drops[0].land == 4;
        assert!([4, 9].contains(&rain.drops[0].land));
    }
    assert!(taken);
}

#[test]
fn it_rains_less_where_the_text_changes() {
    let mut rain = make(json!({"rate": 200, "seed": 9}));
    let (mut busy, mut cold) = (0usize, 0usize);
    // Columns 0..20 of the last row just changed; the rest is still.
    let grid = Grid::new(80, 10);
    let n = 80 * 10;
    let mut heat = HeatView {
        cols: 80,
        rows: 10,
        safe: vec![1.0; n],
        activity: vec![0.0; n],
        occupancy: vec![0.0; n],
        still: vec![60.0; n],
        class: vec![Default::default(); n],
    };
    heat.still[9 * 80..9 * 80 + 20].fill(0.0);
    for t in (0..12_000u64).step_by(FRAME_MS as usize) {
        rain.look(&Under::of(&grid).with_heat(Some(&heat)));
        let mut canvas = Canvas::new(80, 10);
        assert!(rain.render(Duration::from_millis(t), &mut canvas));
        // Every drop ends in a splash: count those of this frame.
        for s in rain.splashes.iter().filter(|s| s.born == t) {
            if s.x < 20 {
                busy += 1;
            } else if (40..60).contains(&s.x) {
                cold += 1;
            }
        }
    }
    assert!(cold > 50, "{cold}");
    assert!(busy * 4 < cold, "{busy} {cold}");
    // No heat: it rains alike everywhere, once the heat is read again.
    rain.look(&Under::of(&grid));
    assert!(rain.weight(3) < 1.0);
    let mut canvas = Canvas::new(80, 10);
    rain.render(Duration::from_millis(12_000 + HEAT_MS), &mut canvas);
    rain.look(&Under::of(&grid));
    assert_eq!(rain.weight(3), 1.0);
}

#[test]
fn a_wide_glyph_is_two_walls_and_shifts_nothing() {
    let mut vt = ptyrc_vt::Vt::new(20, 4);
    vt.feed("\r\n\r\na\t世b".as_bytes());
    let mut rain = make(json!({"rate": 0}));
    rain.resize(20, 4);
    rain.look(&Under::of(vt.grid()));
    rain.build();
    let walls: Vec<usize> = (0..20).filter(|x| rain.wall(*x, 2)).collect();
    assert_eq!(walls, [0, 8, 9, 10]);
}

#[test]
fn transparent_rain_lands_on_the_floor_only() {
    let line = "x".repeat(30);
    let rows = ["", "", "", "", &line, "", "", "", "", "", "", &line];
    let splashes = |rain: &mut Rain, from: u64| {
        let mut at = [0usize; 12];
        run(rain, 30, 12, from, from + 8000, |r| {
            for s in &r.splashes {
                at[s.y] += 1;
            }
        });
        at
    };
    // A line, the cursor, text on the last row: the last row all the same.
    let mut rain = make(json!({"rate": 60, "seed": 5, "transparent": true}));
    look(&mut rain, &rows, Some((3, 8)));
    let at = splashes(&mut rain, 0);
    assert!(at[11] > 40, "{at:?}");
    assert_eq!(at.iter().filter(|n| **n > 0).count(), 1, "{at:?}");
    for _ in 0..50 {
        assert_eq!(rain.landing(3, 0), 11);
    }
    // Tuned off, the drops in the air land on the line again; and back.
    rain.tune(&json!({"transparent": false})).unwrap();
    assert_eq!(rain.params.rate, 60.0);
    let mut canvas = Canvas::new(30, 12);
    assert!(rain.render(Duration::from_millis(8040), &mut canvas));
    assert!(rain.drops.iter().all(|d| d.land != 11), "text on the floor");
    rain.splashes.clear();
    let at = splashes(&mut rain, 8080);
    assert!(at[3] > 20 && at[7] > 0 && at[11] == 0, "{at:?}");
    rain.tune(&json!({"transparent": true})).unwrap();
    assert!(rain.render(Duration::from_millis(16200), &mut canvas));
    assert!(rain.drops.iter().all(|d| d.land == 11));
    assert!(rain.tune(&json!({"transparent": 1})).is_err());
}

#[test]
fn a_running_rain_takes_a_new_rate() {
    let mut rain = make(json!({"rate": 80, "seed": 2}));
    run(&mut rain, 40, 10, 0, 1000, |_| {});
    let drops = rain.drops.len();
    assert!(drops > 0);
    assert!(rain.tune(&json!({"rate": -4})).is_err());
    assert!(rain.tune(&json!({"speed": 4})).is_err());
    assert_eq!(rain.params.rate, 80.0);
    rain.tune(&json!({"rate": 0})).unwrap();
    assert_eq!(rain.drops.len(), drops);
    run(&mut rain, 40, 10, 1040, 4000, |_| {});
    assert!(rain.drops.is_empty());
}

#[test]
fn same_seed_same_rain() {
    let positions = |seed: u64| {
        let mut rain = make(json!({"seed": seed, "rate": 80}));
        run(&mut rain, 60, 20, 0, 2000, |_| {});
        rain.drops
            .iter()
            .map(|d| (d.x, d.y as usize))
            .collect::<Vec<_>>()
    };
    assert_eq!(positions(7), positions(7));
    assert_ne!(positions(7), positions(8));
}

#[test]
fn factory_checks_its_parameters() {
    assert!(rain(&serde_json::Value::Null).is_ok());
    assert!(rain(&json!({"rate": 30})).is_ok());
    assert!(rain(&json!({"rate": -1})).is_err());
    assert!(rain(&json!({"rate": "much"})).is_err());
    let mut done = make(json!({"duration_ms": 100}));
    let mut canvas = Canvas::new(10, 5);
    assert!(done.render(Duration::from_millis(50), &mut canvas));
    assert!(!done.render(Duration::from_millis(100), &mut canvas));
}

/// A screen of `w` by `h` with a line of text every third row, and its
/// heat.
fn screen(w: u16, h: u16) -> (Grid, HeatView) {
    let line = "words in the way ".repeat(w as usize / 20);
    let rows: Vec<&str> = (0..h)
        .map(|y| if y % 3 == 2 { line.as_str() } else { "" })
        .collect();
    let n = w as usize * h as usize;
    let heat = HeatView {
        cols: w,
        rows: h,
        safe: vec![1.0; n],
        activity: vec![0.0; n],
        occupancy: vec![0.0; n],
        still: vec![60.0; n],
        class: vec![Default::default(); n],
    };
    (Grid::from_lines(w, &rows), heat)
}

/// Microseconds of the fastest of forty runs of `f`.
fn fastest(mut f: impl FnMut()) -> f64 {
    (0..40)
        .map(|_| {
            let start = std::time::Instant::now();
            f();
            start.elapsed().as_secs_f64() * 1e6
        })
        .fold(f64::MAX, f64::min)
}

#[test]
#[ignore = "prints the cost of a tick: cargo test --release -p ptyrc-draw-rain bench -- --ignored --nocapture"]
fn bench() {
    for rate in [20, 200] {
        for (w, h) in [(80, 24), (200, 50), (300, 80)] {
            let (grid, heat) = screen(w, h);
            let under = Under::of(&grid).with_heat(Some(&heat));
            let mut rain = make(json!({"rate": rate}));
            let mut canvas = Canvas::new(w, h);
            // A minute of rain, then a tick timed: the canvas cleared,
            // the look, the render.
            let warm = 60_000;
            for t in (0..warm).step_by(FRAME_MS as usize) {
                canvas.clear();
                rain.look(&under);
                rain.render(Duration::from_millis(t), &mut canvas);
            }
            let clear = fastest(|| canvas.clear());
            let look = fastest(|| rain.look(&under));
            let mut t = warm;
            let render = fastest(|| {
                t += FRAME_MS;
                rain.render(Duration::from_millis(t), &mut canvas);
            });
            println!(
                "rain rate={rate} {w}x{h}: clear {clear:.0} look {look:.0} render {render:.0} us; \
                 {:.2}% of a core; {} drops",
                (clear + look + render) * 1e-4 * (1000 / FRAME_MS) as f64,
                rain.drops.len(),
            );
        }
    }
}
