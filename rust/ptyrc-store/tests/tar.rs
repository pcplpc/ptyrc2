// SPDX-License-Identifier: GPL-3.0-or-later
//! The archive of a hosted app: pack, parse, every refusal.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{GPL, MANIFEST, tmp};
use ptyrc_store::tar::{MAX_ENTRIES, pack};
use ptyrc_store::{DirTree, ErrorKind, Kind, TarTree, Tree, check, sha256_hex};

/// A valid script app in `dir`, `extra` files beside.
fn app_dir(dir: &Path, extra: &[(&str, &[u8])]) -> PathBuf {
    let files: Vec<(&str, &[u8])> = vec![
        ("ptyrc-app.toml", MANIFEST.as_bytes()),
        ("LICENSE", GPL),
        ("app.js", b"export function init(args) {}\n"),
        ("assets/grain.txt", b"."),
    ];
    for (p, b) in files.iter().chain(extra) {
        let p = dir.join(p);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b).unwrap();
    }
    dir.to_path_buf()
}

fn packed(dir: &Path) -> Vec<u8> {
    let t = DirTree::new(dir);
    pack(&t, &check(&t).unwrap()).unwrap()
}

fn gz(bytes: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(bytes).unwrap();
    e.finish().unwrap()
}

fn sum(h: &mut [u8]) {
    h[148..156].fill(b' ');
    let s: u32 = h.iter().map(|&b| b as u32).sum();
    h[148..155].copy_from_slice(format!("{s:06o}\0").as_bytes());
}

/// A ustar header, its checksum right.
fn hdr(name: &str, flag: u8, size: u64) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[100..108].copy_from_slice(b"0000644\0");
    h[108..116].copy_from_slice(b"0000000\0");
    h[116..124].copy_from_slice(b"0000000\0");
    h[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
    h[136..148].copy_from_slice(b"00000000000\0");
    h[156] = flag;
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    sum(&mut h);
    h
}

/// Entries `(header, data)`, then two zero blocks.
fn raw(entries: &[(Vec<u8>, &[u8])]) -> Vec<u8> {
    let mut t = Vec::new();
    for (h, d) in entries {
        t.extend_from_slice(h);
        t.extend_from_slice(d);
        t.resize(t.len().next_multiple_of(512), 0);
    }
    t.resize(t.len() + 1024, 0);
    t
}

fn file(name: &str, d: &'static [u8]) -> (Vec<u8>, &'static [u8]) {
    (hdr(name, b'0', d.len() as u64), d)
}

fn refused(tar: &[u8], what: &str) {
    let e = TarTree::parse(&gz(tar)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Tree, "{what}: {e}");
}

#[test]
fn round_trip() {
    let dir = app_dir(&tmp("tar-rt"), &[("src/deep/x.txt", b"x")]);
    let bytes = packed(&dir);
    let t = TarTree::parse(&bytes).unwrap();
    let c = check(&t).unwrap();
    assert_eq!(c.manifest.app.id, "sand-garden");
    let orig = check(&DirTree::new(&dir)).unwrap();
    let paths = |n: &[ptyrc_store::Node]| n.iter().map(|n| n.path.clone()).collect::<Vec<_>>();
    assert_eq!(paths(&c.files), paths(&orig.files));
    assert_eq!(paths(&c.dirs), paths(&orig.dirs));
    for n in &c.files {
        assert_eq!(
            t.read(n).unwrap(),
            std::fs::read(dir.join(&n.path)).unwrap()
        );
    }
    // Packed again from the archive: the same bytes.
    assert_eq!(pack(&t, &c).unwrap(), bytes);
}

#[test]
fn deterministic() {
    let a = app_dir(&tmp("tar-det-a"), &[("z.txt", b"z"), ("b/a.txt", b"a")]);
    let b = tmp("tar-det-b");
    // Another order of creation, other times and modes.
    std::fs::create_dir_all(b.join("b")).unwrap();
    std::fs::write(b.join("b/a.txt"), b"a").unwrap();
    std::fs::write(b.join("z.txt"), b"z").unwrap();
    app_dir(&b, &[]);
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(b.join("app.js"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let (x, y) = (packed(&a), packed(&b));
    assert_eq!(x, y);
    assert_eq!(sha256_hex(&x), sha256_hex(&y));
    assert_eq!(sha256_hex(&x).len(), 64);
    // gzip: no name, mtime 0, OS 255.
    assert_eq!(&x[..10], &[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 2, 255]);
    let tar = {
        let mut d = flate2::read::GzDecoder::new(&x[..]);
        let mut v = Vec::new();
        std::io::Read::read_to_end(&mut d, &mut v).unwrap();
        v
    };
    // Sorted; a directory 0755 with `/`, a file 0644, owner 0, no names.
    let names: Vec<String> = tar
        .chunks(512)
        .filter(|h| &h[257..263] == b"ustar\0")
        .map(|h| {
            String::from_utf8_lossy(&h[..100])
                .trim_end_matches('\0')
                .to_string()
        })
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert!(names.contains(&"b/".to_string()));
    let h = &tar[..512];
    assert_eq!(&h[..13], b"LICENSE\0\0\0\0\0\0");
    assert_eq!(&h[100..108], b"0000644\0");
    assert_eq!(&h[108..124], b"0000000\x000000000\0");
    assert_eq!(&h[136..148], b"00000000000\0");
    assert!(h[265..329].iter().all(|&b| b == 0));
}

#[test]
fn long_paths() {
    let d1 = format!("{}/{}", "d".repeat(60), "e".repeat(60));
    let long = format!("{d1}/{}.txt", "f".repeat(90));
    let dir = app_dir(&tmp("tar-long"), &[(&long, b"long")]);
    let t = TarTree::parse(&packed(&dir)).unwrap();
    let c = check(&t).unwrap();
    let n = c
        .files
        .iter()
        .find(|n| n.path == long)
        .expect("the long path");
    assert_eq!(t.read(n).unwrap(), b"long");
    assert!(c.dirs.iter().any(|n| n.path == d1));
    // One component past 100 bytes: no place in a header.
    let dir = app_dir(&tmp("tar-toolong"), &[(&"g".repeat(120), b"x")]);
    let tree = DirTree::new(&dir);
    let e = pack(&tree, &check(&tree).unwrap()).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Tree);
    assert!(e.hint.unwrap().contains("155"));
}

#[test]
fn hand_built_parses() {
    let t = raw(&[
        file("a.txt", b"hello"),
        (hdr("d/", b'5', 0), b""),
        file("d/e/f.txt", b""),
        (hdr("old.txt", 0, 1), b"x"),
    ]);
    let tree = TarTree::parse(&gz(&t)).unwrap();
    let mut nodes = tree.nodes().unwrap();
    nodes.sort_by(|a, b| a.path.cmp(&b.path));
    let got: Vec<(&str, Kind)> = nodes.iter().map(|n| (n.path.as_str(), n.kind)).collect();
    assert_eq!(
        got,
        [
            ("a.txt", Kind::File),
            ("d", Kind::Dir),
            ("d/e", Kind::Dir),
            ("d/e/f.txt", Kind::File),
            ("old.txt", Kind::File),
        ]
    );
    // EOF in place of the end blocks; one end block.
    let mut short = raw(&[file("a.txt", b"hello")]);
    short.truncate(1024);
    assert!(TarTree::parse(&gz(&short)).is_ok());
    let mut one = raw(&[file("a.txt", b"hello")]);
    one.truncate(1536);
    assert!(TarTree::parse(&gz(&one)).is_ok());
    // `tar -C dir .`: `./` skipped, dropped before each path; GNU magic.
    let mut g = hdr("./d/b.txt", b'0', 1);
    g[257..265].copy_from_slice(b"ustar  \0");
    sum(&mut g);
    let dot = raw(&[
        (hdr("./", b'5', 0), b""),
        (hdr("./d/", b'5', 0), b""),
        (g, b"b"),
    ]);
    let tree = TarTree::parse(&gz(&dot)).unwrap();
    let mut got: Vec<String> = tree.nodes().unwrap().into_iter().map(|n| n.path).collect();
    got.sort();
    assert_eq!(got, ["d", "d/b.txt"]);
    // Zeros after the end: ignored.
    let mut z = raw(&[file("a.txt", b"hello")]);
    z.resize(z.len() + 8192, 0);
    assert!(TarTree::parse(&gz(&z)).is_ok());
}

#[test]
fn refusals() {
    for flag in [
        b'1', b'2', b'3', b'4', b'6', b'7', b'x', b'g', b'L', b'K', b'S', b'V', b'M',
    ] {
        refused(
            &raw(&[(hdr("a", flag, 0), b""), file("b", b"b")]),
            &format!("type {}", flag as char),
        );
    }
    let mut h = hdr("a.txt", b'0', 1);
    h[0] = b'b';
    refused(&raw(&[(h, b"x")]), "checksum");
    let mut h = hdr("a.txt", b'0', 0);
    h[124] = 0x80;
    h[135] = 1;
    sum(&mut h);
    refused(&raw(&[(h, b"x")]), "base-256 size");
    let mut h = hdr("a.txt", b'0', 0);
    h[124..136].copy_from_slice(b"0000000009\0\0");
    sum(&mut h);
    refused(&raw(&[(h, b"")]), "size not octal");
    let mut h = hdr("a.txt", b'0', 0);
    h[257..265].copy_from_slice(b"ustar\0\0\0");
    sum(&mut h);
    refused(&raw(&[(h, b"")]), "no version");
    for p in [
        "../a",
        "/etc/passwd",
        "a//b",
        "a/./b",
        "a\x01b",
        ".git/config",
        "a\\b",
        "././a",
        "./../a",
        "./.git/x",
    ] {
        refused(&raw(&[file(p, b"")]), p);
    }
    refused(&raw(&[file("a", b"1"), file("a", b"2")]), "a duplicate");
    refused(
        &raw(&[file("a", b"1"), file("./a", b"2")]),
        "a duplicate by `./`",
    );
    refused(&raw(&[(hdr("./", b'5', 1), b"x")]), "`./` of a size");
    refused(
        &raw(&[(hdr("d/", b'5', 0), b""), (hdr("d", b'5', 0), b"")]),
        "a duplicate dir",
    );
    refused(
        &raw(&[file("a", b"1"), file("a/b", b"2")]),
        "a file as a parent",
    );
    refused(
        &raw(&[file("a/b", b"1"), file("a", b"2")]),
        "a parent as a file",
    );
    refused(
        &raw(&[(hdr("d/", b'5', 3), b"abc")]),
        "a directory of a size",
    );
    let mut g = raw(&[file("a", b"1")]);
    g.extend_from_slice(b"garbage");
    refused(&g, "garbage after the end");
    let mut g = raw(&[file("a", b"1")]);
    g.truncate(1024);
    g.extend_from_slice(&[0; 512]);
    g.extend_from_slice(&hdr("b", b'0', 0));
    refused(&g, "an entry after an end block");
    let names: Vec<String> = (0..=MAX_ENTRIES).map(|i| format!("f{i}")).collect();
    let many: Vec<_> = names.iter().map(|n| file(n, b"")).collect();
    refused(&raw(&many), "too many entries");
    let ok: Vec<_> = names[..MAX_ENTRIES].iter().map(|n| file(n, b"")).collect();
    assert!(TarTree::parse(&gz(&raw(&ok))).is_ok());
}

#[test]
fn bomb_and_truncation() {
    // 17 MiB of zeros in one file: past the cap, refused while inflating.
    let mut big = hdr("big", b'0', 17 << 20).clone();
    big.resize(512 + (17 << 20) + 1024, 0);
    let packed = gz(&big);
    assert!(packed.len() < 1 << 20);
    let e = TarTree::parse(&packed).unwrap_err();
    assert!(e.message.contains("inflated"), "{e}");
    // A gzip cut short; not gzip at all; a header cut short; data cut short.
    let t = gz(&raw(&[file("a.txt", b"hello")]));
    assert!(TarTree::parse(&t[..t.len() / 2]).is_err());
    assert!(TarTree::parse(&t[..t.len() - 4]).is_err());
    assert!(TarTree::parse(b"not gzip").is_err());
    assert!(TarTree::parse(&[]).is_err());
    let r = raw(&[file("a.txt", b"hello")]);
    refused(&r[..300], "a header cut short");
    let h = hdr("a.txt", b'0', 4096);
    refused(&h, "data cut short");
}

fn have(tool: &str, arg: &str) -> bool {
    let ok = Command::new(tool)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("note: {tool} absent; skipped");
    }
    ok
}

/// What other tools write, in ustar, parses and checks.
#[test]
fn other_tools() {
    let dir = app_dir(&tmp("tar-tools"), &[("src/x.txt", b"x")]);
    let out = tmp("tar-tools-out");
    if have("python3", "--version") {
        let p = out.join("py.tar.gz");
        let script = "import sys, tarfile\n\
            src, dst = sys.argv[1], sys.argv[2]\n\
            with tarfile.open(dst, 'w:gz', format=tarfile.USTAR_FORMAT) as t:\n\
            \x20   for n in ['ptyrc-app.toml', 'LICENSE', 'app.js', 'assets', 'src']:\n\
            \x20       t.add(src + '/' + n, arcname=n)\n";
        let st = Command::new("python3")
            .args(["-I", "-c", script])
            .arg(&dir)
            .arg(&p)
            .status()
            .unwrap();
        assert!(st.success());
        let t = TarTree::parse(&std::fs::read(&p).unwrap()).unwrap();
        let c = check(&t).unwrap();
        assert!(c.files.iter().any(|n| n.path == "src/x.txt"));
    }
    if have("tar", "--version") {
        let p = out.join("dot.tar.gz");
        let st = Command::new("tar")
            .arg("-C")
            .arg(&dir)
            .args(["--format=ustar", "-czf"])
            .arg(&p)
            .arg(".")
            .status()
            .unwrap();
        assert!(st.success());
        let t = TarTree::parse(&std::fs::read(&p).unwrap()).unwrap();
        check(&t).unwrap();
        let p = out.join("gnu.tar.gz");
        let st = Command::new("tar")
            .arg("-C")
            .arg(&dir)
            .args(["--format=ustar", "-czf"])
            .arg(&p)
            .args(["ptyrc-app.toml", "LICENSE", "app.js", "assets", "src"])
            .status()
            .unwrap();
        assert!(st.success());
        let t = TarTree::parse(&std::fs::read(&p).unwrap()).unwrap();
        let c = check(&t).unwrap();
        assert!(c.dirs.iter().any(|n| n.path == "assets"));
    }
}
