// SPDX-License-Identifier: GPL-3.0-or-later

use ptyrc_store::manifest::{self, Entry, OptionValue, Surface};
use ptyrc_store::{ErrorKind, Origin, Reference};

const GOOD: &str = r#"
[app]
id = "sand-garden"
version = "0.3.0"
summary = "Sand that piles on your text."
license = "GPL-3.0-or-later"
api = 1
script = "app.js"
assets = "assets/"
surface = "frame"
tape = "demo.tape"

[permissions]
shape = "the cats walk on the lines"
words = { list = ["food", "fish", "toy"], why = "they go to them" }

[limits]
memory-mib = 128

[options]
rate = { type = "int", default = 20, doc = "grains a second" }
wind = { type = "float", default = 0.5, doc = "to the right" }
"#;

#[test]
fn good() {
    let m = manifest::parse(GOOD).unwrap();
    assert_eq!(m.app.id, "sand-garden");
    assert_eq!(m.app.entry, Entry::Script("app.js".into()));
    assert_eq!(m.app.assets.as_deref(), Some("assets"));
    assert_eq!(m.app.surface, Surface::Frame);
    assert_eq!(m.permissions.len(), 2);
    assert_eq!(m.permissions[1].name, "words");
    assert_eq!(m.permissions[1].words, ["food", "fish", "toy"]);
    assert_eq!(m.limits[0].name, "memory-mib");
    assert_eq!(m.limits[0].value, 128);
    assert_eq!(m.options[0].name, "rate");
    assert_eq!(m.options[0].default, Some(OptionValue::Int(20)));
    assert_eq!(m.options[1].default, Some(OptionValue::Float(0.5)));
}

#[test]
fn minimal_and_rust() {
    let m = manifest::parse(
        "[app]\nid = \"a\"\nversion = \"1\"\nsummary = \"s\"\nlicense = \"GPL-3.0-or-later\"\napi = 1\nrust = \"src/lib.rs\"\nrustc = \"1.85\"\n",
    )
    .unwrap();
    assert_eq!(m.app.entry, Entry::Rust("src/lib.rs".into()));
    assert_eq!(m.app.surface, Surface::Widget);
    assert!(!m.app.wait);
    assert!(m.permissions.is_empty());
}

/// Replaces `from` by `to` in GOOD and expects an error naming `line`
/// and holding `msg`.
fn bad(from: &str, to: &str, line: usize, msg: &str) {
    assert!(GOOD.contains(from), "{from}");
    let src = GOOD.replacen(from, to, 1);
    let e = manifest::parse(&src).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Manifest);
    assert!(
        e.message.starts_with(&format!("ptyrc-app.toml:{line}:")),
        "{e}"
    );
    assert!(e.message.contains(msg), "{e}");
    assert!(e.hint.is_some(), "{e}");
}

#[test]
fn bad_app() {
    bad("GPL-3.0-or-later", "MIT", 6, "license");
    bad("api = 1", "api = 2", 7, "api");
    bad("api = 1", "api = \"1\"", 7, "api");
    bad("id = \"sand-garden\"", "id = \"Sand Garden\"", 3, "id");
    bad("script = \"app.js\"", "script = \"../app.js\"", 8, "script");
    bad("script = \"app.js\"", "script = \"/app.js\"", 8, "script");
    bad("script = \"app.js\"", "script = \"app.py\"", 8, "script");
    bad(
        "script = \"app.js\"",
        "rust = \"a.rs\"\nscript = \"app.js\"",
        8,
        "both",
    );
    bad("script = \"app.js\"", "", 1, "missing");
    bad(
        "surface = \"frame\"",
        "surface = \"everywhere\"",
        10,
        "surface",
    );
    bad("surface = \"frame\"", "rustc = \"1.85\"", 10, "rustc");
    bad("surface = \"frame\"", "wait = \"yes\"", 10, "wait");
    let waits = GOOD.replacen("surface = \"frame\"", "wait = true", 1);
    assert!(manifest::parse(&waits).unwrap().app.wait);
    bad("surface = \"frame\"", "random = [\"-a\"]", 10, "random");
    bad("surface = \"frame\"", "random = [[1]]", 10, "random");
    bad(
        "surface = \"frame\"",
        "random = [[\"a\\u0007\"]]",
        10,
        "random",
    );
    let random = |line: &str| {
        let src = GOOD.replacen("surface = \"frame\"", line, 1);
        manifest::parse(&src).unwrap().app.random
    };
    assert_eq!(
        random("random = [[], [\"-a\"]]"),
        [vec![], vec!["-a".to_string()]]
    );
    assert!(random("random = []").is_empty());
    assert_eq!(random(""), [Vec::<String>::new()]);
    bad("tape = \"demo.tape\"", "colour = \"red\"", 11, "unknown");
    bad("[limits]", "[extra]", 17, "unknown");
    bad(
        "version = \"0.3.0\"",
        "version = \"0.3 beta\"",
        4,
        "version",
    );
}

#[test]
fn bad_permissions() {
    bad(
        "shape = \"the cats",
        "network = \"the cats",
        14,
        "not a permission",
    );
    bad(
        "shape = \"the cats walk on the lines\"",
        "shape = 1",
        14,
        "shape",
    );
    bad(
        "shape = \"the cats walk on the lines\"",
        "shape = \"\"",
        14,
        "reason",
    );
    bad(
        "shape = \"the cats",
        "limit-memory = \"the cats",
        14,
        "not a permission",
    );
    bad(
        "[\"food\", \"fish\", \"toy\"]",
        "[\"fo\"]",
        15,
        "not a word",
    );
    bad(
        "[\"food\", \"fish\", \"toy\"]",
        "[\"two words\"]",
        15,
        "not a word",
    );
    bad(
        "[\"food\", \"fish\", \"toy\"]",
        "[\"a.*\"]",
        15,
        "not a word",
    );
    bad(
        "[\"food\", \"fish\", \"toy\"]",
        "[\"Food\", \"food\"]",
        15,
        "twice",
    );
    bad("[\"food\", \"fish\", \"toy\"]", "[]", 15, "0 words");
    bad("words = { list", "words = { lost", 15, "unknown");
    bad(", why = \"they go to them\" }", " }", 15, "why");
    let many: Vec<String> = (0..33).map(|i| format!("\"word{i}\"")).collect();
    bad(
        "[\"food\", \"fish\", \"toy\"]",
        &format!("[{}]", many.join(", ")),
        15,
        "33 words",
    );
    let long = format!("[\"{}\"]", "a".repeat(33));
    bad("[\"food\", \"fish\", \"toy\"]", &long, 15, "not a word");
}

#[test]
fn bad_limits_options() {
    bad(
        "memory-mib = 128",
        "memory-mib = 32",
        18,
        "above the default",
    );
    bad("memory-mib = 128", "disk-mib = 128", 18, "unknown");
    // Past the runner's ceiling: named.
    bad(
        "memory-mib = 128",
        "memory-mib = 4096",
        18,
        "above the ceiling 512",
    );
    assert!(manifest::parse(&GOOD.replace("memory-mib = 128", "memory-mib = 512")).is_ok());
    // A limit is asked through [limits], never [permissions].
    bad(
        "shape = \"the cats",
        "limit-memory = \"more\"\nshape = \"the cats",
        14,
        "not a permission",
    );
    assert_eq!(manifest::limit_permission("log-mib"), "limit-memory");
    assert_eq!(manifest::limit_permission("tick-ms"), "limit-time");
    bad(
        "rate = { type = \"int\", default = 20",
        "rate = { type = \"int\", default = \"20\"",
        21,
        "type int",
    );
    bad("type = \"int\"", "type = \"list\"", 21, "type");
    bad("rate = {", "front = {", 21, "ptyrc's own");
    bad(", doc = \"grains a second\"", "", 21, "doc");
}

#[test]
fn syntax_and_size() {
    let e = manifest::parse("[app]\nid = \n").unwrap_err();
    assert!(e.message.starts_with("ptyrc-app.toml:2:"), "{e}");
    let big = vec![b'#'; 65 << 10];
    assert!(manifest::parse_bytes(&big).is_err());
    assert!(manifest::parse_bytes(b"\xff").is_err());
    assert!(manifest::parse("summary = 1\n").is_err());
}

#[test]
fn origins() {
    let o = Origin::parse("example.org/someone/sand-garden").unwrap();
    assert_eq!(o.id(), "example.org/someone/sand-garden");
    for same in [
        "https://example.org/someone/sand-garden",
        "https://Example.ORG/someone/sand-garden.git",
        "https://example.org/someone/sand-garden/",
    ] {
        assert_eq!(Origin::parse(same).unwrap(), o, "{same}");
    }
    let (o, r) = Origin::parse_spec("example.org/a/b#apps/sand@v1.0").unwrap();
    assert_eq!(o.id(), "example.org/a/b#apps/sand");
    assert_eq!(o.sub(), Some("apps/sand"));
    assert_eq!(r, Some(Reference::Name("v1.0".into())));
    assert_eq!(o.dir_name(), "example.org%2Fa%2Fb%23apps%2Fsand");
    assert_eq!(
        o.registry_path().unwrap(),
        "apps/example.org/a/b/apps/sand.toml"
    );
    let c = "3f9c".repeat(10);
    let (_, r) = Origin::parse_spec(&format!("example.org/a/b@{}", c.to_uppercase())).unwrap();
    assert_eq!(r, Some(Reference::Commit(c)));
    let (_, r) = Origin::parse_spec("example.org/a/b@HEAD").unwrap();
    assert_eq!(r, Some(Reference::Head));
    let o = Origin::parse("https://gitlab.example.org:8443/group/sub/repo").unwrap();
    assert_eq!(o.id(), "gitlab.example.org:8443/group/sub/repo");
    let l = Origin::parse("local:/srv/apps/sand#x").unwrap();
    assert_eq!(l.id(), "local:/srv/apps/sand#x");
    assert!(l.registry_path().is_none());
}

#[test]
fn hostile_origins() {
    for s in [
        "ext::sh -c touch% /tmp/pwned",
        "ext::sh",
        "fd::17",
        "file:///etc/passwd",
        "http://example.org/a/b",
        "ssh://git@example.org/a/b",
        "git@example.org:a/b",
        "git://example.org/a/b",
        "https://user@example.org/a/b",
        "https://example.org/a/b?x=1",
        "https://example.org/a%2Fb/c",
        "https://-oProxyCommand=x/a/b",
        "-uhttps://example.org/a/b",
        "--upload-pack=touch/a/b",
        "example.org/a",
        "example.org/../b",
        "example.org/a/.git",
        "example.org/a/-b",
        "example.org/a/b#../c",
        "example.org/a/b#.git/hooks",
        "example.org/a/b c",
        "example.org/a/b\n",
        "local:relative/dir",
        "local:/a/../b",
        "local:/a:b",
    ] {
        let e = Origin::parse_spec(s).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Origin, "{s}");
    }
    let long = format!("example.org/a/{}", "b".repeat(100));
    assert!(Origin::parse(&format!("{long}#{}", "c/".repeat(60))).is_err());
}

#[test]
fn hostile_refs() {
    for r in [
        "-v",
        "--upload-pack=touch",
        "--exec=x",
        "a..b",
        "a b",
        "a:b",
        "a~1",
        "a^",
        "@{u}",
        ".hidden",
        "a/.b",
        "a.lock",
        "a//b",
        "",
        "a*",
        "a\x1b[2J",
    ] {
        assert!(Reference::parse(r).is_err(), "{r:?}");
    }
    assert!(Reference::parse("feature/x-1.2").is_ok());
}
