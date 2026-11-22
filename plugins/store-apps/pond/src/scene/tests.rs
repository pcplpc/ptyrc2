// SPDX-License-Identifier: GPL-3.0-or-later
//! The twin's tests (`test_pond.py`) and its pictures (`twin.rs`).

use super::*;
use crate::grid::fnv;
use crate::twin;
use std::collections::HashSet;

fn s(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|r| r.to_string()).collect()
}

/// A pond of `Random(seed)` with the twin's keywords.
fn pond(seed: u64, kw: Value) -> Pond {
    let d = Config::default();
    let num = |k: &str, v: f64| kw[k].as_f64().unwrap_or(v);
    let on = |k: &str| kw[k].as_bool() == Some(true);
    let c = Config {
        pads: num("pads", d.pads),
        drops: num("drops", d.drops),
        koi: on("koi"),
        lotus: on("lotus"),
        rain: on("rain"),
        moon: on("moon"),
        adrift: on("adrift"),
        striders: on("striders"),
        phase: num("phase", d.phase),
        night: num("night", d.night),
        season: (kw["season"].as_str())
            .map_or(d.season, |n| SEASONS.iter().position(|s| *s == n).unwrap()),
        current: num("current", d.current),
    };
    Pond::new(&mut Twister::new(seed), c)
}

/// The cells of `grid` in one of `inks`.
fn inked(grid: &Grid, inks: &[u8]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for y in 0..grid.height() {
        for (x, _, ink) in grid.row(y) {
            if inks.contains(&ink) {
                out.push((x as i64, y as i64));
            }
        }
    }
    out
}

fn glyphs(grid: &Grid) -> Vec<char> {
    (0..grid.height())
        .flat_map(|y| grid.row(y).map(|c| c.1).collect::<Vec<_>>())
        .collect()
}

fn braille(g: char) -> u32 {
    let n = g as u32;
    if (0x2801..0x2900).contains(&n) {
        n - 0x2800
    } else {
        0
    }
}

fn f6(x: f64) -> String {
    format!("{x:.6}")
}

fn ripples(rs: &[Ripple]) -> String {
    (rs.iter())
        .map(|r| {
            format!(
                "{},{},{},{},{},{},{}",
                r.key,
                f6(r.x),
                f6(r.y),
                f6(r.t0),
                f6(r.life),
                r.rings,
                f6(r.reach)
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn text_of(name: &str) -> Vec<String> {
    match name {
        "none" => Vec::new(),
        "words" => s(&["", "", "words in the way", "", "   more words here"]),
        "line" => {
            let mut r = vec![String::new(); 9];
            r.push("x".repeat(80));
            r
        }
        "block" => {
            let mut r = vec![String::new(); 8];
            r.extend(std::iter::repeat_n(
                format!("{}{}", " ".repeat(20), "x".repeat(30)),
                3,
            ));
            r
        }
        _ => unreachable!(),
    }
}

#[test]
fn the_twins_pond() {
    for &(seed, t, w, h, rate, digest) in twin::DROPS {
        assert_eq!(
            fnv(&ripples(&plain_drops(seed, t, w, h, rate))),
            digest,
            "drops {seed} {t}"
        );
    }
    for &(seed, t, digest) in twin::SHOWERS {
        let p = pond(seed, json!({"rain": true}));
        let sd = p.seed;
        let ((l0, l1), (r0, r1), n) = SHOWERS;
        let rs = drops(
            sd + 1,
            t,
            80,
            24,
            &|s| Pond::shower(sd, s),
            (l0, l1),
            (r0, r1),
            n,
        );
        assert_eq!(fnv(&ripples(&rs)), digest, "showers {seed} {t}");
    }
    for &(name, x, y, reach, digest) in twin::SHADOWS {
        let walls = index(&text_of(name), 40, 12);
        let got = match shadow(x, y, reach, &walls) {
            None => 0,
            Some((near, far)) => {
                let part = |v: &[f64]| {
                    (v.iter())
                        .map(|&x| {
                            if x == f64::INFINITY {
                                "inf".into()
                            } else {
                                f6(x)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                };
                fnv(&format!("{};{}", part(&near), part(&far)))
            }
        };
        assert_eq!(got, digest, "shadow {name} {x} {y}");
    }
    for &(seed, kw, (w, h), name, ticks, touches, pics, kisses) in twin::RUNS {
        let mut p = pond(seed, kw.parse().unwrap());
        let walls = index(&text_of(name), w, h);
        let mut at = pics.iter();
        let mut next = at.next();
        for i in 0..=ticks {
            let t = f64::from(i) / 20.0;
            for &(k, x, y) in touches {
                if k == i {
                    p.touch(x, y, t);
                }
            }
            p.step(1.0 / 20.0, w, h, t, Some(&walls));
            if let Some(&(k, digest)) = next
                && k == i
            {
                let got = p.picture(w, h, t, Some(&walls));
                assert_eq!(fnv(&got.text()), digest, "run {seed} {kw} at tick {i}");
                next = at.next();
            }
        }
        assert_eq!(fnv(&ripples(&p.kisses)), kisses, "kisses {seed} {kw}");
    }
    let mut rng = Twister::new(7);
    let mut fly = Fly::new(&mut rng, 80, 24);
    let mut cells = String::new();
    for _ in 0..800 {
        let (x, y) = fly.step(&mut rng, 1.0 / 8.0, 80, 24, None);
        cells += &format!("{x},{y};");
    }
    for _ in 0..60 {
        let (x, y) = fly.step(&mut rng, 1.0 / 20.0, 80, 24, Some((40.0, 12.0, 1.0)));
        cells += &format!("{x},{y};");
    }
    assert_eq!((fnv(&cells), fly.inks), twin::FLY);
}

/// `test_drops`.
#[test]
fn a_drop_falls_once() {
    let at = plain_drops(3, 50.0, 80, 24, 240.0);
    assert!(!at.is_empty() && plain_drops(3, 50.0, 80, 24, 240.0) == at);
    assert!(plain_drops(4, 50.0, 80, 24, 240.0) != at);
    assert!(plain_drops(3, 50.0, 80, 24, 0.0).is_empty());
    for one in &at {
        assert!(one.t0 <= 50.0 && 50.0 < one.t0 + one.life && (1.4..=3.2).contains(&one.life));
        assert!((0.0..160.0).contains(&one.x) && (0.0..96.0).contains(&one.y));
        assert!((1..=2).contains(&one.rings) && (6.0..=20.0).contains(&one.reach));
    }
    let mut keys: HashMap<i64, Ripple> = HashMap::new();
    for tick in 0..2400 {
        for one in plain_drops(3, tick as f64 / 8.0, 80, 24, 24.0) {
            assert_eq!(*keys.entry(one.key).or_insert(one), one);
        }
    }
    // 24 a minute, 5 minutes.
    assert!((90..=150).contains(&keys.len()), "{}", keys.len());
    let wide: HashSet<i64> = (0..2400)
        .flat_map(|t| plain_drops(3, t as f64 / 8.0, 160, 48, 24.0))
        .map(|r| r.key)
        .collect();
    let n = keys.len() as f64;
    assert!(3.2 * n < wide.len() as f64 && (wide.len() as f64) < 4.8 * n);
    let ((l0, l1), (r0, r1), k) = SHOWERS;
    let rain = drops(
        3,
        9.0,
        80,
        24,
        &|s| if s >= 8 { 600.0 } else { 0.0 },
        (l0, l1),
        (r0, r1),
        k,
    );
    assert!(!rain.is_empty());
    assert!(
        rain.iter()
            .all(|r| r.t0 >= 8.0 && r.life <= 1.3 && r.rings == 1 && r.reach <= 6.0)
    );
}

/// `test_ring`.
#[test]
fn a_ring() {
    let one = Ripple::new(7, 0.0, 0.0, 0.0, 2.0, 2, 12.0);
    assert_eq!(one.ring(0.0, 0), Some((0.0, 1.0)));
    assert!(one.ring(-0.1, 0).is_none() && one.ring(2.0, 0).is_none());
    // It leaves fast and slows down: half its way in a fifth of its time.
    let wide: Vec<f64> = (0..20)
        .map(|t| one.ring(t as f64 / 10.0, 0).unwrap().0)
        .collect();
    assert!(wide.is_sorted() && wide[4] > 5.8 && wide[19] > 11.9);
    let faint: Vec<f64> = (0..20)
        .map(|t| one.ring(t as f64 / 10.0, 0).unwrap().1)
        .collect();
    assert!(faint.iter().rev().is_sorted() && faint[19] < 0.06);
    // The second: later, smaller, fainter, and over with the first.
    assert!(one.ring(0.4, 1).is_none());
    for t in [0.5, 1.0, 1.9] {
        let ((r0, s0), (r1, s1)) = (one.ring(t, 0).unwrap(), one.ring(t, 1).unwrap());
        assert!(r1 < r0 - 3.0 && s1 < s0);
    }
    assert!((one.ring(1.99, 1).unwrap().0 - 8.4).abs() < 0.01);
}

/// `test_shadow`.
#[test]
fn the_shadow_of_the_words() {
    assert_eq!(index(&s(&["", "  ab", "c"]), 3, 2).held(), [1]);
    assert_eq!(index(&s(&["", "  ab", "c"]), 3, 2).get(1), [2]);
    let mut rows = s(&["", ""]);
    rows.push(format!("   {}", "x".repeat(30)));
    rows.extend(s(&["", "", "", "      o"]));
    let walls = index(&rows, 40, 8);
    assert_eq!(walls.held(), [2, 6]);
    assert_eq!(walls.get(2).len(), 30);
    let free = shadow(40.0, 20.0, 30.0, &Walls::default()).unwrap();
    assert!(free.0.iter().all(|d| *d == f64::INFINITY));
    assert!(shadow(13.0, 26.0, 30.0, &walls).is_none()); // on the `o`
    assert!(shadow(20.0, 10.0, 30.0, &walls).is_none()); // on the line
    let (near, far) = shadow(36.0, 26.0, 40.0, &walls).unwrap();
    let up = 3 * BINS / 4; // the line, above
    assert!((12.0..=16.0).contains(&near[up]) && far[up] > near[up] + 100.0);
    let left = BINS / 2; // the `o`, a stone: the ring closes behind
    assert!((18.0..=22.0).contains(&near[left]));
    assert!(near[left] + 6.0 <= far[left] && far[left] < near[left] + 20.0);
    assert_eq!(near[BINS / 4], f64::INFINITY); // below: nothing
    // Out of reach: not a shadow.
    let short = shadow(36.0, 26.0, 8.0, &walls).unwrap();
    assert!(short.0.iter().all(|d| *d == f64::INFINITY));
}

/// `test_pond_is_a_map`.
#[test]
fn the_pond_is_a_map() {
    let mut water = pond(1, json!({}));
    let small = water.picture(40, 12, 0.0, None);
    assert_eq!(water.picture(40, 12, 0.0, None), small);
    assert_eq!(pond(1, json!({})).picture(40, 12, 0.0, None), small);
    assert!(pond(2, json!({})).picture(40, 12, 0.0, None) != small);
    let large = water.picture(80, 24, 0.0, None);
    assert_eq!(water.pads.len(), 6);
    assert_eq!(water.leaves(40, 12, 0.0).len(), 1);
    assert!(glyphs(&large).iter().all(|g| braille(*g) > 0));
    let greens = inked(&large, &GREENS).len();
    assert!((15..=90).contains(&greens), "{greens}");
    assert_eq!(water.picture(0, 5, 0.0, None).count(), 0);
    let mut bare = pond(1, json!({"pads": 0, "drops": 0}));
    let all: Vec<u8> = GREENS.iter().chain(&RING).copied().collect();
    assert!(inked(&bare.picture(80, 24, 3.0, None), &all).is_empty());
    // The time: it moves, and a moment is the same at every call.
    let looks: Vec<String> = (0..80)
        .map(|t| water.picture(80, 24, t as f64 / 8.0, None).text())
        .collect();
    assert!(looks.iter().collect::<HashSet<_>>().len() > 60);
    assert_eq!(water.picture(80, 24, 5.0, None).text(), looks[40]);
    // Pads of a cluster do not overlap.
    let leaves = pond(1, json!({"pads": 40})).leaves(80, 24, 0.0);
    for (i, (_, _, a)) in leaves.iter().enumerate() {
        for (_, _, b) in &leaves[..i] {
            if (a.u, a.v) == (b.u, b.v) {
                assert!((a.ox - b.ox).hypot(a.oy - b.oy) >= a.r + b.r);
            }
        }
        assert!((2.4..=5.6).contains(&a.r));
    }
}

/// `test_rings`.
#[test]
fn rings() {
    let mut water = pond(1, json!({"pads": 0, "drops": 0}));
    let one = Ripple::new(7, 80.0, 60.0, 0.0, 3.0, 2, 30.0);
    water.kisses = vec![one];
    assert_eq!(
        inked(&water.picture(80, 24, 0.005, None), &RING),
        [(40, 15)]
    );
    let mut spans = Vec::new();
    for t in [0.2, 0.6, 1.2, 2.4] {
        let cells = inked(&water.picture(80, 24, t, None), &RING);
        let far = (cells.iter())
            .map(|&(x, y)| ((2 * x + 1 - 80) as f64).hypot((4 * y + 2 - 60) as f64))
            .fold(0.0, f64::max);
        assert!((far - one.ring(t, 0).unwrap().0).abs() < 4.5);
        spans.push(far);
    }
    assert!(spans.is_sorted());
    let dots = |g: &Grid| {
        glyphs(g)
            .iter()
            .map(|c| braille(*c).count_ones())
            .sum::<u32>() as f64
    };
    let inks = |g: &Grid| {
        (0..g.height())
            .flat_map(|y| g.row(y).map(|c| c.2).collect::<Vec<_>>())
            .collect::<HashSet<u8>>()
    };
    // It fades: fewer dots a length, darker.
    let (young, old) = (
        water.picture(80, 24, 0.3, None),
        water.picture(80, 24, 2.8, None),
    );
    assert!(RING[..3].iter().any(|i| inks(&young).contains(i)));
    assert!(!RING[..4].iter().any(|i| inks(&old).contains(i)));
    assert!(
        dots(&young) / one.ring(0.3, 0).unwrap().0 > 2.0 * dots(&old) / one.ring(2.8, 0).unwrap().0
    );
    assert_eq!(
        water.picture(80, 24, 3.1, None),
        pond(1, json!({"pads": 0, "drops": 0})).picture(80, 24, 3.1, None)
    );
    // A line of words over it: no ring beyond.
    let mut rows = vec![String::new(); 9];
    rows.push("x".repeat(80));
    let walls = index(&rows, 80, 24);
    for t in [0.6, 1.2, 2.4] {
        let cells = inked(&water.picture(80, 24, t, Some(&walls)), &RING);
        assert!(!cells.is_empty() && cells.iter().all(|c| c.1 >= 9));
    }
    // A drop on a word, on a pad: no ring.
    water.kisses = vec![Ripple::new(8, 80.0, 38.0, 0.0, 3.0, 2, 30.0)];
    assert!(inked(&water.picture(80, 24, 1.0, Some(&walls)), &RING).is_empty());
    let mut padded = pond(1, json!({"drops": 600}));
    let leaves = padded.leaves(80, 24, 4.0);
    let fell = plain_drops(padded.seed, 4.0, 80, 24, 600.0);
    let (drawn, _) = padded.ripples(80, 24, 4.0, &leaves);
    assert!(!drawn.is_empty() && drawn.len() < fell.len());
    for one in &fell {
        let hit =
            (leaves.iter()).any(|&(x, y, p)| (one.x - x as f64).hypot(one.y - y as f64) < p.r);
        assert!(!(hit && drawn.contains(one)));
    }
}

/// `test_koi`.
#[test]
fn koi() {
    let mut rng = Twister::new(5);
    let mut rows = vec![String::new(); 8];
    rows.extend(std::iter::repeat_n(
        format!("{}{}", " ".repeat(20), "x".repeat(30)),
        3,
    ));
    let walls = index(&rows, 80, 24);
    let stone = |x: f64, y: f64| walls.stone(x, y);
    let (mut under, mut steps) = (0, 0);
    let mut fish = Koi::new(&mut rng, 80, 24);
    for n in 0..6 {
        if n > 0 {
            fish = Koi::new(&mut rng, 80, 24);
        }
        for _ in 0..2400 {
            fish.step(&mut rng, 1.0 / 8.0, 80, 24, &stone, None);
            assert!((-14.0..174.0).contains(&fish.x) && (-14.0..110.0).contains(&fish.y));
            assert!(fish.turn.abs() <= 1.6);
            under += usize::from(stone(fish.x, fish.y));
            steps += 1;
        }
        assert_eq!(fish.trail.len(), 24);
        for (a, b) in fish.trail.iter().zip(fish.trail.iter().skip(1)) {
            let gap = (a.0 - b.0).hypot(a.1 - b.1);
            assert!(0.9 < gap && gap < 1.1);
        }
    }
    assert!((under as f64) < 0.03 * steps as f64);
    // Out after a resize: it comes back.
    (fish.x, fish.y) = (300.0, 200.0);
    for _ in 0..800 {
        fish.step(&mut rng, 1.0 / 8.0, 40, 12, &stone, None);
    }
    assert!((0.0..80.0).contains(&fish.x) && (0.0..48.0).contains(&fish.y));
    let mut water = pond(5, json!({"pads": 0, "drops": 0, "koi": true}));
    let mut kisses: HashMap<i64, Ripple> = HashMap::new();
    for i in 0..400 {
        let t = i as f64 / 8.0;
        water.step(1.0 / 8.0, 80, 24, t, None);
        // A ring now and then; gone when it is over.
        assert!(
            water
                .kisses
                .iter()
                .all(|one| t - one.life < one.t0 && one.t0 <= t)
        );
        kisses.extend(water.kisses.iter().map(|one| (one.key, *one)));
    }
    assert!((3..=16).contains(&kisses.len()));
    assert!(
        kisses
            .values()
            .all(|o| (0.0..160.0).contains(&o.x) && (0.0..96.0).contains(&o.y))
    );
    assert_eq!(water.koi.as_ref().unwrap().len(), 2);
    let coats: Vec<u8> = COATS.iter().flat_map(|c| c.iter().copied()).collect();
    let drawn = inked(&water.picture(80, 24, 50.0, None), &coats).len();
    assert!((4..=40).contains(&drawn), "{drawn}");
    water.step(1.0 / 8.0, 200, 60, 50.0, None);
    assert_eq!(water.koi.as_ref().unwrap().len(), 5);
    water.step(1.0 / 8.0, 80, 24, 500.0, None);
    assert!(water.koi.as_ref().unwrap().len() == 2 && water.kisses.len() <= 2);
    let mut still = pond(5, json!({}));
    still.step(1.0 / 8.0, 80, 24, 0.0, None);
    assert!(still.koi.is_none());
}

/// `test_fear`.
#[test]
fn fear() {
    let mut water = pond(
        5,
        json!({"pads": 0, "drops": 0, "koi": true, "striders": true}),
    );
    assert!(water.fear(0.0).is_none());
    water.touch(81.0, 50.0, 10.0);
    assert_eq!(water.fear(10.0), Some((81.0, 50.0, 1.0)));
    assert!(water.fear(9.0).is_none());
    let felt: Vec<f64> = (0..12)
        .map(|t| water.fear(10.0 + t as f64 / 4.0).unwrap().2)
        .collect();
    assert!(felt.iter().rev().is_sorted() && felt[11] < 0.03);
    assert!(water.fear(10.0 + CALM).is_none());
    // A koi near the pointer turns its tail to it, darts, and leaves a ring.
    for i in 0..200 {
        water.step(1.0 / 20.0, 80, 24, i as f64 / 20.0, None);
    }
    {
        let koi = water.koi.as_mut().unwrap();
        let fish = &mut koi[0];
        (fish.x, fish.y, fish.heading, fish.turn, fish.kiss) = (70.0, 50.0, 0.0, 0.0, 99.0);
        (koi[1].x, koi[1].y, koi[1].kiss) = (150.0, 10.0, 99.0);
    }
    water.kisses.clear();
    water.touch(81.0, 50.0, 10.0);
    let mut gone: Vec<f64> = Vec::new();
    for i in 0..40 {
        let before = (
            water.koi.as_ref().unwrap()[0].x,
            water.koi.as_ref().unwrap()[0].y,
        );
        water.step(1.0 / 20.0, 80, 24, 10.0 + i as f64 / 20.0, None);
        let f = &water.koi.as_ref().unwrap()[0];
        gone.push((f.x - before.0).hypot(f.y - before.1) * 20.0);
    }
    let koi = water.koi.as_ref().unwrap();
    let (fish, far) = (&koi[0], &koi[1]);
    assert!(fish.x < 60.0 && (fish.x - 81.0).hypot(fish.y - 50.0) > SHY);
    assert!(gone.iter().cloned().fold(0.0, f64::max) > 2.5 * fish.speed && far.dash == 0.0);
    assert_eq!(water.kisses.len(), 1);
    assert!((water.kisses[0].x - 70.0).abs() <= 1.0 && (water.kisses[0].y - 50.0).abs() <= 1.0);
    // Calm again: its pace, its turns.
    for i in 0..120 {
        water.step(1.0 / 20.0, 80, 24, 14.0 + i as f64 / 20.0, None);
    }
    let fish = &water.koi.as_ref().unwrap()[0];
    assert!(fish.dash < 0.01 && fish.turn.abs() <= 1.6);
    // A strider slides off the pointer, and comes back.
    let mut water = pond(5, json!({"pads": 0, "drops": 0, "striders": true}));
    let (x, y, _) = water.skaters(80, 24, 2.0, &|_, _| false)[0];
    let home = inked(&water.picture(80, 24, 2.0, None), &[STRIDER]);
    water.touch(x + 3.0, y, 2.0);
    let away = inked(&water.picture(80, 24, 2.0, None), &[STRIDER]);
    let least = |c: &[(i64, i64)]| c.iter().map(|c| c.0).min().unwrap();
    assert!(away != home && least(&away) < least(&home) - 3);
    let mut fresh = pond(5, json!({"pads": 0, "drops": 0, "striders": true}));
    assert_eq!(
        inked(&water.picture(80, 24, 2.0 + CALM, None), &[STRIDER]),
        inked(&fresh.picture(80, 24, 2.0 + CALM, None), &[STRIDER])
    );
}

/// `test_current`.
#[test]
fn current() {
    let lit = |p: &mut Pond, t: f64| inked(&p.picture(80, 24, t, None), &SHEEN);
    let mut still = pond(1, json!({"pads": 0, "drops": 0, "current": 0.0}));
    assert_eq!(still.flow, (0.0, 0.0));
    let mut water = pond(1, json!({"pads": 0, "drops": 0}));
    assert!((water.flow.0.hypot(water.flow.1) - FLOW).abs() < 1e-9);
    assert!(lit(&mut water, 0.0) == lit(&mut still, 0.0) && !lit(&mut water, 0.0).is_empty());
    assert!(lit(&mut water, 30.0) != lit(&mut still, 30.0));
    // A strider is carried while it rests.
    let carried = pond(
        1,
        json!({"pads": 0, "drops": 0, "striders": true, "current": 2.0}),
    );
    let (fx, fy) = carried.flow;
    for a in 0..399 {
        let (x0, y0, p0) = carried.skaters(80, 24, a as f64 / 20.0, &|_, _| false)[0];
        let (x1, y1, p1) = carried.skaters(80, 24, (a + 1) as f64 / 20.0, &|_, _| false)[0];
        assert!((x1 - x0).hypot(y1 - y0) < 14.0); // no jump
        if p0.key == p1.key && a as f64 / 20.0 - p0.t0 > 0.3 {
            assert!((x1 - x0 - fx / 20.0).abs() < 1e-6 && (y1 - y0 - fy / 20.0).abs() < 1e-6);
        }
    }
}

/// `test_moon`.
#[test]
fn the_moon() {
    let lit = |phase: f64, t: f64| {
        let mut p = pond(
            1,
            json!({"pads": 0, "drops": 0, "moon": true, "phase": phase}),
        );
        inked(&p.picture(80, 24, t, None), &MOON)
    };
    let full = lit(0.5, 90.0);
    assert!((25..=60).contains(&full.len()), "{}", full.len());
    assert!(lit(0.0, 90.0).is_empty());
    assert!(lit(0.12, 90.0).len() < lit(0.25, 90.0).len() && lit(0.25, 90.0).len() < full.len());
    // In the middle of its way at half the night; out for a fifth.
    assert!(full.iter().all(|c| (33..=47).contains(&c.0)));
    assert!(lit(0.5, 270.0).is_empty() && !lit(0.5, 330.0).is_empty());
    assert!(lit(0.5, 10.0)[0].0 < 40 && 40 < lit(0.5, 200.0)[0].0);
    assert_eq!(moon_at(80, 24, 6.0, 0.0).0, -6.0);
    assert_eq!(moon_at(80, 24, 6.0, 1.0).0, 166.0);
    let mid = moon_at(80, 24, 6.0, 0.5);
    assert!(mid.0 == 80.0 && (mid.1 - 28.8).abs() < 1e-9);
    // The swell shakes it.
    let looks: HashSet<_> = (0..16).map(|t| lit(0.5, 90.0 + t as f64 / 8.0)).collect();
    assert!(looks.len() > 12);
}

/// `test_adrift_striders_lotus_rain`.
#[test]
fn adrift_striders_lotus_rain() {
    for (i, (inks, _, _)) in ADRIFT.iter().enumerate() {
        let mut water = pond(
            1,
            json!({"drops": 0, "pads": 0, "adrift": true, "season": SEASONS[i]}),
        );
        assert!(!inked(&water.picture(80, 24, 2.0, None), inks).is_empty());
        let others: Vec<u8> = (ADRIFT.iter().enumerate())
            .filter(|(j, _)| *j != i)
            .flat_map(|(_, a)| a.0.iter().copied())
            .filter(|ink| !inks.contains(ink))
            .collect();
        assert!(inked(&water.picture(80, 24, 2.0, None), &others).is_empty());
    }
    // They drift: a minute later, elsewhere.
    let mut water = pond(
        1,
        json!({"drops": 0, "pads": 0, "adrift": true, "season": "autumn"}),
    );
    let inks = ADRIFT[2].0;
    assert!(
        inked(&water.picture(80, 24, 0.0, None), inks)
            != inked(&water.picture(80, 24, 60.0, None), inks)
    );
    let mut water = pond(1, json!({"drops": 0, "striders": true}));
    let mut places = HashSet::new();
    for t in 0..240 {
        let t = t as f64 / 8.0;
        let cells = inked(&water.picture(80, 24, t, None), &[STRIDER]);
        assert!((1..=12).contains(&cells.len()));
        places.insert(cells[0]);
        // Never on a pad.
        let leaves = water.leaves(80, 24, t);
        let dry = |x: f64, y: f64| {
            (leaves.iter()).any(|&(a, b, p)| (x - a as f64).hypot(y - b as f64) < p.r + 1.5)
        };
        for (x, y, push) in water.skaters(80, 24, t, &dry) {
            assert!(!dry(x, y) || !dry(push.x, push.y));
            assert!(push.t0 <= t && (x - push.x).hypot(y - push.y) < 30.0);
        }
    }
    assert!(places.len() > 6);
    assert!(
        (0..40).any(|t| !inked(&water.picture(80, 24, t as f64 / 8.0, None), &RING).is_empty())
    );
    let mut plain = pond(1, json!({"drops": 0, "pads": 30}));
    let mut bloom = pond(1, json!({"drops": 0, "pads": 30, "lotus": true}));
    let picture = bloom.picture(80, 24, 0.0, None);
    let flowers: Vec<char> = glyphs(&picture)
        .into_iter()
        .filter(|g| braille(*g) == 0)
        .collect();
    assert!((3..=20).contains(&flowers.len()) && flowers.iter().all(|g| BLOOMS.contains(g)));
    assert_eq!(inked(&picture, &LOTUS).len(), flowers.len());
    assert!(inked(&plain.picture(80, 24, 0.0, None), &LOTUS).is_empty());
    // Showers: many small rings at the height of one, few between two.
    let mut wet = pond(1, json!({"drops": 0, "pads": 0, "rain": true}));
    let rates: Vec<f64> = (0..100).map(|s| Pond::shower(wet.seed, s)).collect();
    assert!(rates.iter().cloned().fold(0.0, f64::max) > 0.95 * RAIN);
    assert!(rates.iter().cloned().fold(f64::MAX, f64::min) < 0.14 * RAIN);
    let high = (0..100)
        .max_by(|a, b| rates[*a as usize].total_cmp(&rates[*b as usize]))
        .unwrap();
    let low = (0..100)
        .min_by(|a, b| rates[*a as usize].total_cmp(&rates[*b as usize]))
        .unwrap();
    let wet_at = |p: &mut Pond, s: i64| inked(&p.picture(80, 24, s as f64, None), &RING).len();
    assert!(wet_at(&mut wet, high) > 4 * wet_at(&mut wet, low));
}

/// `test_fly`.
#[test]
fn a_dragonfly() {
    let mut rng = Twister::new(7);
    let mut fly = Fly::new(&mut rng, 80, 24);
    let cells: Vec<(i64, i64)> = (0..4000)
        .map(|_| fly.step(&mut rng, 1.0 / 8.0, 80, 24, None))
        .collect();
    assert!(
        cells
            .iter()
            .all(|c| (0..80).contains(&c.0) && (0..24).contains(&c.1))
    );
    assert!(cells.iter().collect::<HashSet<_>>().len() > 60);
    // It hovers most of the time, and darts.
    let moves = cells.windows(2).filter(|p| p[0] != p[1]).count() as f64 / 3999.0;
    assert!(0.05 < moves && moves < 0.5);
    assert!(
        cells
            .windows(2)
            .map(|p| (p[0].0 - p[1].0).abs())
            .max()
            .unwrap()
            >= 3
    );
    // The pointer near: it leaves, and stays away.
    let mut at = (0, 0);
    for _ in 0..40 {
        at = fly.step(&mut rng, 1.0 / 20.0, 80, 24, None);
    }
    let (x, y) = (at.0 as f64, at.1 as f64);
    let mut cell = at;
    for _ in 0..60 {
        cell = fly.step(&mut rng, 1.0 / 20.0, 80, 24, Some((x, y, 1.0)));
    }
    assert!((cell.0 as f64 - x).hypot(2.0 * (cell.1 as f64 - y)) >= WARY);
    assert_eq!(
        fly.step(&mut rng, 1.0 / 20.0, 80, 24, Some((x, y, 0.2))),
        cell
    );
    let frames = fly.frames();
    assert_eq!(
        (
            frames[0]["cells"][0][0]["t"].as_str(),
            frames[1]["cells"][0][0]["t"].as_str()
        ),
        (Some("‡"), Some("╪"))
    );
    let inks = (
        frames[0]["cells"][0][0]["fg"].as_u64().unwrap() as u8,
        frames[1]["cells"][0][0]["fg"].as_u64().unwrap() as u8,
    );
    assert!(FLIES.contains(&inks));
}

#[test]
fn seasons_and_days() {
    // 2000-01-01, 2024-03-20 (day 80, a leap year), 2026-11-22 (day 326).
    assert_eq!(day_of_year(946684800000.0, 0.0), 1);
    assert_eq!(day_of_year(1710892800000.0, 0.0), 80);
    assert_eq!(day_of_year(1795305600000.0, 0.0), 326);
    assert_eq!(day_of_year(1795305600000.0, -3600.0), 325);
    assert_eq!(day_of_year(0.0, 0.0), 1);
    let days = [1, 79, 80, 171, 172, 265, 266, 354, 355, 365];
    let got: Vec<&str> = days.iter().map(|d| SEASONS[season_of(*d)]).collect();
    assert_eq!(
        got,
        [
            "winter", "winter", "spring", "spring", "summer", "summer", "autumn", "autumn",
            "winter", "winter"
        ]
    );
}
