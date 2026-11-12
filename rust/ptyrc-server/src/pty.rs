// SPDX-License-Identifier: GPL-3.0-or-later
//! Pseudo terminal: open, spawn a program on it, window size.

use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{Winsize, tcgetwinsize, tcsetwinsize};
use std::fs::OpenOptions;
use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

/// A program running on a pty. `master` is non-blocking.
pub struct Pty {
    pub master: OwnedFd,
    pub child: Child,
}

/// Run `argv` as the session leader of a new pty of `cols x rows`, the
/// pty as its controlling terminal and its stdin, stdout and stderr. `env`
/// is added to the inherited environment.
pub fn spawn(argv: &[String], env: &[(String, String)], cols: u16, rows: u16) -> io::Result<Pty> {
    let Some(program) = argv.first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)?;
    grantpt(&master)?;
    unlockpt(&master)?;
    let name = ptsname(&master, Vec::new())?;
    let name = name
        .to_str()
        .map_err(|_| io::Error::other("pty name is not UTF-8"))?;
    // std opens with O_CLOEXEC: the child keeps only its stdio copies.
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc_noctty())
        .open(name)?;
    set_winsize(&master, cols, rows)?;

    let mut command = Command::new(program);
    command
        .args(&argv[1..])
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    // SAFETY: the closure runs between fork and exec and only makes two
    // async-signal-safe system calls (setsid, ioctl), without allocating.
    unsafe {
        command.pre_exec(|| {
            rustix::process::setsid()?;
            rustix::process::ioctl_tiocsctty(rustix::stdio::stdin())?;
            Ok(())
        });
    }
    let child = command.spawn()?;
    drop(command);

    let flags = fcntl_getfl(&master)?;
    fcntl_setfl(&master, flags | OFlags::NONBLOCK)?;
    Ok(Pty { master, child })
}

fn libc_noctty() -> i32 {
    OFlags::NOCTTY.bits() as i32
}

/// Set the window size of a terminal.
pub fn set_winsize(fd: impl AsFd, cols: u16, rows: u16) -> io::Result<()> {
    let size = Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    Ok(tcsetwinsize(fd, size)?)
}

/// Window size `(cols, rows)` of a terminal; `None` if not one, or
/// unsized.
pub fn winsize(fd: impl AsFd) -> Option<(u16, u16)> {
    let size = tcgetwinsize(fd).ok()?;
    (size.ws_col > 0 && size.ws_row > 0).then_some((size.ws_col, size.ws_row))
}
