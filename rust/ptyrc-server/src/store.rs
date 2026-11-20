// SPDX-License-Identifier: GPL-3.0-or-later
//! A store app, for `app.start {store}`: the installed tree found by
//! the driver itself, its manifest, the runner's command line. The
//! client names an origin or an id, never a program.

use crate::grants::Permission;
use crate::permissions::Declared;
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_store::manifest::{self, AppOption, Entry, OptionValue};
use ptyrc_store::{Dirs, Origin, StoreLock, install};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// The runner. `$PTYRC_BOX` names another.
pub(crate) const BOX: &str = "ptyrc-box";
const ENV_BOX: &str = "PTYRC_BOX";
const MANIFEST: &str = "ptyrc-app.toml";

/// An installed app, ready to start.
pub(crate) struct Found {
    /// The identity: `host/owner/repo[#path]`.
    pub origin: String,
    /// `[app] id`: for the eye.
    pub name: String,
    /// `TIER COMMIT`, the commit cut to 10 characters.
    pub tier: String,
    /// The runner's command line but the grants: `argv` makes it.
    pub base: Vec<String>,
    /// `--manifest` of the runner, without `granted`.
    pub given: Value,
    pub declared: Declared,
    /// Permissions of the manifest the driver does not know.
    pub unknown: Vec<String>,
    /// `[app] surface`.
    pub surface: manifest::Surface,
    /// This commit has not started yet.
    first: bool,
}

fn refused(e: ptyrc_store::Error) -> RpcError {
    let error = RpcError::new(ErrorCode::NotUsable, format!("store: {}", e.message));
    match e.hint {
        Some(hint) => error.with_hint(hint),
        None => error,
    }
}

/// The runner: `$PTYRC_BOX`; else beside this program; else of `PATH`.
fn runner() -> Result<PathBuf, RpcError> {
    let file = |p: PathBuf| p.is_file().then_some(p);
    let named = || std::env::var_os(ENV_BOX).map(PathBuf::from).and_then(file);
    let beside = || file(std::env::current_exe().ok()?.parent()?.join(BOX));
    let in_path = || {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).find_map(|dir| file(dir.join(BOX)))
    };
    named().or_else(beside).or_else(in_path).ok_or_else(|| {
        RpcError::new(ErrorCode::NotUsable, format!("{BOX}: not found")).with_hint(format!(
            "looked in ${ENV_BOX}, beside ptyrc, then in PATH; \
             install it: cargo install --path rust/ptyrc-box"
        ))
    })
}

/// The argument of `init`: the defaults of `[options]`, then `args`.
/// A name not declared, a value of another type: `bad-params`.
pub(crate) fn init_args(options: &[AppOption], args: Option<&Value>) -> Result<Value, RpcError> {
    let json = |v: &OptionValue| match v {
        OptionValue::Int(n) => Value::from(*n),
        OptionValue::Float(x) => Value::from(*x),
        OptionValue::Bool(b) => Value::from(*b),
        OptionValue::Str(s) => Value::from(s.as_str()),
    };
    let mut out: Map<String, Value> = options
        .iter()
        .map(|o| (o.name.clone(), json(&o.default)))
        .collect();
    let Some(args) = args else {
        return Ok(Value::Object(out));
    };
    let names = || {
        let names: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
        match names.is_empty() {
            true => "the app has no option".to_string(),
            false => format!("options: {}", names.join(", ")),
        }
    };
    let given = args
        .as_object()
        .ok_or_else(|| RpcError::bad_params("args: an object, {OPTION: VALUE}"))?;
    for (name, value) in given {
        let option = options.iter().find(|o| &o.name == name).ok_or_else(|| {
            RpcError::bad_params(format!("args: no option `{name}`")).with_hint(names())
        })?;
        let fits = match option.default {
            OptionValue::Int(_) => value.is_i64(),
            OptionValue::Float(_) => value.is_number(),
            OptionValue::Bool(_) => value.is_boolean(),
            OptionValue::Str(_) => value.is_string(),
        };
        if !fits {
            return Err(RpcError::bad_params(format!(
                "args: {name}: {} wanted",
                option.default.type_name()
            )));
        }
        out.insert(name.clone(), value.clone());
    }
    Ok(Value::Object(out))
}

/// What `[permissions]` and `[limits]` declare, and the names not
/// known here. A limit is asked as its permission (`limit-memory`,
/// `limit-time`), without a reason: the pop-up shows the figures.
fn declared(manifest: &manifest::Manifest) -> (Declared, Vec<String>) {
    let mut declared = Declared::default();
    let mut unknown = Vec::new();
    for p in &manifest.permissions {
        match Permission::parse(&p.name) {
            Some(permission) => {
                declared.permissions.insert(permission, p.why.clone());
                if !p.words.is_empty() {
                    declared.words = p.words.clone();
                }
            }
            None => unknown.push(p.name.clone()),
        }
    }
    for l in &manifest.limits {
        let name = manifest::limit_permission(&l.name);
        let permission = Permission::parse(name).expect("a permission of the list");
        declared.permissions.entry(permission).or_default();
        declared.limits.push((l.name.clone(), l.value));
    }
    (declared, unknown)
}

/// `--manifest` of the runner, as `ptyrc dev test` gives it: what the
/// app may know of itself, and `[limits]`; `granted` is added at each
/// start (`with_granted`).
fn given(manifest: &manifest::Manifest) -> Value {
    let permissions: Vec<Value> = (manifest.permissions.iter())
        .map(|p| match p.words.is_empty() {
            true => json!({"name": p.name}),
            false => json!({"name": p.name, "words": p.words}),
        })
        .collect();
    let limits: Map<String, Value> = (manifest.limits.iter())
        .map(|l| (l.name.clone(), json!(l.value)))
        .collect();
    json!({
        "id": manifest.app.id,
        "version": manifest.app.version,
        "permissions": permissions,
        "limits": limits,
    })
}

/// The runner's command line: `base` with `--manifest`, `granted` the
/// permissions held. The limits not granted stay at the default.
pub(crate) fn command_line(base: &[String], given: &Value, granted: &[&str]) -> Vec<String> {
    let mut given = given.clone();
    given["granted"] = json!(granted);
    let mut argv = base.to_vec();
    argv.insert(1, "--manifest".into());
    argv.insert(2, given.to_string());
    argv
}

/// `argv` of a store app started before, its grants `granted`: the
/// `--manifest` given anew. `None`: not a command line of `command_line`.
pub(crate) fn regranted(argv: &[String], granted: &[&str]) -> Option<Vec<String>> {
    if argv.get(1).map(String::as_str) != Some("--manifest") {
        return None;
    }
    let given: Value = serde_json::from_str(argv.get(2)?).ok()?;
    let base: Vec<String> = (argv.iter().take(1))
        .chain(argv.iter().skip(3))
        .cloned()
        .collect();
    Some(command_line(&base, &given, granted))
}

/// The app in `tree`, as installed.
fn of_tree(
    installed: &install::Installed,
    tree: &Path,
    args: Option<&Value>,
) -> Result<Found, RpcError> {
    let source = std::fs::read(tree.join(MANIFEST)).map_err(|e| {
        RpcError::new(
            ErrorCode::NotUsable,
            format!("store: {}: {MANIFEST}: {e}", installed.origin),
        )
        .with_hint(format!(
            "install it again: ptyrc store add {}",
            installed.origin
        ))
    })?;
    let manifest = manifest::parse_bytes(&source).map_err(refused)?;
    let text = |p: PathBuf| p.to_string_lossy().into_owned();
    // A script, or the module built at the install.
    let guest = match &manifest.app.entry {
        Entry::Script(path) => vec!["--script".into(), text(tree.join(path))],
        Entry::Rust(_) => vec![text(tree.join(ptyrc_store::build::MODULE))],
    };
    // `native/...` is the key of the shipped apps in `permissions.toml`.
    if installed.origin.starts_with("native/") {
        return Err(RpcError::new(
            ErrorCode::NotUsable,
            "store: an origin may not begin with native/",
        ));
    }
    let init = init_args(&manifest.options, args)?;
    let (declared, unknown) = declared(&manifest);
    let commit: String = installed.commit.chars().take(10).collect();
    Ok(Found {
        origin: installed.origin.clone(),
        name: manifest.app.id.clone(),
        tier: format!("{} {commit}", installed.tier.as_str()),
        base: [vec![text(runner()?)], guest, vec![init.to_string()]].concat(),
        given: given(&manifest),
        declared,
        unknown,
        surface: manifest.app.surface,
        first: !installed.started,
    })
}

/// The installed app `name` (an id or an origin) with `args`.
pub(crate) fn find(name: &str, args: Option<&Value>) -> Result<Found, RpcError> {
    let dirs = Dirs::from_env().map_err(refused)?;
    let (installed, tree) = install::locate(&dirs, name).map_err(|e| {
        let error = refused(e);
        match error.hint {
            Some(_) => error,
            None => error.with_hint("ptyrc store ls lists what is installed"),
        }
    })?;
    of_tree(&installed, &tree, args)
}

impl Found {
    /// The runner's command line, `granted` the permissions held.
    pub fn argv(&self, granted: &[&str]) -> Vec<String> {
        command_line(&self.base, &self.given, granted)
    }

    /// The commit started once: told to the lock, the tree before it
    /// pruned. Apart from the driver's loop: the lock may be held by an
    /// install.
    pub fn started(&self) {
        if !self.first {
            return;
        }
        let origin = self.origin.clone();
        std::thread::spawn(move || {
            let (Ok(dirs), Ok(origin)) = (Dirs::from_env(), Origin::parse(&origin)) else {
                return;
            };
            if let Ok(_lock) = StoreLock::take(&dirs, || {}) {
                let _ = install::started(&dirs, &origin);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Vec<AppOption> {
        vec![
            AppOption {
                name: "rate".into(),
                default: OptionValue::Int(20),
                doc: String::new(),
            },
            AppOption {
                name: "word".into(),
                default: OptionValue::Str("a".into()),
                doc: String::new(),
            },
        ]
    }

    #[test]
    fn defaults_then_args() {
        assert_eq!(
            init_args(&options(), None).unwrap(),
            json!({"rate": 20, "word": "a"})
        );
        assert_eq!(
            init_args(&options(), Some(&json!({"rate": 3}))).unwrap(),
            json!({"rate": 3, "word": "a"})
        );
    }

    #[test]
    fn an_option_not_declared_or_of_another_type() {
        let e = init_args(&options(), Some(&json!({"speed": 3}))).unwrap_err();
        assert_eq!(e.hint.as_deref(), Some("options: rate, word"));
        assert!(init_args(&options(), Some(&json!({"rate": "3"}))).is_err());
        assert!(init_args(&options(), Some(&json!([1]))).is_err());
        let e = init_args(&[], Some(&json!({"x": 1}))).unwrap_err();
        assert_eq!(e.hint.as_deref(), Some("the app has no option"));
    }

    #[test]
    fn limits_asked_as_their_permission() {
        let m = manifest::parse(
            "[app]\nid = \"a\"\nversion = \"1\"\nsummary = \"s\"\n\
             license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n\
             [permissions]\nshape = \"why\"\n\
             [limits]\nmemory-mib = 128\nlog-mib = 2\ntick-ms = 100\n",
        )
        .unwrap();
        let (d, unknown) = declared(&m);
        assert!(unknown.is_empty());
        let names: Vec<&str> = d.permissions.keys().map(|p| p.name()).collect();
        assert_eq!(names, ["shape", "limit-memory", "limit-time"]);
        assert_eq!(d.limits.len(), 3);
        let g = given(&m);
        assert_eq!(
            g["limits"],
            json!({"log-mib": 2, "memory-mib": 128, "tick-ms": 100})
        );
        assert_eq!(g["permissions"], json!([{"name": "shape"}]));
    }

    #[test]
    fn the_command_line_granted_anew() {
        let base: Vec<String> = ["box", "--script", "app.js", "{}"].map(String::from).into();
        let given = json!({"id": "a", "limits": {"memory-mib": 128}});
        let argv = command_line(&base, &given, &["shape", "limit-memory"]);
        assert_eq!(argv[1], "--manifest");
        assert_eq!(&argv[3..], &base[1..]);
        let told: Value = serde_json::from_str(&argv[2]).unwrap();
        assert_eq!(told["granted"], json!(["shape", "limit-memory"]));
        assert_eq!(told["limits"], json!({"memory-mib": 128}));
        let again = regranted(&argv, &["shape"]).unwrap();
        assert_eq!(again, command_line(&base, &given, &["shape"]));
        assert_eq!(regranted(&base, &[]), None);
    }
}
