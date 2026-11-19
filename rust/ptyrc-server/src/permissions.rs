// SPDX-License-Identifier: GPL-3.0-or-later
//! What an app declares, what the user answered (`permissions.toml`),
//! and the grants of a start. See `plan/design.md`, "Permissions".
//!
//! The file, in `$XDG_CONFIG_HOME/ptyrc/`, else `~/.config/ptyrc/`;
//! written by the driver alone, mode 0600, its directory 0700:
//!
//! ```toml
//! [app."native/flower"]
//! allow = ["shape", "words"]
//! deny = ["text"]
//! ```
//!
//! Read by a parser of this form alone: a table `[app."KEY"]` per
//! identity (`Identity::key`), `allow` and `deny` as arrays of strings
//! on one line, `#` comments.

use crate::grants::{Grants, Permission};
use ptyrc_proto::RpcError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const FILE: &str = "permissions.toml";

/// What an app declares at its start (`app.start {declares}`): each
/// permission with its reason, and the words of `words`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Declared {
    pub permissions: BTreeMap<Permission, String>,
    pub words: Vec<String>,
}

impl Declared {
    /// `{"permissions": {NAME: REASON}, "words": [WORD]}`; both members
    /// optional. A name not known: `bad-params`.
    pub fn from_wire(value: &Value) -> Result<Declared, RpcError> {
        let bad = |what: &str| {
            RpcError::bad_params(format!("declares: {what}"))
                .with_hint(r#"as in {"permissions": {"shape": "a reason"}, "words": ["fish"]}"#)
        };
        let object = value.as_object().ok_or_else(|| bad("an object"))?;
        if let Some(member) = object
            .keys()
            .find(|k| !["permissions", "words"].contains(&k.as_str()))
        {
            return Err(bad(&format!("no member `{member}`")));
        }
        let mut declared = Declared::default();
        if let Some(permissions) = object.get("permissions").filter(|v| !v.is_null()) {
            let permissions = permissions
                .as_object()
                .ok_or_else(|| bad("permissions: {NAME: REASON}"))?;
            for (name, why) in permissions {
                let permission = Permission::parse(name).ok_or_else(|| {
                    RpcError::bad_params(format!("declares: no permission `{name}`")).with_hint(
                        format!(
                            "permissions: {}",
                            Permission::ALL
                                .iter()
                                .map(|p| p.name())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    )
                })?;
                let why = why.as_str().ok_or_else(|| bad("a reason: a string"))?;
                declared.permissions.insert(permission, why.to_string());
            }
        }
        if let Some(words) = object.get("words").filter(|v| !v.is_null()) {
            declared.words = (words.as_array().into_iter().flatten())
                .map(|w| w.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .filter(|_| words.is_array())
                .ok_or_else(|| bad("words: [WORD], strings"))?;
        }
        Ok(declared)
    }
}

/// The answers of the user for one identity. A permission in neither
/// has no answer yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Entry {
    pub allow: BTreeSet<Permission>,
    pub deny: BTreeSet<Permission>,
}

/// The file: an entry per identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Answers {
    pub apps: BTreeMap<String, Entry>,
}

/// The grants of a start, and what has no answer yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub grants: Grants,
    /// Declared, answered neither yes nor no: what the pop-up asks.
    pub missing: Vec<Permission>,
}

/// The grants of `declared`, from the answers of the file. A yes:
/// held; a no: not held, and not asked again; no answer: `missing`,
/// granted only when `headless` (no pop-up can be shown). What is not
/// declared is never held, whatever the file says. A degree of the
/// screen is held only with every declared degree below it.
pub(crate) fn resolve(declared: &Declared, entry: Option<&Entry>, headless: bool) -> Resolved {
    let mut held = BTreeSet::new();
    let mut missing = Vec::new();
    for &permission in declared.permissions.keys() {
        let deny = entry.is_some_and(|e| e.deny.contains(&permission));
        let allow = entry.is_some_and(|e| e.allow.contains(&permission));
        if deny {
            continue;
        }
        if !allow {
            missing.push(permission);
            if !headless {
                continue;
            }
        }
        held.insert(permission);
    }
    // The ladder: a no to a degree is a no to those above it.
    let rungs = || declared.permissions.keys().filter(|p| p.degree().is_some());
    let lowest_lacking = rungs()
        .filter(|p| !held.contains(p))
        .filter_map(|p| p.degree())
        .min();
    if let Some(lacking) = lowest_lacking {
        held.retain(|p| p.degree().is_none_or(|d| d < lacking));
    }
    let words = held.contains(&Permission::Words);
    let mut grants = Grants::of(held);
    if words {
        grants.words = declared.words.clone();
    }
    Resolved { grants, missing }
}

/// The grants of a start: the answers of the file; what has none goes
/// to the pop-up (`without_popup` until it exists).
pub(crate) fn at_start(declared: &Declared, entry: Option<&Entry>, headless: bool) -> Grants {
    let resolved = resolve(declared, entry, headless);
    match resolved.missing.is_empty() || headless {
        true => resolved.grants,
        false => without_popup(declared, entry),
    }
}

/// The pop-up, until it is made: a start with permissions not yet
/// answered is a headless one: what has no answer is granted as
/// declared, nothing is written. The pop-up replaces this: it asks
/// for `Resolved::missing`, writes the answers (`remember`), and starts
/// with `resolve` of the file then.
fn without_popup(declared: &Declared, entry: Option<&Entry>) -> Grants {
    resolve(declared, entry, true).grants
}

/// The directory of the file: `$XDG_CONFIG_HOME/ptyrc`, else
/// `$HOME/.config/ptyrc`. A relative path is not one.
pub(crate) fn dir() -> Option<PathBuf> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = absolute("XDG_CONFIG_HOME").or_else(|| Some(absolute("HOME")?.join(".config")))?;
    Some(base.join("ptyrc"))
}

/// The answers of the file in `dir`, and a line per thing ignored. No
/// file, or one not readable: none.
pub(crate) fn load(dir: &Path) -> (Answers, Vec<String>) {
    match fs::read_to_string(dir.join(FILE)) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Answers::default(), Vec::new()),
        Err(e) => (Answers::default(), vec![format!("{FILE}: {e}")]),
    }
}

/// Write the answers of `key` to the file in `dir`: read again, as
/// another driver may have written since; replaced whole, by a rename.
/// What the pop-up calls.
#[allow(dead_code)]
pub(crate) fn remember(dir: &Path, key: &str, entry: Entry) -> io::Result<()> {
    let (mut answers, _) = load(dir);
    answers.apps.insert(key.to_string(), entry);
    save(dir, &answers)
}

/// Write `answers` to the file in `dir`: a file of its own first, mode
/// 0600, then renamed over it; a reader sees the old file or the new
/// one, never a part. The directory is made 0700.
pub(crate) fn save(dir: &Path, answers: &Answers) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let meta = fs::metadata(dir)?;
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(dir, Permissions::from_mode(0o700))?;
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let part = dir.join(format!(".{FILE}.{}.{n}", std::process::id()));
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&part)?;
        file.write_all(render(answers).as_bytes())?;
        file.sync_all()?;
        fs::rename(&part, dir.join(FILE))
    })();
    if written.is_err() {
        let _ = fs::remove_file(&part);
    }
    written
}

/// The text of the file.
pub(crate) fn render(answers: &Answers) -> String {
    let list = |set: &BTreeSet<Permission>| {
        let names: Vec<String> = set.iter().map(|p| format!("\"{}\"", p.name())).collect();
        format!("[{}]", names.join(", "))
    };
    let mut out = String::from("# Written by the ptyrc driver: the answers of each app.\n");
    for (key, entry) in &answers.apps {
        out.push_str(&format!("\n[app.{}]\n", quote(key)));
        out.push_str(&format!("allow = {}\n", list(&entry.allow)));
        out.push_str(&format!("deny = {}\n", list(&entry.deny)));
    }
    out
}

/// A TOML basic string.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A TOML basic string at the start of `text`: its value and the rest.
fn unquote(text: &str) -> Option<(String, &str)> {
    let mut chars = text.strip_prefix('"')?.char_indices();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &text[i + 2..])),
            '\\' => match chars.next()?.1 {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next().map(|c| c.1)).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// `["a", "b"]`, alone on the rest of a line but for a comment.
fn strings(text: &str) -> Option<Vec<String>> {
    let mut rest = text.trim_start().strip_prefix('[')?.trim_start();
    let mut out = Vec::new();
    loop {
        if let Some(after) = rest.strip_prefix(']') {
            let after = after.trim();
            return (after.is_empty() || after.starts_with('#')).then_some(out);
        }
        let (word, after) = unquote(rest)?;
        out.push(word);
        rest = after.trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
    }
}

/// The answers of `text`, and a line per thing ignored: a line not of
/// this form, a table other than `app`, a member other than `allow`
/// and `deny`, a name not known. A permission both allowed and denied
/// is denied.
pub(crate) fn parse(text: &str) -> (Answers, Vec<String>) {
    let mut answers = Answers::default();
    let mut warnings = Vec::new();
    let mut key: Option<String> = None;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        let mut warn = |what: String| warnings.push(format!("{FILE}:{}: {what}, ignored", n + 1));
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(table) = line.strip_prefix('[') {
            key = (table.strip_prefix("app.").and_then(unquote))
                .filter(|(_, rest)| rest.trim() == "]" || rest.trim_start().starts_with("] #"))
                .map(|(k, _)| k);
            match &key {
                Some(k) => {
                    answers.apps.entry(k.clone()).or_default();
                }
                None => warn("not a table `[app.\"KEY\"]`".into()),
            }
            continue;
        }
        let Some(k) = &key else {
            warn("outside a table `[app.\"KEY\"]`".into());
            continue;
        };
        let Some((member, value)) = line.split_once('=') else {
            warn("not `allow = [...]` nor `deny = [...]`".into());
            continue;
        };
        let member = member.trim();
        if member != "allow" && member != "deny" {
            warn(format!("no member `{member}`"));
            continue;
        }
        let Some(names) = strings(value) else {
            warn(format!("`{member}`: not an array of strings on one line"));
            continue;
        };
        let entry = answers.apps.get_mut(k).expect("made at its table");
        for name in names {
            match Permission::parse(&name) {
                Some(p) if member == "allow" => {
                    entry.allow.insert(p);
                }
                Some(p) => {
                    entry.deny.insert(p);
                }
                None => warn(format!("no permission `{name}`")),
            }
        }
    }
    for entry in answers.apps.values_mut() {
        let deny = entry.deny.clone();
        entry.allow.retain(|p| !deny.contains(p));
    }
    (answers, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grants::Degree;
    use Permission::*;
    use serde_json::json;

    fn declared(permissions: &[Permission]) -> Declared {
        Declared {
            permissions: permissions
                .iter()
                .map(|p| (*p, "why".to_string()))
                .collect(),
            words: vec!["fish".into()],
        }
    }

    fn entry(allow: &[Permission], deny: &[Permission]) -> Entry {
        Entry {
            allow: allow.iter().copied().collect(),
            deny: deny.iter().copied().collect(),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("ptyrc-perm-{tag}-{}", std::process::id()))
            .join("ptyrc");
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_file_goes_both_ways() {
        let mut answers = Answers::default();
        answers
            .apps
            .insert("native/flower".into(), entry(&[Shape, Words], &[Text]));
        answers
            .apps
            .insert("example.org/a \"b\"\\c\n".into(), entry(&[], &[Pointer]));
        answers.apps.insert("native/empty".into(), entry(&[], &[]));
        let text = render(&answers);
        assert!(text.contains(
            "[app.\"native/flower\"]\nallow = [\"shape\", \"words\"]\ndeny = [\"text\"]\n"
        ));
        assert_eq!(parse(&text), (answers.clone(), Vec::new()));

        let dir = temp_dir("trip");
        save(&dir, &answers).unwrap();
        assert_eq!(load(&dir), (answers, Vec::new()));
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.join(FILE)), 0o600);
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no part left");
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_no_is_kept() {
        let dir = temp_dir("no");
        remember(&dir, "native/cat", entry(&[Shape], &[Words])).unwrap();
        remember(&dir, "native/snake", entry(&[KeysArrows], &[])).unwrap();
        let (answers, _) = load(&dir);
        assert_eq!(answers.apps["native/cat"], entry(&[Shape], &[Words]));
        assert_eq!(answers.apps["native/snake"], entry(&[KeysArrows], &[]));
        // Denied, not asked again, at a start with a pop-up or without.
        let cat = declared(&[Shape, Words]);
        for headless in [false, true] {
            let resolved = resolve(&cat, answers.apps.get("native/cat"), headless);
            assert!(resolved.missing.is_empty());
            assert_eq!(resolved.grants, Grants::of([Shape]));
        }
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn what_is_not_known_is_ignored_with_a_warning() {
        let text = "# a comment\n\
                    [app.\"native/a\"]\n\
                    allow = [\"shape\", \"type\"]  # typing\n\
                    deny = [\"shape\", \"clicks\"]\n\
                    color = \"red\"\n\
                    allow = 3\n\
                    [other]\n\
                    allow = [\"text\"]\n\
                    nonsense\n";
        let (answers, warnings) = parse(text);
        assert_eq!(answers.apps.len(), 1);
        assert_eq!(answers.apps["native/a"], entry(&[], &[Shape, Clicks]));
        assert_eq!(
            warnings,
            [
                "permissions.toml:3: no permission `type`, ignored",
                "permissions.toml:5: no member `color`, ignored",
                "permissions.toml:6: `allow`: not an array of strings on one line, ignored",
                "permissions.toml:7: not a table `[app.\"KEY\"]`, ignored",
                "permissions.toml:8: outside a table `[app.\"KEY\"]`, ignored",
                "permissions.toml:9: outside a table `[app.\"KEY\"]`, ignored",
            ]
        );
    }

    #[test]
    fn writers_at_once_leave_one_whole_file() {
        let dir = temp_dir("race");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    for j in 0..20 {
                        let mut answers = Answers::default();
                        for k in 0..20 {
                            let key = format!("native/w{i}-{j}-{k}");
                            answers.apps.insert(key, entry(&[Shape], &[Text]));
                        }
                        save(&dir, &answers).unwrap();
                        let (read, warnings) = load(&dir);
                        assert!(warnings.is_empty());
                        assert_eq!(read.apps.len(), 20, "a whole file, of one writer");
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no part left");
        fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn resolve_each_case() {
        let snake = declared(&[Shape, KeysArrows]);
        // No answer: asked; headless, granted as declared.
        let r = resolve(&snake, None, false);
        assert_eq!(
            (r.grants, r.missing.clone()),
            (Grants::of([]), vec![Shape, KeysArrows])
        );
        let r = resolve(&snake, None, true);
        assert_eq!(r.grants, Grants::of([Shape, KeysArrows]));
        assert_eq!(r.missing, vec![Shape, KeysArrows]);
        // All answered.
        let e = entry(&[Shape], &[KeysArrows]);
        let r = resolve(&snake, Some(&e), false);
        assert_eq!((r.grants, r.missing), (Grants::of([Shape]), vec![]));
        // Some answered: the file holds; headless, the rest granted.
        let e = entry(&[], &[KeysArrows]);
        let r = resolve(&snake, Some(&e), true);
        assert_eq!((r.grants, r.missing), (Grants::of([Shape]), vec![Shape]));
        // A yes to what is not declared is not held.
        let e = entry(&[Shape, KeysArrows, Text, KeysRead], &[]);
        assert_eq!(
            resolve(&snake, Some(&e), false).grants,
            Grants::of([Shape, KeysArrows])
        );
        // The start, until the pop-up: as headless.
        assert_eq!(
            at_start(&snake, None, false),
            Grants::of([Shape, KeysArrows])
        );
        let e = entry(&[], &[Shape]);
        assert_eq!(at_start(&snake, Some(&e), false), Grants::of([KeysArrows]));
    }

    #[test]
    fn an_option_that_declares_more_asks_the_difference_alone() {
        // `flower`: shape, words, answered; `flower -a`: text too.
        let e = entry(&[Shape, Words], &[]);
        let bare = declared(&[Shape, Words]);
        assert!(resolve(&bare, Some(&e), false).missing.is_empty());
        let all = declared(&[Shape, Words, Text]);
        let r = resolve(&all, Some(&e), false);
        assert_eq!(r.missing, vec![Text]);
        assert_eq!(r.grants.degree(), Degree::Words);
        assert_eq!(r.grants.words, ["fish"]);
        assert_eq!(resolve(&all, Some(&e), true).grants.degree(), Degree::Text);
    }

    #[test]
    fn a_no_to_a_degree_is_a_no_to_those_above() {
        let all = declared(&[Shape, Words, Text, Pointer]);
        let e = entry(&[Shape, Text, Pointer], &[Words]);
        let r = resolve(&all, Some(&e), false);
        assert_eq!(r.grants, Grants::of([Shape, Pointer]));
        assert!(r.grants.words.is_empty());
    }

    #[test]
    fn declares_from_the_wire() {
        let value = json!({"permissions": {"shape": "to stand on it", "keys-arrows": "to steer"},
                           "words": ["fish"]});
        let d = Declared::from_wire(&value).unwrap();
        assert_eq!(d.permissions[&Shape], "to stand on it");
        assert_eq!(d.permissions.len(), 2);
        assert_eq!(d.words, ["fish"]);
        assert_eq!(
            Declared::from_wire(&json!({})).unwrap(),
            Declared::default()
        );
        for bad in [
            json!([]),
            json!({"permissions": {"type": "x"}}),
            json!({"permissions": {"shape": 1}}),
            json!({"words": "fish"}),
            json!({"words": [1]}),
            json!({"more": 1}),
        ] {
            let error = Declared::from_wire(&bad).unwrap_err();
            assert_eq!(error.code.as_str(), "bad-params", "{bad}");
        }
    }
}
