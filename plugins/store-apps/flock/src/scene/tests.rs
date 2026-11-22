// SPDX-License-Identifier: GPL-3.0-or-later
//! The twin's tests (`test_flock.py`) and its pictures (`twin.rs`).

use super::*;
use crate::grid::fnv;
use crate::twin;
use ptyrc_app::Value;
use std::collections::HashSet;

fn s(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|r| r.to_string()).collect()
}

fn text_of(name: &str) -> Vec<String> {
    match name {
        "words" => s(&["", "", "words in the way", "", "   more words here"]),
        "low" => {
            let mut rows = vec![String::new(); 20];
            rows.push("   a line of text low on the pane".into());
            rows
        }
        "block" => {
            let mut rows = vec![String::new(); 30];
            let line = format!("{}{}", " ".repeat(40), "x".repeat(60));
            rows.extend(std::iter::repeat_n(line, 6));
            rows
        }
        _ => Vec::new(),
    }
}

fn floats(xs: &[f64]) -> u32 {
    let text: Vec<String> = xs.iter().map(|x| x.to_bits().to_string()).collect();
    fnv(&text.join(","))
}

/// Dots of the cells of `grid` of the inks `inks`.
fn dots(grid: &Grid, inks: &[u8]) -> u32 {
    (0..grid.height())
        .flat_map(|y| grid.row(y).collect::<Vec<_>>())
        .filter(|c| inks.contains(&c.2) && ('\u{2801}'..'\u{2900}').contains(&c.1))
        .map(|c| (c.1 as u32 - 0x2800).count_ones())
        .sum()
}

fn inks(grid: &Grid) -> HashSet<u8> {
    (0..grid.height())
        .flat_map(|y| grid.row(y).map(|c| c.2).collect::<Vec<_>>())
        .collect()
}

fn flat<'a>(w: usize, over: usize, at: f64, dusk: f64) -> Air<'a> {
    Air {
        w,
        over,
        dusk,
        at,
        land: None,
        roosts: &[],
    }
}

/// A sky over flat land, at the stage `at`, a long dusk.
fn air<'a>(w: usize, over: usize) -> Air<'a> {
    flat(w, over, 0.4, 3000.0)
}

fn scene(seed: u64, args: Value) -> Scene {
    Scene::new(&mut Twister::new(seed), crate::options(&args).unwrap())
}

fn hypot(a: Dot, b: Dot) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn median(xs: impl Iterator<Item = f64>) -> f64 {
    let mut v: Vec<f64> = xs.collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

#[test]
fn the_twin() {
    let fs: Vec<f64> = (-8..50).map(|i| i as f64 / 40.0).collect();
    assert_eq!(
        floats(&fs.iter().map(|&f| height(f)).collect::<Vec<_>>()),
        twin::HEIGHT
    );
    assert_eq!(
        floats(&fs.iter().map(|&f| glow(f)).collect::<Vec<_>>()),
        twin::GLOW
    );
    let st: Vec<f64> = (-8..50)
        .map(|i| stage(7.5 * i as f64, 300.0, 0.3))
        .collect();
    assert_eq!(floats(&st), twin::STAGE);
    let mut suns = Vec::new();
    for &f in &fs {
        let (a, b) = sun_at(80, 90.0, 7.0, f);
        let (c, d) = sun_at(200, 234.0, 17.5, f);
        suns.extend([a, b, c, d]);
    }
    assert_eq!(floats(&suns), twin::SUN_AT);
    for &(seed, clouds, pics) in twin::DUSKS {
        let mut d = Dusk::new(&mut Twister::new(seed), clouds);
        for &(w, h, f, t, digest) in pics {
            let sun = sun_at(w, 4.0 * h as f64, 7.0, f);
            let got = fnv(&d.picture(w, h, f, sun, t).text());
            assert_eq!(got, digest, "dusk {seed} {clouds} {w}x{h} {f} {t}");
        }
    }
    for &(seed, water, reeds, wind, pics) in twin::LANDS {
        let mut land = Land::new(&mut Twister::new(seed), water, reeds);
        for &(w, h, t, sun, f, digest) in pics {
            let sun = (sun.1 > 0.0).then_some(sun);
            let got = fnv(&land.picture(w, h, t, wind, sun, f).text());
            assert_eq!(got, digest, "land {seed} {water} {reeds} {w}x{h} {t}");
        }
    }
    for &(seed, kw, segs) in twin::FLOCKS {
        let kw: Value = kw.parse().unwrap();
        let num = |k: &str, d: f64| kw[k].as_f64().unwrap_or(d);
        let on = |k: &str| kw[k].as_bool() == Some(true);
        let mut f = Flock::new(
            &mut Twister::new(seed),
            num("birds", 100.0),
            num("second", 0.0),
            on("join"),
            on("hawk"),
        );
        for &((w, over, at, dusk, t0, ticks, dt, scare), digest) in segs {
            let a = flat(w, over, at, dusk);
            let scare = (scare.2 >= 0.0).then_some(scare);
            let mut xs = Vec::new();
            for i in 0..=ticks {
                for (x, y) in f.fly(&a, t0 + i as f64 * dt, scare) {
                    xs.extend([x, y]);
                }
                match f.hunter() {
                    Some((x, y)) => xs.extend([x, y]),
                    None => xs.push(-1.0),
                }
            }
            assert_eq!(
                floats(&xs),
                digest,
                "flock {seed} {kw} {w}x{over} from {t0}"
            );
        }
    }
    for &(name, w, h, digest) in twin::PERCHES {
        let spots: String = (perches(&text_of(name), w, h).iter())
            .map(|(x, y)| format!("{x},{y};"))
            .collect();
        assert_eq!(fnv(&spots), digest, "perches of {name}");
    }
    for &(seed, kw, (w, h), name, t0, ticks, touches, at) in twin::SCENES {
        let mut sc = scene(seed, kw.parse().unwrap());
        let mut roosts = Vec::new();
        if sc.c.roost {
            roosts = perches(&text_of(name), w, h);
            shuffle(&mut Twister::new(seed), &mut roosts);
        }
        let mut pointer: Option<(f64, f64, f64)> = None;
        let mut pics = Vec::new();
        for i in 0..=ticks {
            let t = t0 + i as f64 / 30.0;
            for &(k, x, y) in touches {
                if k == i {
                    pointer = (x >= 0).then_some((2.0 * x as f64 + 1.0, 4.0 * y as f64 + 2.0, t));
                }
            }
            let scare = pointer.map(|(x, y, p)| (x, y, STILL.max(1.0 - (t - p) / CALM)));
            let fore = sc.fore(w, h, t, &roosts, scare);
            if at.iter().any(|a| a.0 == i) {
                let above = fnv(&sc.above(w, h, t).text());
                let below = fnv(&sc.below(w, h, t).text());
                pics.push((i, above, fnv(&fore.text()), below));
            }
        }
        assert_eq!(pics, at, "scene {seed} {kw} {w}x{h}");
    }
}

#[test]
fn stage_and_glow() {
    assert_eq!(stage(0.0, 300.0, START), START);
    assert!((stage(300.0 * (1.0 + REST), 300.0, START) - START).abs() < 1e-9);
    assert_eq!((height(0.0), height(SET)), (1.0, 0.0));
    // The night and the start of the next dusk are alike: no light.
    assert_eq!((glow(0.0), glow(1.0 + REST), glow(SET)), (0.0, 0.0, 1.0));
    let lights: Vec<f64> = (0..=80).map(|f| glow(f as f64 / 100.0)).collect();
    assert!(lights.windows(2).all(|p| p[0] <= p[1]));
    let (x0, y0) = sun_at(80, 90.0, 7.0, 0.0);
    let (x1, y1) = sun_at(80, 90.0, 7.0, SET);
    assert!(y0 == -7.0 && y1 == 97.0 && 0.0 < x0 && x0 < x1 && x1 < 160.0);
    assert_eq!(sun_at(80, 90.0, 7.0, 1.0), (x1, y1));
}

#[test]
fn the_dusk() {
    let sun = sun_at(80, 92.0, 7.0, 0.7);
    let mut dusk = Dusk::new(&mut Twister::new(1), false);
    let low = dusk.picture(80, 23, 0.7, sun, 0.0);
    assert_eq!(dusk.picture(80, 23, 0.7, sun, 0.0), low);
    let mut two = Dusk::new(&mut Twister::new(2), false);
    assert_ne!(two.picture(80, 23, 0.7, sun, 0.0), low);
    assert!(inks(&low).iter().all(|i| RAMP.contains(i)));
    // Sparse, and most of it low, by the horizon.
    let ink: Vec<u32> = (0..23)
        .map(|y| low.row(y).map(|c| (c.1 as u32 - 0x2800).count_ones()).sum())
        .collect();
    let all: u32 = ink.iter().sum();
    assert!(300 < all && (all as f64) < 0.25 * 160.0 * 92.0);
    assert!(ink[12..].iter().sum::<u32>() > 3 * ink[..12].iter().sum::<u32>());
    // No light: no sky.
    assert_eq!(dusk.picture(80, 23, 0.0, sun, 0.0).count(), 0);
    assert_eq!(dusk.picture(80, 23, 1.0 + REST, sun, 0.0).count(), 0);
    assert_eq!(dusk.picture(0, 5, 0.7, sun, 0.0).count(), 0);
    // Clouds drift.
    let mut clouds = Dusk::new(&mut Twister::new(1), true);
    let at0 = clouds.picture(80, 23, 0.7, sun, 0.0);
    assert!(at0.count() > low.count());
    assert_ne!(clouds.picture(80, 23, 0.7, sun, 60.0), at0);
    assert_eq!(dusk.picture(80, 23, 0.7, sun, 60.0), low);
}

#[test]
fn the_flight() {
    let mut one = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    let small = one.fly(&air(40, 12), 5.0, None);
    assert_eq!(
        (small.len(), one.count(40, 12), one.count(80, 24)),
        (25, 25, 100)
    );
    assert_eq!(one.fly(&air(40, 12), 5.0, None), small);
    // A flock just made is a function of the time.
    let mut again = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    assert_eq!(again.fly(&air(40, 12), 5.0, None), small);
    let mut other = Flock::new(&mut Twister::new(4), 100.0, 0.0, false, false);
    assert_ne!(other.fly(&air(40, 12), 5.0, None), small);
    assert_ne!(one.fly(&air(40, 12), 5.1, None), small);
    let mut none = Flock::new(&mut Twister::new(3), 0.0, 0.0, false, false);
    assert!(none.fly(&air(80, 23), 5.0, None).is_empty());
    // More, in a larger view: up from the land at the next step.
    assert_eq!(one.fly(&air(80, 23), 5.1, None).len(), 25);
    assert_eq!(one.fly(&air(80, 23), 5.2, None).len(), 95);
    assert_eq!(one.fly(&air(40, 12), 5.2, None).len(), 25);
    assert_eq!(one.birds.len(), 95);
    let mut now = Vec::new();
    for (w, over) in [(80, 23), (200, 58)] {
        let a = air(w, over);
        one = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
        let u = one.reach(2.0 * w as f64, 4.0 * over as f64);
        now = one.fly(&a, 0.0, None);
        let (mut inside, mut cells, mut far) = (0usize, 0usize, 0usize);
        for k in 1..400 {
            let after = one.fly(&a, k as f64 / 10.0, None);
            let steps: Vec<f64> = now.iter().zip(&after).map(|(&p, &q)| hypot(p, q)).collect();
            // A bird flies on, without a jump; none stops.
            let max = steps.iter().cloned().fold(0.0, f64::max);
            assert!(max < 2.5 * PACE * u * DT);
            assert!(steps.iter().sum::<f64>() / steps.len() as f64 > 0.3 * PACE * u * DT);
            now = after;
            let (aw, ah) = (2.0 * w as f64, 4.0 * over as f64);
            inside += (now.iter())
                .filter(|(x, y)| (0.0..aw).contains(x) && (0.0..ah).contains(y))
                .count();
            let set: HashSet<(i64, i64)> = (now.iter())
                .map(|&(x, y)| ((x as i64) >> 1, (y as i64) >> 2))
                .collect();
            cells += set.len();
            let mx = median(now.iter().map(|p| p.0));
            let my = median(now.iter().map(|p| p.1));
            far += now
                .iter()
                .filter(|&&p| hypot(p, (mx, my)) > 1.5 * u)
                .count();
        }
        let total = (399 * now.len()) as f64;
        // Most in view; loose: few share a cell; together, less a few.
        assert!(inside as f64 > 0.85 * total);
        assert!(cells as f64 > 0.5 * total);
        assert!(0.002 * total < far as f64 && (far as f64) < 0.15 * total);
    }
    // They fly as one: the same way.
    let n = one.x.len() as f64;
    let (vx, vy) = (
        one.vx.iter().sum::<f64>() / n,
        one.vy.iter().sum::<f64>() / n,
    );
    let speed = (0..one.x.len())
        .map(|i| one.vx[i].hypot(one.vy[i]))
        .sum::<f64>()
        / n;
    assert!(vx.hypot(vy) > 0.4 * speed);
    // A view of another size: the same birds, where they were in it.
    let wide = one.fly(&air(100, 58), 39.9, None);
    assert_eq!(wide.len(), one.count(100, 58));
    for (a, b) in wide.iter().zip(&now) {
        assert!((a.0 - b.0 / 2.0).abs() <= 1e-6 * a.0.abs().max(1e-12));
    }
}

#[test]
fn split_and_scare() {
    let mut one = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    let a = air(80, 23);
    let u = one.reach(160.0, 92.0);
    let mut apart: f64 = 0.0;
    let mut seen = HashSet::new();
    for k in 0..2000 {
        one.fly(&a, k as f64 / 10.0, None);
        seen.insert(one.rift);
        let tribes: HashSet<usize> = one.tribe.iter().copied().collect();
        assert_eq!(tribes.len() > 1, one.rift.is_some());
        if one.rift.is_some() {
            let mine: Vec<usize> = (0..one.tribe.len())
                .filter(|&i| one.tribe[i] != 0)
                .collect();
            let rest: Vec<usize> = (0..one.tribe.len())
                .filter(|&i| one.tribe[i] == 0)
                .collect();
            assert!(0.15 * 95.0 < mine.len() as f64 && (mine.len() as f64) < 0.55 * 95.0);
            let mid = |p: &[usize]| {
                let n = p.len() as f64;
                (
                    p.iter().map(|&i| one.x[i]).sum::<f64>() / n,
                    p.iter().map(|&i| one.y[i]).sum::<f64>() / n,
                )
            };
            apart = apart.max(hypot(mid(&mine), mid(&rest)));
        }
    }
    // Now and then a part goes its own way, and comes back.
    assert!(seen.contains(&None) && (2..=5).contains(&seen.len()) && apart > 1.5 * u);
    // What scares them: they leave the place.
    let mut calm = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    let mut wary = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    let at = calm.fly(&a, 20.0, None);
    assert_eq!(wary.fly(&a, 20.0, None), at);
    let x = median(at.iter().map(|p| p.0));
    let y = median(at.iter().map(|p| p.1));
    let near = |b: &[Dot], r: f64| b.iter().filter(|&&p| hypot(p, (x, y)) < r * u).count();
    assert!(near(&at, 0.4) > 23);
    let (mut here, mut there) = (Vec::new(), Vec::new());
    for k in 201..213 {
        here = calm.fly(&a, k as f64 / 10.0, None);
        there = wary.fly(&a, k as f64 / 10.0, Some((x, y, 1.0)));
    }
    assert!(near(&there, 0.4) <= 3 && 3 < near(&here, 0.4));
    // A pointer that rests scares less far.
    for k in 213..300 {
        let there = wary.fly(&a, k as f64 / 10.0, Some((x, y, STILL)));
        assert!(near(&there, 0.2) <= 6);
    }
}

#[test]
fn the_glide() {
    assert_eq!(glide(0.0, 1.0, 2.0, 0.0), 0.5);
    assert_eq!(glide(0.0, 1.0, 2.0, 1.0), 1.5);
    assert_eq!(glide(3.0, 3.0, 3.0, 0.4), 3.0);
    let mut one = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    let a = air(80, 23);
    let u = one.reach(160.0, 92.0);
    let frames: Vec<Vec<Dot>> = (0..300)
        .map(|j| one.fly(&a, 10.0 + j as f64 * PERIOD, None))
        .collect();
    assert_eq!(one.k, Some(199));
    let hops: Vec<f64> = (frames.windows(2))
        .flat_map(|p| {
            p[0].iter()
                .zip(&p[1])
                .map(|(&a, &b)| hypot(a, b))
                .collect::<Vec<_>>()
        })
        .collect();
    let pace = PACE * u * PERIOD;
    let max = hops.iter().cloned().fold(0.0, f64::max);
    assert!(max < 2.5 * pace && 2.5 * pace < 1.5);
    assert!(hops.iter().sum::<f64>() / hops.len() as f64 > 0.5 * pace);
    // No turn at a step: the speed of a bird changes little in a frame.
    for f in frames.windows(3) {
        for ((a, b), c) in f[0].iter().zip(&f[1]).zip(&f[2]) {
            assert!((c.0 - 2.0 * b.0 + a.0).hypot(c.1 - 2.0 * b.1 + a.1) < 0.25 * pace);
        }
    }
}

#[test]
fn the_land() {
    let mut land = Land::new(&mut Twister::new(5), false, false);
    assert_eq!((land.rows(24), land.rows(60), land.rows(0)), (1, 2, 0));
    let low = land.picture(80, 24, 0.0, 1.0, None, START);
    assert!(low.height() == 1 && low.count() == 80);
    assert!(low.row(0).all(|c| (c.1 as u32 - 0x2800) & 0xC0 != 0));
    assert!((0..160).all(|px| (91..=95).contains(&land.top(px, 24))));
    assert_eq!(land.picture(80, 24, 9.0, 1.0, None, START), low);
    assert_eq!(land.picture(0, 24, 0.0, 1.0, None, START).count(), 0);
    // The sun warms the land under it.
    let lit = land.picture(80, 24, 0.0, 1.0, Some((80.0, 7.0, 208)), 0.7);
    assert_eq!(inks(&lit), SOIL.into_iter().collect());
    // Reeds: they bend, a reed keeps its root.
    let mut reeds = Land::new(&mut Twister::new(5), false, true);
    let still = reeds.picture(80, 24, 0.0, 0.0, None, START);
    assert_eq!((still.height(), reeds.rows(24)), (3, 3));
    assert_eq!(reeds.picture(80, 24, 9.0, 0.0, None, START), still);
    assert!(dots(&still, &REEDS) > 100 && still.row(0).count() < 40);
    let looks: HashSet<String> = (0..40)
        .map(|t| {
            reeds
                .picture(80, 24, t as f64 / 5.0, 1.0, None, START)
                .text()
        })
        .collect();
    assert!(looks.len() > 20);
    // A lake: a far shore, ripples, the path of the sun.
    let mut lake = Land::new(&mut Twister::new(5), true, false);
    assert_eq!((lake.rows(24), lake.rows(60), lake.rows(2)), (4, 9, 2));
    assert_eq!(lake.horizon(24), 4 * 21 - 2);
    let mut dark = lake.picture(80, 24, 0.0, 1.0, None, 0.7);
    assert_eq!(dots(&dark, &[208]), 0);
    dark.keep_last(3);
    assert!(dark.height() == 3 && dots(&dark, &WATER) > 20);
    let mut shine = lake.picture(80, 24, 0.0, 1.0, Some((80.0, 7.0, 208)), 0.7);
    let again = lake.picture(80, 24, 0.4, 1.0, Some((80.0, 7.0, 208)), 0.7);
    assert_ne!(again, shine);
    shine.keep_last(3);
    let path: Vec<usize> = (0..3)
        .flat_map(|y| {
            shine
                .row(y)
                .filter(|c| c.2 == 208)
                .map(|c| c.0)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(path.len() > 8 && path.iter().all(|x| (30..=50).contains(x)));
    // Set for a while: no path.
    let set = lake.picture(80, 24, 0.0, 1.0, Some((80.0, 7.0, 196)), 0.99);
    assert_eq!(dots(&set, &[196]), 0);
}

#[test]
fn the_scene() {
    let mut sc = scene(2, ptyrc_app::json!({"at": 0.5, "dusk": 100}));
    assert_eq!((sc.stage(0.0), sc.rows(24)), (0.5, 1));
    assert_eq!((sc.radius(80, 24), sc.radius(200, 60)), (7.0, 17.5));
    let fore = sc.fore(80, 24, 0.0, &[], None);
    let (cx, cy, colour) = sc.sun(80, 24, 0.0);
    let whole = dots(&fore, &[colour]);
    assert!(130 < whole && whole <= 160); // pi * 7 * 7
    let birds = dots(&fore, &BIRDS);
    assert!(62 < birds && birds <= 100);
    assert_eq!(sc.fore(80, 24, 0.0, &[], None), fore);
    assert_ne!(sc.fore(80, 24, 0.1, &[], None), fore);
    assert_eq!(sc.fore(0, 24, 0.0, &[], None).count(), 0);
    // A bird before the sun: dots off in the disc.
    let shade = sc.fore_of(80, 24, 0.0, &[(cx, cy), (cx + 4.0, cy + 2.0)], None);
    assert_eq!(dots(&shade, &[colour]), whole - 4);
    assert_eq!(dots(&shade, &BIRDS), 0);
    // The sun sets behind the land; the sky under it is the land's.
    assert_eq!(
        dots(&sc.fore(80, 24, 100.0 * (SET - 0.5), &[], None), &SUN),
        0
    );
    let above = sc.above(80, 24, 20.0);
    assert!(above.height() == 24 && above.row(23).count() == 0 && above.count() > 100);
}

#[test]
fn roost_and_join() {
    let mut sc = scene(2, ptyrc_app::json!({"at": 0, "dusk": 100}));
    let flying =
        |sc: &mut Scene, f: f64, roosts: &[(i64, i64)]| sc.birds(80, 24, 100.0 * f, roosts, None).0;
    // Down at the start, all up after, down again for the night.
    let counts: Vec<usize> = [0.0, 0.005, 0.06, 0.5, 0.9, 0.99, 1.05]
        .iter()
        .map(|&f| flying(&mut sc, f, &[]).len())
        .collect();
    assert!(counts[0] == 0 && 0 < counts[1] && counts[1] < 95);
    assert!(counts[2] == 95 && counts[3] == 95);
    assert!(0 < counts[4] && counts[4] <= 95 && counts[5] == 0 && counts[6] == 0);
    // They take off from all along the land, low, and gather.
    let early = flying(&mut sc, 0.02, &[]);
    let xs: Vec<f64> = early.iter().map(|p| p.0).collect();
    let span =
        xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
    assert!(span > 80.0 && early.len() > 23);
    let spread = |sc: &mut Scene, f: f64| {
        let mut xs: Vec<f64> = flying(sc, f, &[]).iter().map(|p| p.0).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs[9 * xs.len() / 10] - xs[xs.len() / 10]
    };
    let wide = spread(&mut sc, 0.03);
    let least = [0.3, 0.5, 0.7]
        .iter()
        .map(|&f| spread(&mut sc, f))
        .fold(f64::MAX, f64::min);
    assert!(wide > 1.5 * least);
    // At dusk they go down apart: over the whole land, not in a column.
    let mut landed: Vec<Option<f64>> = vec![None; 95];
    for k in 850..990 {
        flying(&mut sc, k as f64 / 1000.0, &[]);
        for (i, &s) in sc.flock.state.iter().enumerate() {
            if s == DOWN && landed[i].is_none() {
                landed[i] = Some(sc.flock.x[i]);
            }
        }
    }
    let landed: Vec<f64> = landed.into_iter().flatten().collect();
    assert_eq!(landed.len(), 95);
    let sites: HashSet<i64> = landed
        .iter()
        .map(|x| (x * SITES as f64 / 160.0) as i64)
        .collect();
    assert!(sites.len() as i64 >= SITES - 2);
    let max = landed.iter().cloned().fold(f64::MIN, f64::max);
    let min = landed.iter().cloned().fold(f64::MAX, f64::min);
    assert!(max - min > 120.0);
    // A bird glides down, when the dusk gives it the time.
    let mut one = Flock::new(&mut Twister::new(3), 100.0, 0.0, false, false);
    let mut sink = Vec::new();
    for k in 0..300 {
        one.fly(&flat(80, 23, 0.855, 300.0), k as f64 / 10.0, None);
        for (i, &s) in one.state.iter().enumerate() {
            if s == IN {
                sink.push(one.vy[i] / one.vx[i].hypot(one.vy[i]));
            }
        }
    }
    let mean = sink.iter().sum::<f64>() / sink.len() as f64;
    assert!(sink.len() > 1500 && 0.3 < mean && mean < 0.55);
    // Perches: over the text, where the cell is free.
    let spots = perches(&s(&["", "", "  roost here", "x"]), 80, 24);
    assert!(!spots.is_empty() && spots.len() < 20);
    assert!(spots.iter().all(|&(x, y)| y == 7 && (4..24).contains(&x)));
    assert!(perches(&s(&["top", "under"]), 80, 24).is_empty());
    let perched = flying(&mut sc, 1.0, &spots);
    let want: Vec<Dot> = spots.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
    assert_eq!(perched, want);
    let grid = sc.fore(80, 24, 100.0, &spots, None);
    let mut row = grid.clone();
    row.keep_last(23);
    let mut one_row = Grid::new(80, 1);
    one_row.over(&row, 0);
    assert_eq!(dots(&one_row, &BIRDS) as usize, spots.len());
    // Groups come in from the sides, one after the other.
    let mut sc = scene(2, ptyrc_app::json!({"at": 0, "dusk": 100, "join": true}));
    let counts: Vec<usize> = [0.07, 0.2, 0.4, 0.7]
        .iter()
        .map(|&f| flying(&mut sc, f, &[]).len())
        .collect();
    assert!(15 < counts[0] && counts[0] < 47 && counts[3] == 95);
    assert!(counts.windows(2).all(|p| p[0] < p[1]));
    // Those not in yet have no perch; all have one at night.
    assert_eq!(flying(&mut sc, 0.07, &spots).len(), counts[0]);
    assert_eq!(flying(&mut sc, 1.0, &spots).len(), spots.len());
    // Two flocks: each after its own lead.
    let mut sc = scene(2, ptyrc_app::json!({"double": true}));
    assert_eq!(sc.birds(80, 24, 0.0, &[], None).0.len(), 95 + 34);
    let second: usize = sc.flock.tribe.iter().sum();
    assert!(18 < second && second < 50);
}

#[test]
fn the_hawk() {
    let mut sc = scene(3, ptyrc_app::json!({"hawk": true, "dusk": 3000, "at": 0.4}));
    let u = sc.flock.reach(160.0, 92.0);
    let mut hunts: Vec<Vec<Dot>> = Vec::new();
    let mut was: Option<Dot> = None;
    let mut still = Vec::new();
    let mut close = 0;
    for k in 0..3000 {
        let t = k as f64 / 10.0;
        let (birds, hawk) = sc.birds(80, 24, t, &[], None);
        if hawk.is_some() && was.is_none() {
            hunts.push(Vec::new());
            still = birds.clone();
        }
        if let Some(h) = hawk {
            hunts.last_mut().unwrap().push(h);
            close = close.max(birds.iter().filter(|&&p| hypot(p, h) < 4.0).count());
            if let Some(w) = was {
                // It flies on, faster than they do.
                let step = hypot(h, w);
                assert!(1.5 * PACE * u * DT < step && step < 2.3 * PACE * u * DT);
            }
            if hunts.last().unwrap().len() == 12 {
                assert!(inks(&sc.fore(80, 24, t, &[], None)).contains(&PREY));
            }
        }
        was = hawk;
    }
    // Now and then, not for long; it comes from over the view.
    assert!(2 <= hunts.len() && hunts.len() as f64 <= 300.0 / HAWK.0 + 1.0);
    assert!(
        hunts
            .iter()
            .all(|h| 10 < h.len() && h.len() as f64 <= 10.0 * HAWK.1 + 1.0)
    );
    assert!(hunts.iter().all(|h| h[0].1 < 0.0));
    // It goes for the thick of the flock, and they keep away from it.
    let n = still.len() as f64;
    let m = (
        still.iter().map(|p| p.0).sum::<f64>() / n,
        still.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let nearest = hunts
        .last()
        .unwrap()
        .iter()
        .map(|&p| hypot(p, m))
        .fold(f64::MAX, f64::min);
    assert!(nearest < u && close <= 5);
    // No hawk without the option, nor as they go to roost.
    let mut sc = scene(3, ptyrc_app::json!({"hawk": true, "dusk": 3000, "at": 0.9}));
    assert!(
        (0..2000)
            .step_by(2)
            .all(|k| sc.birds(80, 24, k as f64 / 10.0, &[], None).1.is_none())
    );
}
