// SPDX-License-Identifier: GPL-3.0-or-later
//! Store apps for `ptyrc app` and `ptyrc manage`: an installed app
//! named by its id or its origin, its options read from its manifest,
//! started by the driver (`app.start {store}`).

use crate::manage::catalog::{self, Kind, Opt};
use crate::{Common, Fail};
use ptyrc_store::manifest;
use ptyrc_store::options::{self, OptionType};
use ptyrc_store::shipped::{self, Shipped};
use ptyrc_store::{Dirs, Installed, install};
use ptyrc_store::{Launch, Manifest, OptionValue};
use serde_json::Value;
use std::io::IsTerminal;
use std::process::ExitCode;

const MANIFEST: &str = "ptyrc-app.toml";

/// An installed or shipped app and its manifest.
pub struct StoreApp {
    pub installed: Installed,
    pub manifest: Manifest,
    /// A shipped app (`ptyrc_store::shipped`).
    pub shipped: Option<Shipped>,
}

fn read(installed: Installed, tree: &std::path::Path) -> Result<StoreApp, Fail> {
    let source = std::fs::read(tree.join(MANIFEST)).map_err(|e| Fail {
        status: 5,
        message: format!("{}: {MANIFEST}: {e}", installed.origin),
        hint: Some(format!(
            "install it again: ptyrc store add {}",
            installed.origin
        )),
    })?;
    let manifest = manifest::parse_bytes(&source)?;
    Ok(StoreApp {
        installed,
        manifest,
        shipped: None,
    })
}

fn of_shipped(s: Shipped) -> StoreApp {
    StoreApp {
        installed: s.installed(),
        manifest: s.manifest.clone(),
        shipped: Some(s),
    }
}

/// A word that can only be an origin: `host/owner/repo`, `local:/dir`,
/// `ptyrc/NAME`.
pub fn is_origin(word: &str) -> bool {
    word.contains('/') || word.contains(':')
}

/// The installed or shipped app `name` (an id or an origin); `None`:
/// none such.
pub fn find(name: &str) -> Option<Result<StoreApp, Fail>> {
    let dirs = Dirs::from_env().ok()?;
    match install::resolve(&dirs, name) {
        Ok((_, Some(s))) => Some(Ok(of_shipped(s))),
        Ok((_, None)) => {
            let (installed, tree) = install::locate(&dirs, name).ok()?;
            Some(read(installed, &tree))
        }
        // A shipped tree that does not read: said.
        Err(e) if shipped::name_of(name).is_some() || !e.message.ends_with("not installed") => {
            Some(Err(e.into()))
        }
        Err(_) => None,
    }
}

/// Every installed app whose manifest reads, then the shipped ones.
pub fn installed() -> Vec<StoreApp> {
    let Ok(dirs) = Dirs::from_env() else {
        return Vec::new();
    };
    let list = install::list(&dirs).unwrap_or_default();
    let installed = list.into_iter().filter_map(|a| {
        let (installed, tree) = install::locate(&dirs, &a.origin).ok()?;
        read(installed, &tree).ok()
    });
    let shipped = shipped::list(&dirs).into_iter().flatten().map(of_shipped);
    installed.chain(shipped).collect()
}

/// A shipped Rust app built when its tree has changed (`Shipped::build`):
/// before the driver starts it.
pub fn prepare(app: &StoreApp) -> Result<(), Fail> {
    let Some(s) = app.shipped.as_ref().filter(|s| s.is_rust()) else {
        return Ok(());
    };
    let dirs = Dirs::from_env()?;
    s.build(&dirs, |t| {
        eprintln!("ptyrc: building {} with rustc {}", s.origin(), t.release)
    })?;
    Ok(())
}

/// `--help`: the manifest's (`options::help`), its origin, tier and
/// commit.
fn usage(app: &StoreApp) -> String {
    let m = &app.manifest;
    let mut text = options::help(m, &format!("ptyrc app {}", m.app.id));
    let told = format!(
        "{} {}, {} {}\n",
        app.installed.origin,
        m.app.version,
        app.installed.tier.as_str(),
        app.installed.commit.chars().take(10).collect::<String>(),
    );
    // After the summary: `usage: ...\n\nSUMMARY\n`.
    let summary_end = text.find("\n\n").map_or(0, |i| i + 2) + m.app.summary.len() + 1;
    text.insert_str(summary_end.min(text.len()), &told);
    text
}

/// The words after `ptyrc app ID`, read against the manifest
/// (`ptyrc_store::options::read`), paths against the working directory.
/// `Ok(None)`: `--help`.
pub fn parse_options(app: &StoreApp, words: &[String]) -> Result<Option<Launch>, Fail> {
    let cwd = std::env::current_dir().map_err(|e| Fail {
        status: 5,
        message: format!("the working directory: {e}"),
        hint: None,
    })?;
    options::read(&app.manifest, words, &cwd).map_err(|e| {
        let hint = format!("ptyrc app {} --help lists the options", app.manifest.app.id);
        Fail {
            status: 2,
            message: e.message,
            hint: Some(e.hint.unwrap_or(hint)),
        }
    })
}

/// The params of `app.start` for `app` with the option words `words`.
pub fn start_params(app: &StoreApp, words: &[String]) -> Result<Option<Value>, Fail> {
    let Some(launch) = parse_options(app, words)? else {
        return Ok(None);
    };
    prepare(app)?;
    Ok(Some(launch.params(&app.installed.origin)))
}

/// `ptyrc app ID|ORIGIN [--wait|--no-wait] [OPTION...]`: the driver
/// starts it; the pop-up first, when it asks for something not answered
/// yet. Waiting (`[app] wait = true`: by default): until it exits, its
/// result printed, the command's status that of the result.
pub fn run(common: &Common, app: &StoreApp, words: &[String]) -> Result<ExitCode, Fail> {
    let Some(launch) = parse_options(app, words)? else {
        print!("{}", usage(app));
        return Ok(ExitCode::SUCCESS);
    };
    let wait = launch.launcher.wait.unwrap_or(app.manifest.app.wait);
    prepare(app)?;
    let params = launch.params(&app.installed.origin);
    let mut client = crate::connect(common)?;
    let info = crate::manage::start_with(&mut client, params)?;
    if !wait {
        crate::manage::report(common, &[info]);
        return Ok(ExitCode::SUCCESS);
    }
    let id = info["id"].as_u64().unwrap_or(0);
    let mut last = info;
    while last["state"] != "exited" {
        std::thread::sleep(crate::manage::LOOK_EVERY);
        match crate::manage::find(&crate::manage::list(&mut client)?, id) {
            Some(now) => last = now.clone(),
            // Forgotten: no result.
            None => return Ok(ExitCode::from(1)),
        }
    }
    let result = &last["result"];
    if !result.is_null() {
        let tty = std::io::stdout().is_terminal();
        print!("{}", shown_result(&result["value"], common.json, tty));
    }
    // No result: 1; else the runner's own failure wins.
    let status = match (result["status"].as_u64(), last["status"].as_u64()) {
        (None, _) => 1,
        (Some(_), Some(failed)) if failed != 0 => failed,
        (Some(status), _) => status,
    };
    Ok(ExitCode::from(status.min(255) as u8))
}

/// A result as printed: a string as it is, a list a line each, else
/// JSON; `json`: always JSON. Control characters escaped; to a terminal
/// (`tty`), glyphs only.
fn shown_result(value: &Value, json: bool, tty: bool) -> String {
    let line = |v: &Value| match v {
        Value::String(s) if !json => s.clone(),
        v => v.to_string(),
    };
    let lines: Vec<String> = match value {
        Value::Array(items) if !json => items.iter().map(line).collect(),
        v => vec![line(v)],
    };
    let mut out = String::new();
    for l in lines {
        let escaped: String = l
            .chars()
            .map(|c| match c {
                c if c.is_control() => c.escape_unicode().to_string(),
                c => c.to_string(),
            })
            .collect();
        match tty {
            true => out.push_str(&ptyrc_vt::glyphs_only(&escaped)),
            false => out.push_str(&escaped),
        }
        out.push('\n');
    }
    out
}

/// The installed apps as entries of the menu of `ptyrc manage`, beside
/// the native ones: `names` are taken, an app of such an id goes by its
/// origin. The help says `store`.
pub fn catalog_apps(names: &[&str]) -> Vec<catalog::App> {
    let apps = installed();
    let mut out: Vec<catalog::App> = Vec::new();
    for app in &apps {
        let m = &app.manifest;
        let id = &m.app.id;
        let twice = apps.iter().filter(|a| &a.manifest.app.id == id).count() > 1;
        let name = match twice || names.contains(&id.as_str()) {
            true => app.installed.origin.clone(),
            false => id.clone(),
        };
        let options = catalog_rows(app);
        let positionals = (m.args.iter())
            .map(|p| catalog::Pos {
                name: p.option.name.to_uppercase(),
                help: p.option.doc.clone(),
            })
            .collect();
        // `[app] random`: the draws that read.
        let random = (m.app.random.iter())
            .filter(|words| matches!(parse_options(app, words), Ok(Some(_))))
            .cloned()
            .collect();
        out.push(catalog::App {
            name,
            help: format!("store, {}: {}", app.installed.tier.as_str(), m.app.summary),
            options,
            positionals,
            random,
            widgets: false,
        });
    }
    out
}

/// The menu rows of `app`: its options, then the launcher's.
fn catalog_rows(app: &StoreApp) -> Vec<Opt> {
    let m = &app.manifest;
    let mut options: Vec<Opt> = (m.options.iter())
        .map(|o| Opt {
            flags: o
                .short
                .map(|c| format!("-{c}"))
                .into_iter()
                .chain([format!("--{}", o.name)])
                .collect(),
            kind: match (o.ty, o.choices.is_empty()) {
                (OptionType::Bool, _) => Kind::Flag,
                (_, false) => Kind::Choice,
                _ => Kind::Value,
            },
            choices: o.choices.clone(),
            default: match &o.default {
                None | Some(OptionValue::Bool(_)) => None,
                Some(d) => Some(d.text()),
            },
            help: match o.range() {
                Some(r) => format!("{} ({r})", o.doc),
                None => o.doc.clone(),
            },
        })
        .collect();
    options.extend(launcher_rows());
    options
}

/// The rows of the launcher's options, the same for every store app.
fn launcher_rows() -> Vec<Opt> {
    let row = |flags: &[&str], kind: Kind, choices: &[&str], help: &str| Opt {
        flags: flags.iter().map(|f| f.to_string()).collect(),
        kind,
        choices: choices.iter().map(|c| c.to_string()).collect(),
        default: None,
        help: help.to_string(),
    };
    vec![
        row(&["--front"], Kind::Flag, &[], "in front of the text"),
        row(&["--back"], Kind::Flag, &[], "behind the text"),
        row(&["--frame"], Kind::Value, &[], "the frame it draws on"),
        row(
            &["--prefer"],
            Kind::Choice,
            options::PREFER,
            "where its widgets go",
        ),
        row(
            &["--alpha"],
            Kind::Value,
            &[],
            "the opacity of what it draws, 0 to 1",
        ),
        row(&["--border"], Kind::Value, &[], "the border of its widgets"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn app() -> StoreApp {
        let manifest = manifest::parse(
            "[app]\nid = \"sand\"\nversion = \"1.0.0\"\nsummary = \"Sand.\"\n\
             license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
             [options]\nrate = { type = \"int\", default = 20, min = 1, max = 99, doc = \"grains\" }\n\
             loud = { type = \"bool\", default = false, doc = \"noise\" }\n\
             word = { type = \"string\", default = \"a\", doc = \"a word\" }\n",
        )
        .unwrap();
        let installed = serde_json::from_value(json!({
            "origin": "example.org/a/sand", "id": "sand", "version": "1.0.0",
            "commit": "0123456789abcdef", "tier": "untested", "date": "2026-11-19",
        }))
        .unwrap();
        StoreApp {
            installed,
            manifest,
            shipped: None,
        }
    }

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn options_from_the_manifest() {
        let app = app();
        let parsed = |text: &str| {
            parse_options(&app, &words(text)).map(|l| l.map(|l| Value::Object(l.args)))
        };
        assert_eq!(parsed("").unwrap(), Some(json!({})));
        assert_eq!(
            parsed("--rate 3 --loud --word=fish").unwrap(),
            Some(json!({"rate": 3, "loud": true, "word": "fish"}))
        );
        assert_eq!(parsed("--no-loud").unwrap(), Some(json!({"loud": false})));
        assert_eq!(
            parsed("rate=3 loud=true --word w=x").unwrap(),
            Some(json!({"rate": 3, "loud": true, "word": "w=x"}))
        );
        assert_eq!(parsed("--rate 3 --help").unwrap(), None);
        assert!(parsed("--rate x").is_err());
        assert!(parsed("--rate 0").is_err());
        assert!(parsed("--rate").is_err());
        assert!(parsed("--speed 3").is_err());
        assert!(parsed("--no-rate").is_err());
        assert!(parsed("fish").is_err());
        let p = start_params(&app, &words("--front --alpha 0.5 --rate 2"))
            .unwrap()
            .unwrap();
        assert_eq!(
            p,
            json!({"store": "example.org/a/sand", "args": {"rate": 2}, "front": true, "alpha": 0.5})
        );
        let help = usage(&app);
        assert!(help.contains("--rate N") && help.contains("--front, --back"));
        assert!(
            help.contains("Sand.\nexample.org/a/sand 1.0.0, untested 0123456789\n"),
            "{help}"
        );
        let rows = &catalog_rows(&app);
        assert!(
            rows.iter()
                .any(|o| o.flags == ["--prefer"] && o.choices.len() == 10)
        );
        assert!(rows.iter().any(|o| o.help == "grains (1 to 99)"));
    }

    #[test]
    fn a_result_as_printed() {
        let show = |v: Value| shown_result(&v, false, false);
        assert_eq!(show(json!("done")), "done\n");
        assert_eq!(show(json!(["a", 2, {"b": 1}])), "a\n2\n{\"b\":1}\n");
        assert_eq!(show(json!({"n": 1})), "{\"n\":1}\n");
        assert_eq!(shown_result(&json!(["a"]), true, false), "[\"a\"]\n");
        // An escape sequence never reaches the terminal raw.
        let evil = show(json!("\u{1b}]52;c;aGk=\u{7}x\ny"));
        assert!(
            !evil.contains('\u{1b}') && !evil.contains('\u{7}'),
            "{evil:?}"
        );
        assert_eq!(evil, "\\u{1b}]52;c;aGk=\\u{7}x\\u{a}y\n");
        assert_eq!(shown_result(&json!("\u{202e}abc"), false, true), "abc\n");
    }

    #[test]
    fn what_is_an_origin() {
        assert!(is_origin("example.org/a/b") && is_origin("local:/tmp/x"));
        assert!(!is_origin("sand"));
    }
}
