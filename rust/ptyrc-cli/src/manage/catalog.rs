// SPDX-License-Identifier: GPL-3.0-or-later
//! The apps `ptyrc-app` has and their options: `ptyrc-app --catalog`.

use crate::apps;
use serde_json::Value;
use std::process::{Command, Stdio};

/// How an option is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Given or not.
    Flag,
    /// Given a number of times.
    Count,
    /// One of `choices`.
    Choice,
    /// A typed value.
    Value,
}

/// An option of an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opt {
    /// As `-r`, `--rules`: the last is the one written.
    pub flags: Vec<String>,
    pub kind: Kind,
    pub choices: Vec<String>,
    /// As text; `None`: none, or not a plain value.
    pub default: Option<String>,
    pub help: String,
}

/// A positional argument of an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pos {
    pub name: String,
    pub help: String,
}

/// An app of the catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    pub name: String,
    pub help: String,
    pub options: Vec<Opt>,
    pub positionals: Vec<Pos>,
    /// The argument lists `random app` draws from; empty: never drawn.
    pub random: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub apps: Vec<App>,
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").to_string()
}

fn texts(value: &Value) -> Vec<String> {
    let items = value.as_array().map_or(&[][..], Vec::as_slice);
    items.iter().map(text).collect()
}

/// A default as the user would type it: `16.0` is `16`.
fn default(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e15 => {
                Some(format!("{}", f as i64))
            }
            _ => Some(n.to_string()),
        },
        _ => None,
    }
}

fn option(value: &Value) -> Option<Opt> {
    let flags = texts(&value["flags"]);
    let kind = match value["kind"].as_str()? {
        "flag" => Kind::Flag,
        "count" => Kind::Count,
        "choice" => Kind::Choice,
        "value" => Kind::Value,
        _ => return None,
    };
    (!flags.is_empty()).then(|| Opt {
        flags,
        kind,
        choices: texts(&value["choices"]),
        default: default(&value["default"]),
        help: text(&value["help"]),
    })
}

impl Catalog {
    /// Read what `ptyrc-app --catalog` printed. An option of a kind not
    /// known is skipped: its words go to `more arguments`.
    pub fn parse(json: &str) -> Result<Catalog, String> {
        let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let apps = value["apps"].as_array().ok_or("no `apps`")?;
        let app = |value: &Value| {
            let list = |key: &str| value[key].as_array().map_or(&[][..], Vec::as_slice);
            let name = value["name"].as_str()?.to_string();
            Some(App {
                name,
                help: text(&value["help"]),
                options: list("options").iter().filter_map(option).collect(),
                positionals: (list("positionals").iter())
                    .map(|p| Pos {
                        name: text(&p["name"]),
                        help: text(&p["help"]),
                    })
                    .collect(),
                random: list("random").iter().map(texts).collect(),
            })
        };
        Ok(Catalog {
            apps: apps.iter().filter_map(app).collect(),
        })
    }

    pub fn app(&self, name: &str) -> Option<&App> {
        self.apps.iter().find(|a| a.name == name)
    }
}

/// `ptyrc-app`, found as by `ptyrc app`, and its catalog. `None`: no
/// `ptyrc-app`, or one that has no catalog.
pub fn load() -> Option<Catalog> {
    let program = apps::program(apps::APP)?;
    let output = Command::new(&program)
        .arg("--catalog")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let json = String::from_utf8(output.stdout).ok()?;
    let catalog = Catalog::parse(&json).ok()?;
    (output.status.success() && !catalog.apps.is_empty()).then_some(catalog)
}

#[cfg(test)]
pub(crate) fn sample() -> Catalog {
    Catalog::parse(include_str!("../../tests/catalog.json")).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample() {
        let catalog = sample();
        assert_eq!(catalog.apps.len(), 12);
        let bricks = catalog.app("bricks").unwrap();
        assert_eq!(bricks.help, "brick breaker game that plays alone");
        assert_eq!(bricks.random, vec![vec![], vec!["-a".to_string()]]);
        let rules = &bricks.options[3];
        assert_eq!(rules.flags, ["-r", "--rules"]);
        assert_eq!((rules.kind, rules.default.as_deref()), (Kind::Flag, None));
        let speed = &bricks.options[1];
        assert_eq!(
            (speed.kind, speed.default.as_deref()),
            (Kind::Value, Some("16"))
        );
        let marks = catalog.app("marks").unwrap();
        assert!(marks.random.is_empty());
        assert_eq!(marks.positionals[0].name, "PATTERN");
        assert_eq!(
            catalog.app("snow").unwrap().options[3].default.as_deref(),
            Some("-1")
        );
        assert!(catalog.app("demo").is_none());
    }

    #[test]
    fn what_is_not_a_catalog() {
        assert!(Catalog::parse("usage: ptyrc app").is_err());
        assert!(Catalog::parse("{}").is_err());
        let odd = r#"{"apps":[{"name":"a","options":[{"flags":["-x"],"kind":"new"},
            {"flags":["-y"],"kind":"flag"}]},{"help":"no name"}]}"#;
        let catalog = Catalog::parse(odd).unwrap();
        assert_eq!(catalog.apps.len(), 1);
        assert_eq!(catalog.apps[0].options.len(), 1);
    }
}
