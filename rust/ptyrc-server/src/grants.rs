// SPDX-License-Identifier: GPL-3.0-or-later
//! Who asks, and the check of a request against what it holds. The
//! table and the check are `ptyrc_rules::grants`, shared with the
//! headless runner; here, the driver's identities, surfaces and errors.

use ptyrc_proto::{ErrorCode, RpcError};
use serde_json::Value;

#[cfg(test)]
pub(crate) use ptyrc_rules::grants::METHODS;
pub(crate) use ptyrc_rules::grants::{Degree, Grants, Hand, Permission, Verdict, Why, ladder};

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

/// Where a store app draws: `surface` of its manifest, with the frame
/// it was started on or moved to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    /// Its widgets alone, placed on that frame: a bare sprite is shown
    /// nowhere.
    Widget(u32),
    /// Clipped to that frame.
    Frame(u32),
    /// Anywhere with `screen-wide`; else as `Frame`.
    Screen(u32),
}

impl Surface {
    /// Of the manifest, on `frame`.
    pub fn of(surface: ptyrc_store::manifest::Surface, frame: u32) -> Surface {
        use ptyrc_store::manifest::Surface as Of;
        match surface {
            Of::Widget => Surface::Widget(frame),
            Of::Frame => Surface::Frame(frame),
            Of::Screen => Surface::Screen(frame),
        }
    }

    /// Moved to `frame` (`app.move`).
    pub fn moved(self, frame: u32) -> Surface {
        match self {
            Surface::Widget(_) => Surface::Widget(frame),
            Surface::Frame(_) => Surface::Frame(frame),
            Surface::Screen(_) => Surface::Screen(frame),
        }
    }
}

/// The check of a request. Every request of a connection that
/// presented a token goes through it; one without a token is the
/// user's and is not asked.
pub(crate) fn check(_who: &Identity, grants: &Grants, method: &str, params: &Value) -> Verdict {
    ptyrc_rules::grants::check(grants, method, params)
}

/// The check of one topic of `subscribe`. A topic not held is
/// subscribed and silent (`Lower`); a closed one is `Denied`.
pub(crate) fn check_topic(_who: &Identity, grants: &Grants, topic: &str) -> Verdict {
    ptyrc_rules::grants::check_topic(grants, topic)
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

    #[test]
    fn identities_are_keys() {
        assert_eq!(Identity::Native("cat".into()).key(), "native/cat");
        assert_eq!(
            Identity::Store("example.org/a/b".into()).key(),
            "example.org/a/b"
        );
    }

    #[test]
    fn every_method_and_topic_is_listed_once() {
        let mut listed: Vec<&str> = METHODS.iter().map(|m| m.0).collect();
        let mut known: Vec<&str> = crate::hints::METHODS.to_vec();
        listed.sort_unstable();
        known.sort_unstable();
        assert_eq!(listed, known);
    }

    #[test]
    fn denied_names_the_permission() {
        let error = denied("input.grab", Why::Lacks(Permission::KeysGrab));
        assert_eq!(error.code.as_str(), "denied");
        assert_eq!(error.message, "input.grab: denied");
        assert!(error.hint.unwrap().contains("`keys-grab`"));
    }
}
