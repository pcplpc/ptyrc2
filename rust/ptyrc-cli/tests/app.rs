// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc app`, `ptyrc python`: `ptyrc-app`, `ptyrc-python` found and run.

mod common;

use common::Sandbox;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A stand-in for `ptyrc-app` in `dir`: prints its tag, session and
/// arguments.
fn fake(dir: &Path, tag: &str) {
    fake_named(dir, "ptyrc-app", tag);
}

fn fake_named(dir: &Path, name: &str, tag: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let app = dir.join(name);
    let script =
        format!("#!/bin/sh\necho {tag} \"$PTYRC_SESSION\" \"${{PTYRC_SOCKET-unset}}\" \"$@\"\n");
    std::fs::write(&app, script).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn app(sb: &Sandbox, cwd: &Path, path: &str, args: &[&str]) -> String {
    let out = sb
        .ptyrc()
        .current_dir(cwd)
        .env("PATH", path)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn app_found_near_the_directory_then_in_path() {
    let sb = Sandbox::new("app");
    let deep = sb.dir.join("project/src/deep");
    std::fs::create_dir_all(&deep).unwrap();
    fake(&sb.dir.join("project/venv/bin"), "venv");
    let plain = "/usr/bin:/bin";
    assert_eq!(
        app(&sb, &deep, plain, &["app", "bricks", "-a"]),
        "venv  unset bricks -a\n"
    );
    // `.venv` before `venv`; options after the app's name are its own.
    fake(&sb.dir.join("project/.venv/bin"), "dotvenv");
    assert_eq!(
        app(
            &sb,
            &deep,
            plain,
            &["-s", "x", "app", "bricks", "--help", "-s", "y"]
        ),
        "dotvenv x unset bricks --help -s y\n"
    );
    // `PATH` first.
    let bin = sb.dir.join("bin");
    fake(&bin, "path");
    let path = format!("{}:{plain}", bin.display());
    assert_eq!(
        app(&sb, &deep, &path, &["app", "--help"]),
        "path  unset --help\n"
    );
}

#[test]
fn python_found_and_session_wins_over_socket() {
    let sb = Sandbox::new("python");
    fake_named(&sb.dir.join(".venv/bin"), "ptyrc-python", "py");
    let run = |args: &[&str]| {
        let out = sb
            .ptyrc()
            .current_dir(&sb.dir)
            .env("PATH", "/usr/bin:/bin")
            .env("PTYRC_SOCKET", "/elsewhere.sock")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(
        run(&["python", "s.py", "-a"]),
        "py  /elsewhere.sock s.py -a\n"
    );
    assert_eq!(run(&["-s", "x", "python"]), "py x unset\n");
}
