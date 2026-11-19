// SPDX-License-Identifier: GPL-3.0-or-later
//! The OS profile itself (`ptyrc_box::profile`), from host code: a
//! guest cannot make a system call. Each case runs in a child, this
//! test binary run again on one test (`PTYRC_PROFILE_CASE`), as the
//! profile is not undone and some calls kill the process.

#![cfg(target_os = "linux")]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Output};

const CASE: &str = "PTYRC_PROFILE_CASE";

/// Run `case` in a child, under the profile.
fn child(case: &str) -> Output {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "case", "--nocapture", "--test-threads=1"])
        .env(CASE, case)
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{:?}\n{}{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The child's side: nothing unless run by `child`.
#[test]
fn case() {
    let Ok(case) = std::env::var(CASE) else {
        return;
    };
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let scratch = std::env::temp_dir().join(format!("ptyrc-profile-{}", std::process::id()));
    let status = ptyrc_box::profile::apply();
    eprintln!("{}", status.lines().join("\n"));
    let pid = nix::unistd::getppid();
    match case.as_str() {
        "refused" => refused(&mut a, &mut b, &scratch),
        "execve" => {
            let path = c"/bin/true";
            let _ = nix::unistd::execv(path, &[path]);
        }
        "ptrace" => {
            let _ = nix::sys::ptrace::attach(pid);
        }
        "process_vm_readv" => {
            let mut buf = [0u8; 8];
            let mut local = [std::io::IoSliceMut::new(&mut buf)];
            let remote = [nix::sys::uio::RemoteIoVec { base: 4096, len: 8 }];
            let _ = nix::sys::uio::process_vm_readv(pid, &mut local, &remote);
        }
        // Stopped and continued by the parent meanwhile.
        "stopped" => {
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        other => panic!("{other}: no such case"),
    }
    // A case that kills does not come here.
    println!("case done: {case}");
}

fn denied<T: std::fmt::Debug>(what: &str, r: std::io::Result<T>) {
    match r {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {}
        other => panic!("{what}: {other:?}, not refused"),
    }
}

fn refused(a: &mut UnixStream, b: &mut UnixStream, scratch: &std::path::Path) {
    use std::fs::File;
    use std::net::{TcpListener, TcpStream, UdpSocket};
    denied("open", File::open("/etc/hostname"));
    denied("open, here", File::open(env!("CARGO_MANIFEST_DIR")));
    denied("create", File::create(scratch));
    denied("read_dir", std::fs::read_dir("/"));
    denied("mkdir", std::fs::create_dir(scratch));
    denied("unix connect", UnixStream::connect("/run/ptyrc-none"));
    denied("tcp connect", TcpStream::connect("127.0.0.1:9"));
    denied("tcp listen", TcpListener::bind("127.0.0.1:0"));
    denied("udp", UdpSocket::bind("127.0.0.1:0"));
    // A process is refused at its clone: execve is never reached.
    denied("spawn", Command::new("/bin/true").status());
    // The socket open before still works, both ways, with a timeout.
    a.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    b.write_all(b"ping").unwrap();
    let mut buf = [0u8; 4];
    a.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"ping");
    a.write_all(b"pong").unwrap();
    b.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"pong");
    // So do stderr, threads, time, memory.
    eprintln!("stderr: written");
    let t = std::thread::spawn(|| vec![7u8; 64 << 20].iter().map(|&x| x as u64).sum::<u64>());
    assert_eq!(t.join().unwrap(), 7 * (64 << 20));
    std::thread::sleep(std::time::Duration::from_millis(15));
}

#[test]
fn files_sockets_processes_refused_the_socket_kept() {
    if std::env::var_os(CASE).is_some() {
        return;
    }
    let out = child("refused");
    assert!(out.status.success(), "{}", text(&out));
    let all = text(&out);
    assert!(all.contains("case done: refused"), "{all}");
    assert!(all.contains("stderr: written"), "{all}");
    assert!(all.contains("profile: seccomp"), "{all}");
    // Said, whatever the kernel.
    assert!(
        all.contains("profile: landlock") || all.contains("MISSING: landlock"),
        "{all}"
    );
}

/// SIGSTOP, SIGCONT (the driver's pause): a sleep resumes.
#[test]
fn stopped_and_continued() {
    use nix::sys::signal::{Signal, kill};
    if std::env::var_os(CASE).is_some() {
        return;
    }
    let c = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "case", "--nocapture", "--test-threads=1"])
        .env(CASE, "stopped")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = nix::unistd::Pid::from_raw(c.id() as i32);
    for _ in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        kill(pid, Signal::SIGSTOP).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        kill(pid, Signal::SIGCONT).unwrap();
    }
    let out = c.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("case done: stopped"), "{}", text(&out));
}

/// These kill the process (`SIGSYS`): no path of the runner makes them.
#[test]
fn exec_ptrace_process_vm_kill() {
    if std::env::var_os(CASE).is_some() {
        return;
    }
    for case in ["execve", "ptrace", "process_vm_readv"] {
        let out = child(case);
        assert_eq!(
            out.status.signal(),
            Some(libc_sigsys()),
            "{case}: {}",
            text(&out)
        );
        assert!(!text(&out).contains("case done"), "{case}: {}", text(&out));
    }
}

fn libc_sigsys() -> i32 {
    nix::sys::signal::Signal::SIGSYS as i32
}
