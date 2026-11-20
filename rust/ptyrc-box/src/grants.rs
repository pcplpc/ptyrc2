// SPDX-License-Identifier: GPL-3.0-or-later
//! What a headless app holds, and the check of its requests: the rules
//! of the driver (`ptyrc-server`, `grants.rs` and `lower.rs`) for the
//! methods and topics the headless host answers. `plan/store-model.md`,
//! "A no lowers, it does not break"; `plan/design.md`, "The cut
//! answers".

use std::collections::BTreeSet;

/// What an app learns of the screen: a ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Degree {
    None,
    Layout,
    Shape,
    Words,
    Text,
}

const LADDER: [Degree; 4] = [Degree::Layout, Degree::Shape, Degree::Words, Degree::Text];

impl Degree {
    pub fn name(self) -> &'static str {
        match self {
            Degree::None => "none",
            Degree::Layout => "layout",
            Degree::Shape => "shape",
            Degree::Words => "words",
            Degree::Text => "text",
        }
    }
}

/// The permissions held: those declared, less those denied. A degree is
/// held only with every declared degree below it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    declared: BTreeSet<String>,
    held: BTreeSet<String>,
    /// The words of `words`, as declared.
    pub words: Vec<String>,
    /// The user's words (a `word` option given): found at any degree,
    /// shown from `shape` (`ptyrc-server`, `lower.rs`).
    pub user_words: Vec<String>,
    /// The user's patterns (a `pattern` option given): `(pattern,
    /// regex)`; `screen.find`, `screen.wait` of exactly one answered as
    /// under `text`.
    pub patterns: Vec<(String, bool)>,
}

impl Grants {
    pub fn new<S: AsRef<str>>(declared: &[S], denied: &[S], words: &[S]) -> Grants {
        let set = |v: &[S]| {
            v.iter()
                .map(|s| s.as_ref().to_owned())
                .collect::<BTreeSet<_>>()
        };
        let declared = set(declared);
        let denied = set(denied);
        let mut held: BTreeSet<String> = declared.difference(&denied).cloned().collect();
        let mut gap = false;
        for d in LADDER {
            let name = d.name();
            if gap {
                held.remove(name);
            } else if declared.contains(name) && !held.contains(name) {
                gap = true;
            }
        }
        Grants {
            declared,
            held,
            words: words.iter().map(|s| s.as_ref().to_owned()).collect(),
            user_words: Vec::new(),
            patterns: Vec::new(),
        }
    }

    pub fn holds(&self, name: &str) -> bool {
        self.held.contains(name)
    }

    pub fn declares(&self, name: &str) -> bool {
        self.declared.contains(name)
    }

    /// The permissions held, by name.
    pub fn held(&self) -> Vec<String> {
        self.held.iter().cloned().collect()
    }

    /// The highest degree held.
    pub fn degree(&self) -> Degree {
        let held = LADDER.into_iter().filter(|d| self.holds(d.name()));
        held.max().unwrap_or(Degree::None)
    }

    /// The words found: those declared, when `words` is held; the
    /// user's, at any degree.
    pub fn found_words(&self) -> Vec<String> {
        let mut found = match self.degree() >= Degree::Words {
            true => self.words.clone(),
            false => Vec::new(),
        };
        found.extend(self.user_words.iter().cloned());
        found
    }

    /// The words shown: those found, from `shape`.
    pub fn held_words(&self) -> Vec<String> {
        match self.degree() >= Degree::Shape {
            true => self.found_words(),
            false => Vec::new(),
        }
    }

    /// Whether `word` is among the words shown, case ignored.
    pub fn holds_word(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        self.held_words().iter().any(|h| h.to_lowercase() == word)
    }

    /// Whether `word` is among the words found, case ignored.
    pub fn finds_word(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        self.found_words().iter().any(|h| h.to_lowercase() == word)
    }

    /// Whether `(pattern, regex)` is a pattern of the user.
    pub fn user_pattern(&self, pattern: &str, regex: bool) -> bool {
        (self.patterns.iter()).any(|(p, r)| p == pattern && *r == regex)
    }
}

/// What opens a method or a topic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The default profile.
    Open,
    /// An answer of this degree: below it, cut to the degree held.
    Read(Degree),
    /// An act with no lesser form; a topic: no event without it.
    Needs(&'static str),
    /// Answered without it, in a lesser form (`"degree": "none"`).
    Lowered(&'static str),
    /// Closed to apps whatever they hold.
    Never,
}

/// Every method the driver knows (`ptyrc-server`, `grants::METHODS`);
/// held to it by `tests/protocol.rs`.
pub const METHODS: &[&str] = &[
    "auth",
    "batch",
    "subscribe",
    "session.info",
    "session.resize",
    "session.colors",
    "session.handoff",
    "screen.text",
    "screen.cells",
    "screen.find",
    "screen.shape",
    "screen.cursor",
    "screen.wait",
    "input.send",
    "input.click",
    "input.grab",
    "input.release",
    "tape.status",
    "tape.step",
    "tape.seek",
    "tape.play",
    "tape.pause",
    "tape.quit",
    "frame.list",
    "frame.get",
    "frame.at",
    "frame.text",
    "frame.heat",
    "frame.idle",
    "draw.put",
    "draw.update",
    "draw.remove",
    "draw.clear",
    "draw.list",
    "draw.ext",
    "draw.exts",
    "layer.define",
    "layer.list",
    "widget.declare",
    "widget.show",
    "widget.set",
    "widget.hide",
    "widget.ask",
    "widget.release",
    "widget.focus",
    "widget.blur",
    "widget.list",
    "widget.borders",
    "app.list",
    "app.start",
    "app.enrol",
    "app.stop",
    "app.pause",
    "app.resume",
    "app.restart",
    "app.forget",
    "app.move",
    "app.permissions",
    "app.result",
    "clock.now",
    "clock.zone",
    "storage.get",
    "storage.set",
    "storage.remove",
    "storage.list",
    "file.stat",
    "file.read",
    "file.list",
    "file.write",
    "probe.git",
    "probe.system",
    "debug.overlay",
    "debug.dump",
];

/// Methods closed to apps whatever they hold (`grants::METHODS`, `Never`).
const NEVER: &[&str] = &[
    "session.resize",
    "session.colors",
    "session.handoff",
    "input.send",
    "input.click",
    "layer.define",
    "layer.list",
    "widget.focus",
    "widget.blur",
    "app.start",
    "app.enrol",
    "app.stop",
    "app.pause",
    "app.resume",
    "app.restart",
    "app.forget",
    "app.move",
    "app.permissions",
    "debug.overlay",
    "debug.dump",
];

/// Named keys that are not text: a grab of these alone is `keys-arrows`.
const NOT_TEXT: &[&str] = &[
    "up", "down", "left", "right", "home", "end", "pageup", "pagedown", "insert", "delete",
];

fn not_text(key: &str) -> bool {
    let function = |k: &str| {
        k.strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n))
    };
    NOT_TEXT.contains(&key) || function(key)
}

/// What opens `method` with these params. A method not named here is
/// open, or refused `headless`.
pub fn gate(method: &str, params: &serde_json::Value) -> Gate {
    let words = params.get("words").and_then(|w| w.as_array());
    match method {
        _ if NEVER.contains(&method) || method.starts_with("tape.") => Gate::Never,
        "screen.shape" if words.is_some_and(|w| !w.is_empty()) => Gate::Read(Degree::Words),
        "screen.wait" if params.get("pattern").is_none_or(|p| p.is_null()) => {
            Gate::Read(Degree::Shape)
        }
        "screen.shape" | "screen.cursor" | "frame.heat" | "frame.idle" => Gate::Read(Degree::Shape),
        "screen.text" | "screen.cells" | "screen.find" | "screen.wait" | "frame.text"
        | "session.info" => Gate::Read(Degree::Text),
        "frame.list" | "frame.get" | "frame.at" => Gate::Read(Degree::Layout),
        "input.grab" => {
            let keys = params.get("keys").and_then(|k| k.as_array());
            let arrows = keys.is_some_and(|keys| {
                !keys.is_empty() && keys.iter().all(|k| k.as_str().is_some_and(not_text))
            });
            Gate::Needs(if arrows { "keys-arrows" } else { "keys-grab" })
        }
        "widget.show" if params.get("modal") == Some(&serde_json::Value::Bool(true)) => {
            Gate::Needs("keys-grab")
        }
        "clock.now" | "clock.zone" => Gate::Lowered("clock"),
        _ if method.starts_with("storage.") => Gate::Needs("storage"),
        "file.write" => Gate::Needs("files-write"),
        "probe.git" => Gate::Lowered("probe-git"),
        "probe.system" => Gate::Lowered("probe-system"),
        _ => Gate::Open,
    }
}

/// What opens a topic of `subscribe`.
pub fn topic_gate(topic: &str) -> Gate {
    match topic {
        "screen" | "cursor" | "text" | "idle" | "colors" => Gate::Read(Degree::Shape),
        "frames" => Gate::Read(Degree::Layout),
        "input" => Gate::Needs("keys-read"),
        // Open, as the driver's: what is heard is judged by event.
        "apps" => Gate::Never,
        _ => Gate::Open,
    }
}

/// What `gate` gives for `grants`: `Ok(None)` allowed, `Ok(Some(d))`
/// answered at the degree `d`, `Err(Some(p))` denied for lack of `p`,
/// `Err(None)` closed.
pub fn judge(grants: &Grants, gate: Gate) -> Result<Option<Degree>, Option<&'static str>> {
    match gate {
        Gate::Open => Ok(None),
        Gate::Read(d) if grants.degree() >= d => Ok(None),
        Gate::Read(_) => Ok(Some(grants.degree())),
        Gate::Needs(p) if grants.holds(p) => Ok(None),
        Gate::Needs(p) => Err(Some(p)),
        Gate::Lowered(p) if grants.holds(p) => Ok(None),
        Gate::Lowered(_) => Ok(Some(Degree::None)),
        Gate::Never => Err(None),
    }
}

/// The hint of `denied`: the permission, and the line of the manifest
/// that would ask for it when it is not declared.
pub fn denied_hint(grants: &Grants, lacks: Option<&str>) -> String {
    match lacks {
        None => "closed to apps, whatever they hold: the user's hand only".into(),
        Some(p) if grants.declares(p) => {
            format!("the app does not hold `{p}`: declared, and denied")
        }
        Some(p) => format!(
            "the app does not hold `{p}`; the manifest would ask for it: {}",
            manifest_line(p, &[])
        ),
    }
}

/// The line of `[permissions]` that asks for `name`.
pub fn manifest_line(name: &str, words: &[String]) -> String {
    match name {
        "words" => {
            let list: Vec<String> = words.iter().map(|w| format!("{w:?}")).collect();
            format!(
                "[permissions] words = {{ list = [{}], why = \"...\" }}",
                list.join(", ")
            )
        }
        _ => format!("[permissions] {name} = \"...\""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn g(declared: &[&str], denied: &[&str]) -> Grants {
        Grants::new(declared, denied, &["fish"])
    }

    #[test]
    fn the_ladder() {
        assert_eq!(g(&[], &[]).degree(), Degree::None);
        assert_eq!(g(&["shape", "words"], &[]).degree(), Degree::Words);
        // Unticking one unticks those above it.
        assert_eq!(g(&["shape", "words"], &["shape"]).degree(), Degree::None);
        assert_eq!(g(&["shape", "text"], &["shape"]).degree(), Degree::None);
        assert_eq!(
            g(&["shape", "words", "text"], &["words"]).degree(),
            Degree::Shape
        );
        // Not declared: not below.
        assert_eq!(g(&["text"], &[]).degree(), Degree::Text);
        assert!(g(&["shape", "words"], &[]).holds_word("FISH"));
        assert!(!g(&["shape"], &[]).holds_word("fish"));
    }

    #[test]
    fn gates() {
        let shape = json!({"words": ["fish"]});
        assert_eq!(gate("screen.shape", &shape), Gate::Read(Degree::Words));
        assert_eq!(gate("screen.shape", &json!({})), Gate::Read(Degree::Shape));
        assert_eq!(gate("screen.text", &json!({})), Gate::Read(Degree::Text));
        let arrows = json!({"keys": ["up", "f5"]});
        assert_eq!(gate("input.grab", &arrows), Gate::Needs("keys-arrows"));
        assert_eq!(gate("input.grab", &json!({})), Gate::Needs("keys-grab"));
        assert_eq!(gate("input.send", &json!({})), Gate::Never);
        assert_eq!(gate("tape.seek", &json!({})), Gate::Never);
        assert_eq!(gate("draw.put", &json!({})), Gate::Open);
        assert_eq!(gate("clock.zone", &json!({})), Gate::Lowered("clock"));
        assert_eq!(gate("storage.set", &json!({})), Gate::Needs("storage"));
        assert_eq!(gate("app.result", &json!({})), Gate::Open);
        assert_eq!(gate("file.write", &json!({})), Gate::Needs("files-write"));
        assert_eq!(gate("file.read", &json!({})), Gate::Open);
        assert_eq!(gate("probe.git", &json!({})), Gate::Lowered("probe-git"));
        assert_eq!(gate("asset.read", &json!({})), Gate::Open);
        assert_eq!(topic_gate("input"), Gate::Needs("keys-read"));
        assert_eq!(topic_gate("size"), Gate::Open);
        let none = g(&[], &[]);
        assert_eq!(
            judge(&none, Gate::Read(Degree::Text)),
            Ok(Some(Degree::None))
        );
        assert_eq!(
            judge(&none, Gate::Needs("keys-arrows")),
            Err(Some("keys-arrows"))
        );
        assert!(denied_hint(&none, Some("keys-arrows")).contains("[permissions] keys-arrows"));
        let denied = g(&["keys-arrows"], &["keys-arrows"]);
        assert!(denied_hint(&denied, Some("keys-arrows")).contains("denied"));
    }
}
