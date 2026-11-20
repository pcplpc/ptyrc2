// SPDX-License-Identifier: GPL-3.0-or-later
//! `[options]` and `[args]` of a manifest, and a command line read
//! against them. One reader for every launcher (`ptyrc app`, `ptyrc
//! manage`, `dev test`, `dev run`) and one check for the driver
//! (`app.start {store}`): [`read`], then [`resolve`].

use std::ops::Range;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use toml::de::{DeTable, DeValue};

use crate::error::{Error, ErrorKind, Result};
use crate::manifest::{Cx, Manifest, WORD_LEN, field, is_name};

/// Values of one list option, or positionals.
pub const MAX_VALUES: usize = 16;
/// Files and directories named, a run.
pub const MAX_FILES: usize = 16;
/// Bytes of a pattern.
pub const MAX_PATTERN: usize = 256;
/// Patterns given, a run.
pub const MAX_PATTERNS: usize = 16;
/// Choices of an option.
pub const MAX_CHOICES: usize = 64;
/// `--prefer`, and `prefer` of `app.start`.
pub const PREFER: &[&str] = &[
    "any",
    "bottom",
    "bottom-left",
    "bottom-right",
    "center",
    "left",
    "right",
    "top",
    "top-left",
    "top-right",
];

/// The type of an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionType {
    Int,
    Float,
    Bool,
    String,
    /// The user's word: held for the run (`plan/store-model.md`,
    /// "Degrees: what an app learns of the screen").
    Word,
    /// The user's pattern: `screen.find` held to it.
    Pattern,
    /// A file named: a handle.
    File,
    /// A directory named: a handle.
    Dir,
}

impl OptionType {
    pub const ALL: [OptionType; 8] = [
        OptionType::Int,
        OptionType::Float,
        OptionType::Bool,
        OptionType::String,
        OptionType::Word,
        OptionType::Pattern,
        OptionType::File,
        OptionType::Dir,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            OptionType::Int => "int",
            OptionType::Float => "float",
            OptionType::Bool => "bool",
            OptionType::String => "string",
            OptionType::Word => "word",
            OptionType::Pattern => "pattern",
            OptionType::File => "file",
            OptionType::Dir => "dir",
        }
    }

    pub fn parse(s: &str) -> Option<OptionType> {
        OptionType::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// A file or a directory: a handle.
    pub fn named(self) -> bool {
        matches!(self, OptionType::File | OptionType::Dir)
    }

    /// What a value is called in `--help`.
    fn metavar(self) -> &'static str {
        match self {
            OptionType::Int => "N",
            OptionType::Float => "X",
            OptionType::Bool => "",
            OptionType::String => "TEXT",
            OptionType::Word => "WORD",
            OptionType::Pattern => "PATTERN",
            OptionType::File => "FILE",
            OptionType::Dir => "DIR",
        }
    }
}

/// A default.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    /// `string`, `word`, `pattern`; `file`, `dir`: a plain name.
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

    pub fn to_json(&self) -> Value {
        match self {
            OptionValue::Int(n) => json!(n),
            OptionValue::Float(x) => json!(x),
            OptionValue::Bool(b) => json!(b),
            OptionValue::Str(s) => json!(s),
        }
    }

    /// As shown in `--help` and the menu.
    pub fn text(&self) -> String {
        match self {
            OptionValue::Int(n) => n.to_string(),
            OptionValue::Float(x) => x.to_string(),
            OptionValue::Bool(b) => b.to_string(),
            OptionValue::Str(s) => s.clone(),
        }
    }
}

/// An entry of `[options]`, or the one of `[args]` (`list` then true).
#[derive(Debug, Clone, PartialEq)]
pub struct AppOption {
    pub name: String,
    pub ty: OptionType,
    /// Required of `int`, `float`, `bool`, `string` but in a list.
    /// A list: none, but a file or a directory (the one used when none
    /// is named).
    pub default: Option<OptionValue>,
    pub doc: String,
    /// `-g` for `--grab`.
    pub short: Option<char>,
    /// `string` only: the values allowed.
    pub choices: Vec<String>,
    /// Repeated: `init` gets an array.
    pub list: bool,
    /// Permissions asked only when the option is given (a `bool`: true).
    pub needs: Vec<String>,
    /// `file` only: written back, with `files-write`.
    pub write: bool,
    /// `pattern` only: a regular expression.
    pub regex: bool,
}

impl AppOption {
    /// `int`, `word list`, ...
    pub fn type_name(&self) -> String {
        match self.list {
            true => format!("{} list", self.ty.as_str()),
            false => self.ty.as_str().to_string(),
        }
    }
}

/// `[args]`: one list of positionals.
#[derive(Debug, Clone, PartialEq)]
pub struct Positional {
    pub option: AppOption,
    pub min: usize,
    pub max: usize,
}

const OPTION_KEYS: &[&str] = &[
    "choices", "default", "doc", "list", "needs", "regex", "short", "type", "write",
];
const ARGS_KEYS: &[&str] = &[
    "choices", "default", "doc", "max", "min", "needs", "regex", "type", "write",
];

fn types() -> String {
    let names: Vec<&str> = OptionType::ALL.iter().map(|t| t.as_str()).collect();
    names.join(", ")
}

impl Cx<'_> {
    /// `[options]`, sorted by name.
    pub(crate) fn options(&self, t: &DeTable<'_>) -> Result<Vec<AppOption>> {
        let mut out: Vec<AppOption> = Vec::new();
        for (k, v) in t.iter() {
            let name = k.get_ref().as_ref();
            if !is_name(name, 32) {
                return Err(self.err(
                    k.span(),
                    format!("[options] {name:?}: not a name"),
                    "lower-case letters, digits and `-`, a letter first, 32 at most",
                ));
            }
            if crate::manifest::RESERVED_OPTIONS.contains(&name)
                || name.strip_prefix("no-").is_some()
            {
                return Err(self.err(
                    k.span(),
                    format!("[options] {name}: an option of ptyrc's own, or no-NAME"),
                    format!(
                        "another name; reserved: {}, no-NAME",
                        crate::manifest::RESERVED_OPTIONS.join(", ")
                    ),
                ));
            }
            let DeValue::Table(o) = v.get_ref() else {
                return Err(self.err(
                    v.span(),
                    format!("[options] {name}: not a table"),
                    format!("{name} = {{ type = \"int\", default = 20, doc = \"...\" }}"),
                ));
            };
            let option = self.option(name, o, v.span(), false)?;
            if let Some(c) = option.short
                && let Some(other) = out.iter().find(|o| o.short == Some(c))
            {
                return Err(self.err(
                    self.span(o, "short"),
                    format!("[options] {name}.short: -{c} is also of {}", other.name),
                    "one option a letter",
                ));
            }
            out.push(option);
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// `[args]`: one entry, its name that of the list in `init`.
    pub(crate) fn positional(&self, t: &DeTable<'_>) -> Result<Positional> {
        let mut entries = t.iter();
        let (Some((k, v)), None) = (entries.next(), entries.next()) else {
            return Err(self.err(
                0..0,
                "[args]: one entry wanted",
                "[args] files = { type = \"file\", min = 0, max = 16, doc = \"...\" }",
            ));
        };
        let name = k.get_ref().as_ref();
        if !is_name(name, 32) {
            return Err(self.err(
                k.span(),
                format!("[args] {name:?}: not a name"),
                "lower-case letters, digits and `-`, a letter first, 32 at most",
            ));
        }
        let DeValue::Table(o) = v.get_ref() else {
            return Err(self.err(
                v.span(),
                format!("[args] {name}: not a table"),
                format!("{name} = {{ type = \"string\", max = 4, doc = \"...\" }}"),
            ));
        };
        let option = self.option(name, o, v.span(), true)?;
        let count = |key: &str, absent: usize| -> Result<usize> {
            match field(o, key) {
                None => Ok(absent),
                Some((_, v)) => match v.get_ref() {
                    DeValue::Integer(i) => u64::from_str_radix(i.as_str(), i.radix())
                        .ok()
                        .map(|n| n as usize)
                        .filter(|n| *n <= MAX_VALUES)
                        .ok_or(()),
                    _ => Err(()),
                }
                .map_err(|()| {
                    self.err(
                        v.span(),
                        format!("[args] {name}.{key}: not an integer of 0 to {MAX_VALUES}"),
                        format!("{key} = 1"),
                    )
                }),
            }
        };
        let (min, max) = (count("min", 0)?, count("max", MAX_VALUES)?);
        if min > max {
            return Err(self.err(
                v.span(),
                format!("[args] {name}: min {min} above max {max}"),
                "min <= max",
            ));
        }
        Ok(Positional { option, min, max })
    }

    fn option(
        &self,
        name: &str,
        o: &DeTable<'_>,
        at: Range<usize>,
        positional: bool,
    ) -> Result<AppOption> {
        let section = match positional {
            true => format!("args.{name}"),
            false => format!("options.{name}"),
        };
        let keys = match positional {
            true => ARGS_KEYS,
            false => OPTION_KEYS,
        };
        self.unknown(o, &section, keys)?;
        let flag = |key: &str| -> Result<bool> {
            match field(o, key) {
                None => Ok(false),
                Some((_, v)) => match v.get_ref() {
                    DeValue::Boolean(b) => Ok(*b),
                    _ => Err(self.err(
                        v.span(),
                        format!("[{section}] {key}: not true or false"),
                        format!("{key} = true"),
                    )),
                },
            }
        };
        let strings = |key: &str, hint: &str| -> Result<Vec<String>> {
            let Some((_, v)) = field(o, key) else {
                return Ok(Vec::new());
            };
            let bad = || {
                self.err(
                    v.span(),
                    format!("[{section}] {key}: not an array of strings"),
                    hint.to_string(),
                )
            };
            let DeValue::Array(items) = v.get_ref() else {
                return Err(bad());
            };
            (items.iter())
                .map(|i| match i.get_ref() {
                    DeValue::String(s) => Ok(s.to_string()),
                    _ => Err(bad()),
                })
                .collect()
        };
        let ty = self.string(o, &section, "type", "\"int\"")?;
        let doc = self.string(o, &section, "doc", "\"what it does\"")?;
        let (Some(ty), Some(doc)) = (ty, doc) else {
            return Err(self.err(
                at,
                format!("[{section}]: type and doc are wanted"),
                format!("{name} = {{ type = \"int\", default = 20, doc = \"...\" }}"),
            ));
        };
        let Some(ty) = OptionType::parse(&ty) else {
            return Err(self.err(
                self.span(o, "type"),
                format!("[{section}] type: {ty:?}"),
                format!("types: {}", types()),
            ));
        };
        if doc.is_empty() || doc.chars().count() > 200 || doc.chars().any(char::is_control) {
            return Err(self.err(
                at,
                format!(
                    "[{section}] doc: empty, longer than 200 characters, or a control character"
                ),
                "one line",
            ));
        }
        let list = positional || flag("list")?;
        if list && ty == OptionType::Bool {
            return Err(self.err(
                at,
                format!("[{section}]: a list of bool"),
                "a bool is given or not; a list: another type",
            ));
        }
        let short = match self.string(o, &section, "short", "\"g\"")? {
            None => None,
            Some(s) => {
                let mut chars = s.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_ascii_alphanumeric() && c != 'h' => Some(c),
                    _ => {
                        return Err(self.err(
                            self.span(o, "short"),
                            format!("[{section}] short: {s:?}"),
                            "one letter or digit, not h (help), as short = \"g\"",
                        ));
                    }
                }
            }
        };
        let choices = strings("choices", "choices = [\"auto\", \"spring\"]")?;
        if !choices.is_empty() {
            let bad = choices.len() > MAX_CHOICES
                || (choices.iter()).any(|c| c.is_empty() || c.len() > 64 || c.starts_with('-'))
                || (choices.iter().enumerate()).any(|(i, c)| choices[..i].contains(c));
            if ty != OptionType::String || bad {
                return Err(self.err(
                    self.span(o, "choices"),
                    format!("[{section}] choices: of a string, 1 to {MAX_CHOICES}, each once"),
                    "choices = [\"auto\", \"spring\"], type = \"string\"",
                ));
            }
        }
        let needs = strings("needs", "needs = [\"keys-arrows\"]")?;
        let write = flag("write")?;
        if write && ty != OptionType::File {
            return Err(self.err(
                self.span(o, "write"),
                format!("[{section}] write: of a file only"),
                "type = \"file\", write = true; a directory is never written",
            ));
        }
        let regex = flag("regex")?;
        if regex && ty != OptionType::Pattern {
            return Err(self.err(
                self.span(o, "regex"),
                format!("[{section}] regex: of a pattern only"),
                "type = \"pattern\", regex = true",
            ));
        }
        let default = match field(o, "default") {
            None => None,
            Some((_, dv)) => {
                let wrong = || {
                    self.err(
                        dv.span(),
                        format!("[{section}] default: not of type {}", ty.as_str()),
                        "a default of the type named",
                    )
                };
                let int = |i: &toml::de::DeInteger<'_>| i64::from_str_radix(i.as_str(), i.radix());
                let value = match (ty, dv.get_ref()) {
                    (OptionType::Int, DeValue::Integer(i)) => {
                        OptionValue::Int(int(i).map_err(|_| {
                            self.err(dv.span(), format!("[{section}] default: out of range"), "")
                        })?)
                    }
                    (OptionType::Float, DeValue::Float(f)) => {
                        OptionValue::Float(f.as_str().parse().map_err(|_| wrong())?)
                    }
                    (OptionType::Float, DeValue::Integer(i)) => {
                        OptionValue::Float(int(i).map_err(|_| wrong())? as f64)
                    }
                    (OptionType::Bool, DeValue::Boolean(b)) => OptionValue::Bool(*b),
                    (_, DeValue::String(s))
                        if !matches!(
                            ty,
                            OptionType::Int | OptionType::Float | OptionType::Bool
                        ) =>
                    {
                        OptionValue::Str(s.to_string())
                    }
                    _ => return Err(wrong()),
                };
                Some(value)
            }
        };
        let option = AppOption {
            name: name.to_string(),
            ty,
            default,
            doc,
            short,
            choices,
            list,
            needs,
            write,
            regex,
        };
        if let Some(d) = &option.default {
            let span = self.span(o, "default");
            if list && !ty.named() {
                return Err(self.err(
                    span,
                    format!("[{section}] default: a list starts empty"),
                    "no default, but of a file or a directory",
                ));
            }
            if let OptionValue::Str(s) = d {
                let checked = match ty.named() {
                    true => plain_name(s),
                    false => item(&option, &json!(s)).map(|_| ()),
                };
                if let Err(why) = checked {
                    return Err(self.err(span, format!("[{section}] default: {why}"), ""));
                }
            }
        } else if !list
            && matches!(
                ty,
                OptionType::Int | OptionType::Float | OptionType::Bool | OptionType::String
            )
        {
            return Err(self.err(
                at,
                format!("[{section}]: a default is wanted"),
                format!(
                    "{name} = {{ type = \"{}\", default = ..., doc = \"...\" }}",
                    ty.as_str()
                ),
            ));
        }
        Ok(option)
    }
}

/// The checks of a manifest across sections: what an option needs is
/// declared; a file asks `files`, a write `files-write`.
pub(crate) fn check_across(m: &Manifest) -> std::result::Result<(), String> {
    let declared = |name: &str| m.permissions.iter().any(|p| p.name == name);
    for o in all(m) {
        let section = section(m, o);
        for need in &o.needs {
            if need.starts_with("limit-") || !declared(need) {
                return Err(format!(
                    "[{section}] needs: {need:?} is not in [permissions]; \
                     declare it there, with its reason"
                ));
            }
        }
        if o.ty.named() && !declared("files") {
            return Err(format!(
                "[{section}]: a {} asks files; add files = \"why\" to [permissions]",
                o.ty.as_str()
            ));
        }
        if o.write && !declared("files-write") {
            return Err(format!(
                "[{section}] write: asks files-write; add files-write = \"why\" to [permissions]"
            ));
        }
    }
    if let Some(p) = &m.args
        && m.options.iter().any(|o| o.name == p.option.name)
    {
        return Err(format!(
            "[args] {}: the name of an option; another name",
            p.option.name
        ));
    }
    Ok(())
}

fn section(m: &Manifest, o: &AppOption) -> String {
    match m.args.as_ref().is_some_and(|p| std::ptr::eq(&p.option, o)) {
        true => format!("args.{}", o.name),
        false => format!("options.{}", o.name),
    }
}

/// Every option, then the positional.
fn all(m: &Manifest) -> impl Iterator<Item = &AppOption> {
    m.options.iter().chain(m.args.iter().map(|p| &p.option))
}

/// A name of a default file: one component of `[A-Za-z0-9._-]`, 1 to
/// 64 bytes, not starting with `.`.
fn plain_name(s: &str) -> std::result::Result<(), String> {
    let ok = (1..=64).contains(&s.len())
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    match ok {
        true => Ok(()),
        false => Err(format!(
            "{s:?}: a plain name of the working directory, as \"TODO.md\""
        )),
    }
}

/// A word of the user: the rule of `words`.
pub fn is_word(s: &str) -> bool {
    WORD_LEN.contains(&s.chars().count()) && s.chars().all(char::is_alphanumeric)
}

/// One value of `o`, as JSON: its type, its rule. Not of a file.
fn item(o: &AppOption, v: &Value) -> std::result::Result<Value, String> {
    let wanted = || format!("{} wanted", o.ty.as_str());
    match o.ty {
        OptionType::Int if v.is_i64() => Ok(v.clone()),
        OptionType::Float if v.as_f64().is_some_and(f64::is_finite) => Ok(v.clone()),
        OptionType::Bool if v.is_boolean() => Ok(v.clone()),
        OptionType::String => {
            let s = v.as_str().ok_or_else(wanted)?;
            if !o.choices.is_empty() && !o.choices.iter().any(|c| c == s) {
                return Err(format!("{s:?}: one of {}", o.choices.join(", ")));
            }
            Ok(v.clone())
        }
        OptionType::Word => {
            let s = v.as_str().ok_or_else(wanted)?;
            match is_word(s) {
                true => Ok(v.clone()),
                false => Err(format!(
                    "{s:?}: a word is 3 to 32 letters or digits, whole; \
                     to find anything else the app must ask text"
                )),
            }
        }
        OptionType::Pattern => {
            let s = v.as_str().ok_or_else(wanted)?;
            match !s.is_empty() && s.len() <= MAX_PATTERN && !s.contains('\0') {
                true => Ok(v.clone()),
                false => Err(format!("a pattern of 1 to {MAX_PATTERN} bytes")),
            }
        }
        _ => Err(wanted()),
    }
}

/// A file or a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Dir,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::File => "file",
            FileKind::Dir => "dir",
        }
    }

    fn of(ty: OptionType) -> FileKind {
        match ty {
            OptionType::Dir => FileKind::Dir,
            _ => FileKind::File,
        }
    }
}

/// A file named at the start: `files` of `app.start`, `{"handle",
/// "path", "kind", "write", "default"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGiven {
    /// `f0`, `f1`, ...: what the guest holds.
    pub handle: String,
    /// Absolute, canonical.
    pub path: PathBuf,
    pub kind: FileKind,
    /// Asked: written back.
    pub write: bool,
    /// The manifest's default, not typed by the user: asked as `files`.
    pub default: bool,
}

impl FileGiven {
    pub fn to_wire(&self) -> Value {
        json!({
            "handle": self.handle,
            "path": self.path.to_string_lossy(),
            "kind": self.kind.as_str(),
            "write": self.write,
            "default": self.default,
        })
    }

    /// The wire form; the path absolute.
    pub fn from_wire(v: &Value) -> std::result::Result<FileGiven, String> {
        let bad = || {
            r#"files: [{"handle": "f0", "path": "/abs", "kind": "file" | "dir", "write": bool}]"#
                .to_string()
        };
        let o = v.as_object().ok_or_else(bad)?;
        if let Some(k) = (o.keys())
            .find(|k| !["handle", "path", "kind", "write", "default"].contains(&k.as_str()))
        {
            return Err(format!("files: no member `{k}`"));
        }
        let handle = o.get("handle").and_then(Value::as_str).ok_or_else(bad)?;
        if !(1..=8).contains(&handle.len())
            || !handle
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(format!("files: handle {handle:?}: 1 to 8 of [a-z0-9]"));
        }
        let path = PathBuf::from(o.get("path").and_then(Value::as_str).ok_or_else(bad)?);
        if !path.is_absolute() {
            return Err("files: path: absolute, made so by the launcher".into());
        }
        let kind = match o.get("kind").and_then(Value::as_str) {
            Some("file") => FileKind::File,
            Some("dir") => FileKind::Dir,
            _ => return Err(bad()),
        };
        let flag = |name: &str| match o.get(name) {
            None | Some(Value::Null) => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(bad()),
        };
        Ok(FileGiven {
            handle: handle.to_string(),
            path,
            kind,
            write: flag("write")?,
            default: flag("default")?,
        })
    }
}

/// `path` is what the launcher may send: absolute, canonical, a regular
/// file or a directory as `kind` says; a file to write may be absent,
/// its directory canonical.
pub fn check_named(path: &Path, kind: FileKind, write: bool) -> std::result::Result<(), String> {
    let shown = path.display();
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let canonical = std::fs::canonicalize(path).ok();
            if canonical.as_deref() != Some(path) {
                return Err(format!("{shown}: not a canonical path"));
            }
            let fits = match kind {
                FileKind::File => meta.file_type().is_file(),
                FileKind::Dir => meta.file_type().is_dir(),
            };
            match fits {
                true => Ok(()),
                false => Err(format!(
                    "{shown}: not a {}; regular files and directories only",
                    match kind {
                        FileKind::File => "regular file",
                        FileKind::Dir => "directory",
                    }
                )),
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && kind == FileKind::File && write => {
            let (Some(parent), Some(_)) = (path.parent(), path.file_name()) else {
                return Err(format!("{shown}: no such file"));
            };
            match std::fs::canonicalize(parent) {
                Ok(p) if p == parent && p.is_dir() => Ok(()),
                _ => Err(format!("{shown}: its directory is not a canonical one")),
            }
        }
        Err(e) => Err(format!("{shown}: {e}")),
    }
}

/// What the user typed, made absolute against `cwd` and canonical. A
/// file to write may not exist yet: its directory does.
pub fn name_path(
    cwd: &Path,
    typed: &str,
    kind: FileKind,
    write: bool,
) -> std::result::Result<PathBuf, String> {
    let joined = cwd.join(typed);
    let path = match std::fs::canonicalize(&joined) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && kind == FileKind::File && write => {
            let name = joined
                .file_name()
                .filter(|n| *n != "." && *n != "..")
                .ok_or_else(|| format!("{typed}: no such file"))?;
            let parent = joined.parent().unwrap_or(cwd);
            std::fs::canonicalize(parent)
                .map_err(|e| format!("{typed}: {e}"))?
                .join(name)
        }
        Err(e) => return Err(format!("{typed}: {e}")),
    };
    check_named(&path, kind, write).map_err(|e| e.replace(&path.display().to_string(), typed))?;
    Ok(path)
}

/// The options of the launcher, the same for every store app
/// (principle 4). The app is not told them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Launcher {
    /// `--frame F`.
    pub frame: Option<String>,
    /// `--front`, `--back`.
    pub front: Option<bool>,
    /// `--prefer P`: one of [`PREFER`].
    pub prefer: Option<String>,
    /// `--alpha A`: `0..=1`.
    pub alpha: Option<f64>,
    /// `--border B`: a border name.
    pub border: Option<String>,
    /// `--wait`, `--no-wait`.
    pub wait: Option<bool>,
}

impl Launcher {
    /// The members of `app.start`: `frame`, `front`, `prefer`, `alpha`,
    /// `border`.
    pub fn members(&self, params: &mut Map<String, Value>) {
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                params.insert(k.into(), v);
            }
        };
        put("frame", self.frame.as_ref().map(|f| json!(f)));
        put("front", self.front.map(|f| json!(f)));
        put("prefer", self.prefer.as_ref().map(|p| json!(p)));
        put("alpha", self.alpha.map(|a| json!(a)));
        put("border", self.border.as_ref().map(|b| json!(b)));
    }
}

/// A border name: `[a-z0-9-]`, 1 to 32.
pub fn is_border_name(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A command line read against a manifest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Launch {
    /// `args` of `app.start`: the values given; a file `{"file":
    /// HANDLE, "name": NAME}`.
    pub args: Map<String, Value>,
    pub files: Vec<FileGiven>,
    pub launcher: Launcher,
}

impl Launch {
    /// The params of `app.start` for the installed app `store`.
    pub fn params(&self, store: &str) -> Value {
        let mut p = Map::new();
        p.insert("store".into(), json!(store));
        p.insert("args".into(), Value::Object(self.args.clone()));
        if !self.files.is_empty() {
            let files: Vec<Value> = self.files.iter().map(FileGiven::to_wire).collect();
            p.insert("files".into(), json!(files));
        }
        self.launcher.members(&mut p);
        Value::Object(p)
    }
}

fn usage(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Usage, message)
}

/// The words after `ptyrc app ID`, read against `m`: the launcher's
/// options, then the app's (`--NAME VALUE`, `--NAME=VALUE`, `-S VALUE`,
/// `-SVALUE`, bools `--NAME`, `--no-NAME`, `-abc`; `NAME=VALUE`, the
/// form of `dev run`), positionals for `[args]`, `--` before
/// positionals only. Files made absolute against `cwd`, a default file
/// of the working directory added. `Ok(None)`: `--help`. Checked as the
/// driver checks it ([`resolve`]).
pub fn read(m: &Manifest, words: &[String], cwd: &Path) -> Result<Option<Launch>> {
    let mut launch = Launch::default();
    let mut given: Vec<(&AppOption, Value)> = Vec::new();
    let mut words = words.iter();
    let mut only_positionals = false;
    let find = |name: &str| m.options.iter().find(|o| o.name == name);
    let shorts = |c: char| m.options.iter().find(|o| o.short == Some(c));
    while let Some(word) = words.next() {
        if !only_positionals && (word == "--help" || word == "-h") {
            return Ok(None);
        }
        if !only_positionals && word == "--" {
            only_positionals = true;
            continue;
        }
        let bare = (!word.starts_with('-'))
            .then(|| word.split_once('='))
            .flatten()
            .filter(|(name, _)| find(name).is_some());
        if only_positionals || (!word.starts_with('-') && bare.is_none()) || word == "-" {
            let p = m.args.as_ref().ok_or_else(|| match word.split_once('=') {
                // `NAME=VALUE`, the form of `dev run`.
                Some((name, _)) if !only_positionals => usage(format!("{name}: no such option")),
                _ => usage(format!("{word}: no such argument; an option is --NAME"))
                    .with_hint("the app takes no positional argument: [args] declares none"),
            })?;
            given.push((&p.option, text_value(&p.option, word)?));
            continue;
        }
        if let Some((name, value)) = bare {
            let o = find(name).expect("filtered");
            given.push((
                o,
                flag_value(o, &format!("{name}="), Some(value.into()), false)?,
            ));
            continue;
        }
        if let Some(flag) = word.strip_prefix("--") {
            let (name, mut inline) = match flag.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (flag, None),
            };
            let mut value = |inline: &mut Option<String>, what: &str| {
                inline
                    .take()
                    .or_else(|| words.next().cloned())
                    .ok_or_else(|| usage(format!("--{name}: {what} is wanted")))
            };
            let l = &mut launch.launcher;
            match name {
                "front" | "back" | "wait" | "no-wait" if inline.is_some() => {
                    return Err(usage(format!("--{name}: takes no value")));
                }
                "front" => l.front = Some(true),
                "back" => l.front = Some(false),
                "wait" => l.wait = Some(true),
                "no-wait" => l.wait = Some(false),
                "frame" => l.frame = Some(value(&mut inline, "a frame: an id or a name")?),
                "prefer" => {
                    let p = value(&mut inline, "a side")?;
                    if !PREFER.contains(&p.as_str()) {
                        return Err(usage(format!("--prefer {p}: one of {}", PREFER.join(", "))));
                    }
                    l.prefer = Some(p);
                }
                "alpha" => {
                    let a = value(&mut inline, "an opacity")?;
                    let x = a.parse::<f64>().ok().filter(|x| (0.0..=1.0).contains(x));
                    l.alpha =
                        Some(x.ok_or_else(|| usage(format!("--alpha {a}: a number of 0 to 1")))?);
                }
                "border" => {
                    let b = value(&mut inline, "a border name")?;
                    if !is_border_name(&b) {
                        return Err(usage(format!("--border {b}: a border name, as rounded"))
                            .with_hint("ptyrc widget borders lists them"));
                    }
                    l.border = Some(b);
                }
                _ => {
                    let (o, off) = match (find(name), name.strip_prefix("no-").and_then(find)) {
                        (Some(o), _) => (o, false),
                        (None, Some(o)) if o.ty == OptionType::Bool => (o, true),
                        _ => return Err(usage(format!("--{name}: no such option"))),
                    };
                    let v = match o.ty {
                        OptionType::Bool => inline.take(),
                        _ => Some(value(&mut inline, &format!("a value ({})", o.ty.as_str()))?),
                    };
                    given.push((o, flag_value(o, &format!("--{name}"), v, off)?));
                }
            }
            continue;
        }
        // `-g`, `-abc`, `-n5`, `-n 5`.
        let cluster = &word[1..];
        for (i, c) in cluster.char_indices() {
            let o = shorts(c).ok_or_else(|| usage(format!("-{c}: no such option")))?;
            if o.ty == OptionType::Bool {
                given.push((o, json!(true)));
                continue;
            }
            let rest = &cluster[i + c.len_utf8()..];
            let v = match rest.is_empty() {
                false => rest.to_string(),
                true => words
                    .next()
                    .cloned()
                    .ok_or_else(|| usage(format!("-{c}: a value ({}) is wanted", o.ty.as_str())))?,
            };
            given.push((o, flag_value(o, &format!("-{c}"), Some(v), false)?));
            break;
        }
    }
    // The values, a list gathered; files named, given handles.
    let mut lists: Map<String, Value> = Map::new();
    for (o, v) in given {
        let v = match o.ty.named() {
            true => {
                let typed = v.as_str().unwrap_or_default().to_string();
                file_value(&mut launch.files, o, cwd, &typed, false)?
            }
            false => v,
        };
        match o.list {
            true => match lists.entry(o.name.clone()).or_insert_with(|| json!([])) {
                Value::Array(a) => a.push(v),
                _ => unreachable!("an array"),
            },
            false => {
                lists.insert(o.name.clone(), v);
            }
        }
    }
    // A default file of the working directory, when none is named.
    for o in all(m) {
        if let (true, Some(OptionValue::Str(name)), false) =
            (o.ty.named(), &o.default, lists.contains_key(&o.name))
        {
            let kind = FileKind::of(o.ty);
            if name_path(cwd, name, kind, o.write).is_err() {
                continue;
            }
            let v = file_value(&mut launch.files, o, cwd, name, true)?;
            lists.insert(o.name.clone(), if o.list { json!([v]) } else { v });
        }
    }
    launch.args = lists;
    let args = Value::Object(launch.args.clone());
    resolve(m, Some(&args), &launch.files)?;
    Ok(Some(launch))
}

/// The value of a flag: a bool from `--NAME`, `--no-NAME`,
/// `--NAME=yes`; else the text, typed.
fn flag_value(o: &AppOption, flag: &str, v: Option<String>, off: bool) -> Result<Value> {
    if o.ty != OptionType::Bool {
        return text_value(o, v.as_deref().unwrap_or_default())
            .map_err(|e| usage(format!("{flag}: {}", e.message)));
    }
    match v.as_deref() {
        None | Some("true" | "yes" | "on") => Ok(json!(!off)),
        Some("false" | "no" | "off") => Ok(json!(off)),
        Some(v) => Err(usage(format!("{flag}={v}: true or false"))),
    }
}

/// A word typed, as JSON of the option's type. A file stays its text.
fn text_value(o: &AppOption, v: &str) -> Result<Value> {
    let value = match o.ty {
        OptionType::Int => v.parse::<i64>().map(|n| json!(n)).ok(),
        OptionType::Float => v
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
            .map(|x| json!(x)),
        OptionType::Bool => match v {
            "true" | "yes" | "on" => Some(json!(true)),
            "false" | "no" | "off" => Some(json!(false)),
            _ => None,
        },
        _ => Some(json!(v)),
    };
    let value = value.ok_or_else(|| usage(format!("{v}: {} wanted", o.ty.as_str())))?;
    if o.ty.named() {
        return Ok(value);
    }
    item(o, &value).map_err(|why| usage(format!("{}: {why}", o.name)))
}

/// A file named: its handle added to `files`; the value the guest gets.
fn file_value(
    files: &mut Vec<FileGiven>,
    o: &AppOption,
    cwd: &Path,
    typed: &str,
    default: bool,
) -> Result<Value> {
    if files.len() >= MAX_FILES {
        return Err(usage(format!("{MAX_FILES} files at most a run")));
    }
    let kind = FileKind::of(o.ty);
    let path =
        name_path(cwd, typed, kind, o.write).map_err(|e| usage(format!("{}: {e}", o.name)))?;
    let handle = format!("f{}", files.len());
    files.push(FileGiven {
        handle: handle.clone(),
        path,
        kind,
        write: o.write,
        default,
    });
    Ok(json!({"file": handle, "name": shown_name(typed)}))
}

/// The last component of what was typed, for the eye of the guest.
fn shown_name(typed: &str) -> String {
    let last = typed.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let last: String = last.chars().filter(|c| !c.is_control()).take(255).collect();
    match last.as_str() {
        "" | "." | ".." => "dir".to_string(),
        _ => last,
    }
}

/// A file a run holds, as the driver keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handle {
    pub handle: String,
    pub path: PathBuf,
    pub kind: FileKind,
    /// The manifest says `write`; `files-write` decides.
    pub write: bool,
}

/// A pattern of the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPattern {
    pub pattern: String,
    pub regex: bool,
}

/// `args` and `files` of `app.start`, checked against the manifest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolved {
    /// The argument of `init`: every option, its default when not
    /// given (`null` when it has none, `[]` for a list), the
    /// positionals under the name of `[args]`.
    pub init: Map<String, Value>,
    /// Permissions asked by what was given: `needs`; `files` for a
    /// default file; `files-write` for a file to write. Sorted.
    pub needs: Vec<String>,
    /// The user's words: held for the run.
    pub words: Vec<String>,
    /// The user's patterns: `screen.find` held to them.
    pub patterns: Vec<UserPattern>,
    pub files: Vec<Handle>,
}

impl Resolved {
    /// The permissions of the manifest asked for this run: those not
    /// tied to an option (by `needs`, or `files` and `files-write` to a
    /// file option), and those the options given need.
    pub fn asked<'m>(&self, m: &'m Manifest) -> Vec<&'m str> {
        let tied = |name: &str| {
            all(m).any(|o| o.needs.iter().any(|n| n == name))
                || (matches!(name, "files" | "files-write") && all(m).any(|o| o.ty.named()))
        };
        (m.permissions.iter())
            .map(|p| p.name.as_str())
            .filter(|n| !tied(n) || self.needs.iter().any(|x| x == n))
            .collect()
    }
}

/// `args` (`{NAME: VALUE}`) and `files` of `app.start`, as the driver
/// takes them: a name not declared, a value of another type or outside
/// its rule, a handle not given or given twice: refused.
pub fn resolve(m: &Manifest, args: Option<&Value>, files: &[FileGiven]) -> Result<Resolved> {
    let names = || {
        let names: Vec<&str> = all(m).map(|o| o.name.as_str()).collect();
        match names.is_empty() {
            true => "the app has no option".to_string(),
            false => format!("options: {}", names.join(", ")),
        }
    };
    let empty = Map::new();
    let given = match args {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(o)) => o,
        Some(_) => return Err(usage("args: an object, {OPTION: VALUE}")),
    };
    if files.len() > MAX_FILES {
        return Err(usage(format!("files: {MAX_FILES} at most a run")));
    }
    let mut out = Resolved::default();
    let mut used: Vec<&str> = Vec::new();
    for (name, value) in given {
        let o = all(m)
            .find(|o| &o.name == name)
            .ok_or_else(|| usage(format!("args: no option `{name}`")).with_hint(names()))?;
        let bad = |why: String| usage(format!("args: {name}: {why}"));
        let values: Vec<&Value> = match (o.list, value) {
            (_, Value::Null) => continue,
            (true, Value::Array(a)) => a.iter().collect(),
            (true, _) => return Err(bad(format!("an array of {}", o.ty.as_str()))),
            (false, v) => vec![v],
        };
        let positional = m.args.as_ref().filter(|p| p.option.name == *name);
        let (min, max) = positional.map_or((0, MAX_VALUES), |p| (p.min, p.max));
        if values.len() > max || values.len() < min {
            return Err(bad(format!(
                "{min} to {max} values, {} given",
                values.len()
            )));
        }
        for v in values {
            if o.ty.named() {
                let h = v
                    .get("file")
                    .and_then(Value::as_str)
                    .filter(|_| v.get("name").is_some_and(Value::is_string))
                    .ok_or_else(|| bad(r#"{"file": HANDLE, "name": NAME}"#.into()))?;
                let f = (files.iter().find(|f| f.handle == h))
                    .ok_or_else(|| bad(format!("handle {h:?}: not in files")))?;
                if used.contains(&h) {
                    return Err(bad(format!("handle {h:?}: named twice")));
                }
                if f.kind != FileKind::of(o.ty) || (f.write && !o.write) {
                    return Err(bad(format!(
                        "handle {h:?}: a {} {}",
                        o.ty.as_str(),
                        if o.write { "to write" } else { "to read" }
                    )));
                }
                used.push(h);
                if f.default {
                    out.needs.push("files".into());
                }
                if o.write {
                    out.needs.push("files-write".into());
                }
                out.files.push(Handle {
                    handle: f.handle.clone(),
                    path: f.path.clone(),
                    kind: f.kind,
                    write: o.write,
                });
                continue;
            }
            item(o, v).map_err(bad)?;
            match o.ty {
                OptionType::Word => {
                    let w = v.as_str().expect("checked");
                    if !out
                        .words
                        .iter()
                        .any(|x| x.to_lowercase() == w.to_lowercase())
                    {
                        out.words.push(w.to_string());
                    }
                }
                OptionType::Pattern => {
                    let p = UserPattern {
                        pattern: v.as_str().expect("checked").to_string(),
                        regex: o.regex,
                    };
                    if !out.patterns.contains(&p) {
                        out.patterns.push(p);
                    }
                    if out.patterns.len() > MAX_PATTERNS {
                        return Err(bad(format!("{MAX_PATTERNS} patterns at most a run")));
                    }
                }
                _ => {}
            }
        }
        let on = match value {
            Value::Bool(b) => *b,
            Value::Array(a) => !a.is_empty(),
            _ => true,
        };
        if on {
            out.needs.extend(o.needs.iter().cloned());
        }
        out.init.insert(name.clone(), value.clone());
    }
    if let Some(f) = files.iter().find(|f| !used.contains(&f.handle.as_str())) {
        return Err(usage(format!(
            "files: handle {:?}: named by no option",
            f.handle
        )));
    }
    for o in all(m) {
        if out.init.contains_key(&o.name) {
            continue;
        }
        if let Some(p) = m.args.as_ref().filter(|p| p.option.name == o.name)
            && p.min > 0
        {
            return Err(usage(format!("{}: {} at least", o.name, p.min)));
        }
        let v = match (&o.default, o.list) {
            (_, true) => json!([]),
            (Some(d), false) if !o.ty.named() => d.to_json(),
            _ => Value::Null,
        };
        // A bool true by default asks what it needs.
        if v == Value::Bool(true) {
            out.needs.extend(o.needs.iter().cloned());
        }
        out.init.insert(o.name.clone(), v);
    }
    out.needs.sort();
    out.needs.dedup();
    Ok(out)
}

/// `--help` of a store app: `program` its command (`ptyrc app ID`).
pub fn help(m: &Manifest, program: &str) -> String {
    let positional = m.args.as_ref().map(|p| {
        let name = p.option.name.to_uppercase();
        match (p.min, p.max) {
            (0, 1) => format!(" [{name}]"),
            (1, 1) => format!(" {name}"),
            (0, _) => format!(" [{name}...]"),
            _ => format!(" {name}..."),
        }
    });
    let mut text = format!(
        "usage: {program} [OPTION...]{}\n\n{}\n",
        positional.unwrap_or_default(),
        m.app.summary
    );
    let line = |text: &mut String, flag: String, doc: &str| {
        text.push_str(&format!("  {flag:<24} {doc}\n"));
    };
    let describe = |o: &AppOption| {
        let mut doc = o.doc.clone();
        if !o.choices.is_empty() {
            doc.push_str(&format!(" ({})", o.choices.join(", ")));
        }
        match &o.default {
            Some(OptionValue::Bool(false)) | None => {}
            Some(d) => doc.push_str(&format!(" [default: {}]", d.text())),
        }
        if o.list && m.args.as_ref().is_none_or(|p| p.option.name != o.name) {
            doc.push_str(" [repeated]");
        }
        if !o.needs.is_empty() {
            doc.push_str(&format!(" [asks: {}]", o.needs.join(", ")));
        }
        doc
    };
    if let Some(p) = &m.args {
        text.push_str("\narguments:\n");
        line(
            &mut text,
            p.option.name.to_uppercase(),
            &describe(&p.option),
        );
    }
    if !m.options.is_empty() {
        text.push_str("\noptions:\n");
    }
    for o in &m.options {
        let short = o.short.map(|c| format!("-{c}, ")).unwrap_or_default();
        let value = match o.ty {
            OptionType::Bool => String::new(),
            ty => format!(" {}", ty.metavar()),
        };
        line(
            &mut text,
            format!("{short}--{}{value}", o.name),
            &describe(o),
        );
    }
    if m.options.iter().any(|o| o.ty == OptionType::Bool) {
        line(&mut text, "--no-NAME".into(), "a yes-or-no option, off");
    }
    text.push_str("\noptions of ptyrc, for every app:\n");
    for (flag, doc) in [
        ("--frame F", "the frame it draws on [default: active]"),
        ("--front, --back", "in front of the text, or behind it"),
        ("--prefer SIDE", "where its widgets go: top-right, ..., any"),
        ("--alpha A", "the opacity of what it draws, 0 to 1"),
        ("--border B", "the border of its widgets"),
    ] {
        line(&mut text, flag.into(), doc);
    }
    let wait = if m.app.wait { "--wait" } else { "--no-wait" };
    line(
        &mut text,
        "--wait, --no-wait".into(),
        &format!("wait for its end, print its result [default: {wait}]"),
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::parse;

    const HEAD: &str = "[app]\nid = \"a\"\nversion = \"1\"\nsummary = \"An app.\"\n\
                        license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n";

    fn manifest(rest: &str) -> Manifest {
        parse(&format!("{HEAD}{rest}")).unwrap()
    }

    fn refused(rest: &str) -> String {
        parse(&format!("{HEAD}{rest}")).unwrap_err().message
    }

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ptyrc-options-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    const SNAKE: &str = "[permissions]\nshape = \"the walls\"\nkeys-arrows = \"steering\"\n\
         text = \"all of it\"\n\
         [options]\n\
         grab = { type = \"bool\", default = false, short = \"g\", needs = [\"keys-arrows\"], doc = \"steer\" }\n\
         all = { type = \"bool\", default = false, short = \"a\", needs = [\"text\"], doc = \"all\" }\n\
         rate = { type = \"int\", default = 5, short = \"r\", doc = \"speed\" }\n\
         season = { type = \"string\", default = \"auto\", choices = [\"auto\", \"spring\"], doc = \"when\" }\n\
         word = { type = \"word\", doc = \"to find\" }\n\
         mark = { type = \"pattern\", list = true, regex = true, doc = \"to mark\" }\n";

    #[test]
    fn the_new_keys() {
        let m = manifest(SNAKE);
        let grab = m.options.iter().find(|o| o.name == "grab").unwrap();
        assert_eq!(
            (grab.short, grab.needs.clone()),
            (Some('g'), vec!["keys-arrows".to_string()])
        );
        let mark = m.options.iter().find(|o| o.name == "mark").unwrap();
        assert!(mark.list && mark.regex && mark.default.is_none());
        assert_eq!(mark.type_name(), "pattern list");
    }

    #[test]
    fn what_dev_check_refuses() {
        let opt = |o: &str| format!("[options]\n{o}\n");
        for (rest, said) in [
            (opt("g = { type = \"bool\", default = false, needs = [\"shape\"], doc = \"d\" }"), "needs"),
            (opt("f = { type = \"file\", doc = \"d\" }"), "asks files"),
            (
                "[permissions]\nfiles = \"w\"\n[options]\nf = { type = \"file\", write = true, doc = \"d\" }".into(),
                "files-write",
            ),
            (opt("d = { type = \"dir\", write = true, doc = \"d\" }"), "of a file only"),
            (opt("s = { type = \"int\", default = 1, choices = [\"a\"], doc = \"d\" }"), "choices"),
            (opt("s = { type = \"string\", default = \"b\", choices = [\"a\"], doc = \"d\" }"), "one of a"),
            (opt("w = { type = \"word\", default = \"no way\", doc = \"d\" }"), "a word is"),
            (opt("b = { type = \"bool\", list = true, doc = \"d\" }"), "list of bool"),
            (opt("n = { type = \"int\", list = true, default = 1, doc = \"d\" }"), "starts empty"),
            (opt("n = { type = \"int\", doc = \"d\" }"), "a default is wanted"),
            (opt("x = { type = \"int\", default = 1, short = \"h\", doc = \"d\" }"), "short"),
            (opt("border = { type = \"int\", default = 1, doc = \"d\" }"), "of ptyrc's own"),
            (opt("no-x = { type = \"int\", default = 1, doc = \"d\" }"), "no-NAME"),
            (
                opt("a = { type = \"int\", default = 1, short = \"x\", doc = \"d\" }\n\
                     b = { type = \"int\", default = 1, short = \"x\", doc = \"d\" }"),
                "also of a",
            ),
            (opt("p = { type = \"regex\", doc = \"d\" }"), "type: \"regex\""),
            ("[args]\na = { type = \"word\", doc = \"d\" }\nb = { type = \"word\", doc = \"d\" }".into(), "one entry"),
            ("[args]\na = { type = \"word\", min = 3, max = 2, doc = \"d\" }".into(), "min 3 above max 2"),
            ("[args]\na = { type = \"word\", max = 17, doc = \"d\" }".into(), "0 to 16"),
            (
                "[options]\na = { type = \"int\", default = 1, doc = \"d\" }\n[args]\na = { type = \"word\", doc = \"d\" }".into(),
                "the name of an option",
            ),
            (
                "[permissions]\nfiles = \"f\"\n[options]\nf = { type = \"file\", default = \"../x\", doc = \"d\" }".into(),
                "a plain name",
            ),
        ] {
            let message = refused(&rest);
            assert!(message.contains(said), "{rest}: {message}");
        }
    }

    #[test]
    fn a_command_line() {
        let m = manifest(SNAKE);
        let cwd = scratch("line");
        let read = |text: &str| read(&m, &words(text), &cwd);
        let args = |text: &str| Value::Object(read(text).unwrap().unwrap().args);
        assert_eq!(args(""), json!({}));
        assert_eq!(
            args("-g -r 3 --season spring --word Fish --mark a+ --mark b"),
            json!({"grab": true, "rate": 3, "season": "spring", "word": "Fish", "mark": ["a+", "b"]})
        );
        assert_eq!(args("-gr7"), json!({"grab": true, "rate": 7}));
        assert_eq!(args("-ga"), json!({"grab": true, "all": true}));
        assert_eq!(args("--no-grab rate=2"), json!({"grab": false, "rate": 2}));
        assert_eq!(read("-r 1 --help").unwrap(), None);
        for bad in [
            "--season winter",
            "--word ab",
            "--word two-words",
            "--rate x",
            "--rate",
            "-x",
            "--speed 3",
            "--no-rate",
            "fish",
            "--front=yes",
            "--alpha 2",
            "--prefer middle",
            "--border Bad",
        ] {
            assert!(read(bad).is_err(), "{bad}");
        }
        let l = read("--front --frame 2 --prefer top --alpha 0.5 --border rounded --no-wait")
            .unwrap()
            .unwrap();
        assert_eq!(
            l.launcher,
            Launcher {
                frame: Some("2".into()),
                front: Some(true),
                prefer: Some("top".into()),
                alpha: Some(0.5),
                border: Some("rounded".into()),
                wait: Some(false),
            }
        );
        let p = l.params("example.org/a/b");
        assert_eq!(p["front"], json!(true));
        assert_eq!(p["store"], json!("example.org/a/b"));
        assert!(p.get("files").is_none());
        assert_eq!(read("--back").unwrap().unwrap().launcher.front, Some(false));
    }

    #[test]
    fn needs_only_when_given() {
        let m = manifest(SNAKE);
        let r = |args: Value| resolve(&m, Some(&args), &[]).unwrap();
        let none = r(json!({}));
        assert!(none.needs.is_empty());
        assert_eq!(none.asked(&m), ["shape"]);
        assert_eq!(none.init["rate"], json!(5));
        assert_eq!(none.init["word"], Value::Null);
        assert_eq!(none.init["mark"], json!([]));
        let grab = r(json!({"grab": true}));
        assert_eq!(grab.needs, ["keys-arrows"]);
        assert_eq!(grab.asked(&m), ["keys-arrows", "shape"]);
        assert!(r(json!({"grab": false})).needs.is_empty());
        let all = r(json!({"all": true, "word": "flower", "mark": ["x+"]}));
        assert_eq!(all.needs, ["text"]);
        assert_eq!(all.words, ["flower"]);
        assert_eq!(
            all.patterns,
            [UserPattern {
                pattern: "x+".into(),
                regex: true
            }]
        );
        for bad in [
            json!({"speed": 1}),
            json!({"rate": "3"}),
            json!({"word": "no way"}),
            json!({"mark": "x"}),
            json!({"mark": [""]}),
            json!({"season": "winter"}),
            json!([1]),
        ] {
            assert!(resolve(&m, Some(&bad), &[]).is_err(), "{bad}");
        }
        let many: Vec<String> = (0..17).map(|i| format!("p{i}")).collect();
        assert!(resolve(&m, Some(&json!({"mark": many})), &[]).is_err());
        // A bool true by default asks what it needs.
        let on = manifest(
            "[permissions]\nshape = \"s\"\n[options]\n\
             g = { type = \"bool\", default = true, needs = [\"shape\"], doc = \"d\" }\n",
        );
        assert_eq!(resolve(&on, None, &[]).unwrap().needs, ["shape"]);
        assert!(
            resolve(&on, Some(&json!({"g": false})), &[])
                .unwrap()
                .asked(&on)
                .is_empty()
        );
    }

    const NOTE: &str = "[permissions]\nfiles = \"the files named\"\nfiles-write = \"a box ticked\"\n\
         [options]\nstop = { type = \"file\", doc = \"words left out\" }\n\
         dir = { type = \"dir\", list = true, doc = \"pictures\" }\n\
         [args]\nfiles = { type = \"file\", write = true, default = \"TODO.md\", max = 4, doc = \"lists\" }\n";

    #[test]
    fn files_named() {
        let m = manifest(NOTE);
        let cwd = scratch("files");
        std::fs::write(cwd.join("stop.txt"), "a").unwrap();
        std::fs::create_dir(cwd.join("pics")).unwrap();
        let read = |text: &str| read(&m, &words(text), &cwd);
        // The default, of the working directory: absent, written: named.
        let l = read("").unwrap().unwrap();
        assert_eq!(l.args["files"], json!([{"file": "f0", "name": "TODO.md"}]));
        assert_eq!(
            l.files,
            [FileGiven {
                handle: "f0".into(),
                path: cwd.join("TODO.md"),
                kind: FileKind::File,
                write: true,
                default: true,
            }]
        );
        let r = resolve(&m, Some(&Value::Object(l.args.clone())), &l.files).unwrap();
        assert_eq!(r.needs, ["files", "files-write"]);
        assert_eq!(r.asked(&m), ["files", "files-write"]);
        // Typed by the user: `files` not asked.
        let l = read("--stop stop.txt --dir pics --dir . a.md")
            .unwrap()
            .unwrap();
        assert_eq!(l.args["stop"], json!({"file": "f0", "name": "stop.txt"}));
        assert_eq!(
            l.args["dir"],
            json!([{"file": "f1", "name": "pics"}, {"file": "f2", "name": "dir"}])
        );
        assert_eq!(l.files[1].path, cwd.join("pics"));
        assert_eq!(l.files[2].path, cwd);
        let r = resolve(&m, Some(&Value::Object(l.args.clone())), &l.files).unwrap();
        assert_eq!(r.needs, ["files-write"]);
        assert_eq!(r.files.len(), 4);
        let of = |h: &str| r.files.iter().find(|f| f.handle == h).unwrap();
        assert!(!of("f0").write && of("f3").write);
        // Only what is read: nothing asked.
        let only = manifest(
            "[permissions]\nfiles = \"f\"\n[options]\nstop = { type = \"file\", doc = \"d\" }\n",
        );
        let l = super::read(&only, &words("--stop stop.txt"), &cwd)
            .unwrap()
            .unwrap();
        let r = resolve(&only, Some(&Value::Object(l.args)), &l.files).unwrap();
        assert!(r.needs.is_empty() && r.asked(&only).is_empty());
        // Refused: absent to read, a directory as a file, a FIFO.
        assert!(read("--stop nothing.txt").is_err());
        assert!(read("--stop pics").is_err());
        assert!(read("--dir stop.txt").is_err());
        let fifo = cwd.join("fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(read("--stop fifo").is_err());
        assert!(read("a b c d e").is_err(), "max 4");
        // The driver's side: handles checked.
        let given = |h: &str| FileGiven {
            handle: h.into(),
            path: cwd.join("stop.txt"),
            kind: FileKind::File,
            write: false,
            default: false,
        };
        let ok = json!({"stop": {"file": "f0", "name": "x"}});
        assert!(resolve(&m, Some(&ok), &[given("f0")]).is_ok());
        assert!(
            resolve(&m, Some(&ok), &[given("f1")]).is_err(),
            "not in files"
        );
        assert!(
            resolve(&m, Some(&json!({})), &[given("f0")]).is_err(),
            "named by none"
        );
        let twice =
            json!({"stop": {"file": "f0", "name": "x"}, "files": [{"file": "f0", "name": "x"}]});
        assert!(resolve(&m, Some(&twice), &[given("f0")]).is_err());
        let mut w = given("f0");
        w.write = true;
        assert!(
            resolve(&m, Some(&ok), &[w]).is_err(),
            "write not in the manifest"
        );
        assert!(resolve(&m, Some(&json!({"stop": "/etc/passwd"})), &[]).is_err());
        let wire = FileGiven::from_wire(&given("f0").to_wire()).unwrap();
        assert_eq!(wire, given("f0"));
        assert!(
            FileGiven::from_wire(&json!({"handle": "f0", "path": "rel", "kind": "file"})).is_err()
        );
        assert!(check_named(&cwd.join("pics/../stop.txt"), FileKind::File, false).is_err());
        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn positionals() {
        let m = manifest(
            "[args]\npatterns = { type = \"pattern\", min = 1, max = 3, doc = \"what to mark\" }\n",
        );
        let cwd = scratch("pos");
        let read = |text: &str| read(&m, &words(text), &cwd);
        let l = read("TODO -- --x").unwrap().unwrap();
        assert_eq!(l.args["patterns"], json!(["TODO", "--x"]));
        assert!(read("").is_err(), "min 1");
        assert!(read("a b c d").is_err(), "max 3");
        let r = resolve(&m, Some(&json!({"patterns": ["a"]})), &[]).unwrap();
        assert_eq!(r.patterns[0].pattern, "a");
        assert!(!r.patterns[0].regex);
        let h = help(&m, "ptyrc app a");
        assert!(
            h.starts_with("usage: ptyrc app a [OPTION...] PATTERNS..."),
            "{h}"
        );
        assert!(h.contains("--front, --back"));
        let _ = std::fs::remove_dir_all(&cwd);
    }

    #[test]
    fn help_of_the_manifest() {
        let h = help(&manifest(SNAKE), "ptyrc app a");
        for said in [
            "-g, --grab",
            "[asks: keys-arrows]",
            "-r, --rate N",
            "speed [default: 5]",
            "(auto, spring)",
            "--mark PATTERN",
            "[repeated]",
            "--no-NAME",
            "--border B",
        ] {
            assert!(h.contains(said), "{said}: {h}");
        }
    }
}
