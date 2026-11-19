// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc-app.toml`: parsed and validated. See `plan/store-model.md`,
//! "Manifest".

use std::ops::Range;

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::error::{Error, ErrorKind, Result, check_rel_path, line_of};

/// The manifest's file name, at the root of the app tree.
pub const MANIFEST: &str = "ptyrc-app.toml";
/// The one licence accepted.
pub const LICENSE: &str = "GPL-3.0-or-later";
/// The guest ABI version understood.
pub const API: i64 = 1;
/// Bound on the manifest, bytes.
pub const MAX_MANIFEST: usize = 64 << 10;

/// Permission names accepted in `[permissions]` ("The list").
pub const PERMISSIONS: &[&str] = &[
    "clicks",
    "clicks-grab",
    "clock",
    "files",
    "files-write",
    "front",
    "front-widget",
    "keys-arrows",
    "keys-grab",
    "keys-read",
    "layout",
    "pointer",
    "probe-git",
    "probe-system",
    "screen-wide",
    "shape",
    "storage",
    "text",
    "words",
];

/// `[limits]` keys and their defaults; a value must exceed its default.
pub const LIMITS: &[(&str, u64)] = &[
    ("calls-per-second", 1000),
    ("canvas-screens", 4),
    ("init-ms", 2000),
    ("log-mib", 1),
    ("memory-mib", 64),
    ("tick-ms", 50),
];

/// Option names reserved to ptyrc's launcher (principle 4).
pub const RESERVED_OPTIONS: &[&str] = &["alpha", "back", "frame", "front", "help", "prefer"];

/// Bounds on `words`: count, and characters per word.
pub const MAX_WORDS: usize = 32;
pub const WORD_LEN: std::ops::RangeInclusive<usize> = 3..=32;

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub app: App,
    /// Sorted by name.
    pub permissions: Vec<Permission>,
    /// Sorted by name.
    pub limits: Vec<Limit>,
    /// Sorted by name.
    pub options: Vec<AppOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub version: String,
    pub summary: String,
    pub license: String,
    pub api: i64,
    pub entry: Entry,
    /// Rust only: the oldest compiler that builds.
    pub rustc: Option<String>,
    /// A directory of the tree, no trailing `/`.
    pub assets: Option<String>,
    pub surface: Surface,
    pub tape: Option<String>,
}

/// The program: a script, or the root of a Rust crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Script(String),
    Rust(String),
}

impl Entry {
    pub fn path(&self) -> &str {
        match self {
            Entry::Script(p) | Entry::Rust(p) => p,
        }
    }
}

/// Where the app may draw. Absent from the manifest: `Widget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Surface {
    #[default]
    Widget,
    Frame,
    Screen,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Widget => "widget",
            Surface::Frame => "frame",
            Surface::Screen => "screen",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub name: String,
    pub why: String,
    /// `words` only: the words, as written.
    pub words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limit {
    pub name: String,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppOption {
    pub name: String,
    pub default: OptionValue,
    pub doc: String,
}

/// An option's type is that of its default.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
}

impl OptionValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            OptionValue::Int(_) => "int",
            OptionValue::Float(_) => "float",
            OptionValue::Bool(_) => "bool",
            OptionValue::Str(_) => "string",
        }
    }
}

const APP_KEYS: &[&str] = &[
    "api", "assets", "id", "license", "rust", "rustc", "script", "summary", "surface", "tape",
    "version",
];
const SECTIONS: &[&str] = &["app", "limits", "options", "permissions"];

/// Parses raw bytes: UTF-8, at most [`MAX_MANIFEST`].
pub fn parse_bytes(bytes: &[u8]) -> Result<Manifest> {
    if bytes.len() > MAX_MANIFEST {
        return Err(Error::new(
            ErrorKind::Manifest,
            format!(
                "{MANIFEST}: {} bytes, more than {MAX_MANIFEST}",
                bytes.len()
            ),
        )
        .with_hint("a manifest of 64 KiB at most"));
    }
    let src = std::str::from_utf8(bytes).map_err(|e| {
        Error::new(ErrorKind::Manifest, format!("{MANIFEST}: not UTF-8: {e}"))
            .with_hint("save the manifest as UTF-8")
    })?;
    parse(src)
}

/// Parses and validates a manifest.
pub fn parse(src: &str) -> Result<Manifest> {
    let doc = DeTable::parse(src).map_err(|e| {
        let line = e.span().map(|s| line_of(src, s.start)).unwrap_or(1);
        Error::new(
            ErrorKind::Manifest,
            format!("{MANIFEST}:{line}: {}", e.message().trim()),
        )
        .with_hint("TOML syntax: https://toml.io")
    })?;
    let cx = Cx { src };
    let doc = doc.get_ref();
    for (k, v) in doc.iter() {
        if !SECTIONS.contains(&k.get_ref().as_ref()) {
            return Err(cx.err(
                k.span(),
                format!("unknown key `{}`", k.get_ref()),
                format!("sections: {}", SECTIONS.join(", ")),
            ));
        }
        if !matches!(v.get_ref(), DeValue::Table(_)) {
            return Err(cx.err(
                k.span(),
                format!("`{}` is not a table", k.get_ref()),
                format!("[{}] on a line of its own", k.get_ref()),
            ));
        }
    }
    let Some(app) = table(doc, "app") else {
        return Err(cx.err(
            0..0,
            "no [app] section",
            "[app] with id, version, summary, license, api, script",
        ));
    };
    Ok(Manifest {
        app: cx.app(app)?,
        permissions: match table(doc, "permissions") {
            Some(t) => cx.permissions(t)?,
            None => Vec::new(),
        },
        limits: match table(doc, "limits") {
            Some(t) => cx.limits(t)?,
            None => Vec::new(),
        },
        options: match table(doc, "options") {
            Some(t) => cx.options(t)?,
            None => Vec::new(),
        },
    })
}

fn table<'a, 'i>(doc: &'a DeTable<'i>, key: &str) -> Option<&'a DeTable<'i>> {
    match doc.get(key).map(Spanned::get_ref) {
        Some(DeValue::Table(t)) => Some(t),
        _ => None,
    }
}

type Field<'a, 'i> = (
    &'a Spanned<std::borrow::Cow<'i, str>>,
    &'a Spanned<DeValue<'i>>,
);

fn field<'a, 'i>(t: &'a DeTable<'i>, key: &str) -> Option<Field<'a, 'i>> {
    t.get_key_value(key)
}

struct Cx<'s> {
    src: &'s str,
}

impl Cx<'_> {
    fn err(&self, span: Range<usize>, msg: impl AsRef<str>, hint: impl Into<String>) -> Error {
        let line = line_of(self.src, span.start);
        Error::new(
            ErrorKind::Manifest,
            format!("{MANIFEST}:{line}: {}", msg.as_ref()),
        )
        .with_hint(hint)
    }

    fn unknown(&self, t: &DeTable<'_>, section: &str, known: &[&str]) -> Result<()> {
        for k in t.keys() {
            if !known.contains(&k.get_ref().as_ref()) {
                return Err(self.err(
                    k.span(),
                    format!("[{section}] {}: unknown key", k.get_ref()),
                    format!("keys of [{section}]: {}", known.join(", ")),
                ));
            }
        }
        Ok(())
    }

    fn string(
        &self,
        t: &DeTable<'_>,
        section: &str,
        key: &str,
        hint: &str,
    ) -> Result<Option<String>> {
        match field(t, key) {
            None => Ok(None),
            Some((_, v)) => match v.get_ref() {
                DeValue::String(s) => Ok(Some(s.to_string())),
                _ => Err(self.err(
                    v.span(),
                    format!("[{section}] {key}: not a string"),
                    format!("{key} = {hint}"),
                )),
            },
        }
    }

    fn required(&self, t: &DeTable<'_>, key: &str, hint: &str) -> Result<String> {
        self.string(t, "app", key, hint)?.ok_or_else(|| {
            self.err(
                0..0,
                format!("[app] {key}: missing"),
                format!("{key} = {hint}"),
            )
        })
    }

    fn span(&self, t: &DeTable<'_>, key: &str) -> Range<usize> {
        field(t, key).map(|(_, v)| v.span()).unwrap_or(0..0)
    }

    fn path(&self, t: &DeTable<'_>, key: &str, hint: &str) -> Result<Option<String>> {
        let Some(p) = self.string(t, "app", key, hint)? else {
            return Ok(None);
        };
        let trimmed = p.strip_suffix('/').unwrap_or(&p);
        if let Err(why) = check_rel_path(trimmed) {
            return Err(self.err(
                self.span(t, key),
                format!("[app] {key}: {p:?}: {why}"),
                format!("a path relative to the manifest, as {key} = {hint}"),
            ));
        }
        Ok(Some(trimmed.to_string()))
    }

    fn app(&self, t: &DeTable<'_>) -> Result<App> {
        self.unknown(t, "app", APP_KEYS)?;
        let id = self.required(t, "id", "\"sand-garden\"")?;
        if !is_name(&id, 64) {
            return Err(self.err(
                self.span(t, "id"),
                format!("[app] id: {id:?}: not a name"),
                "lower-case letters, digits and `-`, a letter first, 64 at most",
            ));
        }
        let version = self.required(t, "version", "\"0.1.0\"")?;
        if version.is_empty()
            || version.len() > 64
            || !version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
        {
            return Err(self.err(
                self.span(t, "version"),
                format!("[app] version: {version:?}"),
                "letters, digits, `.`, `+`, `-`; 64 at most, as \"0.1.0\"",
            ));
        }
        let summary = self.required(t, "summary", "\"Sand that piles on your text.\"")?;
        let n = summary.chars().count();
        if n == 0 || n > 200 || summary.chars().any(char::is_control) {
            return Err(self.err(
                self.span(t, "summary"),
                "[app] summary: empty, longer than 200 characters, or a control character",
                "one line of 200 characters at most",
            ));
        }
        let license = self.required(t, "license", "\"GPL-3.0-or-later\"")?;
        if license != LICENSE {
            return Err(self.err(
                self.span(t, "license"),
                format!("[app] license: {license:?} is not accepted"),
                format!("license = \"{LICENSE}\", and the licence text in the tree"),
            ));
        }
        let api = match field(t, "api") {
            None => {
                return Err(self.err(0..0, "[app] api: missing", format!("api = {API}")));
            }
            Some((_, v)) => match v.get_ref() {
                DeValue::Integer(i) if i.radix() == 10 && i.as_str() == "1" => API,
                _ => {
                    return Err(self.err(
                        v.span(),
                        "[app] api: not a version this ptyrc runs",
                        format!("api = {API}"),
                    ));
                }
            },
        };
        let script = self.path(t, "script", "\"app.js\"")?;
        let rust = self.path(t, "rust", "\"src/lib.rs\"")?;
        let entry = match (script, rust) {
            (Some(s), None) => {
                if !s.ends_with(".js") {
                    return Err(self.err(
                        self.span(t, "script"),
                        format!("[app] script: {s:?}: not a .js file"),
                        "script = \"app.js\"",
                    ));
                }
                Entry::Script(s)
            }
            (None, Some(r)) => {
                if !r.ends_with(".rs") {
                    return Err(self.err(
                        self.span(t, "rust"),
                        format!("[app] rust: {r:?}: not a .rs file"),
                        "rust = \"src/lib.rs\"",
                    ));
                }
                Entry::Rust(r)
            }
            (Some(_), Some(_)) => {
                return Err(self.err(
                    self.span(t, "rust"),
                    "[app] script and rust: both given",
                    "one of script = \"app.js\" or rust = \"src/lib.rs\"",
                ));
            }
            (None, None) => {
                return Err(self.err(
                    0..0,
                    "[app] script or rust: missing",
                    "script = \"app.js\", or rust = \"src/lib.rs\"",
                ));
            }
        };
        let rustc = self.string(t, "app", "rustc", "\"1.85\"")?;
        if let Some(r) = &rustc {
            if matches!(entry, Entry::Script(_)) {
                return Err(self.err(
                    self.span(t, "rustc"),
                    "[app] rustc: given for a script app",
                    "remove rustc, or use rust = \"src/lib.rs\"",
                ));
            }
            let parts: Vec<&str> = r.split('.').collect();
            if !(2..=3).contains(&parts.len())
                || parts
                    .iter()
                    .any(|p| p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err(self.err(
                    self.span(t, "rustc"),
                    format!("[app] rustc: {r:?}: not a version"),
                    "rustc = \"1.85\"",
                ));
            }
        }
        let assets = self.path(t, "assets", "\"assets/\"")?;
        let tape = self.path(t, "tape", "\"demo.tape\"")?;
        let surface = match self.string(t, "app", "surface", "\"widget\"")?.as_deref() {
            None | Some("widget") => Surface::Widget,
            Some("frame") => Surface::Frame,
            Some("screen") => Surface::Screen,
            Some(s) => {
                return Err(self.err(
                    self.span(t, "surface"),
                    format!("[app] surface: {s:?}"),
                    "surface = \"widget\", \"frame\" or \"screen\"",
                ));
            }
        };
        Ok(App {
            id,
            version,
            summary,
            license,
            api,
            entry,
            rustc,
            assets,
            surface,
            tape,
        })
    }

    fn why(&self, name: &str, v: &Spanned<DeValue<'_>>, s: &str) -> Result<String> {
        let n = s.chars().count();
        if n == 0 || n > 200 || s.chars().any(char::is_control) {
            return Err(self.err(
                v.span(),
                format!("[permissions] {name}: the reason is empty, longer than 200 characters, or holds a control character"),
                format!("{name} = \"one line: why the app needs it\""),
            ));
        }
        Ok(s.to_string())
    }

    fn permissions(&self, t: &DeTable<'_>) -> Result<Vec<Permission>> {
        let mut out = Vec::new();
        for (k, v) in t.iter() {
            let name = k.get_ref().as_ref();
            if !PERMISSIONS.contains(&name) {
                let hint = if name.starts_with("limit-") {
                    "a limit goes in [limits], as memory-mib = 128".to_string()
                } else {
                    format!("permissions: {}", PERMISSIONS.join(", "))
                };
                return Err(self.err(
                    k.span(),
                    format!("[permissions] {name}: not a permission"),
                    hint,
                ));
            }
            let words_hint = "words = { list = [\"food\", \"fish\"], why = \"...\" }";
            let p = match (name, v.get_ref()) {
                ("words", DeValue::Table(w)) => {
                    self.unknown(w, "permissions.words", &["list", "why"])?;
                    let why = match field(w, "why") {
                        Some((_, wv)) => match wv.get_ref() {
                            DeValue::String(s) => self.why(name, wv, s)?,
                            _ => {
                                return Err(self.err(
                                    wv.span(),
                                    "[permissions] words.why: not a string",
                                    words_hint,
                                ));
                            }
                        },
                        None => {
                            return Err(self.err(
                                v.span(),
                                "[permissions] words.why: missing",
                                words_hint,
                            ));
                        }
                    };
                    let words = self.words(w, v.span(), words_hint)?;
                    Permission {
                        name: name.to_string(),
                        why,
                        words,
                    }
                }
                ("words", _) => {
                    return Err(self.err(v.span(), "[permissions] words: not a table", words_hint));
                }
                (_, DeValue::String(s)) => Permission {
                    name: name.to_string(),
                    why: self.why(name, v, s)?,
                    words: Vec::new(),
                },
                (_, DeValue::Table(w)) => {
                    self.unknown(w, &format!("permissions.{name}"), &["why"])?;
                    let Some((_, wv)) = field(w, "why") else {
                        return Err(self.err(
                            v.span(),
                            format!("[permissions] {name}.why: missing"),
                            format!("{name} = \"why the app needs it\""),
                        ));
                    };
                    let DeValue::String(s) = wv.get_ref() else {
                        return Err(self.err(
                            wv.span(),
                            format!("[permissions] {name}.why: not a string"),
                            format!("{name} = \"why the app needs it\""),
                        ));
                    };
                    Permission {
                        name: name.to_string(),
                        why: self.why(name, wv, s)?,
                        words: Vec::new(),
                    }
                }
                _ => {
                    return Err(self.err(
                        v.span(),
                        format!("[permissions] {name}: not a string"),
                        format!("{name} = \"why the app needs it\""),
                    ));
                }
            };
            out.push(p);
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn words(&self, w: &DeTable<'_>, at: Range<usize>, hint: &str) -> Result<Vec<String>> {
        let Some((_, lv)) = field(w, "list") else {
            return Err(self.err(at, "[permissions] words.list: missing", hint));
        };
        let DeValue::Array(arr) = lv.get_ref() else {
            return Err(self.err(lv.span(), "[permissions] words.list: not an array", hint));
        };
        if arr.is_empty() || arr.len() > MAX_WORDS {
            return Err(self.err(
                lv.span(),
                format!("[permissions] words.list: {} words", arr.len()),
                format!("1 to {MAX_WORDS} words"),
            ));
        }
        let mut seen: Vec<String> = Vec::new();
        let mut out = Vec::new();
        for item in arr.iter() {
            let DeValue::String(s) = item.get_ref() else {
                return Err(self.err(item.span(), "[permissions] words.list: not a string", hint));
            };
            let n = s.chars().count();
            if !WORD_LEN.contains(&n) || !s.chars().all(char::is_alphanumeric) {
                return Err(self.err(
                    item.span(),
                    format!("[permissions] words.list: {s:?} is not a word"),
                    "a word is 3 to 32 letters or digits, literal, whole; no space, no pattern",
                ));
            }
            let lower = s.to_lowercase();
            if seen.contains(&lower) {
                return Err(self.err(
                    item.span(),
                    format!("[permissions] words.list: {s:?} twice"),
                    "each word once; case is ignored",
                ));
            }
            seen.push(lower);
            out.push(s.to_string());
        }
        Ok(out)
    }

    fn limits(&self, t: &DeTable<'_>) -> Result<Vec<Limit>> {
        let names: Vec<&str> = LIMITS.iter().map(|(n, _)| *n).collect();
        self.unknown(t, "limits", &names)?;
        let mut out = Vec::new();
        for (k, v) in t.iter() {
            let name = k.get_ref().as_ref();
            let default = LIMITS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, d)| *d)
                .unwrap_or(0);
            let value = match v.get_ref() {
                DeValue::Integer(i) => u64::from_str_radix(i.as_str(), i.radix()).ok(),
                _ => None,
            };
            match value {
                Some(x) if x > default && x <= 1 << 32 => out.push(Limit {
                    name: name.to_string(),
                    value: x,
                }),
                _ => {
                    return Err(self.err(
                        v.span(),
                        format!("[limits] {name}: not an integer above the default {default}"),
                        format!("only what exceeds the default, as {name} = {}", default * 2),
                    ));
                }
            }
        }
        Ok(out)
    }

    fn options(&self, t: &DeTable<'_>) -> Result<Vec<AppOption>> {
        let mut out = Vec::new();
        let hint =
            |name: &str| format!("{name} = {{ type = \"int\", default = 20, doc = \"...\" }}");
        for (k, v) in t.iter() {
            let name = k.get_ref().as_ref();
            if !is_name(name, 32) {
                return Err(self.err(
                    k.span(),
                    format!("[options] {name:?}: not a name"),
                    "lower-case letters, digits and `-`, a letter first, 32 at most",
                ));
            }
            if RESERVED_OPTIONS.contains(&name) {
                return Err(self.err(
                    k.span(),
                    format!("[options] {name}: an option of ptyrc's own"),
                    format!("another name; reserved: {}", RESERVED_OPTIONS.join(", ")),
                ));
            }
            let DeValue::Table(o) = v.get_ref() else {
                return Err(self.err(
                    v.span(),
                    format!("[options] {name}: not a table"),
                    hint(name),
                ));
            };
            let section = format!("options.{name}");
            self.unknown(o, &section, &["default", "doc", "type"])?;
            let ty = self.string(o, &section, "type", "\"int\"")?;
            let doc = self.string(o, &section, "doc", "\"what it does\"")?;
            let (Some(ty), Some(doc), Some((_, dv))) = (ty, doc, field(o, "default")) else {
                return Err(self.err(
                    v.span(),
                    format!("[options] {name}: type, default and doc are wanted"),
                    hint(name),
                ));
            };
            if doc.is_empty() || doc.chars().count() > 200 || doc.chars().any(char::is_control) {
                return Err(self.err(
                    v.span(),
                    format!("[options] {name}.doc: empty, longer than 200 characters, or a control character"),
                    "one line",
                ));
            }
            let default = match (ty.as_str(), dv.get_ref()) {
                ("int", DeValue::Integer(i)) => i64::from_str_radix(i.as_str(), i.radix())
                    .ok()
                    .map(OptionValue::Int),
                ("float", DeValue::Float(f)) => f.as_str().parse().ok().map(OptionValue::Float),
                ("float", DeValue::Integer(i)) => i64::from_str_radix(i.as_str(), i.radix())
                    .ok()
                    .map(|x| OptionValue::Float(x as f64)),
                ("bool", DeValue::Boolean(b)) => Some(OptionValue::Bool(*b)),
                ("string", DeValue::String(s)) => Some(OptionValue::Str(s.to_string())),
                ("int" | "float" | "bool" | "string", _) => {
                    return Err(self.err(
                        dv.span(),
                        format!("[options] {name}.default: not of type {ty}"),
                        "a default of the type named",
                    ));
                }
                _ => {
                    return Err(self.err(
                        self.span(o, "type"),
                        format!("[options] {name}.type: {ty:?}"),
                        "type = \"int\", \"float\", \"bool\" or \"string\"",
                    ));
                }
            };
            let Some(default) = default else {
                return Err(self.err(
                    dv.span(),
                    format!("[options] {name}.default: out of range"),
                    hint(name),
                ));
            };
            out.push(AppOption {
                name: name.to_string(),
                default,
                doc,
            });
        }
        Ok(out)
    }
}

/// `[a-z][a-z0-9-]*`, at most `max` bytes.
fn is_name(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= max
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}
