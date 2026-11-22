// SPDX-License-Identifier: GPL-3.0-or-later
//! The twin's tests (`test_shore.py`) and its pictures (`twin.rs`).

use super::*;
use crate::grid::fnv;
use crate::twin;
use ptyrc_app::{Value, json};
use std::collections::HashSet;

fn s(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|r| r.to_string()).collect()
}

fn text_of(name: &str) -> Vec<String> {
    let mut rows = Vec::new();
    match name {
        "words" => rows = s(&["", "", "words in the way", "", "   more words here"]),
        "low" => {
            rows = vec![String::new(); 20];
            rows.push("   a line of text low on the pane, in the water".into());
        }
        "block" => {
            rows = vec![String::new(); 30];
            rows.extend(std::iter::repeat_n(
                format!("{}{}", " ".repeat(40), "x".repeat(60)),
                6,
            ));
        }
        _ => {}
    }
    rows
}

/// A shore of `Random(seed)` with the twin's keywords.
fn shore(seed: u64, kw: Value) -> Shore {
    let d = Config::default();
    let num = |k: &str, v: f64| kw[k].as_f64().unwrap_or(v);
    let on = |k: &str| kw[k].as_bool() == Some(true);
    let c = Config {
        period: num("period", d.period),
        ebb: num("ebb", d.ebb),
        grains: num("grains", d.grains),
        day: num("day", d.day),
        drift: on("drift"),
        ripples: on("ripples"),
        crabs: on("crabs"),
        birds: on("birds"),
        walker: on("walker"),
    };
    Shore::new(&mut Twister::new(seed), c)
}

fn floats(xs: &[f64]) -> u32 {
    let text: Vec<String> = xs.iter().map(|x| x.to_bits().to_string()).collect();
    fnv(&text.join(","))
}

fn inks(grid: &Grid, rows: std::ops::Range<usize>) -> HashSet<u8> {
    rows.flat_map(|y| grid.row(y).map(|c| c.2).collect::<Vec<_>>())
        .collect()
}

fn glyphs(grid: &Grid) -> HashSet<char> {
    (0..grid.height())
        .flat_map(|y| grid.row(y).map(|c| c.1).collect::<Vec<_>>())
        .collect()
}

fn found(grid: &Grid, wanted: &str) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for y in 0..grid.height() {
        for (x, g, _) in grid.row(y) {
            if wanted.contains(g) {
                out.push((x as i64, y as i64));
            }
        }
    }
    out
}

fn day(inks: &[Ink], i: usize) -> HashSet<u8> {
    inks.iter().map(|k| k[i]).collect()
}

fn pic(s: &mut Shore, w: usize, h: usize, t: f64) -> Grid {
    s.picture(w, h, t, &[], None)
}

#[test]
fn the_twins_shore() {
    let lace: String = (lace().iter().flatten())
        .map(|v| format!("{},", (v * 4096.0) as i64))
        .collect();
    assert_eq!(fnv(&lace), twin::LACE);
    let grit: Vec<f64> = grit().iter().flatten().copied().collect();
    assert_eq!(floats(&grit), twin::GRIT);
    for &(seed, period, ebb, k, across, h, late, run, at, edges) in twin::WAVES {
        let c = Config {
            period,
            ebb,
            ..Config::default()
        };
        let s = Shore::new(&mut Twister::new(seed), c);
        let one = s.wave(k, across, h);
        assert_eq!(floats(&one.late), late, "late of {seed}");
        assert_eq!(floats(&one.run), run, "run of {seed}");
        assert_eq!(floats(&[one.at, one.bend]), at, "at of {seed}");
        let mut got = Vec::new();
        for i in 0..12 {
            for px in [0, across as i64 / 3, across as i64 - 1] {
                got.push(s.edge(px, k as f64 * period + 0.7 * i as f64, across, h));
            }
        }
        assert_eq!(floats(&got), edges, "edges of {seed}");
    }
    for &(name, w, h, digest) in twin::ROCKS {
        let rocks: String = (rocks_of(&text_of(name), w, h).iter())
            .map(|(x, y)| format!("{x},{y};"))
            .collect();
        assert_eq!(fnv(&rocks), digest, "rocks of {name}");
    }
    for &(seed, kw, (w, h), name, t0, ticks, touches, at) in twin::RUNS {
        let mut s = shore(seed, kw.parse().unwrap());
        let rocks = rocks_of(&text_of(name), w, h);
        let mut pointer = Pointer::default();
        let mut pics = Vec::new();
        for i in 0..=ticks {
            let t = t0 + i as f64 / 8.0;
            for &(k, x, y) in touches {
                if k == i {
                    if x < 0 {
                        pointer.leave(t);
                    } else {
                        pointer.move_to(x, y, t);
                    }
                }
            }
            let grid = s.picture(w, h, t, &rocks, Some(&pointer));
            if at.iter().any(|a| a.0 == i) {
                pics.push((i, fnv(&grid.text())));
            }
        }
        assert_eq!(pics, at, "run {seed} {kw} {w}x{h}");
    }
}

#[test]
fn swash_tide_light() {
    assert_eq!((swash(0.0), swash(1.0), swash(-1.0)), (0.0, 0.0, 0.0));
    assert!((swash(PEAK) - 1.0).abs() < 1e-9);
    let up: Vec<f64> = (0..=10).map(|i| swash(PEAK * i as f64 / 10.0)).collect();
    let back: Vec<f64> = (0..=10)
        .map(|i| swash(PEAK + (1.0 - PEAK) * i as f64 / 10.0))
        .collect();
    assert!(up.windows(2).all(|p| p[0] <= p[1]) && back.windows(2).all(|p| p[0] >= p[1]));
    let mid = tide(0.0, 300.0, 24);
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(near(mid, 96.0 * (HIGH + LOW) / 2.0));
    assert!(near(tide(75.0, 300.0, 24), 96.0 * HIGH));
    assert!(near(tide(225.0, 300.0, 24), 96.0 * LOW));
    assert!(near(tide(300.0, 300.0, 24), mid));
    assert_eq!(light(0.0, 240.0), (0, 0.0));
    assert_eq!(light(0.2 * 240.0, 240.0), (1, 0.0));
    assert_eq!(light(0.4 * 240.0, 240.0), (2, 0.0));
    let (palette, blend) = light(0.17 * 240.0, 240.0);
    assert!(palette == 0 && 0.0 < blend && blend < 1.0);
    assert_eq!(light(240.0, 240.0), light(0.0, 240.0));
}

#[test]
fn the_lace() {
    let net = lace();
    assert!(std::ptr::eq(net, lace()) && net.len() == TILE && net[0].len() == TILE);
    let mut flat: Vec<f64> = net.iter().flatten().copied().collect();
    flat.sort_by(f64::total_cmp);
    // As many dots under `q` as `q` says.
    assert_eq!(flat[0], 0.0);
    assert!((flat[flat.len() / 4] - 0.25).abs() < 1e-6);
}

#[test]
fn the_picture_is_a_function_of_the_time() {
    let (mut one, mut other) = (shore(1, json!({})), shore(1, json!({})));
    let late = pic(&mut one, 60, 20, 13.0);
    assert_ne!(pic(&mut one, 60, 20, 2.0), late);
    assert_eq!(pic(&mut other, 60, 20, 13.0), late);
    assert_ne!(pic(&mut shore(2, json!({})), 60, 20, 13.0), late);
    assert_eq!(pic(&mut one, 0, 0, 0.0).count(), 0);
    pic(&mut one, 1, 1, 5.0);
}

#[test]
fn sand_over_sea() {
    let grid = pic(&mut shore(1, json!({})), 80, 24, 3.0);
    let dry = day(&SAND, 0);
    let sea: HashSet<u8> = day(&SEA, 0).union(&day(&SHALLOW, 0)).copied().collect();
    assert!(inks(&grid, 0..6).is_subset(&dry));
    let low = inks(&grid, 22..24);
    assert!(!low.is_disjoint(&sea) && low.is_disjoint(&dry));
}

#[test]
fn a_wave_runs_up_and_back_and_leaves_a_line() {
    let one = shore(1, json!({"ebb": 1e9}));
    let wave = one.wave(2, 160, 24);
    assert!(Rc::ptr_eq(&wave, &one.wave(2, 160, 24)));
    let still = tide(0.0, 1e9, 24);
    let at = wave.at + wave.late[80];
    let stay = STAY * one.c.period;
    assert!(one.edge(80, at - 0.1, 160, 24) <= still);
    let top = one.edge(80, at + PEAK * stay, 160, 24);
    let want = still - wave.run[80];
    assert!((top - want).abs() <= 1e-6 * want.abs(), "{top} {want}");
    let later = one.edge(80, at + 0.7 * stay, 160, 24);
    assert!(top < later && later < still);

    let mut one = shore(1, json!({"ebb": 1e9}));
    let lines = day(&LINE, 0);
    let period = one.c.period;
    let before = pic(&mut one, 80, 24, wave.at - 0.5 * period);
    let after = pic(&mut one, 80, 24, wave.at + 0.9 * stay);
    let rows: HashSet<usize> = wave
        .run
        .iter()
        .map(|r| ((still - r) as i64 >> 2) as usize)
        .collect();
    assert!(
        rows.iter()
            .any(|&y| !inks(&after, y..y + 1).is_disjoint(&lines))
    );
    assert_ne!(before, after);
    // The foam of a wave on the sand.
    let up = pic(&mut one, 80, 24, wave.at + 0.2 * stay);
    assert!(inks(&up, 0..24).contains(&FOAM[0][0]));
}

#[test]
fn froth_on_the_rocks() {
    let rocks = rocks_of(&s(&["", "", " ab"]), 8, 4);
    let want: Vec<(i64, i64)> = (0..4).flat_map(|x| [(x, 1), (x, 3)]).collect();
    assert_eq!(rocks, want);
    assert!(rocks_of(&s(&["ab", "cd"]), 2, 2).is_empty());
    let mut one = shore(1, json!({"grains": 0}));
    let low: Vec<(i64, i64)> = (0..80).map(|x| (x, 22)).collect();
    let bare = pic(&mut one, 80, 24, 3.0);
    assert_ne!(one.picture(80, 24, 3.0, &low, None), bare);
    // No froth on dry sand.
    let dry: Vec<(i64, i64)> = (0..80).map(|x| (x, 2)).collect();
    assert_eq!(one.picture(80, 24, 3.0, &dry, None), bare);
}

#[test]
fn extras() {
    let plain = pic(&mut shore(1, json!({})), 80, 24, 30.0);
    assert!(glyphs(&plain).iter().all(|g| !"ж›‹,•".contains(*g)));
    let mut full = shore(
        1,
        json!({"drift": true, "ripples": true, "crabs": true, "birds": true, "walker": true}),
    );
    let mut seen = HashSet::new();
    for t in (0..60).step_by(3) {
        seen.extend(glyphs(&pic(&mut full, 80, 24, t as f64)));
    }
    assert!(seen.contains(&'ж') && (seen.contains(&'›') || seen.contains(&'‹')));
    assert!(FINDS.iter().any(|f| seen.contains(&f.0)));
    for kind in ["drift", "ripples", "walker"] {
        let mut kw = json!({});
        kw[kind] = json!(true);
        assert_ne!(pic(&mut shore(1, kw), 80, 24, 30.0), plain, "{kind}");
    }
}

#[test]
fn the_scare() {
    let mut p = Pointer::default();
    assert_eq!((scare(None, 10.0), scare(Some(&p), 10.0)), (None, None));
    p.move_to(7, 3, 10.0);
    assert_eq!(scare(Some(&p), 10.0), Some((7.0, 3.0, 1.0)));
    assert_eq!(scare(Some(&p), 10.0 + STILL), Some((7.0, 3.0, 1.0)));
    let (x, y, much) = scare(Some(&p), 10.0 + STILL + FADE / 2.0).unwrap();
    assert!((x, y) == (7.0, 3.0) && (much - 0.5).abs() < 1e-9);
    assert_eq!(scare(Some(&p), 10.0 + STILL + FADE), None);
    p.leave(11.0);
    assert!((scare(Some(&p), 11.0 + FADE / 2.0).unwrap().2 - 0.5).abs() < 1e-9);
}

#[test]
fn the_animals_flee_the_pointer() {
    let frame = 1.0 / 8.0;
    let mut now = 14.0;
    let mut p = Pointer::default();
    let mut one = shore(3, json!({"birds": true, "crabs": true}));
    let mut alone = shore(3, json!({"birds": true, "crabs": true}));
    let step = |one: &mut Shore, p: &Pointer, now: &mut f64, to: f64| {
        let mut grid = one.picture(80, 24, *now, &[], Some(p));
        while *now < to {
            *now += frame;
            grid = one.picture(80, 24, *now, &[], Some(p));
        }
        grid
    };
    // No pointer, or one that never came: the scene of the time.
    let grid = step(&mut one, &p, &mut now, 14.0);
    assert_eq!(grid, pic(&mut alone, 80, 24, 14.0));
    let birds = found(&grid, "›‹,");
    assert!(birds.len() >= 3 && !found(&grid, "ж").is_empty());
    // On a bird: those about it fly, and land away from it.
    let (bx, by) = birds[0];
    p.move_to(bx, by, now);
    let mut seen = HashSet::new();
    for _ in 0..12 {
        let to = now + frame;
        seen.extend(glyphs(&step(&mut one, &p, &mut now, to)));
    }
    assert!(seen.contains(&'v') || seen.contains(&'^'));
    let to = now + 1.0;
    let grid = step(&mut one, &p, &mut now, to);
    assert!(found(&grid, "v^").is_empty());
    for (x, y) in found(&grid, "›‹,") {
        assert!(((x - bx) as f64).hypot(2.0 * (y - by) as f64) >= NEAR);
    }
    // On a crab: it runs, and stays away.
    let (cx, cy) = found(&grid, "ж")[0];
    p.move_to(cx, cy, now);
    let mut grid = grid;
    for _ in 0..8 {
        let to = now + frame;
        grid = step(&mut one, &p, &mut now, to);
        p.move_to(cx, cy, now);
        assert!(!found(&grid, "ж").contains(&(cx, cy)));
    }
    assert!(!found(&grid, "ж").is_empty());
    for (x, y) in found(&grid, "ж") {
        assert!(((x - cx) as f64).hypot(2.0 * (y - cy) as f64) >= SHY - 1.0);
    }
    // It left: they come back, and the scene is that of the time again.
    p.leave(now);
    let to = now + 60.0;
    step(&mut one, &p, &mut now, to);
    let to = now;
    let back = step(&mut one, &p, &mut now, to);
    assert_eq!(back, pic(&mut alone, 80, 24, now));
    // A jump of the time: what it did to them is forgotten.
    let to = now;
    let (x, y) = found(&step(&mut one, &p, &mut now, to), "›‹,")[0];
    p.move_to(x, y, now);
    let to = now + 1.0;
    step(&mut one, &p, &mut now, to);
    assert_eq!(pic(&mut one, 80, 24, 500.0), pic(&mut alone, 80, 24, 500.0));
}

#[test]
fn night() {
    let mut one = shore(1, json!({"day": 240.0}));
    let light = inks(&pic(&mut one, 80, 24, 0.0), 0..24);
    let dark = inks(&pic(&mut one, 80, 24, 0.45 * 240.0), 0..24);
    assert!(light.contains(&FOAM[0][0]) && !light.contains(&MOON[0][2]));
    assert!(!day(&MOON, 2).is_disjoint(&dark));
    let only_day: HashSet<u8> = day(&SAND, 0).difference(&day(&SAND, 2)).copied().collect();
    assert!(only_day.is_disjoint(&dark));
}
