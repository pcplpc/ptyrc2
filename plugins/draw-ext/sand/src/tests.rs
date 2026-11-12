// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use serde_json::json;

fn make(params: serde_json::Value) -> Sand {
    Sand::new(serde_json::from_value(params).unwrap())
}

/// Render every frame up to `ms`.
fn run(sand: &mut Sand, w: u16, h: u16, from: u64, ms: u64) -> Canvas {
    let mut canvas = Canvas::new(w, h);
    for t in (from..=ms).step_by(FRAME_MS as usize) {
        canvas.clear();
        assert!(sand.render(Duration::from_millis(t), &mut canvas));
    }
    canvas
}

/// The pile, a character per cell: wall, grains (hex), free.
fn rows(sand: &Sand) -> Vec<String> {
    (0..sand.h)
        .map(|cy| {
            (0..sand.w)
                .map(|cx| {
                    if sand.wall(cx, cy) {
                        return '#';
                    }
                    let n = (0..DW * DH)
                        .filter(|k| {
                            matches!(sand.at(cx * DW + k % DW, cy * DH + k / DW), Dot::Grain(..))
                        })
                        .count();
                    match n {
                        0 => ' ',
                        n => char::from_digit(n as u32, 16).unwrap(),
                    }
                })
                .collect()
        })
        .collect()
}

fn look(sand: &mut Sand, rows: &[&str], cursor: Option<(u16, u16)>) {
    let rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    sand.look(&Under {
        rows: &rows,
        cursor,
    });
}

/// Grains in the cells `[x0, x1)` of row `y`.
fn count(sand: &Sand, y: usize, x0: usize, x1: usize) -> usize {
    rows(sand)[y][x0..x1]
        .chars()
        .filter_map(|c| c.to_digit(16))
        .sum::<u32>() as usize
}

#[test]
#[ignore = "prints a pile: cargo test -p ptyrc-draw-sand show -- --ignored --nocapture"]
fn show() {
    let mut walls = vec![""; 15];
    walls[7] = "           a shelf of words";
    walls[14] = "     lower";
    let mut sand = make(json!({"walls": walls, "rate": 150}));
    let mut canvas = Canvas::new(60, 20);
    for t in (0..=30_000u64).step_by(FRAME_MS as usize) {
        canvas.clear();
        sand.render(Duration::from_millis(t), &mut canvas);
        if t % 4992 != 0 {
            continue;
        }
        println!("{t} ms, {} grains, {:?}", sand.grains(), sand.spout);
        for y in 0..sand.h {
            let row: String = (0..sand.w)
                .map(|x| match sand.wall(x, y) {
                    true => '#',
                    false => sand.glyph(x, y).map_or(' ', |g| g.0),
                })
                .collect();
            println!("|{row}|");
        }
    }
}

#[test]
fn a_thin_stream_from_the_middle_makes_a_pile() {
    let mut sand = make(json!({"rate": 100}));
    let canvas = run(&mut sand, 21, 8, 0, 3000);
    assert_eq!(sand.spout.map(|s| (s.x, s.y)), Some((10, 0)));
    let rows = rows(&sand);
    // The stream: one cell wide down to the pile.
    for row in &rows[..3] {
        assert_eq!(row.trim().len(), 1, "{rows:#?}");
        assert_eq!(row.find(|c| c != ' '), Some(10), "{rows:#?}");
    }
    // The pile: on the floor, under the spout, with slopes.
    let floor = rows[7].trim();
    assert!(floor.len() > 9 && floor.len() < 21, "{rows:#?}");
    assert!(rows[7].find(|c| c != ' ').unwrap() < 10, "{rows:#?}");
    assert!(rows[6].trim().len() < floor.len(), "{rows:#?}");
    assert!(sand.grains() > 280 && sand.grains() <= 300);
    assert_eq!(sand.glyph(10, 7).map(|g| g.0), Some('⣿'));
    assert_ne!(canvas, Canvas::new(21, 8));
}

#[test]
fn fills_the_canvas() {
    let mut sand = make(json!({"rate": 400}));
    run(&mut sand, 8, 3, 0, 20_000);
    assert_eq!(sand.grains(), 8 * 3 * DW * DH, "{:#?}", rows(&sand));
}

#[test]
fn lands_on_text_and_spills() {
    let walls = ["", "", "", "###### ########", "", "", ""];
    let mut sand = make(json!({"rate": 100, "walls": walls}));
    run(&mut sand, 30, 7, 0, 400);
    // Over the text nearest the middle, three cells in from its end.
    assert_eq!(
        sand.spout,
        Some(Spout {
            x: 11,
            y: 0,
            on_text: true
        })
    );
    assert!(count(&sand, 2, 7, 15) > 10, "{:#?}", rows(&sand));
    assert_eq!(count(&sand, 6, 0, 30), 0, "{:#?}", rows(&sand));
    run(&mut sand, 30, 7, 400, 12_000);
    let rows = rows(&sand);
    assert_eq!(&rows[3][..6], "######", "{rows:#?}");
    assert_eq!(&rows[3][7..15], "########", "{rows:#?}");
    // Spilt over the end of the text and through the gap, on the floor.
    assert!(count(&sand, 6, 15, 30) > 20, "{rows:#?}");
    assert!(count(&sand, 6, 4, 9) > 8, "{rows:#?}");
}

#[test]
fn new_text_eats_the_sand_and_gone_text_drops_it() {
    let mut sand = make(json!({"rate": 100}));
    look(&mut sand, &["", "", "", "  ########"], None);
    run(&mut sand, 12, 6, 0, 4000);
    let before = sand.grains();
    let held = count(&sand, 2, 0, 12);
    assert!(held > 30 && count(&sand, 1, 0, 12) > 0);

    // Written over the pile: only the cells written lose their grains.
    look(&mut sand, &["", "", "    ####", "  ########"], None);
    let mut canvas = Canvas::new(12, 6);
    sand.render(Duration::from_millis(4000), &mut canvas);
    assert_eq!(&rows(&sand)[2][4..8], "####");
    let eaten = before - sand.grains();
    assert!(eaten > 0 && eaten < held, "{eaten} of {held}");
    assert!(count(&sand, 1, 0, 12) > 0, "the rest stays");

    // The text goes: what it held falls to the floor.
    look(&mut sand, &[], None);
    sand.params.rate = 0.0;
    run(&mut sand, 12, 6, 4000, 6000);
    let rows = rows(&sand);
    assert!(!rows.concat().contains('#'));
    assert_eq!(
        count(&sand, 0, 0, 12) + count(&sand, 1, 0, 12),
        0,
        "{rows:#?}"
    );
    assert!(count(&sand, 5, 0, 12) > 30, "{rows:#?}");
}

#[test]
fn a_resize_keeps_the_sand_inside() {
    let mut sand = make(json!({"rate": 100}));
    run(&mut sand, 20, 6, 0, 4000);
    sand.params.rate = 0.0;
    run(&mut sand, 20, 6, 4000, 5000);
    let before = rows(&sand);
    let all = sand.grains();

    // Narrower and shorter: the top-left stays, the rest is cut off.
    run(&mut sand, 12, 5, 5000, 5000);
    let after = rows(&sand);
    assert_eq!(after.len(), 5);
    for (a, b) in after.iter().zip(&before) {
        assert_eq!(a, &b[..12]);
    }
    let kept = sand.grains();
    assert!(kept > 0 && kept < all);

    // Taller: the pile falls to the new floor, whole.
    run(&mut sand, 12, 9, 5000, 7000);
    assert_eq!(sand.grains(), kept);
    assert_eq!(count(&sand, 3, 0, 12), 0, "{:#?}", rows(&sand));
    assert!(count(&sand, 8, 0, 12) > 0);
}

#[test]
fn the_spout_follows_the_cursor() {
    let mut sand = make(json!({"rate": 100, "cursor": true}));
    look(
        &mut sand,
        &["", "", "", "", "", "         ##"],
        Some((4, 3)),
    );
    run(&mut sand, 20, 6, 0, 1000);
    assert_eq!(sand.spout.map(|s| (s.x, s.y)), Some((4, 0)));
    assert!(count(&sand, 0, 4, 5) > 0 && count(&sand, 0, 5, 20) == 0);
    // Under text: from the first free cell of the column.
    look(&mut sand, &["    ##", "    ##"], Some((5, 3)));
    run(&mut sand, 20, 6, 1000, 1100);
    assert_eq!(sand.spout.map(|s| (s.x, s.y)), Some((5, 2)));
    // Column 0, or no cursor: the spout of a sand without `cursor`.
    for cursor in [Some((0, 3)), None] {
        look(&mut sand, &[], cursor);
        run(&mut sand, 20, 6, 1100, 1200);
        assert_eq!(sand.spout.map(|s| (s.x, s.y)), Some((9, 0)));
    }
}

#[test]
fn a_spout_holds_until_it_is_blocked() {
    let mut sand = make(json!({"rate": 100}));
    look(&mut sand, &["", "", "     ####"], None);
    run(&mut sand, 20, 6, 0, 500);
    let first = sand.spout.unwrap();
    assert_eq!((first.x, first.on_text), (7, true));
    // Text elsewhere: it stays.
    look(
        &mut sand,
        &["", "", "     ####", "", "         ######"],
        None,
    );
    run(&mut sand, 20, 6, 500, 600);
    assert_eq!(sand.spout, Some(first));
    // Text on it: the next free cell of its column, if still the best.
    look(&mut sand, &["########", "", "     ####"], None);
    run(&mut sand, 20, 6, 600, 700);
    let next = sand.spout.unwrap();
    assert!(!sand.wall(next.x, next.y) && next != first);
}

#[test]
fn colors_drift_into_strata() {
    let colors = ["#000000", "#100000", "#200000"];
    let mut sand = make(json!({"rate": 150, "colors": colors, "color_ms": 1000}));
    assert_eq!(sand.shades.len(), 3 * SHADES);
    assert_eq!(sand.shades[0], Color::Rgb(0, 0, 0));
    assert_eq!(sand.shades[SHADES / 2], Color::Rgb(8, 0, 0));
    assert_eq!(sand.shades[3 * SHADES - 1], Color::Rgb(2, 0, 0));
    run(&mut sand, 20, 8, 0, 2500);
    let shade = |x: usize, y: usize| match sand.at(x, y) {
        Dot::Grain(shade, _) => Some(shade as usize),
        _ => None,
    };
    // Old at the bottom, new on top, in between neighbours.
    let bottom = shade(20, 31).unwrap();
    let top = (0..32).find_map(|y| shade(20, y).or(shade(21, y))).unwrap();
    assert!(bottom < SHADES / 2, "{bottom}");
    assert!(top >= 2 * SHADES, "{top}");
    // A cell shows the mean of its grains: no step at a stratum.
    let mut cell = make(json!({"rate": 0, "colors": ["#000000", "#100000"]}));
    run(&mut cell, 2, 1, 0, 0);
    cell.set(0, 3, Dot::Grain(0, false));
    cell.set(1, 3, Dot::Grain(8, false));
    assert_eq!(cell.glyph(0, 0), Some(('⣀', Color::Rgb(4, 0, 0))));
    // Indexed colours of the cube mix; the terminal's 16 do not.
    assert_eq!(rgb(Color::Indexed(196)), Some([255, 0, 0]));
    assert_eq!(rgb(Color::Indexed(244)), Some([128, 128, 128]));
    assert_eq!(rgb(Color::Indexed(3)), None);
    let (plain, per) = shades(&[ColorSpec(Color::Indexed(3)), ColorSpec(Color::Indexed(4))]);
    assert_eq!(plain[per - 1], Color::Indexed(3));
}

#[test]
fn an_avalanche_runs_down_a_cliff() {
    // A column of sleeping grains, eight dots high, alone on the floor.
    let mut sand = make(json!({"rate": 0}));
    run(&mut sand, 10, 3, 0, 0);
    for y in 4..12 {
        sand.set(10, y, Dot::Grain(0, false));
    }
    run(&mut sand, 10, 3, 0, 2000);
    let top = (0..12)
        .find(|y| (0..20).any(|x| !sand.free(x, *y)))
        .unwrap();
    assert_eq!(sand.grains(), 8);
    assert!(top >= 8, "the column fell: {:#?}", rows(&sand));
    // A step of two dots holds.
    let mut sand = make(json!({"rate": 0}));
    run(&mut sand, 10, 3, 0, 0);
    for y in 10..12 {
        sand.set(10, y, Dot::Grain(0, false));
    }
    run(&mut sand, 10, 3, 0, 2000);
    assert!(!sand.free(10, 10));
}

#[test]
fn same_seed_same_pile() {
    let pile = |seed: u64| {
        let mut sand = make(json!({"seed": seed}));
        run(&mut sand, 20, 8, 0, 3000);
        sand.dots.clone()
    };
    assert!(pile(7) == pile(7));
    assert!(pile(7) != pile(8));
}

#[test]
fn a_late_render_catches_up_bounded() {
    let mut sand = make(json!({}));
    let mut canvas = Canvas::new(8, 4);
    assert!(sand.render(Duration::from_secs(3600), &mut canvas));
    assert_eq!(sand.steps, 3600 * 1000 / STEP_MS);
    assert!(sand.grains() <= CATCH_UP as usize * 2);
}

#[test]
fn ends_and_rejects() {
    let mut sand = make(json!({"duration_ms": 500}));
    let mut canvas = run(&mut sand, 8, 4, 0, 400);
    assert!(!sand.render(Duration::from_millis(500), &mut canvas));
    assert!(super::sand(&json!({"colors": []})).is_err());
    assert!(super::sand(&json!({"rate": -1})).is_err());
    assert!(super::sand(&json!({"color_ms": 0})).is_err());
    assert!(super::sand(&json!({"cursor": true})).is_ok());
    assert!(super::sand(&json!(null)).is_ok());
}
