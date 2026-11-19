// SPDX-License-Identifier: GPL-3.0-or-later
//! Install, list, prune, remove; directories under a test root only.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use common::{Item, app_items, bare, commit, tmp};
use ptyrc_store::install::{self, Lock};
use ptyrc_store::{Dirs, ErrorKind, Origin, Reference, Repo, Tier, check};

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o777
}

#[test]
fn install_ls_prune_rm() {
    let root = tmp("install");
    let dirs = Dirs::under(&root.join("xdg"));
    let remote = root.join("remote.git");
    bare(&remote);
    let c1 = commit(
        &remote,
        &app_items(&[Item::Exec("tool.sh", b"#!/bin/sh\n")]),
        "one",
    );
    let origin = Origin::local(&remote, None).unwrap();
    let repo = Repo::open(&dirs.git(&origin)).unwrap();
    let got = repo.fetch(&origin.remote(), &Reference::Head).unwrap();
    assert_eq!(got, c1);
    let tree = repo.tree(&got, None).unwrap();
    let ok = check(&tree).unwrap();
    let e = install::install(
        &dirs,
        &origin,
        &tree,
        &ok,
        &got,
        Tier::Untested,
        "2026-11-19",
    )
    .unwrap();
    assert_eq!(e.id, "sand-garden");

    let t = dirs.tree(&origin, &c1);
    assert_eq!(std::fs::read(t.join("LICENSE")).unwrap(), common::GPL);
    assert_eq!(mode(&t), 0o700);
    assert_eq!(mode(&t.join("assets")), 0o700);
    assert_eq!(mode(&t.join("tool.sh")), 0o400);
    assert_eq!(mode(&t.join("app.js")), 0o400);
    assert_eq!(mode(&dirs.lock()), 0o600);
    assert!(std::fs::read_dir(dirs.app(&origin)).unwrap().count() == 1);

    let lock = std::fs::read_to_string(dirs.lock()).unwrap();
    assert!(lock.contains(&format!("commit = \"{c1}\"")), "{lock}");
    assert!(lock.contains("tier = \"untested\""), "{lock}");
    assert!(lock.contains("date = \"2026-11-19\""), "{lock}");

    // A second commit: the first tree stays until pruned.
    let c2 = commit(&remote, &app_items(&[Item::File("b", b"2")]), "two");
    let got = repo.fetch(&origin.remote(), &Reference::Head).unwrap();
    assert_eq!(got, c2);
    let tree = repo.tree(&got, None).unwrap();
    let ok = check(&tree).unwrap();
    install::install(&dirs, &origin, &tree, &ok, &got, Tier::Tested, "2026-11-20").unwrap();
    assert_eq!(install::trees(&dirs, &origin).unwrap().len(), 2);
    let ls = install::list(&dirs).unwrap();
    assert_eq!(ls.len(), 1);
    assert_eq!(ls[0].commit, c2);
    assert_eq!(ls[0].tier, Tier::Tested);
    assert_eq!(install::prune(&dirs, &origin).unwrap(), [c1]);
    assert_eq!(install::trees(&dirs, &origin).unwrap(), [c2.as_str()]);

    // Installed again: kept as is.
    install::install(&dirs, &origin, &tree, &ok, &got, Tier::Tested, "2026-11-21").unwrap();

    // rm by id; storage purged apart.
    let storage = dirs.storage(&origin);
    std::fs::create_dir_all(&storage).unwrap();
    std::fs::write(storage.join("save"), b"x").unwrap();
    assert!(install::remove(&dirs, "nope").is_err());
    let gone = install::remove(&dirs, "sand-garden").unwrap();
    assert_eq!(gone.origin, origin.id());
    assert!(!dirs.app(&origin).exists());
    assert!(storage.exists());
    assert!(install::purge_storage(&dirs, &origin).unwrap());
    assert!(!storage.exists());
    assert!(!install::purge_storage(&dirs, &origin).unwrap());
    assert!(install::list(&dirs).unwrap().is_empty());
}

#[test]
fn refused() {
    let root = tmp("refused");
    let dirs = Dirs::under(&root.join("xdg"));
    let remote = root.join("remote.git");
    bare(&remote);
    let rust = common::MANIFEST.replace("script = \"app.js\"", "rust = \"src/lib.rs\"");
    let c = commit(
        &remote,
        &app_items(&[
            Item::File("ptyrc-app.toml", rust.as_bytes()),
            Item::File("src/lib.rs", b""),
        ]),
        "rust",
    );
    let origin = Origin::local(&remote, None).unwrap();
    let repo = Repo::open(&dirs.git(&origin)).unwrap();
    repo.fetch(&origin.remote(), &Reference::Head).unwrap();
    let tree = repo.tree(&c, None).unwrap();
    let ok = check(&tree).unwrap();
    let e =
        install::install(&dirs, &origin, &tree, &ok, &c, Tier::Untested, "2026-11-19").unwrap_err();
    assert_eq!(e.kind, ErrorKind::Unsupported);
    assert_eq!(e.message, "Rust apps are not built yet");
    assert!(!dirs.app(&origin).exists());

    let e = install::install(
        &dirs,
        &origin,
        &tree,
        &ok,
        "HEAD",
        Tier::Untested,
        "2026-11-19",
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Install);
    let e =
        install::install(&dirs, &origin, &tree, &ok, &c, Tier::Untested, "19/11/2026").unwrap_err();
    assert_eq!(e.kind, ErrorKind::Install);
}

#[test]
fn resolve_ambiguous() {
    let entry = |o: &str| install::Installed {
        origin: o.into(),
        id: "cat".into(),
        version: "1".into(),
        commit: "a".repeat(40),
        tier: Tier::Approved,
        date: "2026-11-19".into(),
        previous: None,
        started: false,
    };
    let lock = Lock {
        apps: vec![entry("example.org/a/cat"), entry("example.org/b/cat")],
    };
    let e = lock.resolve("cat").unwrap_err();
    assert!(e.hint.unwrap().contains("example.org/b/cat"));
    assert_eq!(
        lock.resolve("https://example.org/a/cat.git")
            .unwrap()
            .origin,
        "example.org/a/cat"
    );
}

#[test]
fn dirs_paths() {
    let d = Dirs::under(Path::new("/r"));
    let o = Origin::parse("example.org/a/b#x").unwrap();
    let c = "c".repeat(40);
    assert_eq!(
        d.tree(&o, &c),
        Path::new("/r/data/ptyrc/apps/example.org%2Fa%2Fb%23x").join(&c)
    );
    assert_eq!(d.lock(), Path::new("/r/data/ptyrc/apps.lock"));
    assert_eq!(
        d.storage(&o),
        Path::new("/r/state/ptyrc/storage/example.org%2Fa%2Fb%23x")
    );
    assert_eq!(
        d.permissions(),
        Path::new("/r/config/ptyrc/permissions.toml")
    );
    assert_eq!(d.store(), Path::new("/r/config/ptyrc/store.toml"));
    assert_eq!(
        d.git(&o),
        Path::new("/r/cache/ptyrc/git/example.org%2Fa%2Fb%23x.git")
    );
}

/// Fetches the head of `remote` and installs it.
fn add(dirs: &Dirs, origin: &Origin) -> String {
    let repo = Repo::open(&dirs.git(origin)).unwrap();
    let got = repo.fetch(&origin.remote(), &Reference::Head).unwrap();
    let tree = repo.tree(&got, None).unwrap();
    let ok = check(&tree).unwrap();
    install::install(dirs, origin, &tree, &ok, &got, Tier::Untested, "2026-11-19").unwrap();
    install::prune(dirs, origin).unwrap();
    got
}

#[test]
fn previous_kept_until_started() {
    let root = tmp("previous");
    let dirs = Dirs::under(&root.join("xdg"));
    let remote = root.join("remote.git");
    bare(&remote);
    let origin = Origin::local(&remote, None).unwrap();
    commit(&remote, &app_items(&[]), "one");
    let c1 = add(&dirs, &origin);
    let (e, dir) = install::locate(&dirs, "sand-garden").unwrap();
    assert_eq!(dir, dirs.tree(&origin, &c1));
    assert!(!e.started && e.previous.is_none());
    assert!(install::started(&dirs, &origin).unwrap().is_empty());

    // c1 started: kept while c2 has not.
    commit(&remote, &app_items(&[Item::File("b", b"2")]), "two");
    let c2 = add(&dirs, &origin);
    let e = Lock::load(&dirs)
        .unwrap()
        .get(&origin.id())
        .cloned()
        .unwrap();
    assert_eq!(e.previous.as_deref(), Some(c1.as_str()));
    assert_eq!(install::trees(&dirs, &origin).unwrap().len(), 2);

    // c3 over a c2 never started: c1 stays the previous, c2 goes.
    commit(&remote, &app_items(&[Item::File("b", b"3")]), "three");
    let c3 = add(&dirs, &origin);
    let mut want = vec![c1.clone(), c3.clone()];
    want.sort();
    assert_eq!(install::trees(&dirs, &origin).unwrap(), want);
    assert!(!install::trees(&dirs, &origin).unwrap().contains(&c2));

    // c3 started: c1 goes.
    assert_eq!(install::started(&dirs, &origin).unwrap(), [c1]);
    assert_eq!(install::trees(&dirs, &origin).unwrap(), [c3.as_str()]);
    let e = Lock::load(&dirs)
        .unwrap()
        .get(&origin.id())
        .cloned()
        .unwrap();
    assert!(e.started && e.previous.is_none());
    let lock = std::fs::read_to_string(dirs.lock()).unwrap();
    assert!(lock.contains("started = true"), "{lock}");

    let other = Origin::parse("example.org/a/b").unwrap();
    assert_eq!(
        install::started(&dirs, &other).unwrap_err().kind,
        ErrorKind::Install
    );
    assert!(install::locate(&dirs, "nope").is_err());
}

#[test]
fn store_lock_excludes() {
    let root = tmp("flock");
    let dirs = Dirs::under(&root.join("xdg"));
    let held = install::StoreLock::take(&dirs, || panic!("free")).unwrap();
    assert_eq!(mode(&dirs.store_lock()), 0o600);
    let (tx, rx) = std::sync::mpsc::channel();
    let d = dirs.clone();
    let t = std::thread::spawn(move || {
        let waited = std::cell::Cell::new(false);
        let _l = install::StoreLock::take(&d, || waited.set(true)).unwrap();
        tx.send(waited.get()).unwrap();
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(rx.try_recv().is_err(), "taken while held");
    drop(held);
    assert!(rx.recv().unwrap(), "on_wait called");
    t.join().unwrap();
}

#[test]
fn purge_grants() {
    let root = tmp("grants");
    let dirs = Dirs::under(&root.join("xdg"));
    let a = Origin::parse("example.org/a/cat").unwrap();
    let b = Origin::parse("example.org/b/dog").unwrap();
    assert!(!install::purge_grants(&dirs, &a).unwrap());
    std::fs::create_dir_all(dirs.permissions().parent().unwrap()).unwrap();
    // As the driver writes it.
    let dog = "[app.\"example.org/b/dog\"]\nallow = [\"shape\"]\ndeny = []\n";
    std::fs::write(
        dirs.permissions(),
        format!(
            "# Written by the ptyrc driver.\n\n[app.\"example.org/a/cat\"]\n\
             allow = [\"shape\"]\ndeny = [\"text\"]\n\n{dog}"
        ),
    )
    .unwrap();
    // A key that is not of the table `app` is not the origin's.
    assert!(!install::purge_grants(&dirs, &Origin::parse("example.org/c/x").unwrap()).unwrap());
    assert!(install::purge_grants(&dirs, &a).unwrap());
    assert!(!install::purge_grants(&dirs, &a).unwrap());
    let text = std::fs::read_to_string(dirs.permissions()).unwrap();
    assert_eq!(text, format!("# Written by the ptyrc driver.\n\n{dog}"));
    assert!(text.contains(&b.id()));
    assert_eq!(mode(&dirs.permissions()), 0o600);
}
