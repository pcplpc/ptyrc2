// SPDX-License-Identifier: GPL-3.0-or-later
//! Registries fetched, indexed, paged and reviewed, against local bare
//! repositories.

mod common;

use common::{Item, MANIFEST, app_items, bare, commit, tmp};
use ptyrc_store::index::{self, Index, IndexApp, IndexPermission, IndexVersion};
use ptyrc_store::manifest::{COLOURS, Colour, PERMISSIONS, colour};
use ptyrc_store::review::{Maintainers, review};
use ptyrc_store::sync::{self, MAX_AGE, Synced};
use ptyrc_store::{Dirs, Origin, Registries, Remote, Repo, Tier, check_all, page};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

#[test]
fn colours() {
    let names: Vec<&str> = COLOURS.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, PERMISSIONS);
    assert_eq!(colour("text"), Colour::Red);
    assert_eq!(colour("front"), Colour::Amber);
    assert_eq!(colour("shape"), Colour::Plain);
    assert_eq!(colour("no-such"), Colour::Red);
}

const SAND: &str = "apps/example.org/a/sand.toml";

fn pointer(versions: &[(&str, &str)]) -> String {
    let mut s = "origin = \"https://example.org/a/sand\"\n".to_string();
    for (c, t) in versions {
        s.push_str(&format!(
            "\n[[version]]\ncommit = \"{c}\"\ntier = \"{t}\"\n"
        ));
    }
    s
}

#[test]
fn sync_registry() {
    let root = tmp("sync");
    let dirs = Dirs::under(&root.join("xdg"));
    let remote = root.join("reg.git");
    bare(&remote);
    let c = "1".repeat(40);
    let text = pointer(&[(&c, "approved")]);
    let head = commit(
        &remote,
        &[
            Item::File(SAND, text.as_bytes()),
            Item::File("index.json", b"{\"format\":1,\"apps\":[]}"),
            Item::File("README.md", b"not copied"),
            Item::File("apps/.hidden/x.toml", b"not copied"),
            Item::Symlink("apps/example.org/a/link.toml", "/etc/passwd"),
            Item::Exec("ci/check.sh", b"#!/bin/sh\n"),
        ],
        "one",
    );
    let store = format!(
        "[[registry]]\nname = \"main\"\nurl = \"{}\"\n",
        remote.display()
    );
    let regs = Registries::parse(&dirs, &store).unwrap();
    let reg = &regs.list[0];
    assert_eq!(reg.dir, dirs.registry().join("main"));

    match sync::sync(reg, at(1000), Some(MAX_AGE)) {
        Synced::Fetched { commit } => assert_eq!(commit, head),
        s => panic!("{s:?}"),
    }
    assert_eq!(std::fs::read_to_string(reg.dir.join(SAND)).unwrap(), text);
    assert!(reg.dir.join("index.json").is_file());
    for gone in [
        "README.md",
        "apps/.hidden/x.toml",
        "apps/example.org/a/link.toml",
        "ci",
    ] {
        assert!(!reg.dir.join(gone).exists(), "{gone}");
    }
    assert_eq!(sync::stamp(reg), Some((1000, head.clone())));
    // Nothing checked out, no repository left.
    let names: Vec<String> = std::fs::read_dir(dirs.registry())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["main"]);
    let sand = Origin::parse("example.org/a/sand").unwrap();
    let (_, p) = regs.lookup(&sand).unwrap().unwrap();
    assert_eq!(p.versions[0].tier, Tier::Approved);

    // Younger than a day: not fetched; older: fetched.
    assert!(matches!(
        sync::sync(reg, at(1000 + 3600), Some(MAX_AGE)),
        Synced::Fresh
    ));
    let c2 = "2".repeat(40);
    let text2 = pointer(&[(&c, "approved"), (&c2, "untested")]);
    let head2 = commit(&remote, &[Item::File(SAND, text2.as_bytes())], "two");
    match sync::sync(reg, at(1000 + 2 * 86400), Some(MAX_AGE)) {
        Synced::Fetched { commit } => assert_eq!(commit, head2),
        s => panic!("{s:?}"),
    }
    assert_eq!(std::fs::read_to_string(reg.dir.join(SAND)).unwrap(), text2);
    assert!(
        !reg.dir.join("index.json").exists(),
        "the copy is the head's"
    );

    // The remote gone: the stale copy is kept, and said.
    std::fs::remove_dir_all(&remote).unwrap();
    match sync::sync(reg, at(1000 + 4 * 86400), Some(MAX_AGE)) {
        Synced::Stale { since, error } => {
            assert_eq!(since, Some(1000 + 2 * 86400));
            assert!(error.message.contains("registry main"), "{error}");
        }
        s => panic!("{s:?}"),
    }
    assert!(reg.dir.join(SAND).is_file());

    // No copy: missing. A path registry: read as it is.
    let regs = Registries::parse(
        &dirs,
        &format!(
            "[[registry]]\nname = \"far\"\nurl = \"{}\"\n\n[[registry]]\nname = \"here\"\npath = \"{}\"\n",
            root.join("none.git").display(),
            root.display()
        ),
    )
    .unwrap();
    assert!(matches!(
        sync::sync(&regs.list[0], at(0), None),
        Synced::Missing { .. }
    ));
    assert!(matches!(
        sync::sync(&regs.list[1], at(0), None),
        Synced::Local
    ));
    let present = regs.lookup_present(&sand).unwrap();
    assert!(present.found.is_none());
    assert_eq!(present.absent.len(), 1);
}

#[test]
fn sync_bounded() {
    let root = tmp("sync-big");
    let dirs = Dirs::under(&root.join("xdg"));
    let remote = root.join("reg.git");
    bare(&remote);
    let big = vec![b'#'; (sync::MAX_POINTER + 1) as usize];
    commit(&remote, &[Item::File(SAND, &big)], "big");
    let regs = Registries::parse(
        &dirs,
        &format!(
            "[[registry]]\nname = \"r\"\nurl = \"{}\"\n",
            remote.display()
        ),
    )
    .unwrap();
    match sync::sync(&regs.list[0], at(0), None) {
        Synced::Missing { error } => assert!(error.message.contains("bytes"), "{error}"),
        s => panic!("{s:?}"),
    }
    assert!(!regs.list[0].dir.exists());
}

/// A registry directory listing `sand` of `app`, its commits as given.
fn registry_dir(root: &Path, versions: &[(&str, &str)]) -> ptyrc_store::Registry {
    let dir = root.join("registry");
    let p = dir.join(SAND);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, pointer(versions)).unwrap();
    ptyrc_store::Registry {
        name: "r".into(),
        dir,
        url: None,
    }
}

#[test]
fn index_built() {
    let root = tmp("index");
    let dirs = Dirs::under(&root.join("xdg"));
    let app = root.join("sand.git");
    bare(&app);
    let c1 = commit(&app, &app_items(&[]), "one");
    let m2 = MANIFEST
        .replace("0.3.0", "0.4.0")
        .replace("shape = ", "text = ");
    let c2 = commit(
        &app,
        &app_items(&[Item::File("ptyrc-app.toml", m2.as_bytes())]),
        "two",
    );
    let c3 = "3".repeat(40);
    let reg = registry_dir(
        &root,
        &[(&c1, "approved"), (&c2, "untested"), (&c3, "approved")],
    );
    let remote = |_: &Origin| Remote::Local(app.clone());
    let ix = index::build(&dirs, &reg, &remote).unwrap();
    assert_eq!(ix.apps.len(), 1);
    let a = &ix.apps[0];
    assert_eq!(a.origin, "example.org/a/sand");
    // c3, approved and the later, cannot be read: its fields are none.
    assert_eq!(a.commit.as_deref(), Some(c3.as_str()));
    assert_eq!(a.id, None);
    assert!(a.versions[2].error.is_some());
    assert_eq!(a.versions[0].version.as_deref(), Some("0.3.0"));
    assert_eq!(a.versions[1].version.as_deref(), Some("0.4.0"));

    let reg = registry_dir(&root, &[(&c1, "approved"), (&c2, "untested")]);
    let ix = index::build(&dirs, &reg, &remote).unwrap();
    let a = &ix.apps[0];
    assert_eq!(a.commit.as_deref(), Some(c1.as_str()));
    assert_eq!(a.tier, Some(Tier::Approved));
    assert_eq!(a.id.as_deref(), Some("sand-garden"));
    assert_eq!(a.version.as_deref(), Some("0.3.0"));
    assert_eq!(a.surface.as_deref(), Some("widget"));
    assert_eq!(a.permissions[0].name, "shape");
    assert_eq!(a.permissions[0].colour, Colour::Plain);
    // Written and read back.
    assert_eq!(Index::parse(&ix.to_json()).unwrap(), ix);
    assert!(Index::parse("{\"format\":2,\"apps\":[]}").is_err());
    let html = page::render(&ix, "ptyrc store");
    assert!(html.contains("sand-garden 0.3.0"));
    assert!(html.contains("<li class=\"plain\"><code>shape</code>"));
}

#[test]
fn page_escapes() {
    let evil = "<script>alert(1)</script>\"'&";
    let ix = Index {
        format: index::FORMAT,
        apps: vec![IndexApp {
            origin: format!("example.org/a/{evil}"),
            url: "https://example.org/a".into(),
            manifest: "ptyrc-app.toml".into(),
            yanked: true,
            id: Some(format!("<b>{evil}")),
            summary: Some(format!("</p><img src=x onerror=alert(1)>{evil}")),
            version: Some(evil.into()),
            license: Some(evil.into()),
            surface: Some(evil.into()),
            tape: Some(format!("<a href=\"javascript:x\">{evil}")),
            commit: None,
            tier: Some(Tier::Tested),
            permissions: vec![
                IndexPermission {
                    name: "text".into(),
                    why: format!("\"><style>body{{display:none}}</style>{evil}"),
                    colour: Colour::Red,
                    words: vec![evil.into()],
                },
                IndexPermission {
                    name: "front".into(),
                    why: "in front".into(),
                    colour: Colour::Amber,
                    words: vec![],
                },
            ],
            versions: vec![IndexVersion {
                commit: format!("é{evil}"),
                tier: Tier::Untested,
                by: Some(evil.into()),
                date: Some(evil.into()),
                yanked: false,
                version: None,
                error: Some(evil.into()),
            }],
        }],
    };
    let html = page::render(&ix, evil);
    // Our own markup only: one <style>, no script, no image, no link.
    assert_eq!(html.matches("<style>").count(), 1, "{html}");
    let allowed = [
        "!doctype", "html", "head", "meta", "title", "style", "body", "h1", "h2", "p", "span",
        "code", "div", "section", "ul", "li", "table", "tr", "th", "td",
    ];
    for tag in html.split('<').skip(1) {
        let tag = &tag[..tag.find('>').expect("a closed tag")];
        let name = tag.trim_start_matches('/').split(' ').next().unwrap();
        assert!(allowed.contains(&name), "<{tag}> in {html}");
        assert!(!tag.contains(" on"), "<{tag}>");
    }
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;&quot;&#39;&amp;"));
    assert!(html.contains("<li class=\"red\"><code>text</code>"));
    assert!(html.contains("<li class=\"amber\"><code>front</code>"));
    assert!(html.contains("<p class=\"yanked\">yanked</p>"));
    assert_eq!(page::escape("a\u{1b}[31mb\u{7}"), "a[31mb");
}

/// A registry repository, BASE and HEAD, and an app to point at.
struct Pr {
    reg: std::path::PathBuf,
    dirs: Dirs,
    app: std::path::PathBuf,
    good: String,
    bad: String,
}

impl Pr {
    fn new(name: &str) -> Pr {
        let root = tmp(name);
        let app = root.join("sand.git");
        bare(&app);
        let bad = commit(
            &app,
            &[Item::File("ptyrc-app.toml", MANIFEST.as_bytes())],
            "no licence",
        );
        let good = commit(&app, &app_items(&[]), "good");
        let reg = root.join("reg.git");
        bare(&reg);
        commit(&reg, &[Item::File("README.md", b"r")], "base");
        Pr {
            reg,
            dirs: Dirs::under(&root.join("xdg")),
            app,
            good,
            bad,
        }
    }

    /// Commits `items` on main and reviews it against the commit before.
    fn change(&self, items: &[Item], author: &str, m: &Maintainers) -> Vec<String> {
        let repo = Repo::open(&self.reg).unwrap();
        let base = repo.commit("main").unwrap();
        commit(&self.reg, items, "change");
        let head = repo.commit("main").unwrap();
        let bt = repo.tree(&base, None).unwrap();
        let ht = repo.tree(&head, None).unwrap();
        let app = self.app.clone();
        let dirs = self.dirs.clone();
        let mut fetch = |o: &Origin, c: &str| {
            let r = Repo::open(&dirs.git(o)).map_err(|e| vec![e])?;
            let one = ptyrc_store::Reference::Commit(c.to_string());
            r.fetch(&Remote::Local(app.clone()), &one)
                .map_err(|e| vec![e])?;
            check_all(&r.tree(c, None).map_err(|e| vec![e])?).map(|_| ())
        };
        let r = review(&bt, &ht, author, m, &mut fetch).unwrap();
        r.problems.into_iter().map(|e| e.message).collect()
    }
}

#[test]
fn review_rules() {
    let pr = Pr::new("review");
    let m = Maintainers::parse("# who may\nmia\ntom tested  # a tester\n").unwrap();
    assert!(m.may("mia", Tier::Approved) && m.may("tom", Tier::Tested));
    assert!(!m.may("tom", Tier::Approved) && !m.may("eve", Tier::Tested));
    assert!(m.may("eve", Tier::Untested));
    assert!(Maintainers::parse("mia god\n").is_err());

    let readme = Item::File("README.md", b"r");
    let g = pr.good.clone();
    let untested = pointer(&[(&g, "untested")]);
    // A new app, untested, by anyone: fine.
    let p = pr.change(&[Item::File(SAND, untested.as_bytes()), readme], "eve", &m);
    assert!(p.is_empty(), "{p:?}");

    // A commit that fails the check.
    let b = pr.bad.clone();
    let two = pointer(&[(&g, "untested"), (&b, "untested")]);
    let p = pr.change(
        &[
            Item::File(SAND, two.as_bytes()),
            Item::File("README.md", b"r"),
        ],
        "eve",
        &m,
    );
    assert!(p.iter().any(|e| e.contains("GPL")), "{p:?}");

    // Raised by a stranger; by a tester, too far; by a tester, with by.
    let readme = || Item::File("README.md", b"r");
    let tested =
        |by: &str| format!("{untested}by = \"{by}\"\n").replace("untested\"\nby", "tested\"\nby");
    // Reset HEAD to the untested pointer for each case.
    let back = |pr: &Pr| {
        pr.change(
            &[Item::File(SAND, untested.as_bytes()), readme()],
            "mia",
            &m,
        );
    };
    back(&pr);
    let p = pr.change(
        &[Item::File(SAND, tested("eve").as_bytes()), readme()],
        "eve",
        &m,
    );
    assert!(p.iter().any(|e| e.contains("untested to tested")), "{p:?}");
    back(&pr);
    let approved = tested("tom").replace("tier = \"tested\"", "tier = \"approved\"");
    let p = pr.change(
        &[Item::File(SAND, approved.as_bytes()), readme()],
        "tom",
        &m,
    );
    assert!(
        p.iter().any(|e| e.contains("untested to approved")),
        "{p:?}"
    );
    back(&pr);
    let p = pr.change(
        &[Item::File(SAND, tested("mia").as_bytes()), readme()],
        "tom",
        &m,
    );
    assert!(!p.is_empty(), "by must be the author: {p:?}");
    back(&pr);
    let p = pr.change(
        &[Item::File(SAND, tested("tom").as_bytes()), readme()],
        "tom",
        &m,
    );
    assert!(p.is_empty(), "{p:?}");

    // A yank, a removal, another file: a maintainer's.
    let yanked = format!("{}yanked = true\n", tested("tom"));
    let p = pr.change(&[Item::File(SAND, yanked.as_bytes()), readme()], "tom", &m);
    assert!(p.iter().any(|e| e.contains("changed")), "{p:?}");
    let p = pr.change(&[Item::File(SAND, yanked.as_bytes()), readme()], "mia", &m);
    assert!(p.is_empty(), "{p:?}");
    let p = pr.change(&[Item::File("README.md", b"edited")], "eve", &m);
    assert!(
        p.iter().any(|e| e.contains("README.md: not a pointer")),
        "{p:?}"
    );
    assert!(p.iter().any(|e| e.contains("removed")), "{p:?}");
    let p = pr.change(&[Item::File("README.md", b"edited again")], "mia", &m);
    assert!(p.is_empty(), "{p:?}");

    // A pointer in the wrong place; one that does not parse; twice.
    let p = pr.change(
        &[Item::File(
            "apps/example.org/a/other.toml",
            untested.as_bytes(),
        )],
        "eve",
        &m,
    );
    assert!(p.iter().any(|e| e.contains("speaks for")), "{p:?}");
    let p = pr.change(&[Item::File(SAND, b"origin = 1\n")], "eve", &m);
    assert!(!p.is_empty());
    let twice = pointer(&[(&g, "untested"), (&g, "untested")]);
    let p = pr.change(&[Item::File(SAND, twice.as_bytes())], "eve", &m);
    assert!(p.iter().any(|e| e.contains("listed twice")), "{p:?}");
}
