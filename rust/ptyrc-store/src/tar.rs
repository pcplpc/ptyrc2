// SPDX-License-Identifier: GPL-3.0-or-later
//! The archive of a hosted app: gzip over ustar, in memory.
//!
//! [`pack`] writes the checked files and directories of a tree, sorted,
//! with fixed metadata: one tree, one byte string, one sha256.
//! [`TarTree::parse`] reads one strictly: inflated to [`MAX_INFLATED`] at
//! most; ustar (POSIX or old GNU magic); regular files and directories
//! only; checksums verified; sizes in octal; one leading `./` dropped and
//! the entry `./` skipped (`tar -C dir .`); paths then by the rule of
//! [`crate::tree::check`]; no duplicate; [`MAX_ENTRIES`] at most.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};

use sha2::{Digest, Sha256};

use crate::error::{Error, ErrorKind, Result, check_rel_path};
use crate::tree::{Checked, Kind, Node, Tree};

/// Bound on the inflated archive, bytes.
pub const MAX_INFLATED: usize = 16 << 20;
/// Bound on the entries of an archive.
pub const MAX_ENTRIES: usize = 1024;

const BLOCK: usize = 512;
const NAME: usize = 100;
const PREFIX: usize = 155;

fn tar_err(msg: impl Into<String>, hint: impl Into<String>) -> Error {
    Error::new(ErrorKind::Tree, msg).with_hint(hint)
}

/// Lower-case hex of the sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `(prefix, name)` of a path in a ustar header.
fn split(path: &str) -> Result<(&str, &str)> {
    if path.len() <= NAME {
        return Ok(("", path));
    }
    for (i, _) in path.match_indices('/') {
        let (prefix, name) = (&path[..i], &path[i + 1..]);
        if prefix.len() <= PREFIX && !name.is_empty() && name.len() <= NAME {
            return Ok((prefix, name));
        }
    }
    Err(tar_err(
        format!("{path}: too long for ustar"),
        "a path of at most 155 bytes of directories, then 100 bytes after a `/`",
    ))
}

fn octal(field: &mut [u8], v: u64) {
    let digits = field.len() - 1;
    let s = format!("{v:0digits$o}");
    field[..digits].copy_from_slice(s.as_bytes());
    field[digits] = 0;
}

fn header(path: &str, dir: bool, size: u64) -> Result<[u8; BLOCK]> {
    let mut h = [0u8; BLOCK];
    let (prefix, name) = split(path)?;
    h[..name.len()].copy_from_slice(name.as_bytes());
    octal(&mut h[100..108], if dir { 0o755 } else { 0o644 });
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size);
    octal(&mut h[136..148], 0);
    h[156] = if dir { b'5' } else { b'0' };
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    octal(&mut h[329..337], 0);
    octal(&mut h[337..345], 0);
    h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    let sum = checksum(&h);
    h[148..154].copy_from_slice(format!("{sum:06o}").as_bytes());
    h[154] = 0;
    h[155] = b' ';
    Ok(h)
}

/// The sum of the header's bytes, its checksum field as spaces.
fn checksum(h: &[u8]) -> u64 {
    h.iter()
        .enumerate()
        .map(|(i, &b)| {
            if (148..156).contains(&i) {
                32
            } else {
                b as u64
            }
        })
        .sum()
}

/// A deterministic `.tar.gz` of the files and directories of `checked`,
/// read from `tree`.
pub fn pack(tree: &dyn Tree, checked: &Checked) -> Result<Vec<u8>> {
    let mut all: Vec<&Node> = checked.files.iter().chain(&checked.dirs).collect();
    all.sort_by(|a, b| a.path.cmp(&b.path));
    let mut tar = Vec::new();
    for n in all {
        if n.kind == Kind::Dir {
            tar.extend_from_slice(&header(&format!("{}/", n.path), true, 0)?);
            continue;
        }
        let data = tree.read(n)?;
        if data.len() as u64 != n.size {
            return Err(tar_err(
                format!("{}: changed while read", n.path),
                "check again",
            ));
        }
        tar.extend_from_slice(&header(&n.path, false, n.size)?);
        tar.extend_from_slice(&data);
        tar.resize(tar.len().next_multiple_of(BLOCK), 0);
    }
    tar.resize(tar.len() + 2 * BLOCK, 0);
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(Vec::new(), flate2::Compression::best());
    gz.write_all(&tar)
        .and_then(|_| gz.finish())
        .map_err(|e| Error::io("gzip", e))
}

/// Inflates `bytes`, refusing past `cap` bytes; never holds more.
fn inflate(bytes: &[u8], cap: usize) -> Result<Vec<u8>> {
    let mut gz = flate2::read::GzDecoder::new(bytes);
    let mut out: Vec<u8> = Vec::new();
    let mut buf = [0u8; 64 << 10];
    loop {
        let n = gz
            .read(&mut buf)
            .map_err(|e| tar_err(format!("archive: not gzip, or cut short: {e}"), "a .tar.gz"))?;
        if n == 0 {
            return Ok(out);
        }
        if out.len() + n > cap {
            return Err(tar_err(
                format!("archive: more than {cap} bytes inflated"),
                "16 MiB at most, unpacked",
            ));
        }
        if out.capacity() < out.len() + n {
            let want = (out.capacity() * 2).max(out.len() + n).min(cap);
            out.reserve_exact(want - out.len());
        }
        out.extend_from_slice(&buf[..n]);
    }
}

/// A NUL-terminated field.
fn cstr(f: &[u8]) -> &[u8] {
    f.iter().position(|&b| b == 0).map_or(f, |i| &f[..i])
}

/// An octal field: spaces before, digits, NUL or spaces after.
fn parse_octal(f: &[u8], what: &str) -> Result<u64> {
    let bad = || {
        tar_err(
            format!("archive: {what}: not an octal number"),
            "a ustar archive",
        )
    };
    if f.first().is_some_and(|b| b & 0x80 != 0) {
        return Err(bad());
    }
    let s = std::str::from_utf8(cstr(f)).map_err(|_| bad())?;
    let s = s.trim_matches(' ');
    if s.is_empty() || s.len() > 12 || !s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return Err(bad());
    }
    u64::from_str_radix(s, 8).map_err(|_| bad())
}

/// A tree read from a `.tar.gz`.
#[derive(Debug, Clone)]
pub struct TarTree {
    nodes: Vec<Node>,
    /// Path to the range of its content in `data`.
    files: BTreeMap<String, (usize, usize)>,
    data: Vec<u8>,
}

impl TarTree {
    /// Inflates and reads a `.tar.gz`; see the module.
    pub fn parse(bytes: &[u8]) -> Result<TarTree> {
        let data = inflate(bytes, MAX_INFLATED)?;
        let hint = "a ustar archive of regular files and directories";
        let mut nodes: Vec<Node> = Vec::new();
        let mut files = BTreeMap::new();
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        let mut implied: BTreeSet<String> = BTreeSet::new();
        let mut at = 0;
        loop {
            if at == data.len() {
                break;
            }
            let Some(h) = data.get(at..at + BLOCK) else {
                return Err(tar_err("archive: cut short in a header", hint));
            };
            if h.iter().all(|&b| b == 0) {
                if data[at..].iter().any(|&b| b != 0) {
                    return Err(tar_err("archive: data after the end blocks", hint));
                }
                break;
            }
            let stored = parse_octal(&h[148..156], "checksum")?;
            if stored != checksum(h) {
                return Err(tar_err("archive: a header's checksum is wrong", hint));
            }
            let posix = &h[257..263] == b"ustar\0" && &h[263..265] == b"00";
            if !posix && &h[257..265] != b"ustar  \0" {
                return Err(tar_err(
                    "archive: not ustar (magic `ustar\\0` and `00`, or `ustar  \\0`)",
                    "tar --format=ustar, or tarfile.USTAR_FORMAT",
                ));
            }
            let kind = match h[156] {
                b'0' | 0 => Kind::File,
                b'5' => Kind::Dir,
                t => {
                    return Err(tar_err(
                        format!("archive: an entry of type {:?}", t as char),
                        "regular files and directories only: no link, device, fifo, pax or GNU header",
                    ));
                }
            };
            let size = parse_octal(&h[124..136], "size")?;
            let (name, prefix) = (cstr(&h[..NAME]), cstr(&h[345..500]));
            let mut path = Vec::with_capacity(prefix.len() + 1 + name.len());
            if !prefix.is_empty() {
                path.extend_from_slice(prefix);
                path.push(b'/');
            }
            path.extend_from_slice(name);
            let path = String::from_utf8(path)
                .map_err(|_| tar_err("archive: a path not in UTF-8", "file names in UTF-8"))?;
            let path = match kind {
                Kind::Dir => path.strip_suffix('/').unwrap_or(&path),
                _ => &path,
            };
            // `tar -C dir .`: `./` before every path, and `./` itself.
            if kind == Kind::Dir && path == "." {
                if size != 0 {
                    return Err(tar_err("archive: ./: a size", hint));
                }
                at += BLOCK;
                continue;
            }
            let path = path.strip_prefix("./").unwrap_or(path).to_string();
            check_rel_path(&path).map_err(|why| {
                tar_err(
                    format!("archive: {path:?}: a path with {why}"),
                    "plain relative names; no `..`, `.git`, `./`, backslash or control character",
                )
            })?;
            at += BLOCK;
            let len = match kind {
                Kind::Dir if size != 0 => {
                    return Err(tar_err(format!("archive: {path}/: a size"), hint));
                }
                _ => usize::try_from(size).unwrap_or(usize::MAX),
            };
            if len.next_multiple_of(BLOCK) > data.len() - at {
                return Err(tar_err(format!("archive: {path}: cut short"), hint));
            }
            let taken = files.contains_key(&path) || dirs.contains(&path);
            if taken || (kind == Kind::File && implied.contains(&path)) {
                return Err(tar_err(format!("archive: {path}: twice"), "each path once"));
            }
            if nodes.len() >= MAX_ENTRIES {
                return Err(tar_err(
                    format!("archive: more than {MAX_ENTRIES} entries"),
                    "512 files at most in an app tree",
                ));
            }
            let mut parent = path.as_str();
            while let Some((p, _)) = parent.rsplit_once('/') {
                if files.contains_key(p) {
                    return Err(tar_err(
                        format!("archive: {p}: a file and a directory"),
                        hint,
                    ));
                }
                implied.insert(p.to_string());
                parent = p;
            }
            match kind {
                Kind::Dir => {
                    dirs.insert(path.clone());
                }
                _ => {
                    files.insert(path.clone(), (at, len));
                }
            }
            nodes.push(Node {
                path,
                kind,
                size,
                id: String::new(),
            });
            at += len.next_multiple_of(BLOCK);
        }
        for p in implied {
            if !dirs.contains(&p) {
                nodes.push(Node {
                    path: p,
                    kind: Kind::Dir,
                    size: 0,
                    id: String::new(),
                });
            }
        }
        Ok(TarTree { nodes, files, data })
    }
}

impl Tree for TarTree {
    fn nodes(&self) -> Result<Vec<Node>> {
        Ok(self.nodes.clone())
    }

    fn read(&self, node: &Node) -> Result<Vec<u8>> {
        match self.files.get(&node.path) {
            Some(&(at, len)) if len as u64 <= node.size => Ok(self.data[at..at + len].to_vec()),
            _ => Err(tar_err(
                format!("{}: not a file of the archive", node.path),
                "regular files only",
            )),
        }
    }
}
