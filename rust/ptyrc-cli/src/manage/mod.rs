// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc manage`: the apps of a session, by sub-verbs. The menu is
//! `ui.rs`.

pub mod ui;

use crate::{Common, Fail, apps, connect, print_json};
use clap::{Args, Subcommand};
use ptyrc_proto::Client;
use serde_json::{Value, json};
use std::io::IsTerminal;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// How long a started app is watched for an early end.
pub const START_WAIT: Duration = Duration::from_millis(400);
/// How long a stop or a restart may take.
const END_WAIT: Duration = Duration::from_secs(5);
const LOOK_EVERY: Duration = Duration::from_millis(25);

#[derive(Args)]
pub struct Manage {
    /// The menu in a modal widget of the session: what ctrl+t ctrl+t runs
    #[arg(long)]
    overlay: bool,
    #[command(subcommand)]
    verb: Option<Verb>,
}

#[derive(Subcommand)]
enum Verb {
    /// List the apps: id, kind, state, since when, pid, name, arguments
    Ls,
    /// Start an app under the driver: `start bricks -a`
    Start {
        /// Any program in place of an app of `ptyrc-app`: `start --cmd
        /// python3 mine.py`
        #[arg(long)]
        cmd: bool,
        /// The app and its options
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        words: Vec<String>,
    },
    /// End apps (SIGTERM; SIGKILL after 2 s)
    Stop(Select),
    /// Freeze apps and hide what they draw
    Pause(Select),
    /// Let paused apps go on, shown again
    #[command(alias = "unpause")]
    Resume(Select),
    /// Stop an app and start it again; with options, in place of the old
    Restart {
        /// An id or a name
        app: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        options: Vec<String>,
    },
    /// Drop exited apps from the list
    Forget(Select),
    /// Move an app to another frame: `move clock 3`, `move 2 idle`;
    /// live when it moves by itself, else restarted with `-f FRAME`
    Move {
        /// Restart it there, even when it moves by itself
        #[arg(long)]
        restart: bool,
        /// An id or a name
        app: String,
        /// A frame id, or raw, active, largest, quietest, idle
        frame: String,
    },
    /// List the frames on screen and the apps on each
    Frames,
    /// Print what an app wrote on stderr
    Log {
        /// An id or a name
        app: String,
    },
}

#[derive(Args)]
struct Select {
    /// Ids or names; a name is every app of that name
    #[arg(required_unless_present = "all")]
    apps: Vec<String>,
    /// Every app, but this one and the manager
    #[arg(long, conflicts_with = "apps")]
    all: bool,
}

pub fn manage(common: &Common, args: Manage) -> Result<ExitCode, Fail> {
    let Some(verb) = args.verb else {
        if !args.overlay && !std::io::stdout().is_terminal() {
            return ls(common);
        }
        return ui::interactive(common, args.overlay);
    };
    match verb {
        Verb::Ls => ls(common),
        Verb::Start { cmd, words } => start_verb(common, cmd, words),
        Verb::Stop(select) => each(common, "stop", &select),
        Verb::Pause(select) => each(common, "pause", &select),
        Verb::Resume(select) => each(common, "resume", &select),
        Verb::Forget(select) => each(common, "forget", &select),
        Verb::Restart { app, options } => restart(common, &app, options),
        Verb::Move {
            restart,
            app,
            frame,
        } => move_to(common, &app, &frame, restart),
        Verb::Frames => frames(common),
        Verb::Log { app } => log(common, &app),
    }
}

fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

fn argv_of(info: &Value) -> Vec<String> {
    let words = info["argv"].as_array().into_iter().flatten();
    words
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// Where `ptyrc-app` stands in the `argv` of an app, when its name
/// follows.
fn app_word(argv: &[String]) -> Option<usize> {
    let at = argv.iter().position(|w| file_name(w) == apps::APP)?;
    (at + 1 < argv.len()).then_some(at)
}

/// The name an app goes by: of one run by `ptyrc-app`, its first word
/// (`bricks`); else the name the driver has.
pub fn label(info: &Value) -> String {
    let argv = argv_of(info);
    match app_word(&argv) {
        Some(at) => argv[at + 1].clone(),
        None => info["name"].as_str().unwrap_or("?").to_string(),
    }
}

/// The arguments of an app: what follows its name.
pub fn options(info: &Value) -> Vec<String> {
    let argv = argv_of(info);
    match app_word(&argv) {
        Some(at) => argv[at + 2..].to_vec(),
        None => argv.get(1..).unwrap_or_default().to_vec(),
    }
}

/// The `argv` of an app with other options.
pub fn with_options(info: &Value, options: &[String]) -> Vec<String> {
    let argv = argv_of(info);
    let keep = app_word(&argv).map_or(1, |at| at + 2);
    let mut argv: Vec<String> = argv.into_iter().take(keep).collect();
    argv.extend(options.iter().cloned());
    argv
}

/// `argv` with `-f FRAME` in place of the `-f`, `--frame` or `--frame=`
/// it had among the options of the app; at the end when it had none.
pub fn with_frame(argv: &[String], frame: &str) -> Vec<String> {
    let first = app_word(argv).map_or(1, |at| at + 2).min(argv.len());
    let mut out = argv[..first].to_vec();
    let mut put = false;
    let mut words = argv[first..].iter();
    while let Some(word) = words.next() {
        let spaced = word == "-f" || word == "--frame";
        let joined = word.starts_with("--frame=")
            || (word.starts_with("-f") && !word.starts_with("--") && word.len() > 2);
        if spaced {
            words.next();
        }
        if !(spaced || joined) {
            out.push(word.clone());
        } else if !put {
            out.extend(["-f".to_string(), frame.to_string()]);
            put = true;
        }
    }
    if !put {
        out.extend(["-f".to_string(), frame.to_string()]);
    }
    out
}

/// A manager: the hotkey's child, or a `ptyrc manage` from elsewhere.
pub fn is_manager(info: &Value) -> bool {
    let argv = argv_of(info);
    let verb = argv.iter().position(|w| file_name(w) == "ptyrc");
    info["name"] == "manage"
        || verb.is_some_and(|at| argv.get(at + 1).is_some_and(|w| w == "manage"))
}

/// The apps of the session, by id.
pub fn list(client: &mut Client) -> Result<Vec<Value>, Fail> {
    let mut result = client.call("app.list", json!({}))?;
    match result["apps"].take() {
        Value::Array(apps) => Ok(apps),
        _ => Ok(Vec::new()),
    }
}

fn find(apps: &[Value], id: u64) -> Option<&Value> {
    apps.iter().find(|a| a["id"] == id)
}

/// The last line of what an app wrote on stderr.
pub fn last_log_line(info: &Value) -> Option<String> {
    let text = std::fs::read_to_string(info["log"].as_str()?).ok()?;
    let line = text.lines().rev().find(|l| !l.trim().is_empty())?;
    Some(line.trim().to_string())
}

fn ended_badly(info: &Value) -> bool {
    info["state"] == "exited" && info["status"] != 0
}

/// The failure of an app that ended at once.
fn early_end(info: &Value) -> Fail {
    Fail {
        status: 1,
        message: format!(
            "app {} ({}) ended at once, status {}",
            info["id"],
            label(info),
            info["status"]
        ),
        hint: last_log_line(info),
    }
}

/// `app.start`, then a watch of `START_WAIT`: an app that ended by then
/// with an error is a failure (status 1), the last line of its log the
/// hint. The info of the app.
pub fn start(client: &mut Client, argv: &[String], name: &str) -> Result<Value, Fail> {
    let started = client.call("app.start", json!({"argv": argv, "name": name}))?;
    let id = started["app"]["id"].as_u64().unwrap_or(0);
    let deadline = Instant::now() + START_WAIT;
    let mut info = started["app"].clone();
    loop {
        if let Some(now) = find(&list(client)?, id) {
            info = now.clone();
        }
        if ended_badly(&info) {
            return Err(early_end(&info));
        }
        if info["state"] == "exited" || Instant::now() >= deadline {
            return Ok(info);
        }
        std::thread::sleep(LOOK_EVERY);
    }
}

/// The `argv` and the name of `ptyrc-app WORDS`.
pub fn app_argv(words: &[String]) -> Result<(Vec<String>, String), Fail> {
    let program = apps::program(apps::APP).ok_or_else(apps::not_found(apps::APP))?;
    let name = words.first().cloned().unwrap_or_default();
    let mut argv = vec![program];
    argv.extend(words.iter().cloned());
    Ok((argv, name))
}

fn since(info: &Value) -> String {
    let secs = info["since"].as_f64().unwrap_or(0.0) as u64;
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

fn state(info: &Value) -> String {
    let state = info["state"].as_str().unwrap_or("?");
    match info["status"].as_i64() {
        Some(status) if state == "exited" => format!("exited {status}"),
        _ => state.to_string(),
    }
}

/// One line for an app that a verb acted on.
fn say(info: &Value) {
    println!("{} {}: {}", info["id"], label(info), state(info));
}

fn report(common: &Common, infos: &[Value]) {
    if common.json {
        print_json(&json!(infos), false);
    } else {
        infos.iter().for_each(say);
    }
}

pub fn ls(common: &Common) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let apps: Vec<Value> = list(&mut client)?
        .into_iter()
        .filter(|a| a["me"] != true)
        .collect();
    if common.json {
        print_json(&json!(apps), false);
        return Ok(ExitCode::SUCCESS);
    }
    if apps.is_empty() {
        eprintln!("ptyrc: no app\n  start one: ptyrc manage start APP");
        return Ok(ExitCode::SUCCESS);
    }
    let rows: Vec<[String; 7]> = apps
        .iter()
        .map(|a| {
            [
                a["id"].to_string(),
                a["kind"].as_str().unwrap_or("?").to_string(),
                state(a),
                since(a),
                a["pid"].to_string(),
                label(a),
                options(a).join(" "),
            ]
        })
        .collect();
    let head = ["ID", "KIND", "STATE", "SINCE", "PID", "NAME", "ARGS"];
    let width = |i: usize| {
        let cells = rows.iter().map(|r| r[i].chars().count());
        cells.max().unwrap_or(0).max(head[i].len())
    };
    let widths: Vec<usize> = (0..6).map(width).collect();
    let line = |cells: &[&str]| {
        let mut out = String::new();
        for (i, cell) in cells.iter().enumerate() {
            match i {
                0 | 3 | 4 => out.push_str(&format!("{cell:>w$}  ", w = widths[i])),
                6 => out.push_str(cell),
                _ => out.push_str(&format!("{cell:w$}  ", w = widths[i])),
            }
        }
        println!("{}", out.trim_end());
    };
    line(&head);
    for row in &rows {
        line(&row.each_ref().map(String::as_str));
    }
    Ok(ExitCode::SUCCESS)
}

fn start_verb(common: &Common, cmd: bool, words: Vec<String>) -> Result<ExitCode, Fail> {
    let (argv, name) = match cmd {
        true => {
            let name = file_name(&words[0]).to_string();
            (words, name)
        }
        false => app_argv(&words)?,
    };
    let mut client = connect(common)?;
    let info = start(&mut client, &argv, &name)?;
    report(common, &[info]);
    Ok(ExitCode::SUCCESS)
}

fn no_app(what: &str, apps: &[Value]) -> Fail {
    let known: Vec<String> = apps
        .iter()
        .filter(|a| a["me"] != true)
        .map(|a| format!("{} ({})", a["id"], label(a)))
        .collect();
    Fail {
        status: 4,
        message: format!("no such app: {what}"),
        hint: Some(match known.is_empty() {
            true => "no app; start one: ptyrc manage start APP".into(),
            false => format!("apps: {}", known.join(", ")),
        }),
    }
}

/// The apps a word names: an id, or a name, every app of it that
/// `wanted` takes.
fn named<'a>(
    apps: &'a [Value],
    word: &str,
    wanted: impl Fn(&Value) -> bool,
) -> Result<Vec<&'a Value>, Fail> {
    if let Ok(id) = word.parse::<u64>() {
        return match find(apps, id) {
            Some(app) => Ok(vec![app]),
            None => Err(no_app(word, apps)),
        };
    }
    let found: Vec<&Value> = apps
        .iter()
        .filter(|a| label(a) == word && wanted(a))
        .collect();
    match found.is_empty() {
        true => Err(no_app(word, apps)),
        false => Ok(found),
    }
}

/// `stop`, `pause`, `resume`, `forget`: one request an app.
fn each(common: &Common, verb: &str, select: &Select) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let apps = list(&mut client)?;
    let wanted = |a: &Value| (a["state"] == "exited") == (verb == "forget");
    let mut chosen: Vec<&Value> = Vec::new();
    if select.all {
        let others = |a: &&Value| a["me"] != true && !is_manager(a) && wanted(a);
        chosen.extend(apps.iter().filter(others));
    }
    for word in &select.apps {
        for app in named(&apps, word, wanted)? {
            if !chosen.iter().any(|c| c["id"] == app["id"]) {
                chosen.push(app);
            }
        }
    }
    let method = format!("app.{verb}");
    let mut infos = Vec::new();
    for app in &chosen {
        let result = client.call(&method, json!({"app": app["id"]}))?;
        let info = match verb {
            "forget" => {
                let mut info = (*app).clone();
                info["state"] = json!("gone");
                info
            }
            _ => result["app"].clone(),
        };
        infos.push(info);
    }
    if verb == "stop" {
        let deadline = Instant::now() + END_WAIT;
        loop {
            let now = list(&mut client)?;
            let mut left = 0;
            for info in &mut infos {
                match find(&now, info["id"].as_u64().unwrap_or(0)) {
                    None => info["state"] = json!("gone"),
                    Some(app) => {
                        left += usize::from(app["state"] != "exited");
                        *info = app.clone();
                    }
                }
            }
            if left == 0 {
                break;
            }
            if Instant::now() >= deadline {
                report(common, &infos);
                return Err(Fail {
                    status: 1,
                    message: format!("{left} app(s) did not end"),
                    hint: None,
                });
            }
            std::thread::sleep(LOOK_EVERY);
        }
    }
    report(common, &infos);
    Ok(ExitCode::SUCCESS)
}

/// The one app `word` names, of any state.
fn one<'a>(apps: &'a [Value], word: &str, why: &str) -> Result<&'a Value, Fail> {
    let found = named(apps, word, |_| true)?;
    let live: Vec<&Value> = found
        .iter()
        .copied()
        .filter(|a| a["state"] != "exited")
        .collect();
    let found = if live.is_empty() { found } else { live };
    match found[..] {
        [app] => Ok(app),
        _ => {
            let ids: Vec<String> = found.iter().map(|a| a["id"].to_string()).collect();
            Err(Fail::usage(
                format!("{word}: {} apps of that name", found.len()),
                format!("{why}; ids: {}", ids.join(", ")),
            ))
        }
    }
}

fn restart(common: &Common, word: &str, options: Vec<String>) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let apps = list(&mut client)?;
    let chosen: Vec<&Value> = match options.is_empty() {
        true => {
            let live = named(&apps, word, |a| a["state"] != "exited");
            live.or_else(|_| named(&apps, word, |_| true))?
        }
        false => vec![one(&apps, word, "options are for one app: name it by id")?],
    };
    let argv = |app: &Value| (!options.is_empty()).then(|| with_options(app, &options));
    let plans = chosen.into_iter().map(|app| (app, argv(app))).collect();
    restart_all(common, &mut client, plans)
}

fn move_to(common: &Common, word: &str, frame: &str, restart: bool) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let apps = list(&mut client)?;
    let app = one(&apps, word, "one app is moved: name it by id")?;
    if app["moves"] == true && !restart {
        let frame = match frame.parse::<u64>() {
            Ok(id) => json!(id),
            Err(_) => json!(frame),
        };
        let mut moved = client.call("app.move", json!({"app": app["id"], "frame": frame}))?;
        if common.json {
            print_json(&moved, false);
        } else {
            let info = moved["app"].take();
            println!("{} {}: frame {}", info["id"], label(&info), moved["frame"]);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let argv = with_frame(&argv_of(app), frame);
    restart_all(common, &mut client, vec![(app, Some(argv))])
}

/// `app.restart` for each app, with another `argv` when given; then
/// wait for each to have started again.
fn restart_all(
    common: &Common,
    client: &mut Client,
    plans: Vec<(&Value, Option<Vec<String>>)>,
) -> Result<ExitCode, Fail> {
    let mut infos = Vec::new();
    for (app, argv) in &plans {
        let mut params = json!({"app": app["id"]});
        if let Some(argv) = argv {
            params["argv"] = json!(argv);
            params["name"] = app["name"].clone();
        }
        infos.push(client.call("app.restart", params)?["app"].take());
    }
    let chosen: Vec<&Value> = plans.iter().map(|(app, _)| *app).collect();
    // Each one started again: one more run, running or ended.
    let runs: Vec<u64> = chosen
        .iter()
        .map(|a| a["runs"].as_u64().unwrap_or(0))
        .collect();
    let deadline = Instant::now() + END_WAIT;
    let mut again: Vec<Option<Instant>> = vec![None; infos.len()];
    loop {
        let now = list(client)?;
        for ((info, runs), again) in infos.iter_mut().zip(&runs).zip(&mut again) {
            if let Some(app) = find(&now, info["id"].as_u64().unwrap_or(0)) {
                *info = app.clone();
                if again.is_none() && app["runs"].as_u64() > Some(*runs) {
                    *again = Some(Instant::now());
                }
            }
        }
        if let Some(bad) = infos.iter().find(|i| ended_badly(i)) {
            return Err(early_end(bad));
        }
        // Watched as a start is, for an early end.
        if again
            .iter()
            .all(|a| a.is_some_and(|at| at.elapsed() >= START_WAIT))
        {
            break;
        }
        if Instant::now() >= deadline + START_WAIT {
            report(common, &infos);
            return Err(Fail {
                status: 1,
                message: "not started again within 5 s".into(),
                hint: None,
            });
        }
        std::thread::sleep(LOOK_EVERY);
    }
    report(common, &infos);
    Ok(ExitCode::SUCCESS)
}

fn frames(common: &Common) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let apps = list(&mut client)?;
    let listed = client.call("frame.list", json!({}))?;
    let on_screen = |f: &&Value| f["state"] != "inactive";
    let frames: Vec<&Value> = (listed["frames"].as_array().into_iter().flatten())
        .filter(on_screen)
        .collect();
    let on = |frame: &Value| -> Vec<&Value> {
        let there = |a: &&Value| {
            let ids = a["frames"].as_array().into_iter().flatten();
            a["me"] != true && ids.clone().any(|id| *id == frame["id"])
        };
        apps.iter().filter(there).collect()
    };
    if common.json {
        let rows: Vec<Value> = frames
            .iter()
            .map(|f| {
                let ids: Vec<&Value> = on(f).iter().map(|a| &a["id"]).collect();
                json!({"frame": f, "apps": ids})
            })
            .collect();
        print_json(&json!(rows), false);
        return Ok(ExitCode::SUCCESS);
    }
    let rows: Vec<[String; 6]> = frames
        .iter()
        .map(|f| {
            let names: Vec<String> = on(f).iter().map(|a| label(a)).collect();
            let key = f["key"].as_str().or(f["support"].as_str()).unwrap_or("-");
            [
                f["id"].to_string(),
                f["kind"].as_str().unwrap_or("?").to_string(),
                format!("{}x{}", f["rect"][2], f["rect"][3]),
                key.to_string(),
                if f["is_idle"] == true { "idle" } else { "busy" }.to_string(),
                names.join(", "),
            ]
        })
        .collect();
    let head = ["ID", "KIND", "SIZE", "KEY", "IDLE", "APPS"];
    let width = |i: usize| {
        let cells = rows.iter().map(|r| r[i].chars().count());
        cells.max().unwrap_or(0).max(head[i].len())
    };
    let widths: Vec<usize> = (0..5).map(width).collect();
    let line = |cells: &[&str]| {
        let mut out = String::new();
        for (i, cell) in cells.iter().enumerate() {
            match i {
                0 => out.push_str(&format!("{cell:>w$}  ", w = widths[i])),
                5 => out.push_str(cell),
                _ => out.push_str(&format!("{cell:w$}  ", w = widths[i])),
            }
        }
        println!("{}", out.trim_end());
    };
    line(&head);
    for row in &rows {
        line(&row.each_ref().map(String::as_str));
    }
    Ok(ExitCode::SUCCESS)
}

fn log(common: &Common, word: &str) -> Result<ExitCode, Fail> {
    let mut client = connect(common)?;
    let apps = list(&mut client)?;
    let app = one(&apps, word, "name it by id")?;
    let Some(path) = app["log"].as_str() else {
        return Err(Fail {
            status: 4,
            message: format!("app {} ({}) has no log", app["id"], label(app)),
            hint: Some("the driver keeps the stderr of the apps it started".into()),
        });
    };
    let text = std::fs::read_to_string(path).map_err(|e| Fail {
        status: 5,
        message: format!("{path}: {e}"),
        hint: None,
    })?;
    if common.json {
        print_json(&json!({"app": app["id"], "log": path, "text": text}), false);
    } else {
        print!("{text}");
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_options() {
        let app = json!({"name": "ptyrc-app", "kind": "client",
            "argv": ["/usr/bin/python3", "/home/u/.local/bin/ptyrc-app", "bricks", "-a"]});
        assert_eq!(label(&app), "bricks");
        assert_eq!(options(&app), ["-a"]);
        let other = ["-r".to_string(), "--speed".to_string(), "8".to_string()];
        assert_eq!(
            with_options(&app, &other),
            [
                "/usr/bin/python3",
                "/home/u/.local/bin/ptyrc-app",
                "bricks",
                "-r",
                "--speed",
                "8"
            ]
        );
        let cmd = json!({"name": "mine", "argv": ["/bin/sh", "-c", "sleep 1"]});
        assert_eq!(label(&cmd), "mine");
        assert_eq!(options(&cmd), ["-c", "sleep 1"]);
        assert_eq!(with_options(&cmd, &other)[..2], ["/bin/sh", "-r"]);
        let bare = json!({"name": "ptyrc-app", "argv": ["ptyrc-app"]});
        assert_eq!(
            (label(&bare), options(&bare).len()),
            ("ptyrc-app".into(), 0)
        );
        let none = json!({"name": "", "argv": []});
        assert_eq!((label(&none), options(&none).len()), (String::new(), 0));
        assert!(is_manager(&json!({"name": "manage", "argv": ["x"]})));
        assert!(is_manager(
            &json!({"name": "ptyrc", "argv": ["/bin/ptyrc", "manage"]})
        ));
        assert!(!is_manager(
            &json!({"name": "ptyrc", "argv": ["/bin/ptyrc", "show"]})
        ));
    }

    #[test]
    fn frames_replace_each_other() {
        let argv = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        let app = |tail: &[&str]| argv(&[&["/bin/ptyrc-app", "fishes"], tail].concat());
        assert_eq!(with_frame(&app(&[]), "3"), app(&["-f", "3"]));
        assert_eq!(
            with_frame(&app(&["-a", "-f", "1", "-b"]), "3"),
            app(&["-a", "-f", "3", "-b"])
        );
        assert_eq!(
            with_frame(&app(&["--frame", "raw"]), "idle"),
            app(&["-f", "idle"])
        );
        assert_eq!(
            with_frame(&app(&["--frame=2", "-a", "-f1"]), "3"),
            app(&["-f", "3", "-a"])
        );
        // The words before the app's name are not its options.
        let cmd = argv(&["python3", "-f", "x"]);
        assert_eq!(with_frame(&cmd, "3"), argv(&["python3", "-f", "3"]));
        let run = argv(&["python3", "-f", "/bin/ptyrc-app", "cat"]);
        assert_eq!(
            with_frame(&run, "3"),
            argv(&["python3", "-f", "/bin/ptyrc-app", "cat", "-f", "3"])
        );
    }

    #[test]
    fn words_name_apps() {
        let apps = [
            json!({"id": 1, "name": "a", "argv": ["a"], "state": "running"}),
            json!({"id": 2, "name": "a", "argv": ["a"], "state": "exited"}),
            json!({"id": 3, "name": "a", "argv": ["a"], "state": "paused"}),
        ];
        let ids = |found: Vec<&Value>| found.iter().map(|a| a["id"].clone()).collect::<Vec<_>>();
        let live = |a: &Value| a["state"] != "exited";
        assert_eq!(ids(named(&apps, "a", live).unwrap()), [1, 3]);
        assert_eq!(ids(named(&apps, "2", live).unwrap()), [2]);
        assert_eq!(named(&apps, "9", live).unwrap_err().status, 4);
        let none = named(&apps, "b", live).unwrap_err();
        assert_eq!(none.hint.unwrap(), "apps: 1 (a), 2 (a), 3 (a)");
        assert_eq!(one(&apps, "a", "why").unwrap_err().status, 2);
        assert_eq!(one(&apps[1..], "a", "why").unwrap()["id"], 3);
        assert_eq!(one(&apps[1..2], "a", "why").unwrap()["id"], 2);
    }
}
