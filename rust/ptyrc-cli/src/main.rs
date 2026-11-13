// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc <verb>`.

mod apps;
mod plugins;
mod replay;
mod tools;
mod verbs;

use clap::{Args, Parser, Subcommand};
use ptyrc_proto::{Client, ClientError, TOPICS, session};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "ptyrc", version, about = "The pty remote controller")]
struct Cli {
    #[command(flatten)]
    common: Common,
    #[command(subcommand)]
    verb: Verb,
}

#[derive(Args)]
pub struct Common {
    /// Session to talk to [default: $PTYRC_SESSION, else the only one]
    #[arg(short, long, global = true)]
    pub session: Option<String>,
    /// Machine-readable output
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Verb {
    /// Wrap a program: it runs as before, and can be piloted
    Run(Run),
    /// List sessions
    Ls,
    /// Send a raw request, print the result
    Call {
        method: String,
        /// Parameters, a JSON value
        params: Option<String>,
    },
    /// Type text
    Send {
        /// As a paste (bracketed when the app asked for it)
        #[arg(long)]
        paste: bool,
        /// Joined with spaces
        #[arg(required = true)]
        text: Vec<String>,
    },
    /// Press keys: enter, esc, up, ctrl+c, f5...; anything else is typed
    Keys {
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// Print the screen as text
    Text(verbs::Text),
    /// Find text on screen: where it is, in frame and screen coordinates
    Find(verbs::Find),
    /// Print the cursor position
    Cursor,
    /// Wait for text, or for the screen to settle
    Wait(verbs::Wait),
    /// List frames: the raw screen and the panes found on it
    Frames(verbs::Frames),
    /// Print the safety heatmap of a frame: 9 free and still, 0 busy
    Heat {
        #[arg(short, long, default_value = "raw")]
        frame: String,
        /// Print classes instead: h hot, t static text, p panel, . free
        #[arg(short, long)]
        classes: bool,
    },
    /// Print how much of a frame is static; status 0 when idle (80 %)
    Idle {
        #[arg(short, long, default_value = "raw")]
        frame: String,
    },
    /// Draw on the screen
    #[command(subcommand)]
    Draw(verbs::Draw),
    /// Mirror the remote screen here (^C to leave)
    Show,
    /// The remote screen here and this keyboard there (ctrl+t to leave)
    Attach(tools::Attach),
    /// Take keys from the session's keyboard before the app; print them
    Grab(tools::Grab),
    /// Click on a cell, or on text, as the app's mouse mode wants it
    Click(tools::Click),
    /// A picture of the screen: SVG, HTML, or text with escapes
    Shot(tools::Shot),
    /// What the session shows and its frames, as JSON
    Snap,
    /// Replay a tape as a session: every client verb works on it
    Replay(replay::Replay),
    /// The clock of a replay: status, step, seek, play, pause, quit
    Tape {
        #[command(subcommand)]
        verb: Option<replay::TapeVerb>,
    },
    /// Run a client app: `ptyrc app bricks`; `ptyrc app` lists them
    App(apps::App),
    /// Debug tools
    #[command(subcommand)]
    Debug(verbs::Debug),
    /// Print events, one JSON object per line
    Events {
        /// screen, cursor, frames, text, idle, draw, input, mouse, size,
        /// exit [default: all but mouse]
        topics: Vec<String>,
        /// The state now as the first events (cursor, frames, idle, size)
        #[arg(long)]
        snapshot: bool,
    },
}

#[derive(Args)]
struct Run {
    /// Session name [default: the driver pid]
    #[arg(long)]
    name: Option<String>,
    /// No host terminal: nothing is displayed
    #[arg(long)]
    headless: bool,
    /// Screen size, COLSxROWS [default: the host's, 80x24 headless]
    #[arg(long, value_parser = parse_size)]
    size: Option<(u16, u16)>,
    /// Headless: append what the host would receive to this file
    #[arg(long, requires = "headless")]
    host_log: Option<PathBuf>,
    /// Plain pass-through: no screen model, no drawing
    #[arg(long)]
    transparent: bool,
    /// Default foreground of the host, #rrggbb [default: asked of the
    /// host terminal; #e5e5e5 without an answer]
    #[arg(long, value_name = "COLOR", value_parser = parse_rgb)]
    host_fg: Option<[u8; 3]>,
    /// Default background of the host, #rrggbb [default: asked of the
    /// host terminal; #000000 without an answer]
    #[arg(long, value_name = "COLOR", value_parser = parse_rgb)]
    host_bg: Option<[u8; 3]>,
    /// Generic heuristics only: no per-app preset (vim, tmux...)
    #[arg(long)]
    no_presets: bool,
    /// Quiet time that ends a burst of app output, in milliseconds
    #[arg(long, default_value_t = 1)]
    coalesce_ms: u64,
    /// Write the session to a tape: `ptyrc replay FILE` plays it again
    #[arg(long, value_name = "FILE")]
    record: Option<PathBuf>,
    /// Record what is typed too (passwords included)
    #[arg(long, requires = "record")]
    record_input: bool,
    /// No demo: ctrl+t twice in a row is the app's, as any key
    #[arg(long)]
    no_demo: bool,
    /// The program and its arguments [default: $SHELL]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

fn parse_rgb(text: &str) -> Result<[u8; 3], String> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    let value = u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6);
    value
        .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .ok_or_else(|| format!("{text}: a colour as #rrggbb"))
}

fn parse_size(text: &str) -> Result<(u16, u16), String> {
    let parsed = text
        .split_once(['x', 'X'])
        .and_then(|(c, r)| Some((c.parse().ok()?, r.parse().ok()?)));
    match parsed {
        Some((cols, rows)) if cols > 0 && rows > 0 => Ok((cols, rows)),
        _ => Err("expected COLSxROWS, as in 80x24".into()),
    }
}

/// A verb that failed: what, what to try, the exit status.
///
/// Statuses: 1 a negative answer (`find` without a match, `wait` timed
/// out), 2 usage or a request that is wrong, 3 no session to talk to,
/// 4 the request names something that is not there (frame, sprite),
/// 5 the connection or the system; `run` adds 126 and 127, as a shell.
#[derive(Debug)]
pub struct Fail {
    pub status: u8,
    pub message: String,
    pub hint: Option<String>,
}

impl Fail {
    /// A usage error: status 2.
    pub fn usage(message: impl Into<String>, hint: impl Into<String>) -> Fail {
        Fail {
            status: 2,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

impl From<String> for Fail {
    fn from(message: String) -> Fail {
        Fail {
            status: 2,
            message,
            hint: None,
        }
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Fail {
        Fail::from(message.to_string())
    }
}

impl From<std::io::Error> for Fail {
    fn from(e: std::io::Error) -> Fail {
        Fail {
            status: 5,
            message: e.to_string(),
            hint: None,
        }
    }
}

impl From<ClientError> for Fail {
    fn from(e: ClientError) -> Fail {
        Fail {
            status: e.exit_status(),
            hint: e.hint(),
            message: e.to_string(),
        }
    }
}

impl From<ptyrc_server::RunError> for Fail {
    fn from(e: ptyrc_server::RunError) -> Fail {
        Fail {
            status: e.exit_status(),
            message: e.message,
            hint: e.hint,
        }
    }
}

/// A JSON argument of the command line.
pub fn json_arg(what: &str, text: &str) -> Result<Value, Fail> {
    serde_json::from_str(text).map_err(|e| {
        Fail::usage(
            format!("{what} is not JSON: {e}"),
            format!("quote it for the shell, as in '{{\"r\": 4}}'; got: {text}"),
        )
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if !matches!(cli.verb, Verb::Run(_) | Verb::Replay(_)) {
        // A client verb piped into `head` ends quietly.
        ptyrc_server::host::default_sigpipe();
    }
    match run(cli) {
        Ok(code) => code,
        Err(fail) => {
            eprintln!("ptyrc: {}", fail.message);
            if let Some(hint) = fail.hint.filter(|h| !h.is_empty()) {
                eprintln!("  {hint}");
            }
            ExitCode::from(fail.status)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, Fail> {
    let common = cli.common;
    match cli.verb {
        Verb::Run(args) => verb_run(args),
        Verb::Ls => verb_ls(&common),
        Verb::Call { method, params } => {
            let params = match params {
                Some(text) => json_arg("params", &text)?,
                None => json!({}),
            };
            let result = call(&common, &method, params)?;
            print_json(&result, !common.json);
            Ok(ExitCode::SUCCESS)
        }
        Verb::Send { paste, text } => {
            call(
                &common,
                "input.send",
                json!({"text": text.join(" "), "paste": paste}),
            )?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::Keys { keys } => {
            call(&common, "input.send", json!({"keys": keys}))?;
            Ok(ExitCode::SUCCESS)
        }
        Verb::Text(args) => verbs::text(&common, args),
        Verb::Find(args) => verbs::find(&common, args),
        Verb::Cursor => verbs::cursor(&common),
        Verb::Wait(args) => verbs::wait(&common, args),
        Verb::Frames(args) => verbs::frames(&common, args),
        Verb::Heat { frame, classes } => verbs::heat(&common, &frame, classes),
        Verb::Idle { frame } => verbs::idle(&common, &frame),
        Verb::Draw(verb) => verbs::draw(&common, verb),
        Verb::Show => verbs::show(&common),
        Verb::Attach(args) => tools::attach(&common, args),
        Verb::Grab(args) => tools::grab(&common, args),
        Verb::Click(args) => tools::click(&common, args),
        Verb::Shot(args) => tools::shot(&common, args),
        Verb::Snap => replay::snap(&common),
        Verb::Replay(args) => replay::replay(args),
        Verb::Tape { verb } => replay::tape(&common, verb),
        Verb::App(args) => apps::app(&common, args),
        Verb::Debug(verb) => verbs::debug(&common, verb),
        Verb::Events { topics, snapshot } => verb_events(&common, topics, snapshot),
    }
}

pub fn connect(common: &Common) -> Result<Client, Fail> {
    Ok(Client::connect_session(common.session.as_deref())?)
}

/// One request on a fresh connection.
pub fn call(common: &Common, method: &str, params: Value) -> Result<Value, Fail> {
    Ok(connect(common)?.call(method, params)?)
}

pub fn print_json(value: &Value, pretty: bool) {
    let text = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    };
    println!("{}", text.expect("a Value serialises"));
}

fn verb_run(args: Run) -> Result<ExitCode, Fail> {
    let argv = if args.command.is_empty() {
        vec![
            std::env::var("SHELL")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "/bin/sh".into()),
        ]
    } else {
        args.command
    };
    let config = ptyrc_server::Config {
        argv,
        name: args.name,
        headless: args.headless,
        size: args.size,
        host_log: args.host_log,
        // Nothing looks at a transparent stream: no reason to hold it.
        coalesce_ms: if args.transparent {
            0
        } else {
            args.coalesce_ms
        },
        host_fg: args.host_fg,
        host_bg: args.host_bg,
        // A transparent stream is not touched: nothing is asked.
        ask_host_colors: !args.transparent,
        record: args.record,
        record_input: args.record_input,
        presets: !args.no_presets && !args.transparent,
        // Nothing is drawn on a transparent stream.
        demo: (!args.no_demo && !args.transparent).then(demo_command),
    };
    let status = ptyrc_server::run(config, plugins::engine(args.transparent, !args.no_presets))?;
    Ok(ExitCode::from(status.clamp(0, 255) as u8))
}

/// What the demo runs: the words of `$PTYRC_DEMO`; else `ptyrc-app demo`,
/// found as by `ptyrc app`.
fn demo_command() -> Vec<String> {
    let words = std::env::var("PTYRC_DEMO").unwrap_or_default();
    let words: Vec<String> = words.split_whitespace().map(str::to_string).collect();
    if !words.is_empty() {
        return words;
    }
    let program = apps::find()
        .and_then(|p| p.into_os_string().into_string().ok())
        .unwrap_or_else(|| apps::APP.into());
    vec![program, "demo".into()]
}

fn verb_ls(common: &Common) -> Result<ExitCode, Fail> {
    let mut rows = Vec::new();
    for s in session::list() {
        // A session that dies between the listing and the hello is skipped.
        if let Ok(client) = Client::connect(&s.path) {
            rows.push(client.hello().clone());
        }
    }
    if common.json {
        print_json(&Value::Array(rows), false);
        return Ok(ExitCode::SUCCESS);
    }
    let width = rows
        .iter()
        .filter_map(|r| r["session"].as_str())
        .map(str::len)
        .max()
        .unwrap_or(0)
        .max(7);
    if rows.is_empty() {
        eprintln!("ptyrc: no session\n  start one: ptyrc run -- CMD");
    } else {
        println!("{:width$}  {:>7}  {:>7}  COMMAND", "SESSION", "PID", "SIZE");
    }
    for row in rows {
        let size = format!("{}x{}", row["size"][0], row["size"][1]);
        let argv: Vec<&str> = row["argv"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        println!(
            "{:width$}  {:>7}  {:>7}  {}",
            row["session"].as_str().unwrap_or("?"),
            row["pid"],
            size,
            argv.join(" ")
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn verb_events(common: &Common, topics: Vec<String>, snapshot: bool) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let topics: Vec<&str> = if topics.is_empty() {
        // The mouse is asked of the host only when named.
        TOPICS.iter().copied().filter(|t| *t != "mouse").collect()
    } else {
        topics.iter().map(String::as_str).collect()
    };
    client.call("subscribe", json!({"topics": topics, "snapshot": snapshot}))?;
    loop {
        match client.next_event(None) {
            Ok(Some(event)) => {
                let exit = event.event == "exit";
                print_json(&json!({"event": event.event, "data": event.data}), false);
                if exit {
                    return Ok(ExitCode::SUCCESS);
                }
            }
            Ok(None) => {}
            Err(ClientError::Closed) => return Ok(ExitCode::SUCCESS),
            Err(e) => return Err(e.into()),
        }
    }
}
