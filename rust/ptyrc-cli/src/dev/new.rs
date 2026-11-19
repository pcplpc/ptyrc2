// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev new`: a repository that runs, from `sdk/template/`.

use crate::Fail;
use crate::store::io_text;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

const GPL: &str = include_str!("../../../../LICENSE.txt");
const TEST: &str = include_str!("../../../../sdk/template/test.sh");
const EXPECTED: &str = include_str!("../../../../sdk/template/expected.txt");

/// `(path, text, executable)`; `{{id}}` is replaced.
type File = (&'static str, &'static str, bool);

const COMMON: &[File] = &[
    ("LICENSE.txt", GPL, false),
    ("test.sh", TEST, true),
    ("expected.txt", EXPECTED, false),
];

const SCRIPT: &[File] = &[
    (
        "ptyrc-app.toml",
        include_str!("../../../../sdk/template/script/ptyrc-app.toml"),
        false,
    ),
    (
        "app.js",
        include_str!("../../../../sdk/template/script/app.js"),
        false,
    ),
    (
        "ptyrc.d.ts",
        include_str!("../../../../sdk/js/ptyrc.d.ts"),
        false,
    ),
    (
        "AGENTS.md",
        include_str!("../../../../sdk/template/script/AGENTS.md"),
        false,
    ),
    (
        ".gitignore",
        include_str!("../../../../sdk/template/script/gitignore"),
        false,
    ),
];

const RUST: &[File] = &[
    (
        "ptyrc-app.toml",
        include_str!("../../../../sdk/template/rust/ptyrc-app.toml"),
        false,
    ),
    (
        "Cargo.toml",
        include_str!("../../../../sdk/template/rust/Cargo.toml.in"),
        false,
    ),
    (
        "Cargo.lock",
        include_str!("../../../../sdk/template/rust/Cargo.lock.in"),
        false,
    ),
    (
        "src/lib.rs",
        include_str!("../../../../sdk/template/rust/src/lib.rs"),
        false,
    ),
    (
        "AGENTS.md",
        include_str!("../../../../sdk/template/rust/AGENTS.md"),
        false,
    ),
    (
        ".gitignore",
        include_str!("../../../../sdk/template/rust/gitignore"),
        false,
    ),
    (
        "vendor/ptyrc-app/README.md",
        include_str!("../../../../sdk/rust/ptyrc-app/README.md"),
        false,
    ),
    (
        "vendor/ptyrc-app/src/lib.rs",
        include_str!("../../../../sdk/rust/ptyrc-app/src/lib.rs"),
        false,
    ),
    (
        "vendor/ptyrc-app/src/ffi.rs",
        include_str!("../../../../sdk/rust/ptyrc-app/src/ffi.rs"),
        false,
    ),
];

const VENDOR_TOML: &str = include_str!("../../../../sdk/rust/ptyrc-app/Cargo.toml");

/// What `[app] id` accepts: lower-case letters, digits, `-`, a letter
/// first, 64 at most.
fn is_id(id: &str) -> bool {
    id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn new(dir: &Path, rust: bool) -> Result<ExitCode, Fail> {
    let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !is_id(id) {
        return Err(Fail::usage(
            format!("{}: not an app id", dir.display()),
            "the last part of NAME is the id: lower-case letters, digits and `-`, a letter first",
        ));
    }
    let empty = match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(e) => return Err(Fail::from(format!("{}: {}", dir.display(), io_text(&e)))),
    };
    if !empty {
        return Err(Fail::usage(
            format!("{}: exists and is not empty", dir.display()),
            "a new directory",
        ));
    }
    // The vendored crate is a member of the app's workspace.
    let vendor_toml = VENDOR_TOML.replace("\n[workspace]\n", "\n");
    let mut files: Vec<(&str, String, bool)> = COMMON
        .iter()
        .chain(if rust { RUST } else { SCRIPT })
        .map(|(p, t, x)| (*p, t.replace("{{id}}", id), *x))
        .collect();
    if rust {
        files.push(("vendor/ptyrc-app/Cargo.toml", vendor_toml, false));
    }
    for (rel, text, exec) in &files {
        let path = dir.join(rel);
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(path.parent().unwrap_or(dir))?;
            std::fs::write(&path, text)?;
            if *exec {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
            Ok(())
        };
        write().map_err(|e| Fail {
            status: 4,
            message: format!("{}: {}", path.display(), io_text(&e)),
            hint: None,
        })?;
    }
    let git = Command::new("git")
        .args(["init", "--quiet"])
        .arg(dir)
        .stdin(Stdio::null())
        .status();
    if !git.is_ok_and(|s| s.success()) {
        eprintln!(
            "ptyrc: git init failed: {} is not a repository",
            dir.display()
        );
    }
    let kind = if rust { "a Rust app" } else { "a script app" };
    println!("{}: {kind}, {} files", dir.display(), files.len());
    println!("next: cd {} && ./test.sh; read AGENTS.md", dir.display());
    Ok(ExitCode::SUCCESS)
}
