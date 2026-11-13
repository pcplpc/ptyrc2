// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc app`: the client apps of `ptyrc-app`, under one name.

use crate::{Common, Fail};
use clap::Args;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The program of the client apps.
pub const APP: &str = "ptyrc-app";

#[derive(Args)]
#[command(disable_help_flag = true)]
pub struct App {
    /// The app and its options, passed to `ptyrc-app` [`--help` lists them]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// `ptyrc-app`: from `PATH`; else from `.venv/bin` or `venv/bin` in the
/// current directory or one above it; else in a directory above this
/// program (a source tree).
pub fn find() -> Option<PathBuf> {
    let in_path = || {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(APP))
            .find(|p| p.is_file())
    };
    let in_venv = |dir: &Path| {
        [".venv/bin", "venv/bin"]
            .iter()
            .map(|bin| dir.join(bin).join(APP))
            .find(|p| p.is_file())
    };
    let above = |start: &Path| start.ancestors().find_map(in_venv);
    let near_cwd = || above(&std::env::current_dir().ok()?);
    let near_exe = || {
        let exe = std::env::current_exe()
            .and_then(|e| e.canonicalize())
            .ok()?;
        above(exe.parent()?)
    };
    in_path().or_else(near_cwd).or_else(near_exe)
}

/// Replace this process with `ptyrc-app ARGS...`; `-s` becomes
/// `$PTYRC_SESSION`.
pub fn app(common: &Common, args: App) -> Result<ExitCode, Fail> {
    let Some(program) = find() else {
        return Err(Fail {
            status: 127,
            message: format!("{APP}: not found"),
            hint: Some(
                "looked in PATH, then .venv/bin and venv/bin here and above; \
                 install ptyrc-apps, or `uv sync` in the source tree"
                    .into(),
            ),
        });
    };
    let mut command = Command::new(&program);
    command.args(&args.args);
    if let Some(session) = &common.session {
        command.env("PTYRC_SESSION", session);
    }
    let e = command.exec();
    Err(Fail {
        status: if e.kind() == std::io::ErrorKind::NotFound {
            127
        } else {
            126
        },
        message: format!("{}: {e}", program.display()),
        hint: None,
    })
}
