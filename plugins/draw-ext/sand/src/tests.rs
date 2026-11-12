// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use ptyrc_core::types::{
    CursorSample, FrameId, FrameSel, Prefer, SurfaceInfo, Surfaces, TextMatch, TextQuery,
};
use ptyrc_vt::{Grid, Rect};
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
    let grid = Grid::from_lines(64, if rows.is_empty() { &[""] } else { rows });
    let mut under = Under::of(&grid);
    under.cursor = cursor;
    under.origin = sand.origin.unwrap_or((0, 0));
    sand.look(&under);
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
    let rate: f64 = std::env::var("RATE")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(150.0);
    let mut sand = make(json!({"walls": walls, "rate": rate}));
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

#[test]
fn a_wide_glyph_is_two_walls_and_shifts_nothing() {
    let mut vt = ptyrc_vt::Vt::new(20, 4);
    vt.feed("\r\n\r\na\t世b".as_bytes());
    let mut sand = make(json!({"rate": 0}));
    sand.look(&Under::of(vt.grid()));
    run(&mut sand, 20, 4, 0, 0);
    assert_eq!(rows(&sand)[2], "#       ###         ");
}

/// `look` at `rows`, the canvas at the frame row `oy`.
fn look_at(sand: &mut Sand, rows: &[&str], oy: i64) {
    let grid = Grid::from_lines(64, rows);
    let mut under = Under::of(&grid);
    under.origin = (0, oy);
    sand.look(&under);
}

#[test]
fn the_pile_scrolls_with_the_text() {
    let text = ["", "", "", "   ##########", "", ""];
    let mut sand = make(json!({"rate": 100}));
    look_at(&mut sand, &text, 0);
    run(&mut sand, 16, 6, 0, 2880);
    sand.params.rate = 0.0;
    run(&mut sand, 16, 6, 2880, 4800);
    let before = rows(&sand);
    let all = sand.grains();
    assert!(count(&sand, 2, 0, 16) > 20, "{before:#?}");

    // The app scrolls two rows: the text and its pile are two rows up.
    look_at(&mut sand, &["", "   ##########", "", "", "", ""], 2);
    run(&mut sand, 16, 6, 4800, 4800);
    let after = rows(&sand);
    assert_eq!(after[0], before[2]);
    assert_eq!(after[1], before[3]);
    // What left the canvas is kept, and comes back.
    assert!(sand.grains() < all);
    look_at(&mut sand, &text, 0);
    run(&mut sand, 16, 6, 4800, 4800);
    assert_eq!(sand.grains(), all);
    assert_eq!(rows(&sand)[..4], before[..4]);

    // A jump far away and back: the same.
    look_at(&mut sand, &[""], 100_000);
    run(&mut sand, 16, 6, 4800, 4800);
    assert_eq!(sand.grains(), 0);
    look_at(&mut sand, &text, 0);
    run(&mut sand, 16, 6, 4800, 4800);
    assert_eq!(rows(&sand)[..4], before[..4]);
}

#[test]
fn sand_out_of_sight_is_bounded() {
    let mut sand = make(json!({"rate": 0}));
    look_at(&mut sand, &[""], 0);
    run(&mut sand, 4, 2, 0, 0);
    for k in 0..(ATTIC as i64 + 50) {
        sand.set(0, 0, Dot::Grain(0, false));
        sand.set(0, 4, Dot::Grain(0, false));
        look_at(&mut sand, &["", "####"], 2 * k + 2);
        run(&mut sand, 4, 2, 0, 0);
    }
    assert_eq!(sand.attic.len(), ATTIC);
    // The nearest rows are the ones kept.
    assert!(sand.attic.contains_key(&(2 * (ATTIC as i64 + 49))));
    assert!(!sand.attic.contains_key(&0));
}

#[test]
fn a_running_sand_takes_new_parameters() {
    let mut sand = make(json!({"rate": 100}));
    run(&mut sand, 20, 6, 0, 2000);
    let (grains, steps) = (sand.grains(), sand.steps);
    assert!(sand.tune(&json!({"rate": -1})).is_err());
    assert!(sand.tune(&json!({"grains": 4})).is_err());
    assert_eq!(sand.params.rate, 100.0);
    sand.tune(&json!({"rate": 0, "colors": ["#102030"]}))
        .unwrap();
    assert_eq!((sand.grains(), sand.steps), (grains, steps));
    run(&mut sand, 20, 6, 2000, 3000);
    assert_eq!(sand.grains(), grains);
    assert!(
        sand.glyph(10, 5)
            .is_some_and(|g| g.1 == Color::Rgb(0x10, 0x20, 0x30))
    );
}

#[test]
fn a_hot_column_is_a_worse_spout() {
    let mut sand = make(json!({"rate": 100}));
    let grid = Grid::new(21, 6);
    let n = 21 * 6;
    let mut heat = ptyrc_core::frames::HeatView {
        cols: 21,
        rows: 6,
        safe: vec![1.0; n],
        activity: vec![0.0; n],
        occupancy: vec![0.0; n],
        still: vec![60.0; n],
        class: vec![Default::default(); n],
    };
    for x in 8..13 {
        heat.class[3 * 21 + x] = HeatClass::Hot;
    }
    sand.look(&Under::of(&grid).with_heat(Some(&heat)));
    run(&mut sand, 21, 6, 0, 100);
    let x = sand.spout.unwrap().x;
    assert!(!(8..13).contains(&x) && (5..16).contains(&x), "{x}");
}

#[test]
fn a_heavy_stream_spreads_and_does_not_choke() {
    let mut sand = make(json!({"rate": 400}));
    run(&mut sand, 60, 20, 0, 4800);
    // All that was owed came out: no column stands under the spout.
    assert!(sand.grains() >= 1900, "{}", sand.grains());
    let rows = rows(&sand);
    let top = rows.iter().position(|r| r.trim().len() > 6).unwrap();
    assert!(top >= 12, "{rows:#?}");
    // The slopes are gentle: the floor is covered far wider than high.
    assert!(rows[19].trim().len() > 4 * (20 - top), "{rows:#?}");
    // A light stream is as it was: one dot a step, friction whole.
    let mut light = make(json!({"rate": 100}));
    run(&mut light, 60, 20, 0, 4800);
    let rows = super::tests::rows(&light);
    assert!(rows[19].trim().len() < 30, "{rows:#?}");
}

/// A frame that remembers rows of text, by frame row.
struct Memory(Vec<(i64, &'static str)>);

impl Surfaces for Memory {
    fn info(&self, _id: FrameId) -> Option<SurfaceInfo> {
        None
    }
    fn resolve(&self, _sel: &FrameSel) -> Option<FrameId> {
        None
    }
    fn find(&self, _id: FrameId, _query: &TextQuery, _known: bool) -> Vec<TextMatch> {
        Vec::new()
    }
    fn place(&self, _: FrameId, _: u16, _: u16, _: Prefer, _: &[Rect]) -> Option<(u16, u16)> {
        None
    }
    fn ink_row(&self, _id: FrameId, y: i64, x: i64, w: u16) -> Option<Vec<bool>> {
        let (_, text) = self.0.iter().find(|(row, _)| *row == y)?;
        let ink = |col: i64| text.chars().nth(col as usize).is_some_and(|c| c != ' ');
        Some((x..x + w as i64).map(|col| col >= 0 && ink(col)).collect())
    }
    fn cursor(&self) -> (u16, u16) {
        (0, 0)
    }
    fn cursor_trail(&self) -> Vec<CursorSample> {
        Vec::new()
    }
}

/// `look_at`, the canvas at the frame column `ox` too.
fn look_xy(sand: &mut Sand, rows: &[&str], at: (i64, i64)) {
    let grid = Grid::from_lines(64, rows);
    let mut under = Under::of(&grid);
    under.origin = at;
    sand.look(&under);
}

#[test]
fn a_sideways_scroll_keeps_the_grains() {
    let text = ["", "", "", "################"];
    let mut sand = make(json!({"rate": 100}));
    look_xy(&mut sand, &text, (0, 0));
    run(&mut sand, 16, 4, 0, 2400);
    sand.params.rate = 0.0;
    run(&mut sand, 16, 4, 2400, 4800);
    let (before, all) = (rows(&sand), sand.grains());
    assert!(all > 100);

    // Ten columns to the right: the left of the pile is out, and kept.
    look_xy(&mut sand, &text, (10, 0));
    run(&mut sand, 16, 4, 4800, 4800);
    let part = sand.grains();
    assert!(part > 0 && part < all, "{part} of {all}");
    assert_eq!(rows(&sand)[2][..6], before[2][10..]);
    // Further, then back: all of it, where it was.
    look_xy(&mut sand, &text, (40, 0));
    run(&mut sand, 16, 4, 4800, 4800);
    assert_eq!(sand.grains(), 0);
    look_xy(&mut sand, &text, (0, 0));
    run(&mut sand, 16, 4, 4800, 4800);
    assert_eq!(sand.grains(), all);
    assert_eq!(rows(&sand)[..3], before[..3]);
    assert!(sand.attic.is_empty());
}

#[test]
fn a_row_out_of_sight_is_bounded_in_width() {
    let mut sand = make(json!({"rate": 0}));
    look_xy(&mut sand, &["", ""], (0, 0));
    run(&mut sand, 4, 2, 0, 0);
    for k in 1..400 {
        sand.set(0, 7, Dot::Grain(0, false));
        look_xy(&mut sand, &["", ""], (4 * k, 0));
        run(&mut sand, 4, 2, 0, 0);
    }
    let strip = &sand.attic[&1];
    assert!(strip.w > WIDE / 2 && strip.w <= WIDE, "{}", strip.w);
    assert!(strip.x0 > 0);
}

#[test]
fn a_shift_moves_the_grains_of_its_rows() {
    let text = ["", "", "", "   ##########", "", ""];
    let mut sand = make(json!({"rate": 100}));
    look_at(&mut sand, &text, 0);
    run(&mut sand, 16, 6, 0, 2880);
    sand.params.rate = 0.0;
    run(&mut sand, 16, 6, 2880, 4800);
    let before = rows(&sand);

    // Two rows are spliced in over the text: rows 1.. are two lower.
    Effect::shift(&mut sand, 1, 100, 2);
    look_at(&mut sand, &["", "", "", "", "", "   ##########"], 0);
    run(&mut sand, 16, 6, 4800, 4800);
    let after = rows(&sand);
    assert_eq!(after[4], before[2]);
    assert_eq!(after[3], before[1]);
    // Rows out of the canvas too.
    Effect::shift(&mut sand, 1, 100, 40);
    look_at(&mut sand, &[""], 0);
    run(&mut sand, 16, 6, 4800, 4800);
    assert_eq!(sand.attic.keys().next(), Some(&43));
}

#[test]
fn sand_falls_out_of_sight_onto_remembered_text() {
    // The frame remembers three rows under the view; the second is text.
    let memory = Memory(vec![(4, ""), (5, "################"), (6, "")]);
    let grid = Grid::from_lines(64, &["", "", "", ""]);
    let mut sand = make(json!({"rate": 100}));
    let below = Grid::from_lines(64, &["", "################", "", ""]);
    let look = |sand: &mut Sand, oy: i64| {
        let grid = if oy == 0 { &grid } else { &below };
        let mut under = Under::of(grid).with_frame(&memory, 1);
        under.origin = (0, oy);
        sand.look(&under);
    };
    look(&mut sand, 0);
    for k in 0..40 {
        run(&mut sand, 16, 4, k * 48, k * 48);
        look(&mut sand, 0);
    }
    // The world is the view and the three rows; the pile is on row 4,
    // over the text of row 5, not on the bottom of the view.
    assert_eq!((sand.h, sand.top, sand.ch), (7, 0, 4));
    assert!(count(&sand, 4, 0, 16) > 20, "{:#?}", rows(&sand));
    assert_eq!(count(&sand, 6, 0, 16), 0);

    // The view goes down four rows: the pile is there.
    let pile = rows(&sand)[4].clone();
    sand.params.rate = 0.0;
    look(&mut sand, 4);
    run(&mut sand, 16, 4, 1920, 1920);
    assert_eq!(sand.top, 0);
    assert_eq!(rows(&sand)[0], pile);
}
