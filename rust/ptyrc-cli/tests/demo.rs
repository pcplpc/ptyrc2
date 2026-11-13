// SPDX-License-Identifier: GPL-3.0-or-later
//! The demo of `ptyrc run`: ctrl+t twice on the host keyboard.

mod common;

use common::{BIN, HostPty, Sandbox, wait_for};
use std::os::unix::fs::PermissionsExt;

fn seen(host: &HostPty) -> String {
    String::from_utf8_lossy(&host.seen).into_owned()
}

#[test]
fn ctrl_t_twice_runs_the_next_app() {
    let sb = Sandbox::new("demo");
    let log = sb.dir.join("log");
    // A stand-in for `ptyrc-app demo`: it notes its session and its
    // number, and stays.
    let app = sb.dir.join("app");
    let script = format!(
        "#!/bin/sh\necho \"$PTYRC_SESSION $2\" >> {}\nexec sleep 60\n",
        log.display()
    );
    std::fs::write(&app, script).unwrap();
    std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
    let demo = format!("PTYRC_DEMO={}", app.display());
    let logged = |text: &str| std::fs::read_to_string(&log).is_ok_and(|l| l == text);

    let (mut host, _driver) =
        HostPty::spawn(&sb, &["env", &demo, BIN, "run", "--name", "d", "cat"]);
    wait_for(|| sb.socket("d").exists(), "the session");
    // Once: the app's, after a while.
    host.type_in(b"a\x14");
    host.wait_seen("a^T");
    assert!(!log.exists());
    // Twice: an app, and nothing for the wrapped one.
    host.seen.clear();
    host.type_in(b"\x14\x14");
    wait_for(|| logged("d 0\n"), "the first app");
    // Again, with the app as a client or not: the next.
    host.type_in(b"\x14");
    host.type_in(b"\x14");
    wait_for(|| logged("d 0\nd 1\n"), "the second app");
    host.type_in(b"z");
    host.wait_seen("z");
    assert!(!seen(&host).contains("^T"), "{:?}", seen(&host));

    // A client is there: the keys are the app's.
    let (mut host, _driver) =
        HostPty::spawn(&sb, &["env", &demo, BIN, "run", "--name", "e", "cat"]);
    wait_for(|| sb.socket("e").exists(), "the session");
    let _client = sb.client("e");
    host.type_in(b"\x14\x14");
    host.wait_seen("^T^T");
    // Asked not to: the same.
    let (mut host, _driver) = HostPty::spawn(
        &sb,
        &["env", &demo, BIN, "run", "--no-demo", "--name", "f", "cat"],
    );
    wait_for(|| sb.socket("f").exists(), "the session");
    host.type_in(b"\x14\x14");
    host.wait_seen("^T^T");
    assert!(logged("d 0\nd 1\n"));
}
