// SPDX-License-Identifier: GPL-3.0-or-later
//! A store app, for `app.start {store}`: the installed tree found by
//! the driver itself, its manifest, the runner's command line. The
//! client names an origin or an id, never a program.

use crate::grants::Permission;
use crate::permissions::Declared;
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_store::manifest::{self, Entry};
use ptyrc_store::options::{self, FileGiven, Handle};
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
    /// `[app] frame`: the frame of a start without one.
    pub frame: Option<String>,
    /// The files named at the start, by handle.
    pub files: Vec<Handle>,
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

/// `args` and `files` of `app.start`, checked against the manifest
/// (`options::resolve`); each file checked again on the disk: absolute,
/// canonical, a regular file or a directory. `bad-params` else.
pub(crate) fn resolve(
    m: &manifest::Manifest,
    args: Option<&Value>,
    files: Option<&Value>,
) -> Result<options::Resolved, RpcError> {
    let bad = |e: ptyrc_store::Error| {
        let error = RpcError::bad_params(e.message);
        match e.hint {
            Some(hint) => error.with_hint(hint),
            None => error,
        }
    };
    let files: Vec<FileGiven> = match files {
        None => Vec::new(),
        Some(Value::Array(list)) => (list.iter())
            .map(FileGiven::from_wire)
            .collect::<Result<_, _>>()
            .map_err(RpcError::bad_params)?,
        Some(_) => {
            return Err(RpcError::bad_params("files: an array")
                .with_hint(r#"[{"handle": "f0", "path": "/abs", "kind": "file"}]"#));
        }
    };
    for f in &files {
        options::check_named(&f.path, f.kind, f.write).map_err(|why| {
            RpcError::bad_params(format!("files: {why}"))
                .with_hint("a path made absolute and canonical by the launcher")
        })?;
    }
    options::resolve(m, args, &files).map_err(bad)
}

/// What `[permissions]` and `[limits]` declare for this run (an option
/// tied to a permission by `needs`: only when given), the user's words
/// and patterns, and the names not known here. A limit is asked as its
/// permission (`limit-memory`, `limit-time`), without a reason: the
/// pop-up shows the figures.
fn declared(manifest: &manifest::Manifest, run: &options::Resolved) -> (Declared, Vec<String>) {
    let mut declared = Declared::default();
    let mut unknown = Vec::new();
    let asked = run.asked(manifest);
    for p in (manifest.permissions.iter()).filter(|p| asked.contains(&p.name.as_str())) {
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
    declared.hand.words = run.words.clone();
    declared.hand.patterns = (run.patterns.iter())
        .map(|p| (p.pattern.clone(), p.regex))
        .collect();
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

/// The app in `tree`, as installed; `seed` the runner's `--seed`.
fn of_tree(
    installed: &install::Installed,
    tree: &Path,
    args: Option<&Value>,
    files: Option<&Value>,
    seed: Option<u64>,
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
    let mut guest = match &manifest.app.entry {
        Entry::Script(path) => vec!["--script".into(), text(tree.join(path))],
        Entry::Rust(_) => vec![text(tree.join(ptyrc_store::build::MODULE))],
    };
    // `[app] assets`: read by the runner, answered by it (`asset.*`).
    if let Some(a) = &manifest.app.assets {
        guest.splice(0..0, ["--assets".into(), text(tree.join(a))]);
    }
    // `--seed N` of the launcher: `random` as `ptyrc dev test --seed N`.
    if let Some(seed) = seed {
        guest.splice(0..0, ["--seed".into(), seed.to_string()]);
    }
    // `native/...` is the key of the shipped apps in `permissions.toml`.
    if installed.origin.starts_with("native/") {
        return Err(RpcError::new(
            ErrorCode::NotUsable,
            "store: an origin may not begin with native/",
        ));
    }
    let run = resolve(&manifest, args, files)?;
    let (declared, unknown) = declared(&manifest, &run);
    let init = Value::Object(run.init);
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
        frame: manifest.app.frame.clone(),
        files: run.files,
        first: !installed.started,
    })
}

/// The installed app `name` (an id or an origin) with `args`, `files`
/// and `seed` of `app.start`.
pub(crate) fn find(
    name: &str,
    args: Option<&Value>,
    files: Option<&Value>,
    seed: Option<u64>,
) -> Result<Found, RpcError> {
    let dirs = Dirs::from_env().map_err(refused)?;
    let (installed, tree) = install::locate(&dirs, name).map_err(|e| {
        let error = refused(e);
        match error.hint {
            Some(_) => error,
            None => error.with_hint("ptyrc store ls lists what is installed"),
        }
    })?;
    of_tree(&installed, &tree, args, files, seed)
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

    const HEAD: &str = "[app]\nid = \"a\"\nversion = \"1\"\nsummary = \"s\"\n\
                        license = \"GPL-3.0-or-later\"\napi = 1\nscript = \"app.js\"\n";

    fn manifest(rest: &str) -> manifest::Manifest {
        manifest::parse(&format!("{HEAD}{rest}")).unwrap()
    }

    const SNAKE: &str = "[permissions]\nshape = \"walls\"\nkeys-arrows = \"steer\"\n\
        words = { list = [\"food\"], why = \"eat\" }\n\
        [options]\nrate = { type = \"int\", default = 20, doc = \"r\" }\n\
        grab = { type = \"bool\", default = false, needs = [\"keys-arrows\"], doc = \"g\" }\n\
        word = { type = \"word\", doc = \"w\" }\n\
        mark = { type = \"pattern\", list = true, doc = \"m\" }\n";

    #[test]
    fn defaults_then_args() {
        let m = manifest(SNAKE);
        let init = |args: Value| Value::Object(resolve(&m, Some(&args), None).unwrap().init);
        assert_eq!(
            init(json!({})),
            json!({"rate": 20, "grab": false, "word": null, "mark": []})
        );
        assert_eq!(init(json!({"rate": 3}))["rate"], json!(3));
        let e = resolve(&m, Some(&json!({"speed": 3})), None).unwrap_err();
        assert_eq!(e.hint.as_deref(), Some("options: grab, mark, rate, word"));
        assert!(resolve(&m, Some(&json!({"rate": "3"})), None).is_err());
        assert!(resolve(&m, Some(&json!([1])), None).is_err());
        assert!(resolve(&m, None, Some(&json!({}))).is_err());
        let e = resolve(&manifest(""), Some(&json!({"x": 1})), None).unwrap_err();
        assert_eq!(e.hint.as_deref(), Some("the app has no option"));
    }

    #[test]
    fn a_permission_by_option() {
        let m = manifest(SNAKE);
        let names = |args: Value| {
            let run = resolve(&m, Some(&args), None).unwrap();
            let (d, _) = declared(&m, &run);
            let names: Vec<&str> = d.permissions.keys().map(|p| p.name()).collect();
            names.join(" ")
        };
        assert_eq!(names(json!({})), "shape words");
        assert_eq!(names(json!({"grab": true})), "shape words keys-arrows");
        assert_eq!(names(json!({"grab": false})), "shape words");
        // The rest answered: the pop-up asks the difference alone.
        let run = resolve(&m, Some(&json!({"grab": true})), None).unwrap();
        let (d, _) = declared(&m, &run);
        let entry = crate::permissions::Entry {
            allow: [Permission::Shape, Permission::Words].into(),
            deny: Default::default(),
        };
        let r = crate::permissions::resolve(&d, Some(&entry), false);
        assert_eq!(r.missing, [Permission::KeysArrows]);
        let (d, _) = declared(&m, &resolve(&m, None, None).unwrap());
        assert!(
            crate::permissions::resolve(&d, Some(&entry), false)
                .missing
                .is_empty()
        );
        let run = resolve(&m, Some(&json!({"word": "Fish", "mark": ["a b"]})), None).unwrap();
        let (d, _) = declared(&m, &run);
        assert_eq!(d.words, ["food"]);
        assert_eq!(d.hand.words, ["Fish"]);
        assert_eq!(d.hand.patterns, [("a b".to_string(), false)]);
    }

    #[test]
    fn files_checked_again() {
        let m = manifest(
            "[permissions]\nfiles = \"f\"\n[options]\nstop = { type = \"file\", doc = \"d\" }\n",
        );
        let args = json!({"stop": {"file": "f0", "name": "x"}});
        let file = |path: &str, kind: &str| json!([{"handle": "f0", "path": path, "kind": kind}]);
        let here =
            std::fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/src/store.rs")).unwrap();
        let here = here.to_string_lossy();
        let run = resolve(&m, Some(&args), Some(&file(&here, "file"))).unwrap();
        assert_eq!(run.files[0].path.to_string_lossy(), here);
        assert!(!run.files[0].write);
        for (path, kind) in [
            ("/dev/null", "file"),
            ("/nothing/here", "file"),
            ("/tmp/../etc/passwd", "file"),
            (&*here, "dir"),
            ("relative", "file"),
        ] {
            assert!(
                resolve(&m, Some(&args), Some(&file(path, kind))).is_err(),
                "{path}"
            );
        }
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
        let (d, unknown) = declared(&m, &options::resolve(&m, None, &[]).unwrap());
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
