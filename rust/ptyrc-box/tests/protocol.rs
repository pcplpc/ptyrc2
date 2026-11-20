// SPDX-License-Identifier: GPL-3.0-or-later
//! `sdk/template/PROTOCOL.md`, the page of a generated app, held to the
//! tables of the driver (`ptyrc-server/src/grants.rs`), of the manifest
//! (`ptyrc-store/src/manifest.rs`) and of this crate.

use ptyrc_box::grants::{self, Gate};
use ptyrc_box::headless::SERVED;
use serde_json::json;
use std::path::Path;

fn read(rel: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// `(name, gate)` of the rows of `const NAME: &[(&str, Gate)] = &[...]`.
fn table(src: &str, name: &str) -> Vec<(String, String)> {
    let start = src.find(&format!("const {name}: &[(&str, Gate)]")).unwrap();
    let body = &src[start..];
    let body = &body[..body.find("\n];").unwrap()];
    body.lines()
        .filter_map(|l| {
            let l = l.trim().strip_prefix("(\"")?;
            let (name, rest) = l.split_once("\", ")?;
            Some((name.to_owned(), rest.trim_end_matches("),").to_owned()))
        })
        .collect()
}

/// The section of `doc` under `## title`.
fn section<'a>(doc: &'a str, title: &str) -> &'a str {
    let head = format!("\n## {title}\n");
    let start = doc.find(&head).unwrap_or_else(|| panic!("## {title}")) + head.len();
    let rest = &doc[start..];
    &rest[..rest.find("\n## ").unwrap_or(rest.len())]
}

fn row(section: &str, name: &str) -> bool {
    section.contains(&format!("\n| `{name}` |"))
}

/// `` `name` `` anywhere, not followed by more of a name.
fn named(doc: &str, name: &str) -> bool {
    let tick = format!("`{name}");
    doc.match_indices(&tick).any(|(i, _)| {
        let next = doc[i + tick.len()..].chars().next();
        !next.is_some_and(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
    })
}

fn kind(gate: &str) -> &str {
    gate.split('(').next().unwrap()
}

#[test]
fn every_method_open_to_apps_and_none_closed() {
    let doc = read("sdk/template/PROTOCOL.md");
    let methods = section(&doc, "Methods");
    let server = read("rust/ptyrc-server/src/grants.rs");
    let rows = table(&server, "METHODS");
    assert!(rows.len() > 50, "{rows:?}");
    let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(names, grants::METHODS, "ptyrc-box grants::METHODS");
    for (name, gate) in &rows {
        // The gate of this crate, as the driver's, without params.
        let ours = grants::gate(name, &json!({}));
        let theirs = match kind(gate) {
            "Never" => Gate::Never,
            "Open" => Gate::Open,
            _ => ours,
        };
        assert_eq!(ours, theirs, "{name}: {gate}");
        match gate.as_str() {
            "Never" => assert!(!named(&doc, name), "{name}: closed, yet in PROTOCOL.md"),
            _ => assert!(row(methods, name), "{name}: no row in PROTOCOL.md, Methods"),
        }
    }
    for m in SERVED {
        assert!(
            named(section(&doc, "Headless (`ptyrc dev test`)"), m),
            "{m}"
        );
    }
}

#[test]
fn every_topic_and_permission() {
    let doc = read("sdk/template/PROTOCOL.md");
    let topics = section(&doc, "Topics");
    let server = read("rust/ptyrc-server/src/grants.rs");
    for (name, gate) in table(&server, "TOPICS") {
        match gate.as_str() {
            "Never" => assert!(!row(topics, &name), "{name}: closed"),
            _ => assert!(row(topics, &name), "{name}: no row in PROTOCOL.md, Topics"),
        }
    }
    let manifest = read("rust/ptyrc-store/src/manifest.rs");
    let list = |name: &str| -> Vec<String> {
        let start = manifest.find(&format!("pub const {name}: &[")).unwrap();
        let body = &manifest[start..];
        let body = &body[body.find("= &[").unwrap()..body.find("];").unwrap()];
        body.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    };
    let permissions = section(&doc, "Permissions");
    let names = list("PERMISSIONS");
    assert_eq!(names.len(), 21, "{names:?}");
    for p in names {
        assert!(
            named(permissions, &p),
            "{p}: not in PROTOCOL.md, Permissions"
        );
    }
    let limits = section(&doc, "Limits");
    for l in list("LIMITS") {
        assert!(named(limits, &l), "{l}: not in PROTOCOL.md, Limits");
    }
}

/// `manifest::CEILINGS` of this crate against `LIMITS` and
/// `limit_permission` of `ptyrc-store`: one row each, the same figures.
#[test]
fn the_ceilings_are_the_manifests() {
    let manifest = read("rust/ptyrc-store/src/manifest.rs");
    let start = manifest.find("pub const LIMITS: &[").unwrap();
    let body = &manifest[start..];
    let body = &body[body.find("= &[").unwrap()..body.find("];").unwrap()];
    let rows: Vec<(String, u64, u64)> = (body.lines())
        .filter_map(|l| {
            let l = l.trim().strip_prefix("(\"")?.strip_suffix("),")?;
            let (name, rest) = l.split_once("\", ")?;
            let (d, c) = rest.split_once(", ")?;
            let n = |s: &str| s.replace('_', "").parse::<u64>().unwrap();
            Some((name.to_owned(), n(d), n(c)))
        })
        .collect();
    let ours: Vec<(String, u64, u64)> = (ptyrc_box::manifest::CEILINGS.iter())
        .map(|(name, d, c, _)| (name.to_string(), *d, *c))
        .collect();
    assert_eq!(rows, ours);
    for (name, _, _, permission) in ptyrc_box::manifest::CEILINGS {
        let memory = ["memory-mib", "canvas-screens", "log-mib"].contains(name);
        let theirs = if memory { "limit-memory" } else { "limit-time" };
        assert_eq!(*permission, theirs, "{name}");
    }
    assert!(manifest.contains(r#""memory-mib" | "canvas-screens" | "log-mib" => "limit-memory""#));
}
