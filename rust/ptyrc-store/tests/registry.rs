// SPDX-License-Identifier: GPL-3.0-or-later
//! Pointer files, `store.toml`, the choice of a version by tier.

mod common;

use common::tmp;
use ptyrc_store::registry::{Pointer, Registries, tier_of};
use ptyrc_store::{Dirs, ErrorKind, Origin, Reference, Tier, Want, choose};

fn c(ch: char) -> String {
    ch.to_string().repeat(40)
}

fn pointer(versions: &str) -> Pointer {
    let src = format!(
        "origin = \"https://example.org/someone/sand-garden\"\nmanifest = \"ptyrc-app.toml\"\n{versions}"
    );
    Pointer::parse("p.toml", &src).unwrap()
}

fn pick(p: Option<&Pointer>, want: Want) -> (Reference, Tier) {
    let r = choose(p, &want).unwrap();
    (r.reference, r.tier)
}

#[test]
fn best_tier_newest() {
    let p = pointer(&format!(
        r#"
[[version]]
commit = "{a}"
tier = "approved"
by = "maintainer"
date = 2026-11-17

[[version]]
commit = "{b}"
tier = "untested"
date = 2026-11-19

[[version]]
commit = "{c}"
tier = "approved"
date = 2026-11-18

[[version]]
commit = "{d}"
tier = "tested"
date = 2026-11-19
"#,
        a = c('a'),
        b = c('b'),
        c = c('c'),
        d = c('d'),
    ));
    assert_eq!(
        pick(Some(&p), Want::default()),
        (Reference::Commit(c('c')), Tier::Approved)
    );
    let latest = Want {
        latest: true,
        ..Want::default()
    };
    assert_eq!(pick(Some(&p), latest), (Reference::Head, Tier::Untested));
    let at = |r: Reference| Want {
        reference: Some(r),
        ..Want::default()
    };
    assert_eq!(
        pick(Some(&p), at(Reference::Commit(c('a')))),
        (Reference::Commit(c('a')), Tier::Approved)
    );
    assert_eq!(
        pick(Some(&p), at(Reference::Commit(c('e')))),
        (Reference::Commit(c('e')), Tier::Untested)
    );
    assert_eq!(
        pick(Some(&p), at(Reference::Name("v1".into()))).1,
        Tier::Untested
    );
    assert_eq!(tier_of(Some(&p), &c('d'), false).unwrap(), Tier::Tested);
    assert_eq!(tier_of(Some(&p), &c('f'), false).unwrap(), Tier::Untested);
    assert_eq!(tier_of(None, &c('a'), false).unwrap(), Tier::Untested);
    assert_eq!(
        pick(None, Want::default()),
        (Reference::Head, Tier::Untested)
    );
}

#[test]
fn ties_and_yanked() {
    let p = pointer(&format!(
        "[[version]]\ncommit = \"{}\"\ntier = \"tested\"\n\n[[version]]\ncommit = \"{}\"\ntier = \"tested\"\n\n[[version]]\ncommit = \"{}\"\ntier = \"approved\"\nyanked = true\n",
        c('a'),
        c('b'),
        c('c')
    ));
    // Equal tiers, no dates: the later in the file. The yanked approved
    // one is passed over.
    assert_eq!(
        pick(Some(&p), Want::default()),
        (Reference::Commit(c('b')), Tier::Tested)
    );
    let at_c = Want {
        reference: Some(Reference::Commit(c('c'))),
        ..Want::default()
    };
    assert_eq!(
        choose(Some(&p), &at_c).unwrap_err().kind,
        ErrorKind::Registry
    );
    let forced = Want {
        force: true,
        ..at_c
    };
    assert_eq!(pick(Some(&p), forced).1, Tier::Approved);
    assert!(tier_of(Some(&p), &c('c'), false).is_err());

    let only_yanked = pointer(&format!(
        "[[version]]\ncommit = \"{}\"\ntier = \"approved\"\nyanked = true\n",
        c('a')
    ));
    assert_eq!(
        pick(Some(&only_yanked), Want::default()),
        (Reference::Head, Tier::Untested)
    );

    let app_yanked = pointer("yanked = true\n");
    assert!(choose(Some(&app_yanked), &Want::default()).is_err());
    let forced = Want {
        force: true,
        ..Want::default()
    };
    assert_eq!(
        pick(Some(&app_yanked), forced),
        (Reference::Head, Tier::Untested)
    );
}

#[test]
fn bad_pointers() {
    for src in [
        "origin = \"http://example.org/a/b\"\n",
        "origin = \"ext::sh\"\n",
        "origin = \"https://example.org/a/b\"\nmanifest = \"app.toml\"\n",
        "origin = \"https://example.org/a/b\"\nmanifest = \"../ptyrc-app.toml\"\n",
        "origin = \"https://example.org/a/b\"\n[[version]]\ncommit = \"abc\"\ntier = \"approved\"\n",
        "origin = \"https://example.org/a/b\"\n[[version]]\ncommit = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\ntier = \"blessed\"\n",
        "origin = \"https://example.org/a/b\"\nextra = 1\n",
    ] {
        let e = Pointer::parse("p.toml", src).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Registry, "{src}");
    }
}

#[test]
fn registries_in_order() {
    let root = tmp("registry");
    let dirs = Dirs::under(&root.join("xdg"));
    let team = root.join("team");
    let main = root.join("main");
    let write = |dir: &std::path::Path, rel: &str, text: &str| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    write(
        &team,
        "apps/example.org/a/cat.toml",
        &format!(
            "origin = \"https://example.org/a/cat\"\n[[version]]\ncommit = \"{}\"\ntier = \"tested\"\n",
            c('1')
        ),
    );
    write(
        &main,
        "apps/example.org/a/cat.toml",
        &format!(
            "origin = \"https://example.org/a/cat\"\n[[version]]\ncommit = \"{}\"\ntier = \"approved\"\n",
            c('2')
        ),
    );
    write(
        &main,
        "apps/example.org/a/mono/apps/sand.toml",
        "origin = \"https://example.org/a/mono\"\nmanifest = \"apps/sand/ptyrc-app.toml\"\n",
    );
    // A file in the wrong place speaks for another origin: refused.
    write(
        &main,
        "apps/example.org/a/liar.toml",
        "origin = \"https://example.org/a/cat\"\n",
    );
    let store = format!(
        "[[registry]]\nname = \"team\"\npath = \"{}\"\n\n[[registry]]\nname = \"main\"\npath = \"{}\"\n",
        team.display(),
        main.display()
    );
    let regs = Registries::parse(&dirs, &store).unwrap();
    let cat = Origin::parse("example.org/a/cat").unwrap();
    let (r, p) = regs.lookup(&cat).unwrap().unwrap();
    assert_eq!(r.name, "team");
    assert_eq!(pick(Some(&p), Want::default()).1, Tier::Tested);
    let sand = Origin::parse("example.org/a/mono#apps/sand").unwrap();
    let (r, _) = regs.lookup(&sand).unwrap().unwrap();
    assert_eq!(r.name, "main");
    assert!(
        regs.lookup(&Origin::parse("example.org/a/none").unwrap())
            .unwrap()
            .is_none()
    );
    let e = regs
        .lookup(&Origin::parse("example.org/a/liar").unwrap())
        .unwrap_err();
    assert!(e.message.contains("speaks for"), "{e}");

    // Loaded from <config>/ptyrc/store.toml; absent: none.
    assert!(Registries::load(&dirs).unwrap().list.is_empty());
    std::fs::create_dir_all(dirs.store().parent().unwrap()).unwrap();
    std::fs::write(dirs.store(), &store).unwrap();
    assert_eq!(Registries::load(&dirs).unwrap(), regs);

    // A registry by URL not yet fetched: an error, not skipped.
    let url = Registries::parse(
        &dirs,
        "[[registry]]\nname = \"far\"\nurl = \"https://example.org/reg\"\n",
    )
    .unwrap();
    assert_eq!(url.list[0].dir, dirs.registry().join("far"));
    assert!(url.lookup(&cat).is_err());

    for bad in [
        "[[registry]]\nname = \"x\"\n",
        "[[registry]]\nname = \"x\"\nurl = \"http://e.org/r\"\n",
        "[[registry]]\nname = \"x\"\npath = \"relative\"\n",
        "[[registry]]\nname = \"../x\"\npath = \"/r\"\n",
        "[[registry]]\nname = \"x\"\npath = \"/r\"\n[[registry]]\nname = \"x\"\npath = \"/s\"\n",
    ] {
        assert!(Registries::parse(&dirs, bad).is_err(), "{bad}");
    }
}

#[test]
fn registry_pointers() {
    let root = tmp("pointers");
    let dirs = Dirs::under(&root.join("xdg"));
    let reg = root.join("reg");
    let regs = Registries::parse(
        &dirs,
        &format!("[[registry]]\nname = \"r\"\npath = \"{}\"\n", reg.display()),
    )
    .unwrap();
    assert!(regs.list[0].pointers().unwrap().is_empty(), "no apps/");
    let put = |rel: &str, origin: &str, manifest: &str| {
        let p = reg.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(
            p,
            format!("origin = \"{origin}\"\nmanifest = \"{manifest}\"\n"),
        )
        .unwrap();
    };
    put(
        "apps/example.org/b/dog.toml",
        "https://example.org/b/dog",
        "ptyrc-app.toml",
    );
    put(
        "apps/example.org/a/cat/x.toml",
        "https://example.org/a/cat",
        "x/ptyrc-app.toml",
    );
    std::fs::write(reg.join("apps/README"), "not a pointer").unwrap();
    let got: Vec<String> = regs.list[0]
        .pointers()
        .unwrap()
        .iter()
        .map(|p| p.origin().unwrap().id())
        .collect();
    assert_eq!(got, ["example.org/a/cat#x", "example.org/b/dog"]);
    put(
        "apps/example.org/c/elsewhere.toml",
        "https://example.org/b/dog",
        "ptyrc-app.toml",
    );
    let e = regs.list[0].pointers().unwrap_err();
    assert_eq!(e.kind, ErrorKind::Registry);
    assert!(e.message.contains("elsewhere"), "{e}");
}
