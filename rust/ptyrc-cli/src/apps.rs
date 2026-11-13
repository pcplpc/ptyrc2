// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc app` and `ptyrc python`: the Python programs, under one name.

use crate::{Common, Fail};
use clap::Args;
use ptyrc_proto::session;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The program of the client apps.
pub const APP: &str = "ptyrc-app";
/// The program of the interactive pilot.
pub const PYTHON: &str = "ptyrc-python";

#[derive(Args)]
#[command(disable_help_flag = true)]
pub struct App {
    /// The app and its options, passed to `ptyrc-app` [`--help` lists them]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Args)]
#[command(disable_help_flag = true)]
pub struct Python {
    /// A script run with `pilot` defined, and its arguments [default: an
    /// interactive session]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// `program`: from `PATH`; else from `.venv/bin` or `venv/bin` in the
/// current directory or one above it; else in a directory above this
/// program (a source tree).
pub fn find(program: &str) -> Option<PathBuf> {
    let in_path = || {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|p| p.is_file())
    };
    let in_venv = |dir: &Path| {
        [".venv/bin", "venv/bin"]
            .iter()
            .map(|bin| dir.join(bin).join(program))
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

/// `find`, as a string.
pub fn program(name: &str) -> Option<String> {
    find(name).and_then(|p| p.into_os_string().into_string().ok())
}

/// The failure of `find(name)`: status 127.
pub fn not_found(name: &str) -> impl FnOnce() -> Fail {
    move || Fail {
        status: 127,
        message: format!("{name}: not found"),
        hint: Some(
            "looked in PATH, then .venv/bin and venv/bin here and above; \
             install ptyrc and ptyrc-apps, or `uv sync` in the source tree"
                .into(),
        ),
    }
}

/// Replace this process with `ptyrc-app ARGS...`.
pub fn app(common: &Common, args: App) -> Result<ExitCode, Fail> {
    exec(APP, common, &args.args)
}

/// Replace this process with `ptyrc-python ARGS...`.
pub fn python(common: &Common, args: Python) -> Result<ExitCode, Fail> {
    exec(PYTHON, common, &args.args)
}

/// Replace this process with `name ARGS...`; `-s` becomes `$PTYRC_SESSION`,
/// and drops `$PTYRC_SOCKET`, which would win over it.
fn exec(name: &str, common: &Common, args: &[String]) -> Result<ExitCode, Fail> {
    let program = find(name).ok_or_else(not_found(name))?;
    let mut command = Command::new(&program);
    command.args(args);
    if let Some(session) = &common.session {
        command.env(session::ENV_SESSION, session);
        command.env_remove(session::ENV_SOCKET);
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
