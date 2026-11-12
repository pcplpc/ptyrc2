// SPDX-License-Identifier: GPL-3.0-or-later
//! Shape of the host stream: what the screen engine writes besides the
//! app's own bytes, and when.

use crate::engine::{ConnId, Ctx, Effects, Engine, Reply, SessionInfo, Token};
use crate::screen::ScreenEngine;
use ptyrc_vt::Vt;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::Duration;

const BSU: &[u8] = b"\x1b[?2026h";
const ESU: &[u8] = b"\x1b[?2026l";

/// An engine with a stand-in driver: time is set by the test, the host is
/// a `Vt` fed with everything written.
struct Rig {
    engine: ScreenEngine,
    info: SessionInfo,
    topics: HashSet<String>,
    now: Duration,
    /// What a real host would show.
    host: Vt,
    next: u64,
}

impl Rig {
    fn new(cols: u16, rows: u16) -> Rig {
        let info = SessionInfo {
            name: "test".into(),
            socket: "/nonexistent".into(),
            argv: vec!["app".into()],
            pid: 1,
            cols,
            rows,
            headless: true,
        };
        let mut rig = Rig {
            engine: ScreenEngine::new(Vec::new(), Vec::new()),
            info,
            topics: HashSet::new(),
            now: Duration::ZERO,
            host: Vt::new(cols, rows),
            next: 0,
        };
        let mut fx = Effects::default();
        let mut ctx = Ctx {
            info: &rig.info,
            topics: &rig.topics,
            now: rig.now,
            fx: &mut fx,
        };
        rig.engine.on_start(&mut ctx);
        // Past the settle time of the raw frame.
        rig.advance(400);
        rig
    }

    fn ctx<R>(&mut self, f: impl FnOnce(&mut ScreenEngine, &mut Ctx) -> R) -> R {
        let mut fx = Effects::default();
        let mut ctx = Ctx {
            info: &self.info,
            topics: &self.topics,
            now: self.now,
            fx: &mut fx,
        };
        f(&mut self.engine, &mut ctx)
    }

    /// App output; returns what went to the host.
    fn burst(&mut self, bytes: &[u8]) -> Vec<u8> {
        let now = self.now;
        let mut out = Vec::new();
        self.ctx(|e, ctx| e.on_burst(bytes, now, &mut out, ctx));
        self.host.feed(&out);
        self.check();
        out
    }

    /// One tick at the current time.
    fn tick(&mut self) -> Vec<u8> {
        let now = self.now;
        let mut out = Vec::new();
        self.ctx(|e, ctx| e.on_tick(now, &mut out, ctx));
        self.host.feed(&out);
        self.check();
        out
    }

    /// Let `ms` pass, ticking at every deadline; returns what went out.
    fn advance(&mut self, ms: u64) -> Vec<u8> {
        let end = self.now + Duration::from_millis(ms);
        let mut out = Vec::new();
        loop {
            match self.engine.next_wake(self.now) {
                Some(wake) if wake <= end => {
                    assert!(wake > self.now || out.len() < 1 << 20, "the loop spins");
                    self.now = wake.max(self.now + Duration::from_micros(1));
                    out.extend(self.tick());
                }
                _ => break,
            }
        }
        self.now = end;
        out
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let token = Token(self.next);
        let reply = self.ctx(|e, ctx| e.handle(ConnId(7), token, method, params, ctx));
        match reply {
            Reply::Ok(value) => value,
            other => panic!("{method}: {other:?}"),
        }
    }

    fn put(&mut self, spec: Value) -> u64 {
        let id = self.call("draw.put", spec)["id"].as_u64().unwrap();
        self.tick();
        id
    }

    /// The invariants of the design: the real host shows what the model
    /// says, and its state is the app's.
    fn check(&self) {
        let e = &self.engine;
        assert_eq!(
            self.host.grid(),
            e.host().grid(),
            "host model drifted from the host"
        );
        if e.app.in_ground() {
            assert!(e.host().same_state(&e.app), "host state is not the app's");
            assert!(
                self.host.same_state(&e.app),
                "real host state is not the app's"
            );
        }
    }

    fn row(&self, row: u16) -> String {
        self.host.grid().row_text(row).trim_end().to_string()
    }
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

fn text_at(x: i64, y: i64, text: &str) -> Value {
    json!({"anchor": {"at": [x, y], "space": "view"}, "content": {"rows": [text]}})
}

#[test]
fn transparent_without_sprites() {
    let mut rig = Rig::new(40, 10);
    for chunk in [
        &b"hello\r\n"[..],
        b"\x1b[2J\x1b[H\x1b[31mred\x1b[0m",
        b"\x1b[",
        b"5;5Hx",
    ] {
        assert_eq!(rig.burst(chunk), chunk);
    }
    assert!(rig.advance(5000).is_empty());
}

#[test]
fn static_sprite_costs_nothing_at_rest() {
    let mut rig = Rig::new(40, 10);
    rig.burst(b"line one\r\nline two\r\n");
    rig.put(text_at(20, 0, "SPRITE"));
    assert_eq!(rig.row(0), "line one            SPRITE");
    assert!(
        rig.advance(10_000).is_empty(),
        "a still sprite writes nothing"
    );
    // Output elsewhere: the burst alone, in one synchronized update.
    let out = rig.burst(b"more");
    assert_eq!(out, [BSU, b"more", ESU].concat());
}

#[test]
fn repair_is_inside_one_update() {
    let mut rig = Rig::new(40, 10);
    rig.put(text_at(2, 0, "SPRITE"));
    // The app writes over the sprite: burst and repair, one update.
    let out = rig.burst(b"\rxxxxxxxxxxxx");
    assert!(out.starts_with(BSU) && out.ends_with(ESU));
    assert_eq!(count(&out, BSU), 1);
    assert_eq!(count(&out, ESU), 1);
    assert_eq!(rig.row(0), "xxSPRITExxxx");
    // The sprite cells are painted once, not erased and painted again.
    assert_eq!(count(&out, b"SPRITE"), 1);
}

#[test]
fn repair_goes_inside_the_apps_update() {
    let mut rig = Rig::new(40, 10);
    rig.put(text_at(2, 0, "SPRITE"));
    let out = rig.burst(b"\x1b[?2026h\rxxxxxxxxxxxx\x1b[?2026l");
    // Ours opens, the app's own opening is a no-op, one end: the app's.
    assert_eq!(count(&out, ESU), 1);
    assert!(out.ends_with(ESU));
    let sprite = out.windows(6).position(|w| w == b"SPRITE").unwrap();
    assert!(sprite < out.len() - ESU.len());
    assert_eq!(rig.row(0), "xxSPRITExxxx");

    // The app holds its update across bursts: nothing of ours ends it.
    let out = rig.burst(b"\x1b[?2026h\ryyyy");
    assert_eq!(count(&out, ESU), 0);
    assert_eq!(rig.row(0), "yySPRITExxxx");
    let out = rig.burst(b"zzzz\x1b[?2026l");
    assert_eq!(count(&out, ESU), 1);
    assert_eq!(count(&out, BSU), 0);
}

#[test]
fn no_injection_inside_a_sequence() {
    let mut rig = Rig::new(40, 10);
    rig.put(text_at(0, 0, "SPRITE"));
    // Cut inside a CSI, then inside a UTF-8 glyph: only app bytes go out.
    assert_eq!(rig.burst(b"\x1b[2"), b"\x1b[2");
    let out = rig.burst(b"J\x1b[Habc\xe4\xb8");
    assert_eq!(out, b"J\x1b[Habc\xe4\xb8");
    let out = rig.burst(b"\x96");
    assert!(out.starts_with(b"\x96"));
    assert_eq!(rig.row(0), "SPRITE");
}

#[test]
fn sprites_stay_out_of_the_scrollback() {
    let mut rig = Rig::new(20, 4);
    rig.burst(b"one\r\ntwo\r\nthree\r\nfour");
    rig.put(text_at(10, 0, "SPRITE"));
    // The burst scrolls row 0 away. Before it, the sprite is unpainted:
    // the row that leaves is the app's.
    let out = rig.burst(b"\r\nfive\r\nsix");
    let app_bytes = out.windows(4).position(|w| w == b"five").unwrap();
    let mut before = Vt::new(20, 4);
    before.feed(b"one\r\ntwo\r\nthree\r\nfour");
    before.feed(&[b"\x1b[1;11HSPRITE\x1b[4;5H", &out[..app_bytes - 2]].concat());
    assert_eq!(before.grid().row_text(0).trim_end(), "one");
    // One update around unpaint, burst and repaint.
    assert_eq!(count(&out, BSU), 1);
    assert_eq!(count(&out, ESU), 1);
    assert_eq!(rig.row(0), "three     SPRITE");
}

#[test]
fn animation_cost_follows_changed_cells() {
    let mut rig = Rig::new(80, 24);
    rig.put(json!({
        "anchor": {"at": [10, 5], "space": "view"}, "loop": true,
        "frames": [{"rows": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab"], "ms": 50},
                   {"rows": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaac"], "ms": 50}],
    }));
    let out = rig.advance(1000);
    let updates = count(&out, BSU);
    assert!(
        (15..=25).contains(&updates),
        "{updates} updates in a second"
    );
    // One changed cell per frame: a move, a glyph, the state back.
    assert!(
        out.len() / updates < 48,
        "{} bytes per frame",
        out.len() / updates
    );
    assert_eq!(count(&out, b"aaaa"), 0, "unchanged cells are not repainted");
}

#[test]
fn wake_deadlines_do_not_spin() {
    let mut rig = Rig::new(40, 10);
    rig.call("debug.overlay", json!({"what": "frames"}));
    rig.put(json!({"effect": {"name": "confetti", "params": {"duration_ms": 500}}}));
    let mut ticks = 0;
    let end = rig.now + Duration::from_secs(3);
    while let Some(wake) = rig.engine.next_wake(rig.now) {
        if wake > end {
            break;
        }
        assert!(wake > rig.now, "deadline in the past");
        rig.now = wake;
        rig.tick();
        ticks += 1;
    }
    assert!(ticks < 400, "{ticks} ticks in three seconds");
}

#[test]
fn exit_leaves_the_host_clean() {
    let mut rig = Rig::new(40, 10);
    rig.burst(b"bye");
    rig.put(text_at(10, 0, "SPRITE"));
    // The app dies inside a sequence.
    rig.burst(b"\x1b[3");
    let mut out = Vec::new();
    rig.ctx(|e, ctx| e.on_exit(&mut out, ctx));
    rig.host.feed(&out);
    assert_eq!(rig.row(0), "bye");
    assert!(rig.host.in_ground());
    assert!(!rig.host.modes().sync_update);
}

#[test]
fn resize_forgets_painted_sprites() {
    let mut rig = Rig::new(40, 10);
    rig.burst(b"text");
    rig.put(text_at(10, 0, "SPRITE"));
    let now = rig.now;
    let mut out = Vec::new();
    rig.info.cols = 30;
    rig.info.rows = 8;
    rig.host.resize(30, 8);
    rig.ctx(|e, ctx| e.on_resize(30, 8, now, &mut out, ctx));
    rig.host.feed(&out);
    // The raw frame settles again, then the sprite is back.
    rig.advance(1000);
    assert_eq!(rig.row(0), "text      SPRITE");
}

/// Mixed text with colours and a status line, as `scripts/bench.sh`.
fn flood(bytes: usize) -> Vec<u8> {
    let words = [
        "the", "quick", "brown", "fox", "jumps", "世界", "🌼", "naïve", "dog",
    ];
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut out = Vec::with_capacity(bytes + 256);
    let mut i = 0;
    while out.len() < bytes {
        if i % 4 == 0 {
            out.extend_from_slice(format!("\x1b[3{}m", i % 8).as_bytes());
        }
        for _ in 0..3 + next() % 16 {
            out.extend_from_slice(words[(next() % 9) as usize].as_bytes());
            out.push(b' ');
        }
        if i % 50 == 0 {
            out.extend_from_slice(b"\x1b[1;4H\x1b[7m status \x1b[0m\x1b[40;1H");
        }
        out.extend_from_slice(b"\x1b[0m\r\n");
        i += 1;
    }
    out
}

/// Cost of the burst path. `cargo test --release -p ptyrc-server bench --
/// --ignored --nocapture`.
#[test]
#[ignore]
fn bench_burst_path() {
    use std::time::Instant;
    let data = flood(32 << 20);
    let mib = data.len() as f64 / (1 << 20) as f64;
    for (label, chunk) in [
        ("64 KiB bursts", 65536),
        ("4 KiB bursts", 4096),
        ("256 B bursts", 256),
    ] {
        let mut vt = Vt::new(120, 40);
        let t = Instant::now();
        for c in data.chunks(chunk) {
            vt.feed(c);
        }
        let feed = t.elapsed().as_secs_f64();
        let mut line = format!("{label}: one Vt {:.0} MiB/s", mib / feed);
        for sprites in [false, true] {
            let mut rig = Rig::new(120, 40);
            if sprites {
                rig.put(text_at(-1, 0, " 12:34:56 "));
                rig.put(text_at(40, 10, "+--------+"));
                rig.put(json!({"effect": {"name": "confetti", "params": {"duration_ms": 600000}}}));
            }
            let t = Instant::now();
            let mut out = Vec::new();
            for c in data.chunks(chunk) {
                out.clear();
                rig.now += Duration::from_micros(300);
                let now = rig.now;
                rig.ctx(|e, ctx| e.on_burst(c, now, &mut out, ctx));
            }
            let secs = t.elapsed().as_secs_f64();
            line.push_str(&format!(
                ", engine{} {:.1} MiB/s ({:.0} us/burst)",
                if sprites { "+sprites" } else { "" },
                mib / secs,
                secs * 1e6 / (data.len() / chunk) as f64
            ));
        }
        println!("{line}");
    }
    // A key echo: one byte on a still screen.
    for sprites in [false, true] {
        let mut rig = Rig::new(120, 40);
        if std::env::var_os("PTYRC_BENCH_FILL").is_some() {
            rig.burst(&data[..200_000]);
        }
        if sprites {
            rig.put(text_at(-1, 0, " 12:34:56 "));
        }
        let mut out = Vec::new();
        let run = |rig: &mut Rig, out: &mut Vec<u8>, n: usize, cold: bool| {
            let mut junk = vec![1u8; 32 << 20];
            let mut spent = Duration::ZERO;
            for i in 0..n {
                if cold {
                    // Evict the caches, as the idle time between keys does.
                    for b in junk.iter_mut().step_by(64) {
                        *b = b.wrapping_add(1);
                    }
                }
                out.clear();
                rig.now += Duration::from_millis(3);
                let now = rig.now;
                // A typed line: a new glyph each time, a new row now and then.
                let key: &[u8] = if i % 60 == 59 { b"\r\n" } else { b"a" };
                let t = Instant::now();
                rig.ctx(|e, ctx| e.on_burst(key, now, out, ctx));
                spent += t.elapsed();
            }
            spent.as_secs_f64() * 1e6 / n as f64
        };
        let hot = run(&mut rig, &mut out, 20_000, false);
        let cold = run(&mut rig, &mut out, 300, true);
        println!(
            "key echo{}: {hot:.1} us/burst hot, {cold:.1} us/burst cold",
            if sprites { " with a sprite" } else { "" },
        );
    }
}
