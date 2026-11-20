// SPDX-License-Identifier: GPL-3.0-or-later
//! What a headless app holds, and the check of its requests: the rules
//! of the driver, `ptyrc_rules::grants`, for permissions named as the
//! manifest names them. `plan/store-model.md`, "A no lowers, it does not
//! break"; `plan/design.md`, "The cut answers".

use ptyrc_rules::grants::{self as rules, Permission, Verdict};
use std::collections::BTreeSet;

pub use ptyrc_rules::grants::{Degree, Gate, Why};

/// The permissions held: those declared, less those denied, on the
/// ladder (`rules::ladder`). A name that is no permission is ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    declared: BTreeSet<Permission>,
    held: BTreeSet<Permission>,
    /// The words of `words`, as declared.
    pub words: Vec<String>,
    /// The user's words (a `word` option given): found at any degree,
    /// shown from `shape`.
    pub user_words: Vec<String>,
    /// The user's patterns (a `pattern` option given): `(pattern,
    /// regex)`; `screen.find`, `screen.wait` of exactly one answered as
    /// under `text`.
    pub patterns: Vec<(String, bool)>,
}

impl Grants {
    pub fn new<S: AsRef<str>>(declared: &[S], denied: &[S], words: &[S]) -> Grants {
        let set = |v: &[S]| -> BTreeSet<Permission> {
            v.iter()
                .filter_map(|s| Permission::parse(s.as_ref()))
                .collect()
        };
        let declared = set(declared);
        let mut held: BTreeSet<Permission> = declared.difference(&set(denied)).copied().collect();
        rules::ladder(declared.iter().copied(), &mut held);
        Grants {
            declared,
            held,
            words: words.iter().map(|s| s.as_ref().to_owned()).collect(),
            user_words: Vec::new(),
            patterns: Vec::new(),
        }
    }

    /// As the driver holds them.
    pub fn rules(&self) -> rules::Grants {
        let mut g = rules::Grants::of(self.held.iter().copied());
        g.words = self.words.clone();
        g.hand = rules::Hand {
            words: self.user_words.clone(),
            patterns: self.patterns.clone(),
        };
        g
    }

    pub fn holds(&self, name: &str) -> bool {
        Permission::parse(name).is_some_and(|p| self.held.contains(&p))
    }

    pub fn declares(&self, name: &str) -> bool {
        Permission::parse(name).is_some_and(|p| self.declared.contains(&p))
    }

    /// The permissions held, by name.
    pub fn held(&self) -> Vec<String> {
        self.held.iter().map(|p| p.name().to_owned()).collect()
    }

    /// The highest degree held.
    pub fn degree(&self) -> Degree {
        self.rules().degree()
    }
}

/// Every method the driver knows.
pub fn methods() -> impl Iterator<Item = &'static str> {
    rules::METHODS.iter().map(|m| m.0)
}

pub fn is_method(method: &str) -> bool {
    methods().any(|m| m == method)
}

/// What opens `method` with these params: the driver's gate; `asset.*`,
/// the runner's, open. `None`: not listed, denied as the driver denies
/// it.
pub fn gate(method: &str, params: &serde_json::Value) -> Option<Gate> {
    match method.starts_with("asset.") {
        true => Some(Gate::Open),
        false => rules::gate(method, params),
    }
}

/// What opens a topic of `subscribe`.
pub fn topic_gate(topic: &str) -> Gate {
    let gate = rules::TOPICS.iter().find(|(name, _)| *name == topic);
    gate.map_or(Gate::Open, |t| t.1)
}

/// What `gate` gives for `grants`: `Ok(None)` allowed, `Ok(Some(d))`
/// answered at the degree `d`, `Err(why)` denied.
pub fn judge(grants: &Grants, gate: Option<Gate>) -> Result<Option<Degree>, Why> {
    match rules::judge(&grants.rules(), gate) {
        Verdict::Allow => Ok(None),
        Verdict::Lower(d) => Ok(Some(d)),
        Verdict::Denied(why) => Err(why),
    }
}

/// The hint of `denied`: as the driver's; for a permission lacking, the
/// line of the manifest that would ask for it when it is not declared.
pub fn denied_hint(grants: &Grants, why: Why) -> String {
    match why {
        Why::Lacks(p) if grants.declares(p.name()) => {
            format!("the app does not hold `{}`: declared, and denied", p.name())
        }
        Why::Lacks(p) => format!(
            "the app does not hold `{}`; the manifest would ask for it: {}",
            p.name(),
            manifest_line(p.name(), &[])
        ),
        Why::Unlisted => "not open to apps".into(),
        Why::Never | Why::Gone => "closed to apps, whatever they hold: the user's hand only".into(),
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
        let shown = |g: Grants| {
            let r = g.rules();
            ptyrc_rules::lower::shown(r.degree(), &r.words, &r.hand)
        };
        assert_eq!(shown(g(&["shape", "words"], &[])), ["fish"]);
        assert!(shown(g(&["shape"], &[])).is_empty());
    }

    #[test]
    fn gates() {
        let gate = |m: &str, p: &serde_json::Value| super::gate(m, p).unwrap();
        let shape = json!({"words": ["fish"]});
        assert_eq!(gate("screen.shape", &shape), Gate::Read(Degree::Words));
        assert_eq!(gate("screen.shape", &json!({})), Gate::Read(Degree::Shape));
        assert_eq!(gate("screen.text", &json!({})), Gate::Read(Degree::Text));
        let arrows = json!({"keys": ["up", "f5"]});
        assert_eq!(
            gate("input.grab", &arrows),
            Gate::Needs(Permission::KeysArrows)
        );
        assert_eq!(
            gate("input.grab", &json!({})),
            Gate::Needs(Permission::KeysGrab)
        );
        assert_eq!(gate("input.send", &json!({})), Gate::Never);
        assert_eq!(gate("tape.seek", &json!({})), Gate::Never);
        assert_eq!(gate("draw.put", &json!({})), Gate::Open);
        assert_eq!(
            gate("clock.zone", &json!({})),
            Gate::Lowered(Permission::Clock)
        );
        assert_eq!(
            gate("storage.set", &json!({})),
            Gate::Needs(Permission::Storage)
        );
        assert_eq!(gate("app.result", &json!({})), Gate::Open);
        assert_eq!(
            gate("file.write", &json!({})),
            Gate::Needs(Permission::FilesWrite)
        );
        assert_eq!(gate("file.read", &json!({})), Gate::Open);
        assert_eq!(
            gate("probe.git", &json!({})),
            Gate::Lowered(Permission::ProbeGit)
        );
        assert_eq!(gate("asset.read", &json!({})), Gate::Open);
        assert_eq!(gate("asset.anything", &json!({})), Gate::Open);
        assert_eq!(super::gate("no.such", &json!({})), None);
        assert_eq!(topic_gate("input"), Gate::Needs(Permission::KeysRead));
        assert_eq!(topic_gate("size"), Gate::Open);
        let none = g(&[], &[]);
        assert_eq!(
            judge(&none, Some(Gate::Read(Degree::Text))),
            Ok(Some(Degree::None))
        );
        assert_eq!(
            judge(&none, Some(Gate::Needs(Permission::KeysArrows))),
            Err(Why::Lacks(Permission::KeysArrows))
        );
        let arrows = Why::Lacks(Permission::KeysArrows);
        assert!(denied_hint(&none, arrows).contains("[permissions] keys-arrows"));
        let denied = g(&["keys-arrows"], &["keys-arrows"]);
        assert!(denied_hint(&denied, arrows).contains("denied"));
        // Not listed: denied as the driver denies it, whatever is held.
        let all = g(&["text", "keys-grab", "storage"], &[]);
        assert_eq!(
            judge(&all, super::gate("no.such", &json!({}))),
            Err(Why::Unlisted)
        );
        assert_eq!(denied_hint(&all, Why::Unlisted), "not open to apps");
    }
}
