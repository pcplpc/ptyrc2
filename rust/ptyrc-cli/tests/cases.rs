// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev test --cases [NAME...] [--bless]`, and the surface of a
//! headless run (`[app] surface`, clipped as the driver clips it). A
//! script app: without the interpreter the test passes and says so,
//! unless `PTYRC_REQUIRE_JS=1`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
const JS: &str = "../../sdk/js/interp/target/ptyrc-js.wasm";

fn has_js() -> bool {
    let ok = Path::new(env!("CARGO_MANIFEST_DIR")).join(JS).is_file();
    if !ok {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_JS").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_JS=1: no interpreter"
        );
        let _ = writeln!(std::io::stderr(), "cases tests SKIPPED: no interpreter");
    }
    ok
}

/// The runner beside `ptyrc`, as built now.
fn build_runner() {
    let built = Command::new(env!("CARGO"))
        .args(["build", "-p", "ptyrc-box", "--bin", "ptyrc-box"])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(built.success(), "cargo build -p ptyrc-box");
}

fn ptyrc(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env_remove("PTYRC")
        .env_remove("PTYRC_JS")
        .env_remove("PTYRC_BOX")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn said(out: &Output) -> String {
    format!(
        "status {:?}\n--- stdout\n{}--- stderr\n{}",
        out.status.code(),
        text(out),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A tree: a sprite and a widget, on `surface`.
fn tree(tag: &str, surface: &str) -> PathBuf {
    let dir =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cases-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    let manifest = format!(
        "[app]\nid = \"probe\"\nversion = \"0.1.0\"\nsummary = \"A probe.\"\n\
         license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\nsurface = \"{surface}\"\n\
         [options]\nword = {{ type = \"string\", default = \"hi\", doc = \"a word\" }}\n"
    );
    std::fs::write(dir.join("ptyrc-app.toml"), manifest).unwrap();
    let js = r#"
import { call } from "ptyrc";
export function init(args) {
  call("draw.put", { anchor: { at: [0, 0] }, content: { rows: ["BARE " + args.word] } });
  call("widget.declare", { widgets: [{ tag: "w", name: "small", size: [4, 1],
                                      content: { rows: ["WIDG"] } }] });
  call("widget.show", { tag: "w" });
}
"#;
    std::fs::write(dir.join("app.js"), js).unwrap();
    dir
}

#[test]
fn cases_run_bless_and_differ() {
    if !has_js() {
        return;
    }
    build_runner();
    let dir = tree("run", "frame");
    let tests = dir.join("tests");
    std::fs::write(tests.join("a.args"), "# a case\n--size 20x8 --for 1500\n").unwrap();
    std::fs::write(
        tests.join("b.args"),
        "--size 20x8 --for 1500 # then the word\n-- --word yo\n",
    )
    .unwrap();
    let out = ptyrc(&dir, &["dev", "test", "--cases"]);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert!(
        text(&out).contains("FAIL tests/a: no tests/a.txt"),
        "{}",
        said(&out)
    );
    let out = ptyrc(&dir, &["dev", "test", ".", "--cases", "--bless"]);
    assert!(out.status.success(), "{}", said(&out));
    assert_eq!(text(&out), "wrote tests/a.txt\nwrote tests/b.txt\n");
    let a = std::fs::read_to_string(tests.join("a.txt")).unwrap();
    assert!(a.contains("BARE hi") && a.contains("WIDG"), "{a}");
    assert!(
        std::fs::read_to_string(tests.join("b.txt"))
            .unwrap()
            .contains("BARE yo")
    );
    let out = ptyrc(&dir, &["dev", "test", "--cases"]);
    assert_eq!(text(&out), "ok   tests/a\nok   tests/b\n", "{}", said(&out));
    std::fs::write(tests.join("b.txt"), a.replace("BARE hi", "BARE no")).unwrap();
    let out = ptyrc(&dir, &["dev", "test", "--cases", "b"]);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    let got = text(&out);
    assert!(
        got.starts_with("FAIL tests/b: the report differs from tests/b.txt")
            && got.contains("\n-BARE no\n+BARE yo\n"),
        "{got}"
    );
    let out = ptyrc(&dir, &["dev", "test", "--cases", "nope"]);
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    let out = ptyrc(&dir, &["dev", "test", "--cases", "--size", "9x9"]);
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
}

/// A case may end with a status other than 0 when its `.txt` says so:
/// `ended: exit: N`.
#[test]
fn a_case_ends_with_the_status_it_says() {
    if !has_js() {
        return;
    }
    build_runner();
    let dir = tree("exit", "frame");
    let js = r#"
import { exit, log } from "ptyrc";
export function init(args) {
  if (args.word === "bad") { log("bad word"); exit(2); }
}
"#;
    std::fs::write(dir.join("app.js"), js).unwrap();
    let tests = dir.join("tests");
    std::fs::write(
        tests.join("bad.args"),
        "--for 100 -- --word bad
",
    )
    .unwrap();
    std::fs::write(
        tests.join("good.args"),
        "--for 100
",
    )
    .unwrap();
    let out = ptyrc(&dir, &["dev", "test", "--cases", "--bless"]);
    assert!(out.status.success(), "{}", said(&out));
    let bad = std::fs::read_to_string(tests.join("bad.txt")).unwrap();
    assert!(bad.lines().any(|l| l == "ended: exit: 2"), "{bad}");
    let out = ptyrc(&dir, &["dev", "test", "--cases"]);
    assert_eq!(
        text(&out),
        "ok   tests/bad\nok   tests/good\n",
        "{}",
        said(&out)
    );
    // The good report for the bad run: a status 2 not said, a failure.
    let good = std::fs::read_to_string(tests.join("good.txt")).unwrap();
    std::fs::write(tests.join("bad.txt"), &good).unwrap();
    std::fs::write(tests.join("good.txt"), &bad).unwrap();
    let out = ptyrc(&dir, &["dev", "test", "--cases"]);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    let got = text(&out);
    assert!(
        got.contains("FAIL tests/bad") && got.contains("FAIL tests/good"),
        "{got}"
    );
}

/// `surface = "widget"`: a bare sprite shown nowhere, said; its widget
/// shown.
#[test]
fn a_widget_surface_shows_no_bare_sprite() {
    if !has_js() {
        return;
    }
    build_runner();
    let dir = tree("widget", "widget");
    let out = ptyrc(
        &dir,
        &["dev", "test", ".", "--size", "20x8", "--for", "1500"],
    );
    let got = text(&out);
    assert!(out.status.success(), "{}", said(&out));
    assert!(!got.contains("BARE") && got.contains("WIDG"), "{got}");
    assert!(got.contains("note: surface \"widget\": a sprite drawn at 0 ms is shown nowhere"));
}

/// `dev probe git DIR`: the answer of the driver's probe, a fixture
/// `dev test --probe git=FILE` gives back as written; `dev probe
/// system`: every part.
#[test]
fn a_probe_fixture_written_and_read() {
    let dir = tree("probe", "frame");
    let git = |args: &[&str]| {
        Command::new("git")
            .args([
                "-c",
                "user.name=a",
                "-c",
                "user.email=a@b",
                "-c",
                "init.defaultBranch=trunk",
            ])
            .args(args)
            .current_dir(&dir)
            .output()
    };
    if !git(&["init", "-q"]).is_ok_and(|o| o.status.success()) {
        let _ = writeln!(std::io::stderr(), "cases tests SKIPPED: no git");
        return;
    }
    assert!(git(&["add", "."]).unwrap().status.success());
    assert!(git(&["commit", "-qm", "first"]).unwrap().status.success());
    let out = ptyrc(
        &dir,
        &[
            "dev", "probe", "git", ".", "--parts", "head,log", "--log", "1",
        ],
    );
    assert!(out.status.success(), "{}", said(&out));
    let answer: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(answer["repo"]["head"]["branch"], "trunk", "{answer}");
    assert_eq!(answer["repo"]["log"][0]["subject"], "first", "{answer}");
    assert!(answer["stamp"].as_str().is_some_and(|s| !s.is_empty()));
    let out = ptyrc(&dir, &["dev", "probe", "system", "--top", "1"]);
    assert!(out.status.success(), "{}", said(&out));
    let answer: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        ["glance", "logins", "listeners", "disks"]
            .iter()
            .all(|p| answer.get(p).is_some())
    );
    let out = ptyrc(&dir, &["dev", "probe", "system", "--log", "1"]);
    assert_eq!(out.status.code(), Some(2), "{}", said(&out));
    if !has_js() {
        return;
    }
    build_runner();
    let fixture = ptyrc(&dir, &["dev", "probe", "git", "--parts", "head"]);
    std::fs::write(dir.join("git.json"), &fixture.stdout).unwrap();
    let manifest = std::fs::read_to_string(dir.join("ptyrc-app.toml")).unwrap();
    let manifest = format!("{manifest}[permissions]\nprobe-git = \"the branch\"\n");
    std::fs::write(dir.join("ptyrc-app.toml"), manifest).unwrap();
    let js = r#"
import { call } from "ptyrc";
export function init() {
  const g = call("probe.git", { parts: ["head"] });
  call("draw.put", { anchor: { at: [0, 0] }, content: { rows: ["on " + g.repo.head.branch] } });
}
"#;
    std::fs::write(dir.join("app.js"), js).unwrap();
    let out = ptyrc(
        &dir,
        &[
            "dev",
            "test",
            ".",
            "--size",
            "20x2",
            "--probe",
            "git=git.json",
        ],
    );
    assert!(text(&out).contains("on trunk"), "{}", said(&out));
}
