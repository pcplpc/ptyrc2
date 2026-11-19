// SPDX-License-Identifier: GPL-3.0-or-later
//! Who asks, what it holds, and the check of a request against it. See
//! `plan/design.md`, "Permissions". No clock, no state: the table and
//! pure functions.

use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::Value;
use std::collections::BTreeSet;

/// What an app learns of the screen: a ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Degree {
    None,
    Layout,
    Shape,
    Words,
    Text,
}

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

macro_rules! permissions {
    ($($variant:ident $name:literal,)*) => {
        /// What the user grants, one yes or no each. The file and the
        /// pop-up, to come, make and name them.
        #[allow(dead_code)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub(crate) enum Permission { $($variant,)* }

        #[allow(dead_code)]
        impl Permission {
            pub const ALL: &[Permission] = &[$(Permission::$variant,)*];

            pub fn name(self) -> &'static str {
                match self { $(Permission::$variant => $name,)* }
            }

            pub fn parse(name: &str) -> Option<Permission> {
                match name { $($name => Some(Permission::$variant),)* _ => None }
            }
        }
    };
}

permissions! {
    Layout "layout",
    Shape "shape",
    Words "words",
    Text "text",
    FrontWidget "front-widget",
    Front "front",
    ScreenWide "screen-wide",
    KeysArrows "keys-arrows",
    KeysGrab "keys-grab",
    KeysRead "keys-read",
    Pointer "pointer",
    Clicks "clicks",
    ClicksGrab "clicks-grab",
    Clock "clock",
    Storage "storage",
    Files "files",
    FilesWrite "files-write",
    ProbeGit "probe-git",
    ProbeSystem "probe-system",
    LimitMemory "limit-memory",
    LimitTime "limit-time",
}

impl Permission {
    /// The degree of the screen it is, when it is one.
    pub fn degree(self) -> Option<Degree> {
        match self {
            Permission::Layout => Some(Degree::Layout),
            Permission::Shape => Some(Degree::Shape),
            Permission::Words => Some(Degree::Words),
            Permission::Text => Some(Degree::Text),
            _ => None,
        }
    }
}

/// Who a connection is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Identity {
    /// A shipped app, or any program the driver started: `native/NAME`.
    Native(String),
    /// A store app: its origin, `host/owner/repo[#path]`.
    Store(String),
}

impl Identity {
    /// The key of `permissions.toml`.
    pub fn key(&self) -> String {
        match self {
            Identity::Native(name) => format!("native/{name}"),
            Identity::Store(origin) => origin.clone(),
        }
    }
}

/// What an identity holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Grants {
    /// Everything, the closed methods too: the session, as the user.
    whole: bool,
    held: BTreeSet<Permission>,
    /// The words of `words`, as declared.
    pub words: Vec<String>,
}

impl Grants {
    /// The whole session.
    pub fn whole() -> Grants {
        Grants {
            whole: true,
            held: BTreeSet::new(),
            words: Vec::new(),
        }
    }

    /// These and the default profile.
    #[allow(dead_code)]
    pub fn of(held: impl IntoIterator<Item = Permission>) -> Grants {
        Grants {
            whole: false,
            held: held.into_iter().collect(),
            words: Vec::new(),
        }
    }

    pub fn holds(&self, permission: Permission) -> bool {
        self.whole || self.held.contains(&permission)
    }

    /// The degree its answers are cut to: None when nothing is cut,
    /// the whole session or `text`.
    pub fn cut(&self) -> Option<Degree> {
        Some(self.degree()).filter(|d| !self.whole && *d < Degree::Text)
    }

    /// The highest degree held.
    pub fn degree(&self) -> Degree {
        if self.whole {
            return Degree::Text;
        }
        let degrees = self.held.iter().filter_map(|p| p.degree());
        degrees.max().unwrap_or(Degree::None)
    }
}

/// What opens a method or a topic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gate {
    /// The default profile: had without asking.
    Open,
    /// An answer of this degree: below it, lowered to the degree held.
    Read(Degree),
    /// An act with no lesser form; a topic: no event without it.
    Needs(Permission),
    /// Closed to apps whatever they hold: the user's and the driver's.
    Never,
}

/// The answer of the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Allow,
    /// Answered as at this degree; a topic: subscribed, no event comes.
    Lower(Degree),
    Denied(Why),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// Not held.
    Lacks(Permission),
    /// `Gate::Never`.
    Never,
    /// Not in the table.
    Unlisted,
    /// The app of the connection left the table.
    Gone,
}

use Degree::{Layout, Shape, Text};
use Gate::{Needs, Never, Open, Read};

/// Every method of the protocol. One not listed is closed to apps.
/// `gate` refines some by their params.
pub(crate) const METHODS: &[(&str, Gate)] = &[
    ("auth", Open),
    ("batch", Open),
    ("subscribe", Open),
    ("session.info", Read(Text)),
    ("session.resize", Never),
    ("session.colors", Never),
    ("session.handoff", Never),
    ("screen.text", Read(Text)),
    ("screen.cells", Read(Text)),
    ("screen.find", Read(Text)),
    ("screen.shape", Read(Shape)),
    ("screen.cursor", Read(Shape)),
    ("screen.wait", Read(Text)),
    ("input.send", Never),
    ("input.click", Never),
    ("input.grab", Needs(Permission::KeysGrab)),
    ("input.release", Open),
    ("tape.status", Never),
    ("tape.step", Never),
    ("tape.seek", Never),
    ("tape.play", Never),
    ("tape.pause", Never),
    ("tape.quit", Never),
    ("frame.list", Read(Layout)),
    ("frame.get", Read(Layout)),
    ("frame.at", Read(Layout)),
    ("frame.text", Read(Text)),
    ("frame.heat", Read(Shape)),
    ("frame.idle", Read(Shape)),
    ("draw.put", Open),
    ("draw.update", Open),
    ("draw.remove", Open),
    ("draw.clear", Open),
    ("draw.list", Open),
    ("draw.ext", Open),
    ("draw.exts", Open),
    ("layer.define", Never),
    ("layer.list", Never),
    ("widget.declare", Open),
    ("widget.show", Open),
    ("widget.set", Open),
    ("widget.hide", Open),
    ("widget.ask", Open),
    ("widget.release", Open),
    ("widget.focus", Never),
    ("widget.blur", Never),
    ("widget.list", Open),
    ("widget.borders", Open),
    ("app.list", Open),
    ("app.start", Never),
    ("app.stop", Never),
    ("app.pause", Never),
    ("app.resume", Never),
    ("app.restart", Never),
    ("app.forget", Never),
    ("app.move", Never),
    ("app.permissions", Never),
    ("debug.overlay", Never),
    ("debug.dump", Never),
];

/// Every topic of `subscribe`.
pub(crate) const TOPICS: &[(&str, Gate)] = &[
    ("screen", Read(Shape)),
    ("cursor", Read(Shape)),
    ("frames", Read(Layout)),
    ("text", Read(Shape)),
    ("idle", Read(Shape)),
    ("draw", Open),
    ("input", Needs(Permission::KeysRead)),
    ("mouse", Needs(Permission::Clicks)),
    ("hover", Needs(Permission::Pointer)),
    ("size", Open),
    ("colors", Read(Shape)),
    ("widgets", Open),
    ("apps", Never),
    ("move", Open),
    ("exit", Open),
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

/// What opens `method` with these params. None: not listed.
pub(crate) fn gate(method: &str, params: &Value) -> Option<Gate> {
    let listed = METHODS.iter().find(|(name, _)| *name == method)?.1;
    let has = |member: &str| params.get(member).is_some_and(|v| !v.is_null());
    Some(match method {
        "screen.shape"
            if params
                .get("words")
                .and_then(Value::as_array)
                .is_some_and(|w| !w.is_empty()) =>
        {
            Read(Degree::Words)
        }
        "screen.wait" if !has("pattern") => Read(Shape),
        "input.grab" => {
            let keys = params.get("keys").and_then(Value::as_array);
            let arrows = keys.is_some_and(|keys| {
                !keys.is_empty() && keys.iter().all(|k| k.as_str().is_some_and(not_text))
            });
            Needs(match arrows {
                true => Permission::KeysArrows,
                false => Permission::KeysGrab,
            })
        }
        // A modal takes the keys while shown.
        "widget.show" if params.get("modal") == Some(&Value::Bool(true)) => {
            Needs(Permission::KeysGrab)
        }
        _ => listed,
    })
}

fn judge(grants: &Grants, gate: Option<Gate>) -> Verdict {
    if grants.whole {
        return Verdict::Allow;
    }
    match gate {
        None => Verdict::Denied(Why::Unlisted),
        Some(Open) => Verdict::Allow,
        Some(Read(degree)) if grants.degree() >= degree => Verdict::Allow,
        Some(Read(_)) => Verdict::Lower(grants.degree()),
        Some(Needs(permission)) if grants.holds(permission) => Verdict::Allow,
        Some(Needs(permission)) => Verdict::Denied(Why::Lacks(permission)),
        Some(Never) => Verdict::Denied(Why::Never),
    }
}

/// The check of a request. Every request of a connection that
/// presented a token goes through it; one without a token is the
/// user's and is not asked.
pub(crate) fn check(who: &Identity, grants: &Grants, method: &str, params: &Value) -> Verdict {
    // The table tells every app and its command line: not a store
    // app's to read, until it lists its own alone.
    if matches!(who, Identity::Store(_)) && method == "app.list" {
        return Verdict::Denied(Why::Never);
    }
    judge(grants, gate(method, params))
}

/// The check of one topic of `subscribe`. A topic not held is
/// subscribed and silent (`Lower`); a closed one is `Denied`.
pub(crate) fn check_topic(_who: &Identity, grants: &Grants, topic: &str) -> Verdict {
    let gate = TOPICS.iter().find(|(name, _)| *name == topic).map(|t| t.1);
    match judge(grants, gate) {
        Verdict::Denied(Why::Lacks(_)) => Verdict::Lower(grants.degree()),
        verdict => verdict,
    }
}

/// The error `denied` of `what`, a method or a topic.
pub(crate) fn denied(what: &str, why: Why) -> RpcError {
    let hint = match why {
        Why::Lacks(permission) => format!(
            "the app does not hold `{}`: declared by the app, granted by the user",
            permission.name()
        ),
        Why::Never => "closed to apps, whatever they hold: the user's hand only".to_string(),
        Why::Unlisted => "not open to apps".to_string(),
        Why::Gone => "the app of this connection has left the table".to_string(),
    };
    RpcError::new(ErrorCode::Denied, format!("{what}: denied")).with_hint(hint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn who() -> Identity {
        Identity::Native("cat".into())
    }

    fn ask(grants: &Grants, method: &str, params: Value) -> Verdict {
        check(&who(), grants, method, &params)
    }

    #[test]
    fn names_go_both_ways() {
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.name()), Some(*p));
        }
        assert_eq!(Permission::parse("type"), None);
        assert_eq!(who().key(), "native/cat");
        assert_eq!(
            Identity::Store("example.org/a/b".into()).key(),
            "example.org/a/b"
        );
    }

    #[test]
    fn the_degree_is_the_highest_held() {
        assert_eq!(Grants::of([]).degree(), Degree::None);
        assert_eq!(Grants::of([Permission::Pointer]).degree(), Degree::None);
        let grants = Grants::of([Permission::Layout, Permission::Words]);
        assert_eq!(grants.degree(), Degree::Words);
        assert_eq!(Grants::whole().degree(), Degree::Text);
        assert!(Degree::None < Degree::Layout && Degree::Words < Degree::Text);
    }

    #[test]
    fn whole_grants_allow_all() {
        let whole = Grants::whole();
        for (method, _) in METHODS {
            assert_eq!(ask(&whole, method, json!({})), Verdict::Allow, "{method}");
        }
        assert_eq!(ask(&whole, "no.such", json!({})), Verdict::Allow);
        for (topic, _) in TOPICS {
            assert_eq!(check_topic(&who(), &whole, topic), Verdict::Allow);
        }
    }

    #[test]
    fn the_default_profile() {
        let none = Grants::of([]);
        assert_eq!(ask(&none, "draw.put", json!({})), Verdict::Allow);
        assert_eq!(
            ask(&none, "screen.text", json!({})),
            Verdict::Lower(Degree::None)
        );
        assert_eq!(
            ask(&none, "input.send", json!({})),
            Verdict::Denied(Why::Never)
        );
        assert_eq!(
            ask(&none, "no.such", json!({})),
            Verdict::Denied(Why::Unlisted)
        );
        assert_eq!(
            ask(&none, "input.grab", json!({})),
            Verdict::Denied(Why::Lacks(Permission::KeysGrab))
        );
    }

    #[test]
    fn reading_is_lowered_to_the_degree_held() {
        let shape = Grants::of([Permission::Shape]);
        assert_eq!(
            ask(&shape, "screen.text", json!({})),
            Verdict::Lower(Degree::Shape)
        );
        assert_eq!(ask(&shape, "screen.shape", json!({})), Verdict::Allow);
        assert_eq!(
            ask(&shape, "screen.shape", json!({"words": ["fish"]})),
            Verdict::Lower(Degree::Shape)
        );
        assert_eq!(
            ask(&shape, "screen.shape", json!({"words": []})),
            Verdict::Allow
        );
        assert_eq!(ask(&shape, "frame.list", json!({})), Verdict::Allow);
        assert_eq!(
            ask(&shape, "screen.wait", json!({"stable_ms": 5})),
            Verdict::Allow
        );
        assert_eq!(
            ask(&shape, "screen.wait", json!({"pattern": "x"})),
            Verdict::Lower(Degree::Shape)
        );
        let text = Grants::of([Permission::Text]);
        assert_eq!(ask(&text, "screen.find", json!({})), Verdict::Allow);
        assert_eq!(
            ask(&text, "debug.dump", json!({})),
            Verdict::Denied(Why::Never)
        );
    }

    #[test]
    fn a_grab_is_told_by_its_keys() {
        let arrows = Grants::of([Permission::KeysArrows]);
        let grab = |keys: Value| ask(&arrows, "input.grab", json!({"keys": keys}));
        assert_eq!(grab(json!(["up", "down", "f5"])), Verdict::Allow);
        let lacks = Verdict::Denied(Why::Lacks(Permission::KeysGrab));
        assert_eq!(grab(json!(["up", "q"])), lacks);
        assert_eq!(grab(json!(["f13"])), lacks);
        assert_eq!(grab(json!([])), lacks);
        assert_eq!(ask(&arrows, "input.grab", json!({})), lacks);
        assert_eq!(ask(&arrows, "widget.show", json!({"modal": true})), lacks);
        assert_eq!(
            ask(&arrows, "widget.show", json!({"modal": false})),
            Verdict::Allow
        );
    }

    #[test]
    fn a_topic_not_held_is_silent() {
        let none = Grants::of([]);
        let topic = |grants: &Grants, t: &str| check_topic(&who(), grants, t);
        assert_eq!(topic(&none, "size"), Verdict::Allow);
        assert_eq!(topic(&none, "text"), Verdict::Lower(Degree::None));
        assert_eq!(topic(&none, "input"), Verdict::Lower(Degree::None));
        assert_eq!(topic(&none, "apps"), Verdict::Denied(Why::Never));
        assert_eq!(topic(&none, "nope"), Verdict::Denied(Why::Unlisted));
        let keys = Grants::of([Permission::KeysRead, Permission::Shape]);
        assert_eq!(topic(&keys, "input"), Verdict::Allow);
        assert_eq!(topic(&keys, "hover"), Verdict::Lower(Degree::Shape));
    }

    #[test]
    fn every_method_and_topic_is_listed_once() {
        let mut listed: Vec<&str> = METHODS.iter().map(|m| m.0).collect();
        let mut known: Vec<&str> = crate::hints::METHODS.to_vec();
        listed.sort_unstable();
        known.sort_unstable();
        assert_eq!(listed, known);
        listed.dedup();
        assert_eq!(listed.len(), METHODS.len());
        let topics: Vec<&str> = TOPICS.iter().map(|t| t.0).collect();
        assert_eq!(topics, ptyrc_proto::TOPICS);
    }

    #[test]
    fn denied_names_the_permission() {
        let error = denied("input.grab", Why::Lacks(Permission::KeysGrab));
        assert_eq!(error.code.as_str(), "denied");
        assert_eq!(error.message, "input.grab: denied");
        assert!(error.hint.unwrap().contains("`keys-grab`"));
    }
}
