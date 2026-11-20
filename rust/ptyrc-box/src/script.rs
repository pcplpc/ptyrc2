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

/// Member of the `init` JSON: unit mode, the `test_*` exports of the
/// main file run one an `event` of topic `UNIT`, not `init`.
pub const UNIT: &str = "ptyrc:unit";
/// The interpreter's line that names the tests, consumed here.
const TESTS_LINE: &str = "ptyrc:tests";

/// A host for unit tests: no session; the log kept.
struct Bench {
    log: Vec<String>,
    random: u64,
}

impl crate::Host for Bench {
    fn call(&mut self, method: &str, _: Value) -> Result<Value, crate::CallError> {
        let mut e =
            crate::CallError::new("not-usable", format!("{method}: no session in a unit test"));
        e.hint = Some("test the logic; ptyrc dev test for the screen".into());
        Err(e)
    }
    fn size(&self) -> (u32, u32) {
        (80, 24)
    }
    fn log(&mut self, text: &str) {
        self.log.extend(text.lines().map(str::to_owned));
    }
    fn random(&mut self, buf: &mut [u8]) {
        // splitmix64, seed 1: the same run every time.
        for chunk in buf.chunks_mut(8) {
            self.random = self.random.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.random;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            let z = (z ^ (z >> 31)).to_le_bytes();
            chunk.copy_from_slice(&z[..chunk.len()]);
        }
    }
}

/// Unit tests of a script: `init` (`args` with `UNIT`) loads the main
/// file and names its `test_*` exports; each runs in its own `event`,
/// under the limits of an event. The lines of the report, and the
/// status: 0 all passed, 1 one failed or the run ended.
pub fn unit(wasm: &[u8], args: &Value, limits: crate::Limits) -> (Vec<String>, u8) {
    let mut args = args.clone();
    if let Value::Object(o) = &mut args {
        o.insert(UNIT.to_owned(), Value::Bool(true));
    }
    let bench = Bench {
        log: Vec::new(),
        random: 1,
    };
    let mut runner = match crate::Runner::load(wasm, bench, limits) {
        Ok(r) => r,
        Err(e) => return (vec![format!("ended: {e}")], 1),
    };
    let mut names: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut take = |runner: &mut crate::Runner<Bench>, out: &mut Vec<String>| {
        for line in std::mem::take(&mut runner.host_mut().log) {
            match line.strip_prefix(TESTS_LINE) {
                Some(rest) if names.is_empty() => {
                    names = rest.split_whitespace().map(str::to_owned).collect()
                }
                _ => out.push(line),
            }
        }
        names.len()
    };
    let done = runner.init(&args, 0);
    let count = take(&mut runner, &mut out);
    if let Err(e) = done {
        let status = end_status(&e, &mut out);
        return (out, status);
    }
    for i in 0..=count {
        runner.resume();
        let mark = out.len();
        let done = runner.event(UNIT, &Value::Null, 0);
        take(&mut runner, &mut out);
        if let Err(e) = done {
            if !matches!(e, crate::Error::Exit(_)) {
                let name = names.get(i).map_or("?", String::as_str);
                out.insert(mark, format!("FAIL {name}"));
                let passed = out.iter().filter(|l| l.starts_with("ok   ")).count();
                out.push(format!(
                    "{passed} passed, {} failed, the rest not run",
                    i + 1 - passed
                ));
            }
            let status = end_status(&e, &mut out);
            return (out, status);
        }
    }
    out.push("ended: the tests did not end".into());
    (out, 1)
}

fn end_status(e: &crate::Error, out: &mut Vec<String>) -> u8 {
    // The runner's `exit: N`: the status tells it.
    if matches!(e, crate::Error::Exit(_)) && out.last() == Some(&e.to_string()) {
        out.pop();
    }
    match e {
        crate::Error::Exit(0) => 0,
        crate::Error::Exit(_) => 1,
        e => {
            // The runner logged it; said once.
            if !out.iter().any(|l| l.contains(&e.to_string())) {
                out.push(format!("ended: {e}"));
            }
            1
        }
    }
}
