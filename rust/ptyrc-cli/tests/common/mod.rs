// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared by the tests of the tools: a private runtime directory, the
//! binary, a host pty.
#![allow(dead_code)]

use ptyrc_proto::Client;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const BIN: &str = env!("CARGO_BIN_EXE_ptyrc");
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// A private runtime directory: tests see neither each other nor the
/// user's sessions.
pub struct Sandbox {
    pub dir: PathBuf,
}

impl Sandbox {
    pub fn new(tag: &str) -> Sandbox {
        let dir =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Sandbox { dir }
    }

    pub fn socket(&self, name: &str) -> PathBuf {
        self.dir.join("ptyrc").join(format!("{name}.sock"))
    }

    pub fn ptyrc(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .env("XDG_RUNTIME_DIR", &self.dir)
            .env_remove("PTYRC_SESSION")
            .env_remove("PTYRC_SOCKET");
        command
    }

    /// Run a verb to its end.
    pub fn run(&self, args: &[&str]) -> Output {
        self.ptyrc()
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    /// A driver or a replay in the background, its socket up.
    pub fn spawn(&self, name: &str, args: &[&str]) -> Proc {
        let child = Proc(
            self.ptyrc()
                .args(args)
                .stdin(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let path = self.socket(name);
        wait_for(|| UnixStream::connect(&path).is_ok(), "socket");
        child
    }

    /// A headless driver named `name` running `argv`.
    pub fn headless(&self, name: &str, size: &str, argv: &[&str]) -> Proc {
        let mut args = vec!["run", "--name", name, "--headless", "--size", size, "--"];
        args.extend(argv);
        self.spawn(name, &args)
    }

    pub fn client(&self, name: &str) -> Client {
        Client::connect(&self.socket(name)).unwrap()
    }

    /// The environment of a program hosted on a pty of the test.
    pub fn env(&self) -> Vec<(String, String)> {
        vec![
            (
                "XDG_RUNTIME_DIR".to_string(),
                self.dir.to_string_lossy().into_owned(),
            ),
            ("PS1".to_string(), "$ ".to_string()),
            ("ENV".to_string(), String::new()),
        ]
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn wait_for(mut ready: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A process killed when the test leaves it behind.
pub struct Proc(pub Child);

impl Proc {
    pub fn wait_exit(&mut self) -> i32 {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < deadline, "the process did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// The test as a terminal: it owns a pty and runs a program on it.
pub struct HostPty {
    pub master: OwnedFd,
    pub seen: Vec<u8>,
}

impl HostPty {
    /// Run `argv` on a new pty of 80x24.
    pub fn spawn(sb: &Sandbox, argv: &[&str]) -> (HostPty, Proc) {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let pty = ptyrc_server::pty::spawn(&argv, &sb.env(), 80, 24).unwrap();
        let host = HostPty {
            master: pty.master,
            seen: Vec::new(),
        };
        (host, Proc(pty.child))
    }

    pub fn pump(&mut self) {
        let mut buf = [0u8; 65536];
        while let Ok(n) = rustix::io::read(&self.master, &mut buf) {
            if n == 0 {
                break;
            }
            self.seen.extend_from_slice(&buf[..n]);
        }
    }

    pub fn wait_seen(&mut self, needle: &str) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump();
            if String::from_utf8_lossy(&self.seen).contains(needle) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {needle:?} in {:?}",
                String::from_utf8_lossy(&self.seen)
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn type_in(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while !rest.is_empty() {
            match rustix::io::write(&self.master, rest) {
                Ok(n) => rest = &rest[n..],
                Err(_) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    /// Drain the pty until the program is gone; its exit status.
    pub fn wait_exit(&mut self, proc: &mut Proc) -> i32 {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.pump();
            if let Some(status) = proc.0.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < deadline, "the process did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
