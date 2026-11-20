// SPDX-License-Identifier: GPL-3.0-or-later
//! The shipped apps: found without `store add`, tier `shipped`, hidden
//! by an installed app of their id; a Rust one built into the cache.

mod common;

use common::tmp;
use ptyrc_store::build::{MODULE, Toolchain};
use ptyrc_store::{Dirs, Tier, install, shipped};
use std::path::Path;

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, q) = (e.path(), to.join(e.file_name()));
        match e.file_type().unwrap().is_dir() {
            true if e.file_name() != "target" => copy(&p, &q),
            true => {}
            false => {
                std::fs::copy(&p, &q).unwrap();
            }
        }
    }
}

fn examples(kind: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk")
        .join(kind)
        .join("examples")
}

#[test]
fn found_without_add() {
    let root = tmp("shipped");
    let dirs = Dirs::under(&root);
    assert!(install::all(&dirs).unwrap().is_empty());
    copy(&examples("js").join("sand"), &root.join("shipped/sand"));
    std::fs::create_dir_all(root.join("shipped/empty")).unwrap();
    let all = install::all(&dirs).unwrap();
    assert_eq!(all.len(), 1, "{all:?}");
    let sand = &all[0];
    assert_eq!(
        (sand.origin.as_str(), sand.id.as_str()),
        ("ptyrc/sand", "sand")
    );
    assert_eq!(sand.tier, Tier::Shipped);
    assert!(sand.started && sand.previous.is_none());
    for name in ["sand", "ptyrc/sand"] {
        let (e, dir) = install::locate(&dirs, name).unwrap();
        assert_eq!(e.origin, "ptyrc/sand");
        assert_eq!(dir, root.join("shipped/sand"));
    }
    let e = install::locate(&dirs, "ptyrc/none").unwrap_err();
    assert!(e.message.contains("not shipped"), "{e}");
    // The lock and `store rm` know nothing of it.
    assert!(install::list(&dirs).unwrap().is_empty());
    assert!(install::remove(&dirs, "ptyrc/sand").is_err());
    // A directory name not the id: refused, named.
    copy(&examples("js").join("sand"), &root.join("shipped/dune"));
    let e = install::locate(&dirs, "dune").unwrap_err();
    assert!(e.message.contains("ptyrc/dune") && e.hint.unwrap().contains("id = \"dune\""));
    assert_eq!(
        shipped::list(&dirs).iter().filter(|s| s.is_err()).count(),
        1
    );
}

#[test]
fn an_installed_app_hides_a_shipped_one() {
    let root = tmp("shipped-hidden");
    let dirs = Dirs::under(&root);
    copy(&examples("js").join("sand"), &root.join("shipped/sand"));
    let lock = ptyrc_store::Lock {
        apps: vec![
            serde_json::from_value(serde_json::json!({
                "origin": "example.org/a/sand", "id": "sand", "version": "1",
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "tier": "tested", "date": "2026-11-20",
            }))
            .unwrap(),
        ],
    };
    lock.save(&dirs).unwrap();
    let (e, _) = install::resolve(&dirs, "sand").unwrap();
    assert_eq!(e.origin, "example.org/a/sand");
    let (e, s) = install::resolve(&dirs, "ptyrc/sand").unwrap();
    assert_eq!((e.origin.as_str(), s.is_some()), ("ptyrc/sand", true));
    assert_eq!(install::all(&dirs).unwrap().len(), 2);
}

#[test]
fn a_rust_app_built_into_the_cache() {
    let root = tmp("shipped-rust");
    let dirs = Dirs::under(&root);
    copy(
        &examples("rust").join("confetti"),
        &root.join("shipped/confetti"),
    );
    let s = shipped::get(&dirs, "confetti").unwrap().unwrap();
    let e = s.tree(&dirs).unwrap_err();
    assert!(e.message.contains("not built"), "{e}");
    assert!(e.hint.unwrap().contains("ptyrc app ptyrc/confetti"));
    if let Err(e) = Toolchain::find() {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_RUSTC").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_RUSTC=1: {e}"
        );
        eprintln!("skipped: {e}");
        return;
    }
    let mut built = 0;
    let dir = s.build(&dirs, |_| built += 1).unwrap();
    assert_eq!(dir, s.cache(&dirs).join("tree"));
    assert!(dir.join(MODULE).is_file() && dir.join("ptyrc-app.toml").is_file());
    assert_eq!(install::locate(&dirs, "confetti").unwrap().1, dir);
    // Built once; again when the tree changes.
    s.build(&dirs, |_| built += 1).unwrap();
    assert_eq!(built, 1);
    let lib = root.join("shipped/confetti/src/lib.rs");
    let text = std::fs::read_to_string(&lib).unwrap();
    std::fs::write(&lib, format!("{text}\n// changed\n")).unwrap();
    assert!(s.tree(&dirs).is_err());
    s.build(&dirs, |_| built += 1).unwrap();
    assert_eq!(built, 2);
    // Installed beside its tree: that one, nothing built.
    std::fs::write(root.join("shipped/confetti").join(MODULE), b"\0asm").unwrap();
    assert_eq!(s.tree(&dirs).unwrap(), root.join("shipped/confetti"));
}

/// Two `ptyrc app` of a shipped Rust app at once, not built: one
/// builds, the other waits and starts that build.
#[test]
fn a_rust_app_built_once_for_two_at_once() {
    let root = tmp("shipped-twice");
    let dirs = Dirs::under(&root);
    copy(
        &examples("rust").join("confetti"),
        &root.join("shipped/confetti"),
    );
    if let Err(e) = Toolchain::find() {
        assert!(
            std::env::var_os("PTYRC_REQUIRE_RUSTC").is_none_or(|v| v != "1"),
            "PTYRC_REQUIRE_RUSTC=1: {e}"
        );
        eprintln!("skipped: {e}");
        return;
    }
    let s = shipped::get(&dirs, "confetti").unwrap().unwrap();
    let built = std::sync::atomic::AtomicUsize::new(0);
    let dirs_got: Vec<_> = std::thread::scope(|sc| {
        let all: Vec<_> = (0..3)
            .map(|_| {
                sc.spawn(|| {
                    s.build(&dirs, |_| {
                        built.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    })
                })
            })
            .collect();
        all.into_iter()
            .map(|r| r.join().unwrap().unwrap())
            .collect()
    });
    assert_eq!(built.into_inner(), 1);
    assert!(dirs_got.iter().all(|d| d.join(MODULE).is_file()));
}
