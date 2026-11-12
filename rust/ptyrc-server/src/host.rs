// SPDX-License-Identifier: GPL-3.0-or-later
//! The host terminal: raw mode with guaranteed restore, the one write
//! path, size.

use crate::pty;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use rustix::stdio::{stdin, stdout};
use rustix::termios::{OptionalActions, Termios, isatty, tcgetattr, tcsetattr};
use std::fs::File;
use std::io::{self, Write};
use std::sync::Mutex;

/// Termios of the host before raw mode; `None` when there is nothing to
/// restore.
static SAVED: Mutex<Option<Termios>> = Mutex::new(None);

/// Put the host terminal back as it was. Idempotent; called at exit, on
/// panic and on fatal signals.
pub fn restore() {
    let saved = match SAVED.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    if let Some(termios) = saved {
        let _ = tcsetattr(stdin(), OptionalActions::Drain, &termios);
    }
}

/// Where host output goes.
pub struct Host {
    /// Stdin is a terminal in raw mode, read for keys.
    pub interactive: bool,
    headless: bool,
    log: Option<File>,
}

impl Host {
    /// The real terminal on stdin/stdout. Raw mode if stdin is a terminal.
    pub fn terminal() -> io::Result<Host> {
        let interactive = isatty(stdin());
        if interactive {
            let saved = tcgetattr(stdin())?;
            let mut raw = saved.clone();
            raw.make_raw();
            *SAVED.lock().unwrap_or_else(|p| p.into_inner()) = Some(saved);
            let hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                restore();
                hook(info);
            }));
            tcsetattr(stdin(), OptionalActions::Drain, &raw)?;
        }
        Ok(Host {
            interactive,
            headless: false,
            log: None,
        })
    }

    /// No terminal: output is dropped, or appended to `log`.
    pub fn headless(log: Option<File>) -> Host {
        Host {
            interactive: false,
            headless: true,
            log,
        }
    }

    /// Size `(cols, rows)` of the host terminal, if there is one.
    pub fn size(&self) -> Option<(u16, u16)> {
        if self.headless {
            return None;
        }
        pty::winsize(stdout()).or_else(|| pty::winsize(stdin()))
    }

    /// The one path to the host: everything, in order, entirely.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        if self.headless {
            return match &mut self.log {
                Some(log) => log.write_all(bytes),
                None => Ok(()),
            };
        }
        let mut rest = bytes;
        while !rest.is_empty() {
            match rustix::io::write(stdout(), rest) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => rest = &rest[n..],
                Err(Errno::INTR) => {}
                // Somebody made the terminal non-blocking: wait for room.
                Err(Errno::AGAIN) => {
                    let out = stdout();
                    let mut fds = [PollFd::new(&out, PollFlags::OUT)];
                    match poll(&mut fds, None) {
                        Ok(_) | Err(Errno::INTR) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if self.interactive {
            restore();
        }
    }
}
