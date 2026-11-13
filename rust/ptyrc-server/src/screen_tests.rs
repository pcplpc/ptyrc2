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
    /// Events and completions of the hooks, in order.
    events: Vec<(String, Value)>,
    done: Vec<(Token, Value)>,
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
            events: Vec::new(),
            done: Vec::new(),
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
        let r = f(&mut self.engine, &mut ctx);
        self.events.extend(fx.events);
        self.done
            .extend(fx.completions.into_iter().map(|(t, r)| (t, r.unwrap())));
        r
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
    json!({"anchor": {"at": [x, y], "space": "view"}, "content": {"rows": [text]}, "layer": "front"})
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
fn a_scroll_unpaints_only_the_rows_that_leave() {
    let mut rig = Rig::new(20, 4);
    rig.burst(b"one\r\ntwo\r\nthree\r\nfour");
    rig.put(text_at(10, 3, "SPRITE"));
    // Row 0 leaves; the sprite is on row 3: nothing before the burst.
    let out = rig.burst(b"\r\nfive");
    assert!(out.starts_with(&[BSU, b"\r\nfive"].concat()), "{out:?}");
    assert_eq!(count(&out, BSU), 1);
    // It scrolled to row 2 with the text, and is put back on row 3.
    assert_eq!(rig.row(2), "four");
    assert_eq!(rig.row(3), "five      SPRITE");
    assert_eq!(count(&out, b"SPRITE"), 1);
    // A burst that also deletes a line: any row may leave, all is unpainted.
    let out = rig.burst(b"\x1b[1;1H\x1b[M\x1b[4;1H\r\nsix");
    assert!(
        !out.starts_with(&[BSU, b"\x1b[1;1H\x1b[M"].concat()),
        "{out:?}"
    );
    assert_eq!(rig.row(3), "six       SPRITE");
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

/// A `screen.wait` left pending; its token.
fn pending(rig: &mut Rig, params: Value) -> Token {
    rig.next += 1;
    let token = Token(rig.next);
    let reply = rig.ctx(|e, ctx| e.handle(ConnId(7), token, "screen.wait", params, ctx));
    assert!(matches!(reply, Reply::Pending), "{reply:?}");
    token
}

#[test]
fn wait_idle_answers_when_the_frame_is_idle() {
    let mut rig = Rig::new(40, 10);
    rig.burst(b"hello");
    let token = pending(&mut rig, json!({"idle": true, "timeout_ms": 30000}));
    rig.advance(9000);
    assert!(rig.done.is_empty(), "idle before 10 s");
    rig.advance(1500);
    let (t, result) = rig.done.pop().expect("answered");
    assert_eq!(t, token);
    assert_eq!(result["matched"], true);
    assert_eq!(result["frame"], 0);
    assert_eq!(result["idle"]["idle"], true);
    // Idle now: answered at once.
    let now = rig.call("screen.wait", json!({"idle": true, "frame": "idle"}));
    assert_eq!(now["matched"], true);
}

#[test]
fn wait_idle_times_out_on_a_busy_screen() {
    let mut rig = Rig::new(40, 10);
    let token = pending(&mut rig, json!({"idle": true, "timeout_ms": 3000}));
    for i in 0..40 {
        // Most of the screen changes, again and again.
        let fill = format!("\x1b[H{}", format!("{i:040}").repeat(10));
        rig.burst(fill.as_bytes());
        rig.advance(100);
    }
    assert_eq!(rig.done, vec![(token, json!({"matched": false}))]);
}

#[test]
fn idle_events_on_change_only() {
    let mut rig = Rig::new(40, 10);
    rig.topics.insert("idle".into());
    rig.burst(b"hello");
    rig.advance(12000);
    let idle: Vec<_> = rig
        .events
        .iter()
        .filter(|(t, d)| t == "idle" && d["frame"] == 0)
        .collect();
    assert_eq!(idle.len(), 1, "{idle:?}");
    assert_eq!(idle[0].1["idle"], true);
    rig.events.clear();
    for i in 0..5 {
        let fill = format!("\x1b[H{}", format!("{i:040}").repeat(10));
        rig.burst(fill.as_bytes());
        rig.advance(300);
    }
    let idle: Vec<_> = rig
        .events
        .iter()
        .filter(|(t, d)| t == "idle" && d["frame"] == 0)
        .collect();
    assert_eq!(idle.len(), 1, "{idle:?}");
    assert_eq!(idle[0].1["idle"], false);
    // No subscriber: no wake for the watch.
    rig.topics.clear();
    rig.advance(1000);
    let before = rig.engine.ticks;
    rig.advance(5000);
    assert_eq!(rig.engine.ticks, before);
}

#[test]
fn a_forgotten_island_takes_its_sprites() {
    let mut rig = Rig::new(40, 20);
    rig.topics.insert("draw".into());
    let page = |k: usize| -> String {
        let rows: Vec<String> = (0..12).map(|i| format!("run {k:02} out {i:02}")).collect();
        format!("\x1b[2J\x1b[H{}", rows.join("\r\n"))
    };
    let pane = |rig: &mut Rig| {
        let frames = rig.call("frame.list", json!({}))["frames"].clone();
        let pane = frames
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["kind"] == "pane");
        pane.expect("a pane").clone()
    };
    rig.burst(page(0).as_bytes());
    rig.advance(2000);
    let first = pane(&mut rig);
    let at = json!([20, first["origin"][1].as_i64().unwrap() + 3]);
    let anchor = json!({"at": at, "space": "frame"});
    let spec = json!({"frame": first["id"], "anchor": anchor, "content": {"rows": ["<MK"]}});
    let id = rig.put(spec);
    assert!(rig.row(3).ends_with("<MK"), "{:?}", rig.row(3));
    // A shell cleared again and again: an island per clear, eight kept.
    for k in 1..=12 {
        rig.burst(page(k).as_bytes());
        rig.advance(600);
    }
    assert_eq!(pane(&mut rig)["id"], first["id"]);
    assert_eq!(rig.call("draw.list", json!({}))["sprites"], json!([]));
    let draw: Vec<_> = rig.events.iter().filter(|(t, _)| t == "draw").collect();
    assert_eq!(draw.len(), 1, "{draw:?}");
    assert_eq!(draw[0].1["removed"], json!([id]));
    assert_eq!(draw[0].1["frame"], first["id"]);
}

fn topic<'a>(rig: &'a Rig, name: &str) -> Vec<&'a Value> {
    let named = rig.events.iter().filter(|(t, _)| t == name);
    named.map(|(_, d)| d).collect()
}

fn lines(k: usize, n: usize) -> String {
    let rows: Vec<String> = (k..k + n).map(|i| format!("line {i:03} of text")).collect();
    rows.join("\r\n")
}

#[test]
fn the_idle_watch_sleeps_until_a_frame_can_be_idle() {
    let mut rig = Rig::new(40, 10);
    rig.topics.insert("idle".into());
    rig.burst(b"hello");
    rig.advance(3000);
    let before = rig.engine.ticks;
    rig.advance(8000);
    // One look when the ten seconds are over, not one every 250 ms.
    assert!(
        rig.engine.ticks - before <= 3,
        "{}",
        rig.engine.ticks - before
    );
    let mut idle = topic(&rig, "idle");
    idle.retain(|d| d["frame"] == 0);
    assert_eq!(idle.len(), 1, "{idle:?}");
    assert_eq!(idle[0]["idle"], true);
    // Idle: nothing to wait for.
    rig.advance(2000);
    let before = rig.engine.ticks;
    rig.advance(60000);
    assert_eq!(rig.engine.ticks, before);
}

#[test]
fn a_snapshot_tells_the_state_of_a_topic() {
    let mut rig = Rig::new(40, 10);
    rig.burst(b"hello");
    rig.advance(11000);
    let now = rig.now;
    let idle = rig.engine.snapshot("idle", now);
    assert_eq!(idle.len(), 2, "the screen and its pane: {idle:?}");
    assert_eq!(idle[0]["frame"], 0);
    assert_eq!(idle[0]["idle"], true);
    let cursor = rig.engine.snapshot("cursor", now);
    assert_eq!(cursor.len(), 1);
    assert_eq!(cursor[0]["x"], 5);
    assert_eq!(cursor[0]["pos"], json!([5, 0]));
    let frames = rig.engine.snapshot("frames", now);
    assert_eq!(frames[0]["frames"][0]["id"], 0);
    assert!(rig.engine.snapshot("screen", now).is_empty());
}

#[test]
fn the_cursor_event_is_in_frame_cells() {
    let mut rig = Rig::new(40, 12);
    rig.topics.insert("cursor".into());
    rig.burst(format!("\x1b[H{}", lines(0, 12)).as_bytes());
    rig.advance(2000);
    rig.events.clear();
    rig.burst(b"\x1b[5;3H");
    let cursor = topic(&rig, "cursor");
    assert_eq!(cursor.len(), 1, "{cursor:?}");
    let first = cursor[0].clone();
    assert_eq!((&first["x"], &first["y"]), (&json!(2), &json!(4)));
    let frame = first["frame"].as_u64().expect("a frame");
    assert_ne!(frame, 0, "the pane, not the raw frame");
    let y = first["pos"][1].as_i64().unwrap();
    // The text scrolls under a cursor that stays: told again.
    rig.events.clear();
    rig.burst(format!("\x1b[H\x1b[M\x1b[12;1H{}\x1b[5;3H", lines(12, 1)).as_bytes());
    let cursor = topic(&rig, "cursor");
    assert_eq!(cursor.len(), 1, "{cursor:?}");
    assert_eq!((&cursor[0]["x"], &cursor[0]["y"]), (&json!(2), &json!(4)));
    assert_eq!(cursor[0]["frame"], frame);
    assert_eq!(cursor[0]["pos"][1], y + 1);
}

#[test]
fn text_events_name_the_frame_rows_that_changed() {
    let mut rig = Rig::new(40, 12);
    rig.topics.insert("text".into());
    rig.burst(format!("\x1b[H{}", lines(0, 12)).as_bytes());
    rig.advance(2000);
    let pane = rig.call("frame.list", json!({}))["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["kind"] == "pane")
        .expect("a pane")
        .clone();
    let top = pane["origin"][1].as_i64().unwrap();
    rig.events.clear();
    // The same text again: no change.
    rig.burst(b"\x1b[4;1Hline 003 of text");
    assert!(topic(&rig, "text").is_empty());
    // One row rewritten.
    rig.burst(b"\x1b[4;1Hline 003 of TEXT");
    let of_pane = |rig: &Rig| -> Vec<Value> {
        let all = topic(rig, "text").into_iter().cloned();
        all.filter(|d| d["frame"] == pane["id"]).collect()
    };
    let text = of_pane(&rig);
    assert_eq!(text.len(), 1, "{text:?}");
    assert_eq!(text[0]["rows"], json!([top + 3]));
    assert_eq!(text[0]["origin"], pane["origin"]);
    // A scroll: the origin moves, no row changed; the row that comes
    // into view is new, not changed.
    rig.events.clear();
    rig.burst(format!("\x1b[H\x1b[M\x1b[12;1H{}", lines(12, 1)).as_bytes());
    let text = of_pane(&rig);
    assert_eq!(text.len(), 1, "{text:?}");
    assert_eq!(text[0]["rows"], json!([]));
    assert_eq!(text[0]["origin"][1], top + 1);
    // No subscriber: nothing kept.
    rig.topics.clear();
    rig.burst(b"x");
    assert!(rig.engine.shown_is_empty());
}

#[test]
fn draw_list_tells_the_style() {
    let mut rig = Rig::new(40, 10);
    let tint = |fg: &str| json!({"content": {"rows": ["  "], "style": {"fg": fg}, "opaque": true}, "blend": "tint"});
    let red = rig.put(tint("red"));
    let blue = rig.put(tint("blue"));
    let plain = rig.put(text_at(0, 0, "x"));
    let list = rig.call("draw.list", json!({}))["sprites"].clone();
    let of = |id: u64| {
        let all = list.as_array().unwrap().iter();
        all.clone().find(|s| s["id"] == id).unwrap().clone()
    };
    assert_eq!(of(red)["style"], json!({"fg": "red"}));
    assert_eq!(of(blue)["style"], json!({"fg": "blue"}));
    assert!(of(plain).get("style").is_none());
}

#[test]
fn host_colours_set_at_run_time_remix_in_one_repaint() {
    let mut rig = Rig::new(40, 6);
    rig.put(json!({
        "anchor": {"at": [2, 1], "space": "view"}, "alpha": 0.5,
        "content": {"rows": ["##"], "opaque": true, "style": {"bg": "#ffffff"}},
    }));
    let bg = |rig: &Rig| rig.host.grid().cell(2, 1).style.bg;
    assert_eq!(bg(&rig), ptyrc_vt::Color::Rgb(128, 128, 128));
    // A static sprite: nothing goes out until the colours change.
    assert!(rig.advance(500).is_empty());
    let colors = ptyrc_core::scene::HostColors {
        fg: [0, 0, 0],
        bg: [255, 255, 255],
    };
    rig.ctx(|e, ctx| e.on_host_colors(colors, ctx));
    let out = rig.tick();
    assert_eq!(count(&out, BSU), 1, "one synchronized update");
    assert_eq!(bg(&rig), ptyrc_vt::Color::Rgb(255, 255, 255));
    // Told the same again: nothing.
    rig.ctx(|e, ctx| e.on_host_colors(colors, ctx));
    assert!(rig.tick().is_empty());
}

#[test]
fn a_declared_layer_name_is_used_and_hinted() {
    let mut rig = Rig::new(40, 6);
    let layers = rig.call("layer.define", json!({"name": "stage", "z": -50}));
    assert_eq!(layers["layers"][1], json!({"name": "stage", "z": -50}));
    assert_eq!(rig.call("layer.list", json!({})), layers);
    let mut spec = text_at(1, 1, "x");
    spec["layer"] = json!("stage");
    let id = rig.put(spec.clone());
    let sprites = rig.call("draw.list", json!({}));
    assert_eq!(sprites["sprites"][0]["layer"], -50);
    rig.call("draw.update", json!({"id": id, "layer": "hud"}));
    rig.call("draw.update", json!({"id": id, "layer": "stage"}));

    let mut fail = |method: &str, params: Value| {
        let reply = rig.ctx(|e, ctx| e.handle(ConnId(7), Token(99), method, params, ctx));
        match reply {
            Reply::Err(e) => e,
            other => panic!("{method}: {other:?}"),
        }
    };
    spec["layer"] = json!("stagee");
    let error = fail("draw.put", spec);
    assert_eq!(error.message, "bad layer: stagee");
    let hint = error.hint.unwrap();
    assert!(hint.starts_with("did you mean stage?"), "{hint}");
    assert!(
        hint.contains("declared: stage (-50)") && hint.contains("back, front"),
        "{hint}"
    );
    for params in [
        json!({"name": "app", "z": 3}),
        json!({"name": "low", "z": 0}),
        json!({"name": "back", "z": -3}),
        json!({"name": "low"}),
    ] {
        fail("layer.define", params);
    }
}

impl Rig {
    fn fail(&mut self, conn: u64, method: &str, params: Value) -> ptyrc_proto::RpcError {
        let reply = self.ctx(|e, ctx| e.handle(ConnId(conn), Token(99), method, params, ctx));
        match reply {
            Reply::Err(e) => e,
            other => panic!("{method}: {other:?}"),
        }
    }

    fn call_as(&mut self, conn: u64, method: &str, params: Value) -> Value {
        let reply = self.ctx(|e, ctx| e.handle(ConnId(conn), Token(98), method, params, ctx));
        match reply {
            Reply::Ok(value) => value,
            other => panic!("{method}: {other:?}"),
        }
    }
}

fn clock(rig: &mut Rig) -> u64 {
    let tags = rig.call(
        "widget.declare",
        json!({"widgets": [
            {"tag": "clock", "size": [5, 1], "content": {"rows": ["12:00"]}},
            {"tag": "clock", "name": "big", "size": [11, 3]},
        ]}),
    );
    assert_eq!(tags, json!({"tags": ["clock"]}));
    let shown = rig.call("widget.show", json!({"tag": "clock", "fill": 0.1}));
    shown["widget"]["id"].as_u64().unwrap()
}

#[test]
fn a_widget_is_laid_out_drawn_and_told() {
    let mut rig = Rig::new(40, 10);
    rig.topics.insert("widgets".into());
    rig.burst(b"some text");
    let id = clock(&mut rig);
    // The screen is not quiet yet: hidden, then placed in the same look
    // that finds the room.
    assert_eq!(topic(&rig, "widgets")[0]["state"], "hidden");
    rig.advance(1500);
    let placed = topic(&rig, "widgets").last().copied().unwrap().clone();
    assert_eq!(placed["state"], "placed");
    assert_eq!(placed["variant"], "5x1");
    assert_eq!(placed["size"], json!([5, 1]));
    let [x, y] = [
        placed["rect"][0].as_u64().unwrap(),
        placed["rect"][1].as_u64().unwrap(),
    ];
    let (x, y) = (x as usize, y as u16);
    assert_eq!(&rig.row(y + 1)[x..], "│12:00│");
    assert_eq!(&rig.row(y + 2)[x..], "└ cl… ┘");
    let list = rig.call("widget.list", json!({}));
    assert_eq!(list["widgets"][0], placed);
    assert_eq!(
        rig.engine.snapshot("widgets", rig.now),
        vec![placed.clone()]
    );
    let sprites = rig.call("draw.list", json!({}));
    assert_eq!(sprites["sprites"][0]["id"], placed["sprite"]);
    // A quiet screen: no wake for the widgets.
    assert_eq!(rig.engine.widgets.next_wake(), None);

    // Text written over it shows; pressed, it moves elsewhere.
    let at = format!("\x1b[{};{}HOVER", y + 2, x + 2);
    rig.burst(at.as_bytes());
    assert!(rig.row(y + 1).contains("OVER"));
    rig.advance(400);
    let moved = topic(&rig, "widgets").last().copied().unwrap().clone();
    assert_eq!(moved["id"], id);
    assert_ne!(moved["rect"], placed["rect"]);
    assert!(rig.row(y + 1).contains("OVER"));

    rig.call(
        "widget.set",
        json!({"id": id, "content": {"rows": ["13:00"]}, "title": ""}),
    );
    rig.tick();
    let (x, y) = (
        moved["rect"][0].as_u64().unwrap() as usize,
        moved["rect"][1].as_u64().unwrap() as u16,
    );
    let cells = |rig: &Rig, row: u16| rig.row(row).chars().skip(x).take(7).collect::<String>();
    assert_eq!(cells(&rig, y), "┌─────┐");
    assert_eq!(cells(&rig, y + 1), "│13:00│");

    // The connection goes: so does the widget, and the host is clean.
    rig.ctx(|e, ctx| e.on_disconnect(ConnId(7), ctx));
    rig.tick();
    assert_eq!(topic(&rig, "widgets").last().unwrap()["state"], "gone");
    assert!(!rig.row(y).contains('┌'));
    assert!(
        rig.call("widget.list", json!({}))["widgets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn widget_errors_and_hints() {
    let mut rig = Rig::new(40, 10);
    let e = rig.fail(7, "widget.show", json!({"tag": "clock"}));
    assert_eq!(
        (e.code, e.message.as_str()),
        (ptyrc_proto::ErrorCode::BadParams, "no such tag: clock")
    );
    assert!(e.hint.unwrap().contains("`widget.declare` first"));
    let id = clock(&mut rig);
    let e = rig.fail(7, "widget.show", json!({"tag": "clok"}));
    assert_eq!(
        e.hint.unwrap(),
        "did you mean clock? tags of this connection: clock"
    );
    let e = rig.fail(7, "widget.show", json!({"tag": "clock", "border": "rond"}));
    assert_eq!(e.message, "no such border: rond");
    assert!(
        e.hint
            .unwrap()
            .starts_with("did you mean round? borders: plain, round")
    );
    let e = rig.fail(
        7,
        "widget.show",
        json!({"tag": "clock", "border": {"corner": "++++"}}),
    );
    assert_eq!(e.message, "bad border");
    assert!(e.hint.unwrap().contains("\"corners\""));
    let e = rig.fail(7, "widget.show", json!({"tag": "clock", "frame": "activ"}));
    assert_eq!(e.code, ptyrc_proto::ErrorCode::NoFrame);
    assert!(e.hint.unwrap().starts_with("did you mean active?"));
    let e = rig.fail(7, "widget.show", json!({"tag": "clock", "frame": 42}));
    assert_eq!(e.code, ptyrc_proto::ErrorCode::NoFrame);
    let e = rig.fail(7, "widget.show", json!({"tag": "clock", "fill": 2}));
    assert!(e.hint.unwrap().contains("0.1 to 1"));
    let e = rig.fail(7, "widget.show", json!({"tag": "clock", "layer": "bak"}));
    assert!(e.hint.unwrap().starts_with("did you mean back?"));
    let e = rig.fail(
        7,
        "widget.set",
        json!({"id": id, "variant": "bgi", "title": "x"}),
    );
    assert_eq!(e.hint.unwrap(), "did you mean big? variants: 5x1, big");
    // Not placed yet: no variant shown.
    let e = rig.fail(
        7,
        "widget.set",
        json!({"id": id, "content": {"rows": ["x"]}}),
    );
    assert_eq!(e.code, ptyrc_proto::ErrorCode::NotUsable);
    assert_eq!(e.hint.unwrap(), "variants: 5x1, big");
    let e = rig.fail(7, "widget.set", json!({"id": id, "rows": ["x"]}));
    assert_eq!(e.code, ptyrc_proto::ErrorCode::BadParams);
    assert!(e.message.contains("unknown field `rows`"), "{}", e.message);
    // Another connection's widget is not there for this one.
    let e = rig.fail(8, "widget.hide", json!({"id": id}));
    assert_eq!(
        (e.code, e.message.as_str()),
        (ptyrc_proto::ErrorCode::NoSprite, "no such widget: 1")
    );
    assert_eq!(
        e.hint.unwrap(),
        "this connection shows no widget: `widget.show` first"
    );
    let e = rig.fail(7, "widget.set", json!({"id": 5, "title": "x"}));
    assert_eq!(e.hint.unwrap(), "widgets of this connection: 1");
    let e = rig.fail(
        7,
        "widget.declare",
        json!({"widgets": [{"tag": "a", "size": [0, 2]}]}),
    );
    assert!(e.message.contains("from 1x1 to 200x60"));
    let e = rig.fail(7, "widget.decalre", json!({}));
    assert!(e.hint.unwrap().starts_with("did you mean widget.declare?"));
    assert_eq!(
        rig.call("widget.borders", json!({}))["borders"],
        json!(["plain", "round", "heavy", "double", "ascii", "none"])
    );
    rig.call("widget.hide", json!({"id": id}));
    assert!(
        rig.call_as(8, "widget.list", json!({}))["widgets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn widgets_of_two_connections_and_draw_remove() {
    let mut rig = Rig::new(60, 16);
    rig.advance(1500);
    let decl = json!({"widgets": [{"tag": "w", "size": [10, 2]}, {"tag": "w", "size": [20, 4]}],
                      "border": "heavy"});
    rig.call_as(1, "widget.declare", decl.clone());
    rig.call_as(2, "widget.declare", decl);
    let a = rig.call_as(
        1,
        "widget.show",
        json!({"tag": "w", "layer": "front", "prefer": "top-left"}),
    );
    let b = rig.call_as(2, "widget.show", json!({"tag": "w", "title": "two"}));
    rig.tick();
    let (a, b) = (&a["widget"], &b["widget"]);
    assert_eq!(a["state"], "placed");
    assert_eq!(b["state"], "placed");
    assert_eq!(a["border"], "heavy");
    let rect = |w: &Value| {
        let r: Vec<u16> = (0..4)
            .map(|i| w["rect"][i].as_u64().unwrap() as u16)
            .collect();
        ptyrc_vt::Rect::new(r[0], r[1], r[2], r[3])
    };
    assert!(!rect(a).intersects(&rect(b)));
    let list = rig.call("draw.list", json!({}));
    let layers: Vec<i64> = list["sprites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["layer"].as_i64().unwrap())
        .collect();
    assert!(layers.contains(&1) && layers.contains(&-1));
    // `draw.remove` on its sprite: the widget goes.
    rig.call("draw.remove", json!({"id": b["sprite"]}));
    rig.tick();
    let left = rig.call("widget.list", json!({}));
    assert_eq!(left["widgets"].as_array().unwrap().len(), 1);
    assert_eq!(left["widgets"][0]["id"], a["id"]);
}

#[test]
fn a_resize_lays_widgets_out_again() {
    let mut rig = Rig::new(60, 16);
    rig.advance(1500);
    rig.call(
        "widget.declare",
        json!({"widgets": [{"tag": "w", "size": [8, 2]}, {"tag": "w", "size": [40, 10]}]}),
    );
    let shown = rig.call("widget.show", json!({"tag": "w"}));
    assert_eq!(shown["widget"]["variant"], "40x10");
    let now = rig.now;
    let mut out = Vec::new();
    rig.info.cols = 30;
    rig.info.rows = 8;
    rig.host.resize(30, 8);
    rig.ctx(|e, ctx| e.on_resize(30, 8, now, &mut out, ctx));
    rig.host.feed(&out);
    rig.advance(3000);
    let info = &rig.call("widget.list", json!({}))["widgets"][0];
    assert_eq!(info["state"], "placed");
    assert_eq!(info["variant"], "8x2");
    let [x, _, w, _] = [0, 1, 2, 3].map(|i| info["rect"][i].as_u64().unwrap());
    assert!(x + w <= 30);
}
