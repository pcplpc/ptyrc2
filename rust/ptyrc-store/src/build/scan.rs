// SPDX-License-Identifier: GPL-3.0-or-later
//! What a Rust store app may not have: the rule of `rustc` alone (std
//! and `ptyrc-app`; no other crate, no build script, no macro crate),
//! and the doors of the sources: a path out of the tree, `#[link]`,
//! `#![feature]`, `extern crate` of another crate, a procedural macro.
//!
//! A light lexer: comments and literals are skipped, identifiers seen.
//! The wall is the profile of the build; this says why before it.

use crate::error::{Error, ErrorKind};
use crate::tree::{Checked, Tree};

/// The rule, named in every hint.
pub const RULE: &str = "a store app is built by rustc alone: std and ptyrc-app, no other crate, \
                        no build.rs, no macro crate";

/// The directory of the vendored copy a template writes; never built.
pub const VENDORED: &str = "vendor/ptyrc-app/";

/// The name of the built module in an installed tree.
pub const MODULE: &str = "app.wasm";

/// Crates `extern crate` may name.
const CRATES: &[&str] = &["std", "core", "alloc", "ptyrc_app", "self"];

fn refused(message: String, hint: impl Into<String>) -> Error {
    Error::new(ErrorKind::Build, message).with_hint(hint)
}

/// Every problem of the tree under the rule; empty: none.
pub fn sources(tree: &dyn Tree, checked: &Checked) -> Vec<Error> {
    let mut bad = Vec::new();
    for f in &checked.files {
        if f.path.starts_with(VENDORED) {
            continue;
        }
        match f.path.as_str() {
            "build.rs" => bad.push(refused(
                "build.rs: a build script".into(),
                format!("remove it; what it makes goes in the tree as a file ({RULE})"),
            )),
            MODULE => bad.push(refused(
                format!("{MODULE}: the name of the module the install builds"),
                "remove it, or rename it",
            )),
            "Cargo.toml" => match tree.read(f) {
                Ok(bytes) => bad.extend(cargo_toml(&String::from_utf8_lossy(&bytes))),
                Err(e) => bad.push(e),
            },
            p if p.ends_with(".rs") => match tree.read(f) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => bad.extend(rust(p, &text)),
                    Err(_) => bad.push(refused(format!("{p}: not UTF-8"), "Rust source in UTF-8")),
                },
                Err(e) => bad.push(e),
            },
            _ => {}
        }
    }
    bad
}

/// `Cargo.toml`: kept for `cargo test` of the author; the store does
/// not read it but for what it would make cargo do.
fn cargo_toml(text: &str) -> Vec<Error> {
    let mut bad = Vec::new();
    let table: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => {
            return vec![refused(
                format!("Cargo.toml: not TOML: {}", e.message()),
                "fix it, or remove it: the store does not need it",
            )];
        }
    };
    let get = |t: &toml::Table, k: &str| t.get(k).and_then(toml::Value::as_table).cloned();
    let mut deps = |where_: &str, t: Option<toml::Table>| {
        for name in t.into_iter().flat_map(|t| t.into_iter().map(|(k, _)| k)) {
            if name != "ptyrc-app" {
                bad.push(refused(
                    format!("Cargo.toml: [{where_}] {name}: a dependency"),
                    format!("remove it; [dev-dependencies] stay yours ({RULE})"),
                ));
            }
        }
    };
    deps("dependencies", get(&table, "dependencies"));
    deps("build-dependencies", get(&table, "build-dependencies"));
    for (target, t) in get(&table, "target").unwrap_or_default() {
        if let Some(t) = t.as_table() {
            deps(
                &format!("target.{target}.dependencies"),
                get(t, "dependencies"),
            );
            deps(
                &format!("target.{target}.build-dependencies"),
                get(t, "build-dependencies"),
            );
        }
    }
    if get(&table, "package").is_some_and(|p| p.contains_key("build")) {
        bad.push(refused(
            "Cargo.toml: [package] build: a build script".into(),
            format!("remove it ({RULE})"),
        ));
    }
    let lib = get(&table, "lib").unwrap_or_default();
    if ["proc-macro", "proc_macro"]
        .iter()
        .any(|k| lib.contains_key(*k))
    {
        bad.push(refused(
            "Cargo.toml: [lib] proc-macro: a macro crate".into(),
            format!("remove it ({RULE})"),
        ));
    }
    bad
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    /// A string literal, its value; `None`: an escape not decoded.
    Str(Option<String>),
    Punct(char),
    /// A number, a char, a lifetime.
    Other,
}

/// Tokens with their line; comments dropped.
fn lex(src: &str) -> Vec<(Tok, usize)> {
    let c: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut out) = (0, 1, Vec::new());
    let ident_start = |ch: char| ch == '_' || ch.is_alphabetic();
    let ident_cont = |ch: char| ch == '_' || ch.is_alphanumeric();
    while i < c.len() {
        let ch = c[i];
        let at = line;
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if c[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if let Some((tok, next)) = raw_or_prefixed(&c, i) {
            line += c[i..next].iter().filter(|&&x| x == '\n').count();
            out.push((tok, at));
            i = next;
        } else if ch == '"' {
            let (value, next) = quoted(&c, i + 1, '"');
            line += c[i..next].iter().filter(|&&x| x == '\n').count();
            out.push((Tok::Str(value), at));
            i = next;
        } else if ch == '\'' {
            // A char literal, or a lifetime.
            if c.get(i + 1) == Some(&'\\') || c.get(i + 2) == Some(&'\'') {
                let (_, next) = quoted(&c, i + 1, '\'');
                i = next;
            } else {
                i += 1;
                while i < c.len() && ident_cont(c[i]) {
                    i += 1;
                }
            }
            out.push((Tok::Other, at));
        } else if ch.is_ascii_digit() {
            while i < c.len()
                && (ident_cont(c[i])
                    || (c[i] == '.' && c.get(i + 1).is_some_and(char::is_ascii_digit)))
            {
                i += 1;
            }
            out.push((Tok::Other, at));
        } else if ident_start(ch) {
            let start = i;
            while i < c.len() && ident_cont(c[i]) {
                i += 1;
            }
            out.push((Tok::Ident(c[start..i].iter().collect()), at));
        } else {
            out.push((Tok::Punct(ch), at));
            i += 1;
        }
    }
    out
}

/// `r"..."`, `r#"..."#`, `b"..."`, `br"..."`, `c"..."`, `cr"..."`,
/// `b'x'`, `r#ident`: the token and the index past it.
fn raw_or_prefixed(c: &[char], i: usize) -> Option<(Tok, usize)> {
    let mut j = i;
    let prefix: String = c[i..]
        .iter()
        .take_while(|x| x.is_ascii_alphabetic())
        .collect();
    let raw = match prefix.as_str() {
        _ if c.get(i + prefix.len()) == Some(&'\'') && prefix == "b" => {
            let (_, next) = quoted(c, i + 2, '\'');
            return Some((Tok::Other, next));
        }
        "r" | "br" | "cr" => true,
        "b" | "c" => false,
        _ => return None,
    };
    j += prefix.len();
    if raw {
        let hashes = c[j..].iter().take_while(|&&x| x == '#').count();
        if c.get(j + hashes) != Some(&'"') {
            // `r#ident`: a raw identifier.
            if prefix == "r" && hashes == 1 {
                let start = j + 1;
                let mut k = start;
                while k < c.len() && (c[k] == '_' || c[k].is_alphanumeric()) {
                    k += 1;
                }
                return (k > start).then(|| (Tok::Ident(c[start..k].iter().collect()), k));
            }
            return None;
        }
        let start = j + hashes + 1;
        let mut k = start;
        while k < c.len() {
            if c[k] == '"'
                && c[k + 1..]
                    .iter()
                    .take(hashes)
                    .filter(|&&x| x == '#')
                    .count()
                    == hashes
            {
                let value: String = c[start..k].iter().collect();
                return Some((Tok::Str(Some(value)), k + 1 + hashes));
            }
            k += 1;
        }
        Some((Tok::Str(None), c.len()))
    } else if c.get(j) == Some(&'"') {
        let (value, next) = quoted(c, j + 1, '"');
        Some((Tok::Str(value), next))
    } else {
        None
    }
}

/// From after the opening quote to past the closing one: the value,
/// `None` when an escape is not one of the plain ones.
fn quoted(c: &[char], mut i: usize, end: char) -> (Option<String>, usize) {
    let mut out = Some(String::new());
    while i < c.len() {
        let ch = c[i];
        if ch == end {
            return (out, i + 1);
        }
        if ch == '\\' {
            let e = c.get(i + 1).copied().unwrap_or(' ');
            let plain = match e {
                'n' => Some('\n'),
                't' => Some('\t'),
                'r' => Some('\r'),
                '0' => Some('\0'),
                '\\' | '"' | '\'' => Some(e),
                _ => None,
            };
            match (plain, &mut out) {
                (Some(p), Some(s)) => s.push(p),
                _ => out = None,
            }
            i += 2;
            continue;
        }
        if let Some(s) = &mut out {
            s.push(ch);
        }
        i += 1;
    }
    (out, c.len())
}

/// `path`, named in `file`, stays inside the tree: relative, and no
/// `..` above the root. `#[path]` and `include*!` resolve from the
/// file's directory.
fn inside(file: &str, path: &str) -> bool {
    if path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return false;
    }
    let mut depth: Vec<&str> = file.split('/').collect();
    depth.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if depth.pop().is_none() {
                    return false;
                }
            }
            p => depth.push(p),
        }
    }
    true
}

const INCLUDES: &[&str] = &["include", "include_str", "include_bytes"];
/// Before an identifier: it is a name being defined, or a field.
const DEFINES: &[&str] = &[
    "fn", "let", "mut", "struct", "enum", "mod", "const", "static", "type", "trait", "union",
];
const ATTRS_REFUSED: &[(&str, &str)] = &[
    ("link", "an import from another module than ptyrc"),
    ("proc_macro", "a procedural macro"),
    ("proc_macro_derive", "a procedural macro"),
    ("proc_macro_attribute", "a procedural macro"),
    ("crate_type", "the kind of crate is the build's"),
    ("no_core", "an unstable feature"),
];

/// The problems of one source file.
fn rust(file: &str, src: &str) -> Vec<Error> {
    let t = lex(src);
    let mut bad = Vec::new();
    let at = |i: usize| t.get(i).map(|x| &x.0);
    let ident = |i: usize| match at(i) {
        Some(Tok::Ident(s)) => Some(s.as_str()),
        _ => None,
    };
    let punct = |i: usize, p: char| at(i) == Some(&Tok::Punct(p));
    let path_err = |line: usize, what: &str, path: Option<&str>| {
        let shown = path.map_or("a path not a plain literal".into(), |p| format!("{p:?}"));
        refused(
            format!("{file}:{line}: {what} {shown}: out of the tree"),
            "a relative path inside the app's tree, as a string literal",
        )
    };
    // Attribute nesting: the index of the `]` that ends it.
    let mut attr_end: Option<usize> = None;
    let mut i = 0;
    while i < t.len() {
        let line = t[i].1;
        if attr_end.is_some_and(|e| i > e) {
            attr_end = None;
        }
        if punct(i, '#') && attr_end.is_none() {
            let inner = punct(i + 1, '!');
            let open = if inner { i + 2 } else { i + 1 };
            if punct(open, '[') {
                attr_end = Some(close(&t, open));
                if inner && ident(open + 1) == Some("feature") {
                    bad.push(refused(
                        format!("{file}:{line}: #![feature]: an unstable feature"),
                        format!("stable Rust only ({RULE})"),
                    ));
                }
            }
        }
        let Some(name) = ident(i) else {
            i += 1;
            continue;
        };
        if attr_end.is_some() {
            if let Some((_, why)) = ATTRS_REFUSED.iter().find(|(n, _)| *n == name) {
                bad.push(refused(
                    format!("{file}:{line}: #[{name}]: {why}"),
                    format!("remove it ({RULE})"),
                ));
            }
            if name == "feature" && punct(i + 1, '(') {
                bad.push(refused(
                    format!("{file}:{line}: feature(...): an unstable feature"),
                    format!("stable Rust only ({RULE})"),
                ));
            }
            if name == "path" && punct(i + 1, '=') {
                match at(i + 2) {
                    Some(Tok::Str(Some(p))) if inside(file, p) => {}
                    Some(Tok::Str(Some(p))) => bad.push(path_err(line, "#[path]", Some(p))),
                    _ => bad.push(path_err(line, "#[path]", None)),
                }
            }
        }
        if name == "extern" && ident(i + 1) == Some("crate") {
            let krate = ident(i + 2).unwrap_or("");
            if !CRATES.contains(&krate) {
                bad.push(refused(
                    format!("{file}:{line}: extern crate {krate}: another crate"),
                    format!("std, core, alloc and ptyrc_app only ({RULE})"),
                ));
            }
        }
        if INCLUDES.contains(&name) {
            let defined = i > 0
                && (punct(i - 1, '.')
                    || matches!(at(i - 1), Some(Tok::Ident(k)) if DEFINES.contains(&k.as_str())));
            let colon = punct(i + 1, ':') && !punct(i + 2, ':');
            let assigned = punct(i + 1, '=') && !punct(i + 2, '=');
            if punct(i + 1, '!') {
                let lit = match (at(i + 2), at(i + 3)) {
                    (Some(Tok::Punct('(' | '[' | '{')), Some(Tok::Str(s))) => Some(s.clone()),
                    _ => None,
                };
                match lit {
                    Some(Some(p)) if inside(file, &p) => {}
                    Some(p) => bad.push(path_err(line, &format!("{name}!"), p.as_deref())),
                    None => bad.push(path_err(line, &format!("{name}!"), None)),
                }
            } else if !(defined || colon || assigned || punct(i + 1, '(')) {
                bad.push(refused(
                    format!(
                        "{file}:{line}: {name}: the macro named other than as {name}!(\"path\")"
                    ),
                    "call it with a literal path; a name of your own: rename it",
                ));
            }
        }
        i += 1;
    }
    bad
}

/// The index of the delimiter that closes the one at `open`.
fn close(t: &[(Tok, usize)], open: usize) -> usize {
    let mut depth = 0usize;
    for (k, (tok, _)) in t.iter().enumerate().skip(open) {
        match tok {
            Tok::Punct('[' | '(' | '{') => depth += 1,
            Tok::Punct(']' | ')' | '}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    t.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problems(src: &str) -> Vec<String> {
        rust("src/lib.rs", src)
            .into_iter()
            .map(|e| e.message)
            .collect()
    }

    #[test]
    fn plain_code_passes() {
        let src = r##"
            //! include_str!("/etc/passwd") in a comment
            #![allow(dead_code)]
            #[cfg(feature = "link")]
            use ptyrc_app::{App, json};
            extern crate alloc;
            /* #[link(name = "x")] */
            const S: &str = include_str!("../assets/a.txt");
            const B: &[u8] = include_bytes!("data.bin");
            #[path = "more/x.rs"] mod x;
            struct S { include: u8 }
            fn include(path: &str) -> &str { let r = r#"include_str!("/etc")"#; path }
            fn f<'a>(s: S, c: char) { let _ = s.include; let _ = 'x'; let _ = b'"'; let l = 'a'; }
        "##;
        assert_eq!(problems(src), Vec::<String>::new());
    }

    #[test]
    fn doors_refused() {
        let cases = [
            (
                r#"const S: &str = include_str!("/home/u/.ssh/id");"#,
                "out of the tree",
            ),
            (
                r#"const S: &str = include_str!("../../x");"#,
                "out of the tree",
            ),
            (
                r#"const S: &[u8] = include_bytes!(concat!("/et", "c"));"#,
                "out of the tree",
            ),
            (r#"#[path = "/etc/passwd"] mod p;"#, "#[path]"),
            (r#"#[cfg_attr(all(), path = "../../x")] mod p;"#, "#[path]"),
            ("use std::include_str as inc;", "named other than"),
            ("m!(include_bytes);", "named other than"),
            (
                "macro_rules! m { ($p:expr) => { include!($p) } }",
                "out of the tree",
            ),
            ("#![feature(rustc_attrs)]", "#![feature]"),
            (
                r#"#[link(wasm_import_module = "wasi_snapshot_preview1")] unsafe extern "C" {}"#,
                "#[link]",
            ),
            ("extern crate proc_macro;", "extern crate proc_macro"),
            ("#[proc_macro] pub fn m() {}", "#[proc_macro]"),
            ("#![crate_type = \"proc-macro\"]", "#[crate_type]"),
            ("const X: &str = r#include_str!(\"/x\");", "out of the tree"),
        ];
        for (src, want) in cases {
            let got = problems(src);
            assert!(got.iter().any(|m| m.contains(want)), "{src}: {got:?}");
        }
    }

    #[test]
    fn cargo_toml_rules() {
        let ok = "[package]\nname = \"a\"\n[dependencies]\nptyrc-app = { path = \"vendor/ptyrc-app\" }\n[dev-dependencies]\nproptest = \"1\"\n";
        assert!(cargo_toml(ok).is_empty());
        let bad = "[package]\nname = \"a\"\nbuild = \"gen.rs\"\n[dependencies]\nserde = \"1\"\n[build-dependencies]\ncc = \"1\"\n[target.'cfg(unix)'.dependencies]\nlibc = \"1\"\n[lib]\nproc-macro = true\n";
        let got: Vec<String> = cargo_toml(bad).into_iter().map(|e| e.message).collect();
        assert_eq!(got.len(), 5, "{got:?}");
        assert!(
            cargo_toml(bad)
                .iter()
                .all(|e| e.hint.as_deref().unwrap().contains(RULE))
        );
    }
}
