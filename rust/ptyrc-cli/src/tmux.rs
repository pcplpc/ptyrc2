// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc tmux`: put a driver under the client of a running tmux session,
//! and take it away.
//!
//! The session stays in the tmux server. `attach` has tmux replace the
//! client by `ptyrc tmux wrap` (`detach-client -E`): a driver, on the
//! same terminal, whose app is a new client of the same session.
//! `detach` asks the driver for a handoff: it ends and a plain client
//! takes its place. The tmux session names its driver in `@ptyrc`.

use crate::{Common, Fail, apps, connect, plugins, print_json};
use clap::{Args, Subcommand};
use ptyrc_proto::session;
use ptyrc_tui_tmux::server::{OPTION, OPTION_ERROR, Server, Session, session_name};
use serde_json::json;
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

/// How long a swap may take.
const SWAP_WAIT: Duration = Duration::from_secs(5);
/// How long an app is watched for an early end.
const APP_WAIT: Duration = Duration::from_millis(400);

#[derive(Subcommand)]
pub enum Tmux {
    /// Put ptyrc under this tmux session: same windows, now piloted
    Attach(Attach),
    /// Take ptyrc away: the tmux session goes on, plain
    Detach(Target),
    /// The driver over a client of a session, in place of this process
    #[command(hide = true)]
    Wrap(Wrap),
}

#[derive(Args)]
pub struct Attach {
    #[command(flatten)]
    target: Target,
    /// Run a client app on the session: `--app fishes`, `--app "fishes
    /// -a"`; repeatable
    #[arg(long, value_name = "APP")]
    app: Vec<String>,
}

#[derive(Args)]
pub struct Target {
    /// The tmux session [default: the one this pane is in]
    #[arg(short, long, value_name = "SESSION")]
    target: Option<String>,
    /// Socket name of the tmux server, as `tmux -L`
    #[arg(short = 'L', value_name = "NAME", conflicts_with = "socket")]
    socket_name: Option<String>,
    /// Socket path of the tmux server, as `tmux -S` [default: the server
    /// of this pane, else tmux's]
    #[arg(short = 'S', value_name = "PATH")]
    socket: Option<String>,
}

impl Target {
    /// The server meant, and whether this process is in a pane of it.
    fn server(&self) -> (Server, bool) {
        match (&self.socket, &self.socket_name, Server::enclosing()) {
            (Some(path), _, _) => (Server::at(path), false),
            (_, Some(name), _) => (Server::named(name), false),
            (None, None, Some(server)) => (server, true),
            (None, None, None) => (Server::default_server(), false),
        }
    }
}

#[derive(Args)]
pub struct Wrap {
    /// Socket of the tmux server [default: tmux's]
    #[arg(short = 'S', long)]
    socket: Option<String>,
    /// Session id
    #[arg(short, long)]
    target: String,
    /// `ptyrc-app`, as found by `attach`
    #[arg(long)]
    program: Option<String>,
    #[arg(long)]
    app: Vec<String>,
}

pub fn tmux(common: &Common, verb: Tmux) -> Result<ExitCode, Fail> {
    match verb {
        Tmux::Attach(args) => attach(common, args),
        Tmux::Detach(args) => detach(common, args),
        Tmux::Wrap(args) => wrap(args),
    }
}

fn fail(status: u8, message: impl Into<String>, hint: &str) -> Fail {
    Fail {
        status,
        message: message.into(),
        hint: Some(hint.into()),
    }
}

/// The tmux session meant: `target`, else the one of this pane, else
/// the one tmux takes. And whether this process is in a pane of its
/// server.
fn find_session(target: &Target) -> Result<(Server, Session, bool), Fail> {
    let (server, inside) = target.server();
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|p| inside && !p.is_empty());
    let named = target.target.as_deref().or(pane.as_deref());
    server
        .session(named)
        .map_err(|e| {
            let what = if e.is_empty() { "no session".into() } else { e };
            fail(3, format!("tmux: {what}"), "tmux sessions: tmux ls")
        })
        .map(|session| (server, session, inside))
}

/// The live driver `session` names in `@ptyrc`.
fn driver_of(server: &Server, session: &Session) -> Option<String> {
    let name = server.option(session, OPTION)?;
    (session::valid_name(&name) && session::is_live(&session::socket_path(&name))).then_some(name)
}

fn wait_for<T>(mut look: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + SWAP_WAIT;
    loop {
        if let Some(found) = look() {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Run `ptyrc-app WORDS` on the session `name`, left behind: it ends with
/// the session. An app that ends at once with an error is one.
fn start_apps(program: &str, apps: &[String], name: &str) -> Result<(), Fail> {
    let spawn = |app: &String| {
        Command::new(program)
            .args(app.split_whitespace())
            .env(session::ENV_SESSION, name)
            .env_remove(session::ENV_SOCKET)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|e| fail(5, format!("{program}: {e}"), ""))
    };
    let mut children = apps.iter().map(spawn).collect::<Result<Vec<_>, _>>()?;
    let deadline = Instant::now() + APP_WAIT;
    while Instant::now() < deadline {
        for (app, child) in apps.iter().zip(&mut children) {
            if let Ok(Some(status)) = child.try_wait()
                && !status.success()
            {
                return Err(fail(
                    1,
                    format!("app `{app}` ended at once"),
                    &format!("try it: ptyrc -s {name} app {app}"),
                ));
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn attach(common: &Common, args: Attach) -> Result<ExitCode, Fail> {
    let program = match args.app.is_empty() {
        true => apps::program(apps::APP),
        false => Some(apps::program(apps::APP).ok_or_else(apps::not_found(apps::APP))?),
    };
    let (server, session, inside) = find_session(&args.target)?;
    let report = |name: &str, already: bool| {
        if common.json {
            print_json(
                &json!({"session": name, "tmux": session.name, "already": already}),
                false,
            );
        } else if already {
            println!("ptyrc session {name}, as before");
        } else {
            println!("ptyrc session {name}; to remove it: ptyrc tmux detach");
        }
    };
    let join = |name: &str| match &program {
        Some(program) => start_apps(program, &args.app, name),
        None => Ok(()),
    };
    if let Some(name) = driver_of(&server, &session) {
        // Attached already: the apps join.
        report(&name, true);
        join(&name)?;
        return Ok(ExitCode::SUCCESS);
    }
    let clients = server
        .clients(Some(&session))
        .map_err(|e| fail(5, format!("tmux: {e}"), ""))?;
    let Some(client) = clients.first() else {
        // No client to replace: this terminal becomes one.
        if inside || !std::io::stdin().is_terminal() {
            return Err(fail(
                3,
                format!("no client is attached to tmux session {}", session.name),
                "from a terminal outside tmux, the same command attaches one",
            ));
        }
        return wrap(Wrap {
            socket: server.socket().ok(),
            target: session.id,
            program,
            app: args.app,
        });
    };
    let exe = std::env::current_exe()?;
    let mut argv = vec![
        exe.to_string_lossy().into_owned(),
        "tmux".into(),
        "wrap".into(),
        "-t".into(),
        session.id.clone(),
    ];
    if let Ok(socket) = server.socket() {
        argv.extend(["-S".into(), socket]);
    }
    if let Some(program) = &program {
        argv.extend(["--program".into(), program.clone()]);
    }
    server.unset_option(&session, OPTION_ERROR);
    server
        .swap(&client.tty, &argv)
        .map_err(|e| fail(5, format!("tmux: {e}"), ""))?;
    let outcome = wait_for(|| {
        if let Some(name) = driver_of(&server, &session) {
            return Some(Ok(name));
        }
        server.option(&session, OPTION_ERROR).map(Err)
    });
    match outcome {
        Some(Ok(name)) => {
            report(&name, false);
            join(&name)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Err(why)) => {
            server.unset_option(&session, OPTION_ERROR);
            Err(fail(5, why, "the tmux client is plain, as before"))
        }
        None => Err(fail(
            5,
            "the driver did not start",
            "the tmux session is untouched: tmux attach",
        )),
    }
}

fn detach(common: &Common, args: Target) -> Result<ExitCode, Fail> {
    let name = match &common.session {
        Some(name) => Some(name.clone()),
        None => {
            let (server, session, _) = find_session(&args)?;
            driver_of(&server, &session)
        }
    };
    let Some(name) = name else {
        if common.json {
            print_json(&json!({"session": null}), false);
        } else {
            eprintln!("ptyrc: not attached: nothing to do");
        }
        return Ok(ExitCode::SUCCESS);
    };
    let named = Common {
        session: Some(name.clone()),
        json: common.json,
    };
    let mut driver = connect(&named)?;
    // The client of the driver leaves by tmux, without a word on the
    // terminal; hung up when tmux does not know it.
    let pid = driver.hello()["pid"].as_u64();
    let (server, _) = args.server();
    let clients = server.clients(None).unwrap_or_default();
    let client = clients.iter().find(|c| Some(u64::from(c.pid)) == pid);
    driver.call("session.handoff", json!({"hangup": client.is_none()}))?;
    if let Some(client) = client
        && server.swap(&client.tty, &["true".to_string()]).is_err()
    {
        driver.call("session.handoff", json!({}))?;
    }
    let socket = session::socket_path(&name);
    if wait_for(|| (!session::is_live(&socket)).then_some(())).is_none() {
        return Err(fail(5, format!("session {name} did not end"), ""));
    }
    if common.json {
        print_json(&json!({"session": name}), false);
    }
    Ok(ExitCode::SUCCESS)
}

/// A name no live session has: `tmux-<name>`, then `tmux-<name>-2`...
fn free_name(session: &Session) -> String {
    let base = session_name(&session.name);
    let live = |name: &str| session::is_live(&session::socket_path(name));
    (1..)
        .map(|n| match n {
            1 => base.clone(),
            n => format!("{base}-{n}"),
        })
        .find(|name| !live(name))
        .expect("a name is free")
}

/// Replace this process by `argv`.
fn exec(argv: &[String]) -> Fail {
    let e = Command::new(&argv[0]).args(&argv[1..]).exec();
    fail(127, format!("{}: {e}", argv[0]), "")
}

fn wrap(args: Wrap) -> Result<ExitCode, Fail> {
    let server = match &args.socket {
        Some(socket) => Server::at(socket),
        None => Server::default_server(),
    };
    let session = server
        .session(Some(&args.target))
        .map_err(|e| fail(3, format!("tmux: {e}"), "tmux sessions: tmux ls"))?;
    let plain = server.attach_argv(&session);
    if driver_of(&server, &session).is_some() {
        // One driver per tmux session: a plain client.
        return Err(exec(&plain));
    }
    let name = free_name(&session);
    let program = args.program.or_else(|| apps::program(apps::APP));
    let command = |app: &String| {
        let mut argv = vec![program.clone().unwrap_or_else(|| apps::APP.into())];
        argv.extend(app.split_whitespace().map(str::to_string));
        argv
    };
    let config = ptyrc_server::Config {
        argv: plain.clone(),
        name: Some(name.clone()),
        demo: Some(crate::demo_command(program.as_deref())),
        apps: args.app.iter().map(command).collect(),
        handoff: true,
        ..Default::default()
    };
    let _ = server.set_option(&session, OPTION, &name);
    let result = ptyrc_server::run(config, plugins::engine(false, true));
    if server.option(&session, OPTION).as_deref() == Some(&name) {
        server.unset_option(&session, OPTION);
    }
    match result {
        // Detached by tmux, or the session ended: so does this.
        Ok(exit) if !exit.handoff => Ok(ExitCode::from(exit.status.clamp(0, 255) as u8)),
        Ok(_) => Err(exec(&plain)),
        // No driver: the client is plain, the session is not lost.
        Err(e) => {
            let _ = server.set_option(&session, OPTION_ERROR, &e.message);
            Err(exec(&plain))
        }
    }
}
