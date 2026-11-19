// SPDX-License-Identifier: GPL-3.0-or-later
//! Store apps for `ptyrc app` and `ptyrc manage`: an installed app
//! named by its id or its origin, its options read from its manifest,
//! started by the driver (`app.start {store}`).

use crate::manage::catalog::{self, Kind, Opt};
use crate::{Common, Fail};
use ptyrc_store::manifest::{self, Manifest, OptionValue};
use ptyrc_store::{Dirs, Installed, install};
use serde_json::{Map, Value, json};
use std::process::ExitCode;

const MANIFEST: &str = "ptyrc-app.toml";

/// An installed app and its manifest.
pub struct StoreApp {
    pub installed: Installed,
    pub manifest: Manifest,
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
    })
}

/// A word that can only be an origin: `host/owner/repo`, `local:/dir`.
pub fn is_origin(word: &str) -> bool {
    word.contains('/') || word.contains(':')
}

/// The installed app `name` (an id or an origin); `None`: none such.
pub fn find(name: &str) -> Option<Result<StoreApp, Fail>> {
    let dirs = Dirs::from_env().ok()?;
    let (installed, tree) = install::locate(&dirs, name).ok()?;
    Some(read(installed, &tree))
}

/// Every installed app whose manifest reads.
pub fn installed() -> Vec<StoreApp> {
    let Ok(dirs) = Dirs::from_env() else {
        return Vec::new();
    };
    let list = install::list(&dirs).unwrap_or_default();
    list.into_iter()
        .filter_map(|a| {
            let (installed, tree) = install::locate(&dirs, &a.origin).ok()?;
            read(installed, &tree).ok()
        })
        .collect()
}

fn usage(app: &StoreApp) -> String {
    let m = &app.manifest;
    let mut text = format!(
        "usage: ptyrc app {} [OPTION...]\n\n{}\n{} {}, {} {}\n",
        m.app.id,
        m.app.summary,
        app.installed.origin,
        m.app.version,
        app.installed.tier.as_str(),
        app.installed.commit.chars().take(10).collect::<String>(),
    );
    if !m.options.is_empty() {
        text.push_str("\noptions:\n");
    }
    for o in &m.options {
        let (value, default) = match &o.default {
            OptionValue::Bool(b) => (String::new(), b.to_string()),
            OptionValue::Int(n) => (" N".into(), n.to_string()),
            OptionValue::Float(x) => (" X".into(), x.to_string()),
            OptionValue::Str(s) => (" TEXT".into(), format!("{s:?}")),
        };
        let flag = format!("--{}{value}", o.name);
        text.push_str(&format!("  {flag:<20} {} [default: {default}]\n", o.doc));
    }
    if m.options
        .iter()
        .any(|o| matches!(o.default, OptionValue::Bool(_)))
    {
        text.push_str("  --no-NAME            a yes-or-no option, off\n");
    }
    text
}

/// `--NAME VALUE`, `--NAME=VALUE`; of a yes-or-no option `--NAME`,
/// `--no-NAME`: the `args` of `app.start {store}`. `Ok(None)`: `--help`.
pub fn parse_options(app: &StoreApp, words: &[String]) -> Result<Option<Value>, Fail> {
    let options = &app.manifest.options;
    let bad = |message: String| Fail {
        status: 2,
        message,
        hint: Some(format!(
            "ptyrc app {} --help lists the options",
            app.manifest.app.id
        )),
    };
    let mut args = Map::new();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        if word == "--help" || word == "-h" {
            return Ok(None);
        }
        let Some(flag) = word.strip_prefix("--") else {
            return Err(bad(format!("{word}: an option is wanted, as --NAME VALUE")));
        };
        let (name, mut inline) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (flag, None),
        };
        let found = |name: &str| options.iter().find(|o| o.name == name);
        let (option, off) = match (found(name), name.strip_prefix("no-").and_then(found)) {
            (Some(option), _) => (option, false),
            (None, Some(option)) if matches!(option.default, OptionValue::Bool(_)) => {
                (option, true)
            }
            _ => return Err(bad(format!("--{name}: no such option"))),
        };
        let mut value = |what: &str| {
            inline
                .take()
                .or_else(|| words.next().cloned())
                .ok_or_else(|| bad(format!("--{name}: {what} is wanted")))
        };
        let parsed = match &option.default {
            OptionValue::Bool(_) => match inline.take().as_deref() {
                None => json!(!off),
                Some("true" | "yes" | "on") => json!(!off),
                Some("false" | "no" | "off") => json!(off),
                Some(v) => return Err(bad(format!("--{name}={v}: true or false"))),
            },
            OptionValue::Int(_) => {
                let v = value("an integer")?;
                json!(
                    v.parse::<i64>()
                        .map_err(|_| bad(format!("--{name} {v}: an integer")))?
                )
            }
            OptionValue::Float(_) => {
                let v = value("a number")?;
                let x = v.parse::<f64>().ok().filter(|x| x.is_finite());
                json!(x.ok_or_else(|| bad(format!("--{name} {v}: a number")))?)
            }
            OptionValue::Str(_) => json!(value("a text")?),
        };
        args.insert(option.name.clone(), parsed);
    }
    Ok(Some(Value::Object(args)))
}

/// The params of `app.start` for `app` with the option words `words`.
pub fn start_params(app: &StoreApp, words: &[String]) -> Result<Option<Value>, Fail> {
    Ok(parse_options(app, words)?.map(|args| json!({"store": app.installed.origin, "args": args})))
}

/// `ptyrc app ID|ORIGIN [OPTION...]`: the driver starts it; the pop-up
/// first, when it asks for something not answered yet.
pub fn run(common: &Common, app: &StoreApp, words: &[String]) -> Result<ExitCode, Fail> {
    let Some(params) = start_params(app, words)? else {
        print!("{}", usage(app));
        return Ok(ExitCode::SUCCESS);
    };
    let mut client = crate::connect(common)?;
    let info = crate::manage::start_with(&mut client, params)?;
    crate::manage::report(common, &[info]);
    Ok(ExitCode::SUCCESS)
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
        let options = (m.options.iter())
            .map(|o| Opt {
                flags: vec![format!("--{}", o.name)],
                kind: match o.default {
                    OptionValue::Bool(_) => Kind::Flag,
                    _ => Kind::Value,
                },
                choices: Vec::new(),
                default: match &o.default {
                    OptionValue::Bool(_) => None,
                    OptionValue::Int(n) => Some(n.to_string()),
                    OptionValue::Float(x) => Some(x.to_string()),
                    OptionValue::Str(s) => Some(s.clone()),
                },
                help: o.doc.clone(),
            })
            .collect();
        out.push(catalog::App {
            name,
            help: format!("store, {}: {}", app.installed.tier.as_str(), m.app.summary),
            options,
            positionals: Vec::new(),
            random: Vec::new(),
            widgets: false,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> StoreApp {
        let manifest = manifest::parse(
            "[app]\nid = \"sand\"\nversion = \"1.0.0\"\nsummary = \"Sand.\"\n\
             license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
             [options]\nrate = { type = \"int\", default = 20, doc = \"grains\" }\n\
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
        }
    }

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn options_from_the_manifest() {
        let app = app();
        let parsed = |text: &str| parse_options(&app, &words(text));
        assert_eq!(parsed("").unwrap(), Some(json!({})));
        assert_eq!(
            parsed("--rate 3 --loud --word=fish").unwrap(),
            Some(json!({"rate": 3, "loud": true, "word": "fish"}))
        );
        assert_eq!(parsed("--no-loud").unwrap(), Some(json!({"loud": false})));
        assert_eq!(parsed("--rate 3 --help").unwrap(), None);
        assert!(parsed("--rate x").is_err());
        assert!(parsed("--rate").is_err());
        assert!(parsed("--speed 3").is_err());
        assert!(parsed("--no-rate").is_err());
        assert!(parsed("fish").is_err());
        let help = usage(&app);
        assert!(help.contains("--rate N") && help.contains("example.org/a/sand"));
    }

    #[test]
    fn what_is_an_origin() {
        assert!(is_origin("example.org/a/b") && is_origin("local:/tmp/x"));
        assert!(!is_origin("sand"));
    }
}
