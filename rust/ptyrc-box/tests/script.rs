// SPDX-License-Identifier: GPL-3.0-or-later
//! Script apps through the interpreter guest (`sdk/js`).
//!
//! The interpreter is built apart:
//!
//! ```text
//! sdk/js/interp/build.sh
//! ```
//!
//! Where it is not built, each test passes and tests nothing; the
//! first says so on the terminal (not captured). `PTYRC_REQUIRE_JS=1`:
//! they fail instead.
//!
//! Every test runs under the OS profile (`ptyrc_box::profile`), applied
//! once the files are read (`interpreter`).

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

/// The sources of `sdk/js/examples/confetti`, read before the profile.
fn confetti() -> &'static BTreeMap<String, String> {
    static SOURCES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    SOURCES.get_or_init(|| script::sources(&root().join("sdk/js/examples/confetti")).unwrap())
}

/// The interpreter; read, with every other file, before the OS profile
/// is applied.
fn interpreter() -> Option<&'static [u8]> {
    static WASM: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    WASM.get_or_init(|| {
        let wasm = read_interpreter();
        confetti();
        let status = ptyrc_box::profile::apply();
        let _ = writeln!(std::io::stderr(), "{}", status.lines().join("\n"));
        if cfg!(target_os = "linux") {
            assert!(!status.applied.is_empty(), "{status:?}");
        }
        wasm
    })
    .as_deref()
}

fn read_interpreter() -> Option<Vec<u8>> {
    let path = std::env::var_os(script::ENV_JS).map_or_else(
        || root().join("sdk/js/interp/target/ptyrc-js.wasm"),
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
             sdk/js/interp/build.sh",
            path.display()
        );
    }
    wasm
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

/// `manifest()`: what the runner was told of the manifest, granted;
/// `null` without one. Not among the arguments.
#[test]
fn manifest_is_the_runners() {
    let Some(wasm) = interpreter() else { return };
    let files = [(
        "app.js",
        "import { log, manifest } from 'ptyrc';
         export function init(args) {
           const m = manifest();
           log('args', Object.keys(args).join());
           log('manifest', m === null ? 'null' : m.id + ' ' + m.granted.join() + ' ' + m.limits['tick-ms']);
           m && (m.id = 'changed');
           log('again', manifest()?.id);
         }",
    )];
    let given = ptyrc_box::manifest::Given::parse(
        r#"{"id": "a.b", "version": "1.0.0",
            "permissions": [{"name": "screen.shape"}, {"name": "limit-time"}],
            "granted": ["screen.shape", "limit-time"], "limits": {"tick-ms": 80}}"#,
    )
    .unwrap();
    let granted = ["screen.shape".to_owned(), "limit-time".to_owned()];
    let mut o = options(&files, json!({"x": 1}));
    o.args = ptyrc_box::manifest::with_member(o.args, given.member(&granted));
    let r = run(wasm, &o).unwrap();
    assert!(logged(&r, "args x"), "{r}");
    assert!(logged(&r, "manifest a.b screen.shape,limit-time 80"), "{r}");
    assert!(logged(&r, "again a.b"), "{r}");
    let r = run(wasm, &options(&files, json!({}))).unwrap();
    assert!(logged(&r, "manifest null"), "{r}");
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
                "app.js:2:3: ReferenceError: setTimeout is not defined",
                "    at init (app.js:2:3)",
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
                "hint: \"ptyrc\", \"ptyrc/deck\", \"ptyrc/canvas\", \"ptyrc/sight\", or a file",
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
    // A queue of jobs without end is a loop like another.
    for text in [
        "export function init() { const f = () => { Promise.resolve().then(f); }; f(); }",
        "export function init() { const f = () => queueMicrotask(f); f(); }",
        "export async function init() { for (;;) await null; }",
    ] {
        let r = quick(&[("app.js", text)]).unwrap().unwrap();
        assert_eq!(r.end, Some(Error::Time("init")), "{text}\n{r}");
    }
    let r = quick(&[(
        "app.js",
        "import { period } from 'ptyrc';
         export function init() { period(50); }
         export async function tick() { for (;;) await null; }",
    )])
    .unwrap()
    .unwrap();
    assert_eq!(r.end, Some(Error::Time("tick")), "{r}");
    // A chain of promises, each resolved with the next, holds them all:
    // the memory or the time ends it.
    let r = quick(&[(
        "app.js",
        "export function init() { const f = () => Promise.resolve().then(f); f(); }",
    )])
    .unwrap()
    .unwrap();
    assert!(
        r.end == Some(Error::Time("init")) || r.met.iter().any(|m| m == "memory"),
        "{r}"
    );
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

/// Recursion of the script, through natives too: a `RangeError` the
/// script catches, before the runner's stack runs out.
#[test]
fn recursion_is_a_range_error() {
    let text = r#"
        import { log } from "ptyrc";
        let depth = 0;
        function f(n) { depth = n; return f(n + 1) + 1; }
        function g(n) { depth = n; return [1].map(() => g(n + 1))[0]; }
        function h(n) { depth = n; return JSON.parse("1", () => h(n + 1)); }
        function s(n) { depth = n; return String({ toString: () => s(n + 1) }); }
        function r(n) { depth = n; return "a".replace(/a/, () => r(n + 1)); }
        function p(n) { depth = n; return new Proxy({}, { get: () => p(n + 1).x }).x; }
        function q(n) { depth = n; return [2, 1].sort(() => q(n + 1)); }
        function e(n) { depth = n; return eval("e(n + 1)"); }
        const o = { get x() { depth++; return this.x; } };
        export function init() {
          for (const [name, run] of Object.entries({ f, g, h, s, r, p, q, e, o: () => o.x })) {
            depth = 0;
            try { run(0); } catch (err) { log(name, err.name, depth); }
          }
        }"#;
    let Some(r) = app(text) else { return };
    assert_eq!(r.end, None, "{r}");
    for name in ["f", "g", "h", "s", "r", "p", "q", "e", "o"] {
        let line = (r.log.iter())
            .find(|l| l.split_whitespace().nth(1) == Some(name))
            .unwrap_or_else(|| panic!("{name}: {r}"));
        assert!(line.contains("RangeError"), "{line}");
    }
    let f = (r.log.iter()).find(|l| l.split_whitespace().nth(1) == Some("f"));
    let depth = f.and_then(|l| l.split_whitespace().last()?.parse::<u32>().ok());
    assert!(depth.unwrap_or(0) > 900, "{f:?}");
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
    let sources = confetti();
    let options = Options {
        size: (40, 12),
        args: script::init_args("app.js", sources, json!({"rate": 40})),
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

/// `script::unit` on `files`, the first the file of the tests.
fn unit(files: &[(&str, &str)], limits: Limits) -> Option<(Vec<String>, u8)> {
    let wasm = interpreter()?;
    let o = options(files, json!({}));
    Some(script::unit(wasm, &o.args, limits))
}

#[test]
fn unit_tests_each_export_in_order_the_place_of_a_failure() {
    let tests = r#"
        import { assert, assertEqual, assertThrows } from "ptyrc";
        import { add } from "./lib.js";
        export function test_b_fails() { assertEqual(add(1, 2), 4, "sum"); }
        export function test_a_adds() { assertEqual(add(1, 2), 3); assert(true); }
        export function test_c_throws() { assertThrows(() => null.x, TypeError); }
        export async function test_d_async() { assertEqual(await Promise.resolve([1, {a: 2}]), [1, {a: 2}]); }
        export function helper() { throw new Error("not a test"); }
        export const test_e = 3;
    "#;
    let lib = "export function add(a, b) { return a + b; }";
    let Some((lines, status)) = unit(&[("tests.js", tests), ("lib.js", lib)], Limits::default())
    else {
        return;
    };
    assert_eq!(status, 1, "{lines:#?}");
    assert_eq!(
        lines,
        [
            "ok   test_a_adds",
            "FAIL test_b_fails",
            "tests.js:4:54: AssertionError: sum: 3, expected 4",
            "    at test_b_fails (tests.js:4:54)",
            "ok   test_c_throws",
            "ok   test_d_async",
            "3 passed, 1 failed",
        ]
    );
    let Some((lines, status)) = unit(
        &[("tests.js", "export function test_x() {}")],
        Limits::default(),
    ) else {
        return;
    };
    assert_eq!(
        (lines, status),
        (vec!["ok   test_x".into(), "1 passed, 0 failed".into()], 0)
    );
}

#[test]
fn unit_tests_under_the_limits_of_an_event() {
    let limits = Limits {
        hard: Duration::from_millis(300),
        ..Limits::default()
    };
    let tests = "export function test_a() {}\nexport function test_b() { for (;;) {} }\nexport function test_c() {}";
    let Some((lines, status)) = unit(&[("tests.js", tests)], limits.clone()) else {
        return;
    };
    assert_eq!(status, 1);
    assert_eq!(lines[..2], ["ok   test_a", "FAIL test_b"], "{lines:#?}");
    assert!(lines[2].starts_with("time:"), "{lines:#?}");
    assert_eq!(lines[3], "1 passed, 1 failed, the rest not run");
    // No test; a test file that does not load.
    for (text, said) in [
        ("export function check() {}", "no export test_*"),
        ("export function test_a() {", "tests.js:1:27: "),
    ] {
        let (lines, status) = unit(&[("tests.js", text)], limits.clone()).unwrap();
        assert_eq!(status, 1);
        assert!(lines.iter().any(|l| l.contains(said)), "{lines:#?}");
    }
    // A request: no session.
    let text =
        r#"import { call } from "ptyrc"; export function test_a() { call("session.info"); }"#;
    let (lines, _) = unit(&[("tests.js", text)], limits).unwrap();
    assert!(
        lines[1].contains("PtyrcError: session.info: no session"),
        "{lines:#?}"
    );
}

#[test]
fn text_bytes_and_base64() {
    let tests = r##"
        import { assertEqual, cells, Canvas } from "ptyrc";
        const enc = new TextEncoder(), dec = new TextDecoder();
        export function test_utf8() {
          const b = enc.encode("aé€😀");
          assertEqual(b, new Uint8Array([97, 0xc3, 0xa9, 0xe2, 0x82, 0xac, 0xf0, 0x9f, 0x98, 0x80]));
          assertEqual(dec.decode(b), "aé€😀");
          assertEqual(enc.encode("\ud800"), new Uint8Array([0xef, 0xbf, 0xbd]));
          assertEqual(dec.decode(new Uint8Array([0xff, 0x41, 0xe2, 0x82, 0xc0, 0xed, 0xa0, 0x80])),
                      "�A�����");
        }
        export function test_base64() {
          const b = Uint8Array.fromBase64("G1sxOzJB");
          assertEqual(dec.decode(b), "\x1b[1;2A");
          assertEqual(b.toBase64(), "G1sxOzJB");
          assertEqual(btoa("hi"), "aGk=");
          assertEqual(atob("aGk="), "hi");
        }
        export function test_cells_and_runs() {
          assertEqual(cells("a日́"), 3);
          const c = new Canvas(5, 2);
          c.text(0, 0, "ab", { fg: 9, bold: true });
          c.set(3, 0, "日", { bg: "#102030" });
          c.set(0, 1, "x", { fg: "default" });
          assertEqual(c.runs(), [
            [{ t: "ab", fg: 9, bold: true }, { t: " " }, { t: "日", bg: "#102030" }],
            [{ t: "x", fg: "default" }, { t: "    " }],
          ]);
        }
    "##;
    let Some((lines, status)) = unit(&[("tests.js", tests)], Limits::default()) else {
        return;
    };
    assert_eq!(status, 0, "{lines:#?}");
}

/// A deck: two variants, a canvas, headless: the variant that fits.
const DECK: &str = r#"
    import { Canvas, period } from "ptyrc";
    import { Deck } from "ptyrc/deck";
    let deck, big;
    export function init(args) {
      deck = new Deck({ border: "ascii", every: args.every });
      big = new Canvas(6, 2);
      deck.add("w", { big: [6, 2], small: [3, 1] }, (w, h) => {
        if (h === 1) return `${deck.now / 1000}s`;
        big.clear().text(0, 0, "BIG", { fg: 9 });
        big.text(0, 1, `${deck.now}`);
        return big;
      }, { key: () => {} });
      period(1000);
    }
    export function tick(now) { deck.tick(now); }
    export function event(topic, data) { deck.event(topic, data); }
"#;

fn deck(size: (u32, u32), args: Value) -> Option<Report> {
    let wasm = interpreter()?;
    let mut o = options(&[("app.js", DECK)], args);
    (o.size, o.duration_ms, o.at) = (size, 2000, vec![2000]);
    Some(run(wasm, &o).unwrap())
}

#[test]
fn a_deck_draws_the_variant_that_fits() {
    let Some(r) = deck((40, 8), json!({})) else {
        return;
    };
    assert_eq!(r.end, None, "{r}");
    let screen = r.screens[0].1.join("\n");
    assert!(
        screen.contains("|BIG   |") && screen.contains("|2000  |"),
        "{r}"
    );
    let Some(r) = deck((12, 6), json!({})) else {
        return;
    };
    assert!(r.screens[0].1.join("\n").contains("|2s |"), "{r}");
    // Until headless serves them (PROTOCOL.md, Headless): asked one by one.
    for q in &r.requests {
        if let Some(code) = &q.refused {
            assert!(
                code == "headless" && ["batch", "widget.ask"].contains(&q.method.as_str()),
                "{r}"
            );
        }
    }
}

/// Waits on headless `batch` (gap 6, `headless.rs`): then no refusal.
#[test]
fn a_deck_sends_one_batch_a_tick() {
    let Some(r) = deck((40, 8), json!({"every": true})) else {
        return;
    };
    assert!(
        r.requests
            .iter()
            .all(|q| q.method != "batch" || q.refused.is_none()),
        "{r}"
    );
}

#[test]
fn deck_pieces() {
    let tests = r#"
        import { assertEqual } from "ptyrc";
        import { Field, Listing, Keys, cut, fit, wrap } from "ptyrc/deck";
        export function test_field() {
          const f = new Field("ab");
          for (const k of ["left", "x", "end", "space", "日"]) f.key(k);
          assertEqual([f.text, f.cursor], ["axb 日", 5]);
          f.key("ctrl+w"); assertEqual(f.text, "axb ");
          f.key("ctrl+a"); f.key("delete"); assertEqual(f.text, "xb ");
          assertEqual(f.key("f1"), false);
          assertEqual(f.render(4, true), [{ t: "x", inverse: true }, { t: "b " }, { t: " " }]);
          assertEqual(new Field("").render(3, false, "name"), [{ t: "nam", dim: true }]);
        }
        export function test_listing() {
          const l = new Listing(10);
          assertEqual(l.window(3), [0, 1, 2]);
          l.key("pagedown"); l.key("j");
          assertEqual([l.sel, l.window(3), l.mark(3)], [4, [2, 3, 4], "5/10"]);
          l.key("end"); assertEqual(l.window(3), [7, 8, 9]);
        }
        export function test_keys() {
          const k = new Keys();
          assertEqual(k.feed({ key: "a", bytes: "YQ==" }), { key: "a" });
          assertEqual(k.feed({ bytes: btoa("\x1b[1;5D") }), { key: "ctrl+left" });
          assertEqual(k.feed({ bytes: btoa("\x1b[3;2~") }), { key: "shift+delete" });
          assertEqual(k.feed({ bytes: btoa("\x1b[200~") }), null);
          assertEqual(k.feed({ bytes: new Uint8Array([0xc3, 0xa9, 13]).toBase64() }), null);
          assertEqual(k.feed({ bytes: btoa("\x1b[201~") }), { paste: "é\n" });
          assertEqual(k.feed({ paste: "a\r\nb" }), { paste: "a\nb" });
        }
        export function test_cells() {
          assertEqual(cut("日本語", 4), "日…");
          assertEqual(fit([{ t: "ab", fg: 1 }, { t: "cd" }], 3), [{ t: "ab", fg: 1 }, { t: "c" }]);
          assertEqual(fit("a", 3), [{ t: "a" }, { t: "  " }]);
          assertEqual(wrap("one two three four", 9, 2), ["one two", "three fo…"]);
        }
    "#;
    let Some((lines, status)) = unit(&[("tests.js", tests)], Limits::default()) else {
        return;
    };
    assert_eq!(status, 0, "{lines:#?}");
}

/// Clock, storage, result, files, assets from memory, as the driver
/// answers them; nothing of the machine read.
#[test]
fn host_functions_from_memory() {
    let Some(wasm) = interpreter() else { return };
    let text = r#"
        import { call } from "ptyrc";
        const ask = (m, p, k) => {
          try { const r = call(m, p || {}); return k ? r[k] : r; } catch (e) { return e.code; }
        };
        export function init(args) {
          const out = [];
          out.push(ask("clock.now", {}, "unix_ms"));
          out.push(ask("storage.set", { key: "k", value: "v1" }, "used"));
          out.push(ask("storage.get", { key: "seed" }, "value"));
          out.push(ask("file.read", { file: args.save.file }, "text"));
          out.push(ask("file.write", { file: args.save.file, text: "done" }, "size"));
          out.push(ask("asset.read", { name: "a/b.txt" }, "text"));
          out.push(ask("asset.read", { name: "../ptyrc-app.toml" }));
          const b = ask("batch", { calls: [{ method: "clock.now" }, { method: "app.result",
                                             params: { value: "r", status: 3 } }] });
          out.push((b.results[0].error || { code: "ok" }).code, JSON.stringify(b.results[1].result));
          call("draw.put", { anchor: { at: [0, 0] }, content: { rows: [out.join(" ")] } });
        }"#;
    let mut o = options(&[("app.js", text)], json!({}));
    o.size = (80, 4);
    o.host.wall0 = 1_000_000;
    o.host.storage.insert("seed".into(), "s0".into());
    o.host.files.push(ptyrc_box::hostfn::Named {
        option: "save".into(),
        name: "TODO.md".into(),
        entries: None,
        content: ptyrc_box::hostfn::Content {
            bytes: Some(b"todo".to_vec()),
            mtime_ms: 1,
        },
        written: false,
        write: true,
    });
    let mut assets = ptyrc_box::assets::Assets::default();
    assets.files.insert("a/b.txt".into(), b"asset".to_vec());
    o.assets = Some(assets);
    let r = run(wasm, &o).unwrap();
    assert_eq!(r.end, None, "{r}");
    assert_eq!(
        r.screens[0].1[0], "1000000 4 s0 todo 4 asset bad-params bad-request {}",
        "{r}"
    );
    assert_eq!(r.result, Some((json!("r"), 3)));
    assert_eq!(r.storage.get("k").map(String::as_str), Some("v1"));
    assert_eq!(r.written.len(), 1);
    let text = r.to_string();
    assert!(text.contains("result: r (status 3)"), "{text}");
    assert!(
        text.contains("storage: k = \"v1\"\nstorage: seed = \"s0\" (given, not written)\n"),
        "{text}"
    );
    assert!(text.contains("files: save written, 4 bytes"), "{text}");
    let j = r.to_json();
    assert_eq!(j["files"][0]["text"], "done");
    assert_eq!(j["storage"]["k"], "v1");
    // Not held: the clock lowered, storage and writes denied.
    o.grants = Some(ptyrc_box::grants::Grants::new(
        &[] as &[&str],
        &[],
        &[] as &[&str],
    ));
    let r = run(wasm, &o).unwrap();
    assert_eq!(
        r.screens[0].1[0], "0 denied denied todo denied asset bad-params ok {}",
        "{r}"
    );
    assert_eq!(
        r.storage_lines(),
        ["storage: not held: the storage given (--storage, 1 key) not used"]
    );
    assert!(!r.to_string().contains("s0"), "{r}");
    assert_eq!(r.to_json()["storage_held"], false);
}

/// The scene steps on its own clock, as the driver's: the screen at T
/// is the same whatever other times are asked.
#[test]
fn the_screen_at_t_whatever_else_is_asked() {
    let Some(wasm) = interpreter() else { return };
    let text = r#"
        import { call } from "ptyrc";
        export function init(args) {
          call("draw.put", { effect: { name: "sand", params: { rate: 200, seed: 7 } } });
        }"#;
    let mut o = options(&[("app.js", text)], json!({}));
    o.duration_ms = 1500;
    o.at = vec![1500];
    let alone = run(wasm, &o).unwrap();
    o.at = vec![100, 250, 700, 1100, 1500];
    let many = run(wasm, &o).unwrap();
    assert_eq!(alone.screens[0], many.screens[4], "{alone}");
    assert!(alone.screens[0].1.iter().any(|r| !r.trim().is_empty()));
}

/// Topic `idle`, as the driver tells it: a blank screen idle from the
/// start; a screen written, once still long enough.
#[test]
fn idle_told() {
    let Some(wasm) = interpreter() else { return };
    let text = r#"
        import { call, log } from "ptyrc";
        export function init(args) { call("subscribe", { topics: ["idle"] }); }
        export function event(topic, data) { log(`${topic} ${data.idle} ${data.frame}`); }"#;
    let mut o = options(&[("app.js", text)], json!({}));
    o.duration_ms = 12_000;
    // `(ms, idle)` of each event, from the log: `MS idle IDLE FRAME`.
    let told = |r: &Report| -> Vec<(u64, bool)> {
        (r.log.iter())
            .filter_map(|l| {
                let w: Vec<&str> = l.split_whitespace().collect();
                (w.get(1) == Some(&"idle")).then(|| (w[0].parse().unwrap(), w[2] == "true"))
            })
            .collect()
    };
    let blank = run(wasm, &o).unwrap();
    assert_eq!(told(&blank), [(0, true), (0, true)], "{blank:?}");
    // A pane full of text, written again at 1 s: idle 10 s later.
    let full = |w: &str| format!("{}\n", w.repeat(4)).repeat(9);
    o.under = ptyrc_box::headless::Under::Screens(vec![
        (0, full("busy pane ")),
        (1000, full("pane busy ")),
    ]);
    let written = run(wasm, &o).unwrap();
    let t = told(&written);
    assert!(
        !t.is_empty()
            && t.iter()
                .all(|&(ms, idle)| idle && (10_000..=11_500).contains(&ms)),
        "{t:?}"
    );
}
