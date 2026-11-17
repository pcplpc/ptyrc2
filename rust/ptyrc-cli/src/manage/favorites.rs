// SPDX-License-Identifier: GPL-3.0-or-later
//! The options each app was last given in the menu:
//! `$XDG_CONFIG_HOME/ptyrc/favorites.json`, else
//! `~/.config/ptyrc/favorites.json`. `{"apps": {NAME: [ARG...]}}`.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// The arguments of each app, by name. No entry: never configured.
pub type Favorites = BTreeMap<String, Vec<String>>;

const FILE: &str = "favorites.json";

/// The config directory of ptyrc. A relative `XDG_CONFIG_HOME` is not
/// one.
fn dir() -> Option<PathBuf> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = absolute("XDG_CONFIG_HOME").or_else(|| Some(absolute("HOME")?.join(".config")))?;
    Some(base.join("ptyrc"))
}

/// What the file says. Entries that are not lists of strings are
/// skipped.
pub fn parse(text: &str) -> Favorites {
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let Some(apps) = value["apps"].as_object() else {
        return Favorites::new();
    };
    let args = |value: &Value| -> Option<Vec<String>> {
        (value.as_array()?.iter())
            .map(|word| word.as_str().map(str::to_string))
            .collect()
    };
    (apps.iter())
        .filter_map(|(name, value)| Some((name.clone(), args(value)?)))
        .collect()
}

/// The favorites of the file in `dir`. No file, or not one of ours:
/// none.
pub fn load_from(dir: &Path) -> Favorites {
    std::fs::read_to_string(dir.join(FILE)).map_or_else(|_| Favorites::new(), |text| parse(&text))
}

/// Set the arguments of one app in the file of `dir`: read again, as
/// another menu may have written; replaced whole, by a rename.
pub fn remember_in(dir: &Path, name: &str, args: &[String]) -> io::Result<()> {
    let mut favorites = load_from(dir);
    favorites.insert(name.to_string(), args.to_vec());
    let text = format!("{:#}\n", json!({"apps": favorites}));
    std::fs::create_dir_all(dir)?;
    let part = dir.join(format!("{FILE}.{}", std::process::id()));
    std::fs::write(&part, text)?;
    std::fs::rename(&part, dir.join(FILE)).inspect_err(|_| {
        let _ = std::fs::remove_file(&part);
    })
}

/// The favorites of the user.
pub fn load() -> Favorites {
    dir().map_or_else(Favorites::new, |dir| load_from(&dir))
}

/// Set the arguments of one app of the user.
pub fn remember(name: &str, args: &[String]) -> io::Result<()> {
    let dir = dir().ok_or_else(|| io::Error::other("no config directory: HOME is not set"))?;
    remember_in(&dir, name, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn what_the_file_says() {
        let favorites = parse(r#"{"apps": {"bricks": ["-a"], "clock": [], "odd": [1], "x": 2}}"#);
        assert_eq!(favorites.len(), 2);
        assert_eq!(favorites["bricks"], s(&["-a"]));
        assert!(favorites["clock"].is_empty());
        assert!(parse("").is_empty() && parse("[]").is_empty() && parse("{").is_empty());
    }

    #[test]
    fn remembered_one_app_at_a_time() {
        let dir = std::env::temp_dir().join(format!("ptyrc-favorites-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dir = dir.join("ptyrc");
        assert!(load_from(&dir).is_empty());
        remember_in(&dir, "bricks", &s(&["--speed", "3 4"])).unwrap();
        remember_in(&dir, "clock", &[]).unwrap();
        remember_in(&dir, "bricks", &s(&["-r"])).unwrap();
        let favorites = load_from(&dir);
        assert_eq!(favorites.len(), 2);
        assert_eq!(favorites["bricks"], s(&["-r"]));
        assert!(favorites["clock"].is_empty());
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [FILE]);
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
