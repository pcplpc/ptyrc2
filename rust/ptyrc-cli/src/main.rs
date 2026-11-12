// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc <verb>`.

mod plugins;

use clap::{Args, Parser, Subcommand};
use ptyrc_proto::{Client, ClientError, session};
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
struct Common {
    /// Session to talk to [default: $PTYRC_SESSION, else the only one]
    #[arg(short, long, global = true)]
    session: Option<String>,
    /// Machine-readable output
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
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
    /// Print events, one JSON object per line
    Events {
        /// screen, cursor, frames, input, size, exit [default: all]
        topics: Vec<String>,
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
    /// Quiet time that ends a burst of app output, in milliseconds
    #[arg(long, default_value_t = 1)]
    coalesce_ms: u64,
    /// The program and its arguments [default: $SHELL]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
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

const TOPICS: [&str; 6] = ["screen", "cursor", "frames", "input", "size", "exit"];

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("ptyrc: {message}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let common = cli.common;
    match cli.verb {
        Verb::Run(args) => verb_run(args),
        Verb::Ls => verb_ls(&common),
        Verb::Call { method, params } => {
            let params = match params {
                Some(text) => serde_json::from_str(&text).map_err(|e| format!("params: {e}"))?,
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
        Verb::Events { topics } => verb_events(&common, topics),
    }
}

fn connect(common: &Common) -> Result<Client, String> {
    Client::connect_session(common.session.as_deref()).map_err(|e| e.to_string())
}

/// One request on a fresh connection.
fn call(common: &Common, method: &str, params: Value) -> Result<Value, String> {
    connect(common)?
        .call(method, params)
        .map_err(|e| e.to_string())
}

fn print_json(value: &Value, pretty: bool) {
    let text = if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    };
    println!("{}", text.expect("a Value serialises"));
}

fn verb_run(args: Run) -> Result<ExitCode, String> {
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
        coalesce_ms: args.coalesce_ms,
    };
    let status = ptyrc_server::run(config, plugins::engine()).map_err(|e| e.to_string())?;
    Ok(ExitCode::from(status.clamp(0, 255) as u8))
}

fn verb_ls(common: &Common) -> Result<ExitCode, String> {
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
    if !rows.is_empty() {
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

fn verb_events(common: &Common, topics: Vec<String>) -> Result<ExitCode, String> {
    let mut client = connect(common)?;
    let topics: Vec<&str> = if topics.is_empty() {
        TOPICS.to_vec()
    } else {
        topics.iter().map(String::as_str).collect()
    };
    client.subscribe(&topics).map_err(|e| e.to_string())?;
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
            Err(e) => return Err(e.to_string()),
        }
    }
}
