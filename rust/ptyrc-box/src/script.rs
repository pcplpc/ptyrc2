// SPDX-License-Identifier: GPL-3.0-or-later
//! Script apps: the interpreter (`sdk/js`) is the guest; the sources
//! ride in the JSON of `init`, under two reserved members.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Member of the `init` JSON: an object, file name to text.
pub const SOURCES: &str = "ptyrc:sources";
/// Member of the `init` JSON: the entry, a name of `SOURCES`.
pub const MAIN: &str = "ptyrc:main";
/// The interpreter module, when not beside the runner.
pub const ENV_JS: &str = "PTYRC_JS";
/// Its file name.
pub const MODULE: &str = "ptyrc-js.wasm";

/// A script and the others of its tree, in all.
const MAX_BYTES: u64 = 1 << 20;
const MAX_FILES: usize = 512;
const MAX_DEPTH: usize = 16;

/// The scripts of a tree: every regular `.js` and `.mjs` under `root`,
/// by relative path with `/`. A symlink is not followed; a name that
/// is not UTF-8 is an error.
pub fn sources(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut found = BTreeMap::new();
    let mut bytes = 0u64;
    walk(root, "", 0, &mut found, &mut bytes)?;
    Ok(found)
}

fn walk(
    dir: &Path,
    prefix: &str,
    depth: usize,
    found: &mut BTreeMap<String, String>,
    bytes: &mut u64,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{}: too deep", dir.display()));
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| format!("{}: a name that is not UTF-8", path.display()))?;
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            if name != ".git" && name != "target" && name != "node_modules" {
                walk(&path, &format!("{prefix}{name}/"), depth + 1, found, bytes)?;
            }
        } else if kind.is_file() && (name.ends_with(".js") || name.ends_with(".mjs")) {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            *bytes += text.len() as u64;
            if *bytes > MAX_BYTES || found.len() >= MAX_FILES {
                return Err(format!(
                    "{}: more than 1 MiB of scripts, or {MAX_FILES} files",
                    dir.display()
                ));
            }
            found.insert(format!("{prefix}{name}"), text);
        }
    }
    Ok(())
}

/// The JSON of `init` for a script app: `args` (an object; anything
/// else counts as `{}`) and the reserved members, which win over
/// members of `args` with their names.
pub fn init_args(main: &str, sources: &BTreeMap<String, String>, args: Value) -> Value {
    let mut out = match args {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    let files = sources
        .iter()
        .map(|(name, text)| (name.clone(), Value::String(text.clone())))
        .collect();
    out.insert(SOURCES.to_owned(), Value::Object(files));
    out.insert(MAIN.to_owned(), Value::String(main.to_owned()));
    Value::Object(out)
}

/// Built in the source tree, from `target/<profile>/`.
const BUILT: &str = "../../sdk/js/interp/target/ptyrc-js.wasm";

/// Where the interpreter is: `PTYRC_JS`; else `ptyrc-js.wasm` beside
/// the program; else `../share/ptyrc/ptyrc-js.wasm` from there; else,
/// the program in `target/<profile>/` of the source tree, the module
/// built there (`sdk/js/README.md`).
pub fn interpreter_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(ENV_JS) {
        return Ok(PathBuf::from(path));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().unwrap_or(Path::new("."));
    let tried = [
        dir.join(MODULE),
        dir.join("../share/ptyrc").join(MODULE),
        dir.join(BUILT),
    ];
    tried.iter().find(|p| p.is_file()).cloned().ok_or_else(|| {
        format!(
            "the script interpreter is not installed ({}); build it: sdk/js/interp/build.sh \
             (sdk/js/README.md), or set {ENV_JS}",
            tried[0].display()
        )
    })
}

/// The interpreter module.
pub fn interpreter() -> Result<Vec<u8>, String> {
    let path = interpreter_path()?;
    std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}
