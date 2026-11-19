// SPDX-License-Identifier: GPL-3.0-or-later
//! The registry's check of a pull request, BASE to HEAD: the mechanical
//! rules of `plan/store-model.md`, Registry. Who may set a tier is a
//! file of names given by the caller; the policy is the owner's.
//!
//! - Only pointer files change (`apps/**.toml`), each in the place its
//!   origin names: one file per app. Any other path, a removed pointer,
//!   a removed version, a change to a listed version other than its tier
//!   raised, or to the app's `yanked`: a maintainer's (`approved`).
//! - A new version is `untested`, or set by one who may say its tier.
//! - A tier raised (`tested`, `approved`) is set by one who may say it,
//!   and `by` is that one.
//! - Each new commit is fetched; its tree passes the check of the
//!   install (`ptyrc dev check`).

use std::collections::BTreeMap;

use crate::error::{Error, ErrorKind, Result};
use crate::origin::Origin;
use crate::registry::{Pointer, Tier, Version};
use crate::tree::{Kind, Tree};

fn rev_err(msg: impl Into<String>, hint: impl Into<String>) -> Error {
    Error::new(ErrorKind::Registry, msg).with_hint(hint)
}

/// Who may say which tier: one name per line, optionally followed by
/// the highest tier it may set (`tested` or `approved`, the default);
/// `#` begins a comment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Maintainers {
    pub list: Vec<(String, Tier)>,
}

impl Maintainers {
    pub fn parse(src: &str) -> Result<Maintainers> {
        let mut list = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut f = line.split_whitespace();
            let name = f.next().expect("not empty").to_string();
            let tier = match f.next() {
                None | Some("approved") => Tier::Approved,
                Some("tested") => Tier::Tested,
                Some(t) => {
                    return Err(rev_err(
                        format!("maintainers:{}: {t:?}", i + 1),
                        "NAME [tested|approved]",
                    ));
                }
            };
            if f.next().is_some() {
                return Err(rev_err(
                    format!("maintainers:{}: more than a name and a tier", i + 1),
                    "NAME [tested|approved]",
                ));
            }
            list.push((name, tier));
        }
        Ok(Maintainers { list })
    }

    /// Whether `who` may set `tier`. Anyone may say `untested`.
    pub fn may(&self, who: &str, tier: Tier) -> bool {
        tier == Tier::Untested || self.list.iter().any(|(n, t)| n == who && *t >= tier)
    }
}

/// A pointer file's path: `apps/**.toml`.
pub fn is_pointer_path(path: &str) -> bool {
    path.starts_with("apps/") && path.ends_with(".toml")
}

/// The regular files of `tree`, by path: (blob id, node).
fn files(tree: &dyn Tree) -> Result<BTreeMap<String, crate::tree::Node>> {
    let mut out = BTreeMap::new();
    for n in tree.nodes()? {
        if n.kind != Kind::Dir {
            out.insert(n.path.clone(), n);
        }
    }
    Ok(out)
}

/// Reads and parses a pointer of `tree`; it must speak for its place.
fn pointer(tree: &dyn Tree, node: &crate::tree::Node) -> Result<Pointer> {
    if node.kind != Kind::File || node.size > crate::sync::MAX_POINTER {
        return Err(rev_err(
            format!("{}: not a regular file of 1 MiB at most", node.path),
            "a pointer file",
        ));
    }
    let bytes = tree.read(node)?;
    let src = std::str::from_utf8(&bytes)
        .map_err(|_| rev_err(format!("{}: not UTF-8", node.path), "a pointer file"))?;
    let p = Pointer::parse(&node.path, src)?;
    let o = p.origin()?;
    if o.registry_path().as_deref() != Some(node.path.as_str()) {
        return Err(rev_err(
            format!("{}: speaks for {o}", node.path),
            format!("its place is {}", o.registry_path().unwrap_or_default()),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for v in &p.versions {
        if !seen.insert(&v.commit) {
            return Err(rev_err(
                format!("{}: commit {} listed twice", node.path, v.commit),
                "one [[version]] per commit",
            ));
        }
    }
    Ok(p)
}

/// Fetches a commit of an origin and checks its tree: `Err` with every
/// problem.
pub type CheckCommit<'a> = dyn FnMut(&Origin, &str) -> std::result::Result<(), Vec<Error>> + 'a;

/// What a check found: the problems, and the commits checked.
#[derive(Debug, Default)]
pub struct Review {
    pub problems: Vec<Error>,
    /// `(origin, commit)` fetched and checked.
    pub checked: Vec<(String, String)>,
}

/// Checks the change from `base` to `head` made by `author`. `check`
/// fetches a commit of an origin and checks its tree: `Err` with every
/// problem.
pub fn review(
    base: &dyn Tree,
    head: &dyn Tree,
    author: &str,
    maintainers: &Maintainers,
    check: &mut CheckCommit<'_>,
) -> Result<Review> {
    let (b, h) = (files(base)?, files(head)?);
    let mut out = Review::default();
    let maintainer = maintainers.may(author, Tier::Approved);
    let for_maintainer =
        |what: String| rev_err(what, format!("a maintainer's change; {author} is not one"));
    let mut paths: Vec<&String> = b.keys().chain(h.keys()).collect();
    paths.sort();
    paths.dedup();
    for path in paths {
        let (old, new) = (b.get(path), h.get(path));
        if old.map(|n| (&n.id, n.kind)) == new.map(|n| (&n.id, n.kind)) {
            continue;
        }
        if !is_pointer_path(path) {
            if !maintainer {
                out.problems
                    .push(for_maintainer(format!("{path}: not a pointer file")));
            }
            continue;
        }
        let Some(new) = new else {
            if !maintainer {
                out.problems
                    .push(for_maintainer(format!("{path}: removed")));
            }
            continue;
        };
        let p = match pointer(head, new) {
            Ok(p) => p,
            Err(e) => {
                out.problems.push(e);
                continue;
            }
        };
        // The base's, when it read; a base that did not read is replaced.
        let was = old.and_then(|o| pointer(base, o).ok());
        let origin = p.origin()?;
        out.problems
            .extend(versions(path, was.as_ref(), &p, author, maintainers));
        for v in &p.versions {
            let listed = was
                .as_ref()
                .is_some_and(|w| w.versions.iter().any(|x| x.commit == v.commit));
            if listed {
                continue;
            }
            out.checked.push((origin.id(), v.commit.clone()));
            if let Err(errs) = check(&origin, &v.commit) {
                for e in errs {
                    out.problems.push(Error {
                        message: format!("{path}: {}: {}", short(&v.commit), e.message),
                        ..e
                    });
                }
            }
        }
    }
    Ok(out)
}

fn short(c: &str) -> &str {
    &c[..c.len().min(12)]
}

/// The rules on the versions of one pointer, `was` its base.
fn versions(
    path: &str,
    was: Option<&Pointer>,
    p: &Pointer,
    author: &str,
    m: &Maintainers,
) -> Vec<Error> {
    let mut out = Vec::new();
    let maintainer = m.may(author, Tier::Approved);
    let mut need = |ok: bool, what: String, who: &str| {
        if !ok {
            out.push(rev_err(format!("{path}: {what}"), who.to_string()));
        }
    };
    let by_ok = |v: &Version| v.by.as_deref() == Some(author);
    let tier_hint = |t: Tier| format!("{t} is set by one who may say it, with by = \"{author}\"");
    if let Some(w) = was {
        need(
            maintainer || (w.yanked == p.yanked && w.manifest == p.manifest),
            "yanked or manifest changed".into(),
            "a maintainer's change",
        );
        for o in &w.versions {
            if !p.versions.iter().any(|v| v.commit == o.commit) {
                need(
                    maintainer,
                    format!("{}: removed", short(&o.commit)),
                    "a maintainer's change; to withdraw a version: yanked = true",
                );
            }
        }
    }
    for v in &p.versions {
        let o = was.and_then(|w| w.versions.iter().find(|x| x.commit == v.commit));
        let c = short(&v.commit);
        match o {
            None => {
                if v.tier > Tier::Untested {
                    need(
                        m.may(author, v.tier) && by_ok(v),
                        format!("{c}: new at {}", v.tier),
                        &tier_hint(v.tier),
                    );
                }
                if v.yanked {
                    need(
                        maintainer,
                        format!("{c}: new and yanked"),
                        "a maintainer's change",
                    );
                }
            }
            Some(o) if o == v => {}
            Some(o) => {
                let raised = v.tier > o.tier && v.yanked == o.yanked;
                if raised {
                    need(
                        m.may(author, v.tier) && by_ok(v),
                        format!("{c}: {} to {}", o.tier, v.tier),
                        &tier_hint(v.tier),
                    );
                } else {
                    need(
                        maintainer,
                        format!("{c}: changed"),
                        "a maintainer's change; others raise a tier, nothing else",
                    );
                }
            }
        }
    }
    out
}
