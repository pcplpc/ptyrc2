// SPDX-License-Identifier: GPL-3.0-or-later
//! Fetch and check against local bare repositories.

mod common;

use common::{Item, app_items, bare, commit, tmp};
use ptyrc_store::{
    DirTree, ErrorKind, Node, Origin, Reference, Remote, Repo, Tree, check, tree::Kind,
};

/// A remote holding `items`, fetched into a fresh cache; the result of
/// fetch then check.
fn fetch_check(name: &str, items: &[Item]) -> ptyrc_store::Result<ptyrc_store::Checked> {
    let root = tmp(name);
    let remote = root.join("remote.git");
    bare(&remote);
    let c = commit(&remote, items, "one");
    let repo = Repo::open(&root.join("cache.git"))?;
    let got = repo.fetch(&Remote::Local(remote), &Reference::Head)?;
    assert_eq!(got, c);
    check(&repo.tree(&got, None)?)
}

#[test]
fn good_tree() {
    let ok = fetch_check("good", &app_items(&[Item::Exec("tool.sh", b"#!/bin/sh\n")])).unwrap();
    assert_eq!(ok.manifest.app.id, "sand-garden");
    let paths: Vec<&str> = ok.files.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "LICENSE",
            "app.js",
            "assets/grain.txt",
            "ptyrc-app.toml",
            "tool.sh"
        ]
    );
    assert_eq!(ok.dirs.len(), 1);
}

#[test]
fn by_commit_branch_and_sub() {
    let root = tmp("refs");
    let remote = root.join("remote.git");
    bare(&remote);
    let first = commit(&remote, &app_items(&[]), "one");
    let mut nested: Vec<Item> = Vec::new();
    let manifest = common::MANIFEST.replace("sand-garden", "nested");
    nested.push(Item::File("apps/n/ptyrc-app.toml", manifest.as_bytes()));
    nested.push(Item::File("apps/n/LICENSE", common::GPL));
    nested.push(Item::File("apps/n/app.js", b""));
    nested.push(Item::File("apps/n/assets/x", b""));
    let second = commit(&remote, &nested, "two");
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    let r = Remote::Local(remote.clone());
    assert_eq!(
        repo.fetch(&r, &Reference::Commit(first.clone())).unwrap(),
        first
    );
    assert_eq!(
        repo.fetch(&r, &Reference::Name("main".into())).unwrap(),
        second
    );
    let t = repo.tree(&second, Some("apps/n")).unwrap();
    assert_eq!(check(&t).unwrap().manifest.app.id, "nested");
    assert!(repo.tree(&second, Some("apps/none")).is_err());
    assert!(repo.tree(&first, None).is_ok());
}

#[test]
fn absent_commit_and_ref() {
    let root = tmp("absent");
    let remote = root.join("remote.git");
    bare(&remote);
    commit(&remote, &app_items(&[]), "one");
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    let r = Remote::Local(remote);
    let e = repo
        .fetch(&r, &Reference::Commit("0".repeat(40)))
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch);
    assert!(e.hint.is_some());
    assert!(repo.fetch(&r, &Reference::Name("nope".into())).is_err());
    let e = repo
        .fetch(&Remote::Local(root.join("missing.git")), &Reference::Head)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch);
}

fn refused(name: &str, items: &[Item], needle: &str) {
    let e = fetch_check(name, items).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::Tree | ErrorKind::Fetch), "{e}");
    assert!(e.to_string().contains(needle), "{e}");
}

#[test]
fn symlink() {
    refused(
        "symlink",
        &app_items(&[Item::Symlink("key", "/home/u/.ssh/id_ed25519")]),
        "symlink",
    );
}

#[test]
fn submodule() {
    refused(
        "submodule",
        &app_items(&[Item::Submodule("vendor/lib")]),
        "submodule",
    );
}

#[test]
fn dotdot() {
    // Refused by git's object check at fetch, or by ours.
    let e = fetch_check("dotdot", &app_items(&[Item::File("../escape", b"x")])).unwrap_err();
    assert!(e.message.contains("hasDotdot"), "{e}");
    assert!(matches!(e.kind, ErrorKind::Tree | ErrorKind::Fetch), "{e}");
    let e = fetch_check("dotgit", &app_items(&[Item::File(".git/config", b"x")])).unwrap_err();
    assert!(matches!(e.kind, ErrorKind::Tree | ErrorKind::Fetch), "{e}");
}

#[test]
fn too_large() {
    let big = vec![b'x'; (8 << 20) + 1];
    refused("big", &app_items(&[Item::File("big.bin", &big)]), "8 MiB");
    let script = vec![b' '; (1 << 20) + 1];
    refused(
        "script",
        &app_items(&[Item::File("app.js", &script)]),
        "1 MiB",
    );
    let names: Vec<String> = (0..510).map(|i| format!("f/{i}")).collect();
    let many: Vec<Item> = names.iter().map(|n| Item::File(n, b"")).collect();
    refused("many", &app_items(&many), "512");
}

#[test]
fn missing_parts() {
    refused(
        "nolicence",
        &[
            Item::File("ptyrc-app.toml", common::MANIFEST.as_bytes()),
            Item::File("app.js", b""),
            Item::File("assets/x", b""),
        ],
        "GPL",
    );
    refused(
        "wronglicence",
        &app_items(&[Item::File("LICENSE", b"MIT License")]),
        "GPL",
    );
    refused(
        "noscript",
        &[
            Item::File("ptyrc-app.toml", common::MANIFEST.as_bytes()),
            Item::File("COPYING", common::GPL),
            Item::File("assets/x", b""),
        ],
        "app.js",
    );
    refused(
        "noassets",
        &[
            Item::File("ptyrc-app.toml", common::MANIFEST.as_bytes()),
            Item::File("COPYING", common::GPL),
            Item::File("app.js", b""),
        ],
        "assets",
    );
    refused(
        "nomanifest",
        &[Item::File("LICENSE", common::GPL)],
        "ptyrc-app.toml",
    );
    let e = fetch_check(
        "badmanifest",
        &app_items(&[Item::File("ptyrc-app.toml", b"[app]\nid = 1\n")]),
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Manifest);
}

#[test]
fn hostile_remotes() {
    let root = tmp("hostile");
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    let mark = root.join("pwned");
    for url in [
        format!("ext::sh -c touch% {}", mark.display()),
        format!("https://x::sh -c touch% {}", mark.display()),
        format!("file://{}", root.display()),
        "-uhttps://example.org/a/b".to_string(),
        "--upload-pack=touch /tmp/x".to_string(),
        "http://127.0.0.1:1/a/b".to_string(),
        "https://example.org/a b".to_string(),
    ] {
        let e = repo
            .fetch(&Remote::Https(url.clone()), &Reference::Head)
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Fetch, "{url}");
    }
    for p in ["relative", "/a:b", "/x\n"] {
        assert!(
            repo.fetch(&Remote::Local(p.into()), &Reference::Head)
                .is_err(),
            "{p}"
        );
    }
    for r in [
        Reference::Name("--upload-pack=touch /tmp/x".into()),
        Reference::Name("-q".into()),
        Reference::Commit("--exec=x".into()),
        Reference::Commit("ABC".into()),
    ] {
        let e = repo.fetch(&Remote::Local(root.clone()), &r).unwrap_err();
        assert!(
            matches!(e.kind, ErrorKind::Fetch | ErrorKind::Origin),
            "{r:?}"
        );
    }
    assert!(repo.tree("--output=/tmp/x", None).is_err());
    assert!(repo.tree(&"0".repeat(40), Some("../x")).is_err());
    assert!(!mark.exists());
}

/// The local repository's own configuration (a hook path, an ext
/// transport as `url.insteadOf`) is not ours: it must not be followed.
#[test]
fn remote_config_ignored() {
    let root = tmp("config");
    let remote = root.join("remote.git");
    bare(&remote);
    let mark = root.join("pwned");
    let hook = remote.join("hooks/post-upload-pack");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, format!("#!/bin/sh\ntouch {}\n", mark.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let c = commit(&remote, &app_items(&[]), "one");
    let cfg = remote.join("config");
    let mut text = std::fs::read_to_string(&cfg).unwrap();
    text.push_str(&format!(
        "[uploadpack]\n\tpackObjectsHook = touch {}\n",
        mark.display()
    ));
    std::fs::write(&cfg, text).unwrap();
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    assert_eq!(
        repo.fetch(&Remote::Local(remote), &Reference::Head)
            .unwrap(),
        c
    );
    // Our own cache has no hook.
    let hooks = repo.dir().join("hooks");
    assert!(!hooks.exists() || std::fs::read_dir(&hooks).unwrap().next().is_none());
    assert!(!mark.exists());
}

#[test]
fn shallow() {
    let root = tmp("shallow");
    let remote = root.join("remote.git");
    bare(&remote);
    commit(&remote, &app_items(&[]), "one");
    let c2 = commit(&remote, &app_items(&[Item::File("b", b"2")]), "two");
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    repo.fetch(&Remote::Local(remote), &Reference::Head)
        .unwrap();
    let shallow = std::fs::read_to_string(repo.dir().join("shallow")).unwrap();
    assert_eq!(shallow.trim(), c2);
}

#[test]
fn dir_tree() {
    let root = tmp("dir");
    let app = root.join("app");
    for (p, b) in [
        ("ptyrc-app.toml", common::MANIFEST.as_bytes()),
        ("LICENSE", common::GPL),
        ("app.js", b"".as_slice()),
        ("assets/x", b""),
        (".git/HEAD", b"ref: refs/heads/main\n"),
    ] {
        let f = app.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, b).unwrap();
    }
    let t = DirTree::new(&app);
    let ok = check(&t).unwrap();
    assert!(ok.files.iter().all(|n| !n.path.starts_with(".git")));
    assert_eq!(t.read(&ok.files[0]).unwrap(), common::GPL);
    std::os::unix::fs::symlink("/etc/passwd", app.join("assets/link")).unwrap();
    let e = check(&t).unwrap_err();
    assert!(e.message.contains("symlink"), "{e}");
    let nodes = t.nodes().unwrap();
    assert!(nodes.iter().any(|n| n.kind == Kind::Symlink));
    std::fs::remove_file(app.join("assets/link")).unwrap();
    std::fs::create_dir_all(app.join("sub/.git")).unwrap();
    assert!(check(&t).is_err());
}

#[test]
fn local_origin_fetch() {
    let root = tmp("origin");
    let remote = root.join("remote.git");
    bare(&remote);
    let c = commit(&remote, &app_items(&[]), "one");
    let o = Origin::local(&remote, None).unwrap();
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    assert_eq!(repo.fetch(&o.remote(), &Reference::Head).unwrap(), c);
}

/// An in-memory tree: what git's own check would refuse reaches ours.
struct Fake(Vec<Node>);

impl Tree for Fake {
    fn nodes(&self) -> ptyrc_store::Result<Vec<Node>> {
        Ok(self.0.clone())
    }
    fn read(&self, node: &Node) -> ptyrc_store::Result<Vec<u8>> {
        Ok(match node.path.as_str() {
            "ptyrc-app.toml" => common::MANIFEST.as_bytes().to_vec(),
            "LICENSE" => common::GPL.to_vec(),
            _ => Vec::new(),
        })
    }
}

#[test]
fn check_refuses_names() {
    let node = |p: &str, kind| Node {
        path: p.into(),
        kind,
        size: 0,
        id: String::new(),
    };
    let base = || {
        vec![
            Node {
                size: common::MANIFEST.len() as u64,
                ..node("ptyrc-app.toml", Kind::File)
            },
            node("LICENSE", Kind::File),
            node("app.js", Kind::File),
            node("assets", Kind::Dir),
        ]
    };
    assert!(check(&Fake(base())).is_ok());
    for bad in [
        "../x",
        "a/../../x",
        "/etc/passwd",
        "a//b",
        ".git/config",
        "a/.GIT/x",
        "a\\b",
        "a\nb",
        ".",
    ] {
        let mut v = base();
        v.push(node(bad, Kind::File));
        let e = check(&Fake(v)).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Tree, "{bad:?}");
    }
    let mut v = base();
    v.push(node("dev", Kind::Other));
    assert!(check(&Fake(v)).is_err());
}

#[test]
fn fetch_bounded() {
    let root = tmp("bound");
    let remote = root.join("remote.git");
    bare(&remote);
    // Incompressible: the pack is about as large as the blob.
    let mut noise = vec![0u8; 3 << 20];
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    for b in noise.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = x as u8;
    }
    commit(
        &remote,
        &app_items(&[Item::File("other/noise.bin", &noise)]),
        "one",
    );
    let repo = Repo::open(&root.join("cache.git"))
        .unwrap()
        .max_fetch(1 << 20);
    let e = repo
        .fetch(&Remote::Local(remote.clone()), &Reference::Head)
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Fetch);
    assert!(e.message.contains("more than 1048576 bytes"), "{e}");
    assert!(e.hint.unwrap().contains("#path"));
    let packs = std::fs::read_dir(root.join("cache.git/objects/pack")).unwrap();
    assert_eq!(packs.count(), 0, "what the fetch wrote is removed");
    // The default bound takes it.
    let repo = Repo::open(&root.join("cache.git")).unwrap();
    repo.fetch(&Remote::Local(remote), &Reference::Head)
        .unwrap();
}

#[test]
fn check_all_lists_every_problem() {
    let root = tmp("all");
    let dir = root.join("app");
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    let manifest = common::MANIFEST.replace("surface = \"widget\"", "tape = \"demo.tape\"");
    std::fs::write(dir.join("ptyrc-app.toml"), manifest).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", dir.join("link")).unwrap();
    let e = ptyrc_store::check_all(&DirTree::new(&dir)).unwrap_err();
    let text: Vec<String> = e.iter().map(|e| e.message.clone()).collect();
    assert_eq!(e.len(), 4, "{text:?}");
    assert!(text[0].contains("link: a symlink"), "{text:?}");
    assert!(text[1].contains("GPL"), "{text:?}");
    assert!(text[2].contains("app.js"), "{text:?}");
    assert!(text[3].contains("demo.tape"), "{text:?}");
    assert!(e.iter().all(|e| e.hint.is_some()));
    // `check` gives the first.
    assert_eq!(check(&DirTree::new(&dir)).unwrap_err(), e[0]);
}
