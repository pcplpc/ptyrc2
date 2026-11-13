// SPDX-License-Identifier: GPL-3.0-or-later
//! The hotkey of `ptyrc run`: ctrl+t twice on the host keyboard.

mod common;

use common::{BIN, HostPty, Sandbox, wait_for};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

fn seen(host: &HostPty) -> String {
    String::from_utf8_lossy(&host.seen).into_owned()
}

#[test]
fn ctrl_t_twice_runs_the_command_then_ends_it() {
    let sb = Sandbox::new("hotkey");
    let log = sb.dir.join("log");
    // A stand-in for the manager: it notes its session and its id, and
    // stays.
    let app = sb.dir.join("app");
    let script = format!(
        "#!/bin/sh\necho \"$PTYRC_SESSION $PTYRC_APP $1\" >> {}\nexec sleep 60\n",
        log.display()
    );
    std::fs::write(&app, script).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    let hotkey = format!("PTYRC_HOTKEY={} menu", app.display());
    let logged = |text: &str| std::fs::read_to_string(&log).is_ok_and(|l| l == text);

    let (mut host, _driver) =
        HostPty::spawn(&sb, &["env", &hotkey, BIN, "run", "--name", "d", "cat"]);
    wait_for(|| sb.socket("d").exists(), "the session");
    // A client is there or not: the same. One that only asks of the
    // table is no app.
    let mut client = sb.client("d");
    let mut managers = move || -> Vec<Value> {
        let apps = client.call("app.list", json!({})).unwrap();
        let apps = apps["apps"].as_array().unwrap().clone();
        apps.into_iter().filter(|a| a["name"] == "manage").collect()
    };
    // Once: the app's, after a while.
    host.type_in(b"a\x14");
    host.wait_seen("a^T");
    assert!(!log.exists());
    // Twice: the command, a child named `manage`; nothing for the app.
    host.seen.clear();
    host.type_in(b"\x14\x14");
    wait_for(|| logged("d 1 menu\n"), "the command");
    let running = managers();
    assert_eq!(running.len(), 1);
    assert_eq!(
        (&running[0]["kind"], &running[0]["state"]),
        (&json!("child"), &json!("running"))
    );
    // Again, over two reads: it ends, and leaves the table.
    host.type_in(b"\x14");
    host.type_in(b"\x14");
    wait_for(|| managers().is_empty(), "the end of the command");
    // And again: another run.
    host.type_in(b"\x14\x14");
    wait_for(|| logged("d 1 menu\nd 2 menu\n"), "the second run");
    host.type_in(b"z");
    host.wait_seen("z");
    assert!(!seen(&host).contains("^T"), "{:?}", seen(&host));

    // Asked not to, by either name: the keys are the app's.
    for (name, flag) in [("e", "--no-hotkey"), ("f", "--no-demo")] {
        let (mut host, _driver) = HostPty::spawn(
            &sb,
            &["env", &hotkey, BIN, "run", flag, "--name", name, "cat"],
        );
        wait_for(|| sb.socket(name).exists(), "the session");
        host.type_in(b"\x14\x14");
        host.wait_seen("^T^T");
    }
    // A command that is not there: the bell.
    let (mut host, _driver) = HostPty::spawn(
        &sb,
        &[
            "env",
            "PTYRC_HOTKEY=/no/such/program",
            BIN,
            "run",
            "--name",
            "g",
            "cat",
        ],
    );
    wait_for(|| sb.socket("g").exists(), "the session");
    host.type_in(b"\x14\x14");
    host.wait_seen("\x07");
    assert!(logged("d 1 menu\nd 2 menu\n"));
}
