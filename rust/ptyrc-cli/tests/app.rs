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

#[test]
fn run_with_apps_from_the_start() {
    let sb = Sandbox::new("runapp");
    let bin = sb.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = sb.dir.join("log");
    // A stand-in for `ptyrc-app`: it declares nothing, notes its
    // session and arguments, stays, and notes its SIGTERM.
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --declares ]; then echo null; exit 0; fi\n\
         log={}\necho \"$PTYRC_SESSION $*\" >> $log\n\
         trap 'echo \"term $1\" >> $log; exit' TERM\nsleep 60 & wait\n",
        log.display()
    );
    let app = bin.join("ptyrc-app");
    std::fs::write(&app, script).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let run = |path: &str, args: &[&str]| {
        sb.ptyrc()
            .current_dir(&sb.dir)
            .env("PATH", path)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let logged = |text: &str| {
        let mut lines: Vec<String> = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        lines.sort();
        lines.join("\n") == text
    };

    // Both started on the session; `--app` after the program is its own.
    let sh = "sh -c 'echo \"$@\"; sleep 0.2' x --app y";
    let out = run(
        &path,
        &[
            "run",
            "--headless",
            "--name",
            "r",
            "--app",
            "bricks",
            "--app",
            "snow -a",
            "--",
            "sh",
            "-c",
            sh,
        ],
    );
    assert!(out.status.success(), "{out:?}");
    common::wait_for(
        || logged("r bricks\nr snow -a\nterm bricks\nterm snow"),
        "the apps, started then terminated",
    );

    // A first argument that starts with `-` is an option of `run`.
    let out = run(&path, &["run", "--headless", "--bogus", "true"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

/// A stand-in for `ptyrc-app` that declares `shape` and `text`; run, it
/// does `run` (sh), as the same process.
fn declaring(dir: &Path, run: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let app = dir.join("ptyrc-app");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = --declares ]; then\n\
         echo '{{\"permissions\": {{\"shape\": \"a\", \"text\": \"b\"}}}}'; exit 0; fi\n{run}\n"
    );
    std::fs::write(&app, script).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_foreground_app_is_enrolled_and_held_to_its_answers() {
    let sb = Sandbox::new("enrol");
    let _driver = sb.headless("e", "40x4", &["sh", "-c", "echo SECRET; exec cat"]);
    let mut client = sb.client("e");
    client
        .call("screen.wait", serde_json::json!({"pattern": "SECRET"}))
        .unwrap();
    let bin = sb.dir.join("bin");
    declaring(&bin, &format!("exec {} text", common::BIN));
    let path = format!("{}:/usr/bin:/bin", bin.display());
    // Headless, no answer in the file: what it declares.
    assert!(app(&sb, &sb.dir, &path, &["app", "peek"]).contains("SECRET"));
    // A no in the file: its answers cut, in the same form.
    let dir = sb.config().join("ptyrc");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("permissions.toml"),
        "[app.\"native/peek\"]\nallow = [\"shape\"]\ndeny = [\"text\"]\n",
    )
    .unwrap();
    let cut = app(&sb, &sb.dir, &path, &["app", "peek"]);
    assert!(!cut.contains("SECRET") && !cut.trim().is_empty(), "{cut}");
    // Gone with its connection.
    common::wait_for(
        || {
            let apps = client.call("app.list", serde_json::json!({})).unwrap();
            !apps.to_string().contains("peek")
        },
        "the app gone",
    );
}

#[test]
fn the_token_of_an_enrolled_app_is_its_own() {
    let sb = Sandbox::new("enrol-token");
    let _driver = sb.headless("e", "40x4", &["cat"]);
    let bin = sb.dir.join("bin");
    let token = sb.dir.join("token");
    declaring(
        &bin,
        &format!(
            "echo \"$PTYRC_TOKEN\" > {}.new; mv {0}.new {0}; exec sleep 30",
            token.display()
        ),
    );
    let mut child = common::Proc(
        sb.ptyrc()
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .args(["app", "steal"])
            .spawn()
            .unwrap(),
    );
    common::wait_for(|| token.exists(), "the token");
    let token = std::fs::read_to_string(&token).unwrap();
    let mut client = sb.client("e");
    let apps = client.call("app.list", serde_json::json!({})).unwrap();
    let app = &apps["apps"][0];
    assert_eq!(
        (&app["name"], &app["kind"]),
        (&"steal".into(), &"client".into())
    );
    assert_eq!(app["foreground"], true);
    // Another process: refused, then closed.
    let stolen = ptyrc_proto::Client::connect_as(&sb.socket("e"), Some(token.trim()));
    let Err(ptyrc_proto::ClientError::Rpc(error)) = stolen else {
        panic!("a token of another process accepted");
    };
    assert_eq!(error.code.as_str(), "denied");
    // Stopped as a client is: its process.
    let id = app["id"].clone();
    client
        .call("app.stop", serde_json::json!({"app": id}))
        .unwrap();
    assert_ne!(child.wait_exit(), 0);
}
