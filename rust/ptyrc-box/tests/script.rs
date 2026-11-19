// SPDX-License-Identifier: GPL-3.0-or-later
//! Script apps through the interpreter guest (`sdk/js`).
//!
//! The interpreter is built apart:
//!
//! ```text
//! cargo build --release --manifest-path sdk/js/interp/Cargo.toml
//! ```
//!
//! Where it is not built, each test passes and tests nothing; the
//! first says so on the terminal (not captured). `PTYRC_REQUIRE_JS=1`:
//! they fail instead.

use ptyrc_box::headless::{Options, Report, run};
use ptyrc_box::{Error, Limits, script};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn interpreter() -> Option<&'static [u8]> {
    static WASM: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    WASM.get_or_init(|| {
        let path = std::env::var_os(script::ENV_JS).map_or_else(
            || root().join("sdk/js/interp/target/wasm32-unknown-unknown/release/ptyrc_js.wasm"),
            PathBuf::from,
        );
        let wasm = std::fs::read(&path).ok();
        if wasm.is_none() {
            assert!(
                std::env::var_os("PTYRC_REQUIRE_JS").is_none(),
                "no interpreter at {}",
                path.display()
            );
            // Past the capture of the test harness: seen in every run.
            let _ = writeln!(
                std::io::stderr(),
                "script tests SKIPPED: no interpreter at {}; \
                 cargo build --release --manifest-path sdk/js/interp/Cargo.toml",
                path.display()
            );
        }
        wasm
    })
    .as_deref()
}

fn options(files: &[(&str, &str)], args: Value) -> Options {
    let sources: BTreeMap<String, String> = files
        .iter()
        .map(|(n, t)| ((*n).to_owned(), (*t).to_owned()))
        .collect();
    Options {
        size: (40, 10),
        args: script::init_args(files[0].0, &sources, args),
        duration_ms: 200,
        at: vec![200],
        ..Options::default()
    }
}

fn play(files: &[(&str, &str)]) -> Option<Report> {
    let wasm = interpreter()?;
    Some(run(wasm, &options(files, json!({}))).unwrap())
}

/// One file, `app.js`.
fn app(text: &str) -> Option<Report> {
    play(&[("app.js", text)])
}

fn logged(report: &Report, needle: &str) -> bool {
    report.log.iter().any(|l| l.contains(needle))
}

#[test]
fn draws_and_ticks() {
    let Some(report) = play(&[
        (
            "app.js",
            r##"
        import { Canvas, period, log, call } from "ptyrc";
        import { word } from "./lib/word.js";
        let c;
        export function init(args) {
          period(50);
          c = new Canvas(10, 2);
          log("size", call("session.info").size);
        }
        export function tick(now) {
          c.clear();
          c.text(0, 0, word + " " + now, { fg: "#ff0000", bold: true });
          c.present({ anchor: { at: [1, 1] } });
        }
        "##,
        ),
        ("lib/word.js", "export const word = 'hi';"),
    ]) else {
        return;
    };
    assert_eq!(report.end, None, "{report}");
    assert!(report.ticks >= 3, "{report}");
    let screen = &report.screens[0].1;
    assert!(screen[1].starts_with(" hi "), "{report}");
}

#[test]
fn args_reach_init_without_the_reserved_members() {
    let Some(wasm) = interpreter() else { return };
    let files = [(
        "app.js",
        "import { log } from 'ptyrc';\nexport function init(args) { log(args); }",
    )];
    let report = run(wasm, &options(&files, json!({"rate": 3}))).unwrap();
    assert!(logged(&report, r#"{"rate":3}"#), "{report}");
}

#[test]
fn time_is_session_time_and_random_is_the_hosts() {
    let Some(a) = app(
        "import { log, period, random } from 'ptyrc';
         export function init() { period(50); log('at', Date.now(), Math.random(), random(4).join()); }
         export function tick(now) { if (now === 100) log('tick', now, Date.now()); }",
    ) else {
        return;
    };
    assert!(logged(&a, "at 0 "), "{a}");
    assert!(logged(&a, "tick 100 100"), "{a}");
    let b = app(
        "import { log, period, random } from 'ptyrc';
         export function init() { period(50); log('at', Date.now(), Math.random(), random(4).join()); }
         export function tick(now) { if (now === 100) log('tick', now, Date.now()); }",
    )
    .unwrap();
    assert_eq!(a.log, b.log);
}

/// Errors name the file, the line, and the cure when known.
#[test]
fn errors_name_the_place_and_the_cure() {
    let cases: &[(&str, &[&str])] = &[
        (
            "export function init() {\n  let x = ;\n}",
            &["app.js:2:11: unexpected token"],
        ),
        (
            "export function init() {\n  setTimeout(() => {}, 10);\n}",
            &[
                "app.js:1:",
                "ReferenceError: setTimeout is not defined",
                "hint: no timer",
            ],
        ),
        (
            "export function init() { fetch('https://example.org'); }",
            &["fetch is not defined", "hint: no network"],
        ),
        (
            "import fs from 'fs';\nexport function init() {}",
            &[
                "import \"fs\": no such module",
                "hint: \"ptyrc\", or a file",
            ],
        ),
        (
            "import x from '../x.js';\nexport function init() {}",
            &["outside the app"],
        ),
        (
            "import x from './nope.js';\nexport function init() {}",
            &[
                "`nope.js`: no such file in the app",
                "the app holds: app.js",
            ],
        ),
        (
            "let a = 1;",
            &[
                "exports none of `init`, `tick`, `event`",
                "hint: export function init",
            ],
        ),
        (
            "import { Canvas } from 'ptyrc';\nexport function init() { new Canvas(2, 2).set(0, 0, 'x', { fg: 'red' }); }",
            &["app.js:2:", "\"red\" is no colour"],
        ),
        (
            "import { Canvas } from 'ptyrc';\nexport function init() { new Canvas(100000, 100000); }",
            &[
                "RangeError",
                "hint: the canvases of an app hold four screens",
            ],
        ),
        (
            "export async function init() { throw new Error('late'); }",
            &["Error: late"],
        ),
        (
            "await new Promise(() => {});\nexport function init() {}",
            &["no promise outlives a call"],
        ),
    ];
    for (text, wanted) in cases {
        let Some(report) = app(text) else { return };
        assert_eq!(report.end, Some(Error::Exit(1)), "{text}\n{report}");
        for w in *wanted {
            assert!(logged(&report, w), "{text}\nwanted: {w}\n{:#?}", report.log);
        }
    }
}

/// A request refused and not caught: told once, the app goes on.
#[test]
fn a_refusal_does_not_end_the_app() {
    let Some(report) = app("import { call, period, log, PtyrcError } from 'ptyrc';
         export function init() {
           period(50);
           try { call('input.send', { text: 'x' }); }
           catch (e) { log('caught', e instanceof PtyrcError, e.code); }
         }
         export function tick() { call('input.send', { text: 'x' }); }")
    else {
        return;
    };
    assert_eq!(report.end, None, "{report}");
    assert!(report.ticks >= 3, "{report}");
    assert!(logged(&report, "caught true headless"), "{report}");
    let told = report
        .log
        .iter()
        .filter(|l| l.contains("PtyrcError"))
        .count();
    assert_eq!(told, 1, "{report}");
}

// Hostile scripts: each ends contained.

fn quick(files: &[(&str, &str)]) -> Option<Result<Report, Error>> {
    let wasm = interpreter()?;
    let mut o = options(files, json!({}));
    o.limits = Limits {
        hard: Duration::from_millis(400),
        init: Duration::from_millis(400),
        ..Limits::default()
    };
    Some(run(wasm, &o))
}

#[test]
fn hostile_loops() {
    let start = Instant::now();
    let Some(r) = quick(&[("app.js", "export function init() { for (;;) {} }")]) else {
        return;
    };
    assert_eq!(r.unwrap().end, Some(Error::Time("init")));
    let r = quick(&[("app.js", "for (;;) {}\nexport function init() {}")]).unwrap();
    assert_eq!(r.unwrap().end, Some(Error::Time("init")));
    let r = quick(&[(
        "app.js",
        "import { period } from 'ptyrc';
         export function init() { period(50); }
         export function tick() { for (;;) {} }",
    )])
    .unwrap()
    .unwrap();
    assert_eq!(r.end, Some(Error::Time("tick")));
    assert_eq!(r.ticks, 1);
    // A promise chain without end is a loop like another.
    let r = quick(&[(
        "app.js",
        "export function init() { const f = () => Promise.resolve().then(f); f(); }",
    )])
    .unwrap();
    assert_eq!(r.unwrap().end, Some(Error::Time("init")));
    assert!(start.elapsed() < Duration::from_secs(60));
}

#[test]
fn hostile_memory() {
    for text in [
        "export function init() { const a = []; for (;;) a.push(new Uint8Array(1 << 20)); }",
        "export function init() { let s = 'x'; for (;;) s += s; }",
        "export function init() { const a = []; for (;;) a.push({ a }); }",
    ] {
        let Some(r) = quick(&[("app.js", text)]) else {
            return;
        };
        let r = r.unwrap();
        assert!(
            matches!(
                r.end,
                Some(Error::Exit(1) | Error::Trap(_) | Error::Time(_) | Error::Memory)
            ),
            "{text}\n{r}"
        );
    }
    let r =
        app("export function init() { const a = []; for (;;) a.push(new Uint8Array(1 << 20)); }")
            .unwrap();
    assert!(r.met.iter().any(|m| m == "memory"), "{r}");
}

#[test]
fn hostile_recursion() {
    for text in [
        "function f(n) { return f(n + 1) + 1; }\nexport function init() { f(0); }",
        "export function init() { let s = '['.repeat(200000); JSON.parse(s); }",
        "export function init() { let a = []; for (let i = 0; i < 100000; i++) a = [a]; JSON.stringify(a); }",
        "export function init() { new Function('return ' + '('.repeat(100000))(); }",
    ] {
        let Some(r) = quick(&[("app.js", text)]) else {
            return;
        };
        let r = r.unwrap();
        assert!(
            matches!(
                r.end,
                Some(Error::Exit(1) | Error::Trap(_) | Error::Stack | Error::Time(_))
            ),
            "{text}\n{r}"
        );
    }
}

/// The API is not the app's to replace, nor the host functions its to
/// reach.
#[test]
fn hostile_modules() {
    // A file named as the API is a file, by its relative path only.
    let Some(r) = play(&[
        (
            "app.js",
            "import { log } from 'ptyrc';
             import * as real from 'ptyrc';
             import * as mine from './ptyrc.js';
             export function init() { log('real', typeof real.Canvas, 'mine', mine.call('x')); }",
        ),
        ("ptyrc.js", "export const call = () => 'shadow';"),
        ("ptyrc", "export const call = () => 'shadow';"),
    ]) else {
        return;
    };
    assert!(logged(&r, "real function mine shadow"), "{r}");
    for text in [
        "import * as n from 'ptyrc:native';\nexport function init() {}",
        "export async function init() { await import('ptyrc:native'); }",
        "import x from '/etc/passwd';\nexport function init() {}",
        "import x from 'https://example.org/x.js';\nexport function init() {}",
        "import x from './../../x.js';\nexport function init() {}",
    ] {
        let r = app(text).unwrap();
        assert_eq!(r.end, Some(Error::Exit(1)), "{text}\n{r}");
        assert!(
            logged(&r, "no such module") || logged(&r, "outside the app"),
            "{text}\n{r}"
        );
    }
    // The exports of the API cannot be assigned.
    let r = app("import * as api from 'ptyrc';
         import { log } from 'ptyrc';
         export function init() {
           try { api.call = () => 1; } catch (e) { log('frozen', e.name); }
         }")
    .unwrap();
    assert!(logged(&r, "frozen TypeError"), "{r}");
}

/// Reserved members given as arguments do not pass for the sources.
#[test]
fn hostile_arguments() {
    let Some(wasm) = interpreter() else { return };
    let files = [(
        "app.js",
        "import { log } from 'ptyrc';\nexport function init(a) { log('mine', Object.keys(a).join()); }",
    )];
    let evil = json!({
        "ptyrc:main": "evil.js",
        "ptyrc:sources": {"evil.js": "export function init() { throw 1; }"},
        "x": 1,
    });
    let report = run(wasm, &options(&files, evil)).unwrap();
    assert!(logged(&report, "mine x"), "{report}");
}

#[test]
fn hostile_floods_and_text() {
    let Some(r) = app("import { call, log, Canvas, period } from 'ptyrc';
         export function init() { period(50); }
         export function tick(now) {
           const c = new Canvas(8, 1);
           c.text(0, 0, 'a\\u001b]52;c;eA==\\u0007b\\u009b', { fg: 3 });
           c.present();
           log('esc\\u001b[2J');
           let refused = 0;
           for (let i = 0; i < 3000; i++) {
             try { call('draw.list'); } catch (e) { if (e.code === 'limit') refused++; }
           }
           if (now === 50) log('refused', refused > 0);
           for (let i = 0; i < 100000; i++) new Canvas(4000, 4000);
         }")
    else {
        return;
    };
    assert!(logged(&r, "refused true") || r.end.is_some(), "{r}");
    for (_, screen) in &r.screens {
        for row in screen {
            assert!(!row.chars().any(char::is_control), "{row:?}");
        }
    }
}

/// Text sprites and widgets show in the headless screen; a widget once
/// laid out (under a second on a still screen).
#[test]
fn sprites_and_widgets_show() {
    let Some(wasm) = interpreter() else { return };
    let text = r#"
        import { call } from "ptyrc";
        export function init() {
          call("draw.put", { anchor: { at: [0, 0] }, content: { rows: ["top"] } });
          call("widget.declare", { widgets: [{ tag: "w", name: "small", size: [5, 1],
                                                content: { rows: ["hello"] } }] });
          call("widget.show", { tag: "w" });
        }"#;
    let mut o = options(&[("app.js", text)], json!({}));
    (o.duration_ms, o.at) = (1500, vec![1500]);
    let r = run(wasm, &o).unwrap();
    assert_eq!(r.end, None, "{r}");
    assert!(r.requests.iter().all(|q| q.refused.is_none()), "{r}");
    let screen = r.screens[0].1.join("\n");
    assert!(screen.starts_with("top"), "{r}");
    assert!(screen.contains("hello"), "{r}");
}

/// The example: the same run gives the same text.
#[test]
fn confetti_is_the_same_twice() {
    let Some(wasm) = interpreter() else { return };
    let dir = root().join("sdk/js/examples/confetti");
    let sources = script::sources(&dir).unwrap();
    let options = Options {
        size: (40, 12),
        args: script::init_args("app.js", &sources, json!({"rate": 40})),
        duration_ms: 2000,
        at: vec![500, 1000, 2000],
        ..Options::default()
    };
    let a = run(wasm, &options).unwrap();
    assert_eq!(a.end, None, "{a}");
    assert!(a.met.is_empty(), "{a}");
    let inked = |r: &Report| -> usize {
        r.screens[2]
            .1
            .iter()
            .map(|row| row.chars().filter(|c| *c != ' ').count())
            .sum()
    };
    assert!(inked(&a) > 10, "{a}");
    let b = run(wasm, &options).unwrap();
    assert_eq!(a.to_string(), b.to_string());
}

/// The speed of the tier, told on stderr: a 200x60 canvas, every cell
/// set by the script, each tick. `--ignored --nocapture`, in release.
#[test]
#[ignore = "a measure, not a test"]
fn speed() {
    let Some(wasm) = interpreter() else { return };
    let start = Instant::now();
    app("export function init() {}");
    eprintln!(
        "the first start, the module compiled: {:?}",
        start.elapsed()
    );
    let start = Instant::now();
    app("export function init() {}");
    eprintln!("a start after it: {:?}", start.elapsed());
    for (name, body) in [
        (
            "set, every cell",
            "for (let y = 0; y < 60; y++) for (let x = 0; x < 200; x++) c.set(x, y, 0x41 + ((x + y + n) % 26), S[(x + n) % 6]);",
        ),
        (
            "fill, 60 rows",
            "for (let y = 0; y < 60; y++) c.fill(0, y, 200, 1, 0x41 + ((y + n) % 26), S[(y + n) % 6]);",
        ),
        (
            "arithmetic, 1e5 steps",
            "let a = 0; for (let i = 0; i < 100000; i++) a = (a * 31 + i) % 65521; c.set(0, 0, 0x41 + (a % 26));",
        ),
    ] {
        let text = format!(
            "import {{ Canvas, period, style, rgb }} from 'ptyrc';
             const S = [1, 2, 3, 4, 5, 6].map((i) => style({{ fg: rgb(i * 40, 99, 255 - i * 40) }}));
             let c, n = 0;
             export function init() {{ c = new Canvas(200, 60); period(33); }}
             export function tick() {{ n++; {body} c.present(); }}"
        );
        let files = [("app.js", text.as_str())];
        let mut o = options(&files, json!({}));
        o.size = (200, 60);
        o.duration_ms = 3300;
        o.limits.late = Duration::from_secs(1);
        let start = Instant::now();
        let report = run(wasm, &o).unwrap();
        let spent = start.elapsed();
        eprintln!(
            "{name}: {:.2} ms a tick ({} ticks, the runner's part included), end {:?}, met {:?}",
            spent.as_secs_f64() * 1000.0 / report.ticks.max(1) as f64,
            report.ticks,
            report.end,
            report.met
        );
    }
}
