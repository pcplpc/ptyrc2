// SPDX-License-Identifier: GPL-3.0-or-later
//! `probe.git`: a repository read with the commands of `run::Cmd`, as
//! JSON. Blocking; for the driver's worker.

pub mod parse;
pub mod run;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{Map, Value, json};

use parse::{Change, DAYS, Kind};
use run::{Cmd, Git};

/// Commits of `log` by default, and at most.
pub const LOG: u32 = 30;
pub const LOG_MAX: u32 = 200;
/// Paths of `changes` kept; `counts` counts them all.
pub const CHANGES: usize = 500;
/// Stashes, branches, work trees kept.
pub const LIST: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Head,
    Changes,
    Log,
    Stashes,
    Branches,
    Worktrees,
    Days,
    State,
}

impl Part {
    pub const ALL: [Part; 8] = [
        Part::Head,
        Part::Changes,
        Part::Log,
        Part::Stashes,
        Part::Branches,
        Part::Worktrees,
        Part::Days,
        Part::State,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Part::Head => "head",
            Part::Changes => "changes",
            Part::Log => "log",
            Part::Stashes => "stashes",
            Part::Branches => "branches",
            Part::Worktrees => "worktrees",
            Part::Days => "days",
            Part::State => "state",
        }
    }

    pub fn parse(name: &str) -> Option<Part> {
        Part::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// What is asked. `log`: clamped to 1..=`LOG_MAX`; `since`: the stamp
/// of the last answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub parts: Vec<Part>,
    pub log: u32,
    pub since: Option<String>,
}

impl Default for Ask {
    fn default() -> Ask {
        Ask {
            parts: Part::ALL.to_vec(),
            log: LOG,
            since: None,
        }
    }
}

impl Ask {
    /// From the params of `probe.git` (`repo` is the driver's and is
    /// ignored). Err: the rule broken, for `bad-params`.
    pub fn from_json(params: &Value) -> Result<Ask, String> {
        let mut ask = Ask::default();
        let Some(params) = params.as_object() else {
            return if params.is_null() {
                Ok(ask)
            } else {
                Err("params: an object".into())
            };
        };
        if let Some(parts) = params.get("parts") {
            let list = parts.as_array().ok_or("parts: a list of names")?;
            ask.parts.clear();
            for p in list {
                let part = p.as_str().and_then(Part::parse).ok_or_else(|| {
                    let names: Vec<_> = Part::ALL.iter().map(|p| p.name()).collect();
                    format!("parts: of {}", names.join(", "))
                })?;
                if !ask.parts.contains(&part) {
                    ask.parts.push(part);
                }
            }
        }
        if let Some(log) = params.get("log") {
            let n = log.as_u64().ok_or("log: a count, 1 to 200")?;
            ask.log = n.clamp(1, LOG_MAX as u64) as u32;
        }
        if let Some(since) = params.get("since") {
            ask.since = Some(since.as_str().ok_or("since: a stamp")?.to_string());
        }
        Ok(ask)
    }

    fn has(&self, part: Part) -> bool {
        self.parts.contains(&part)
    }
}

/// The time of the answer: epoch seconds, and the offset of the local
/// zone east of UTC at that time (for `days`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Now {
    pub at: f64,
    pub offset: i64,
}

/// The answer when `probe-git` is not held.
pub fn not_held() -> Value {
    json!({"repo": null, "degree": "none"})
}

/// The repository holding `dir`, as `probe.git` answers.
pub fn probe(dir: &Path, ask: &Ask, now: Now) -> Value {
    probe_with(Path::new("git"), dir, ask, now)
}

/// `probe` with another `git` program (tests).
pub fn probe_with(program: &Path, dir: &Path, ask: &Ask, now: Now) -> Value {
    if let (Some(since), Some((gitdir, common))) = (&ask.since, discover(dir)) {
        let stamp = stamp(&gitdir, &common, ask);
        if *since == stamp {
            return json!({"unchanged": true, "stamp": stamp});
        }
    }
    match read(program, dir, ask, now) {
        Some((repo, stamp)) => json!({"repo": repo, "stamp": stamp}),
        None => json!({"repo": null, "stamp": ""}),
    }
}

/// The git directory and the common one of the work tree holding
/// `dir`, found as git finds them, without git: `.git` a directory,
/// or a file `gitdir: PATH`; `commondir` in it. Canonical.
pub fn discover(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let dir = dir.canonicalize().ok()?;
    for at in dir.ancestors() {
        let dot = at.join(".git");
        let gitdir = if dot.is_dir() {
            dot
        } else if dot.is_file() {
            let text = read_small(&dot)?;
            at.join(text.strip_prefix("gitdir:")?.trim())
        } else {
            continue;
        };
        let gitdir = gitdir.canonicalize().ok()?;
        let common = match read_small(&gitdir.join("commondir")) {
            Some(rel) => gitdir.join(rel.trim()).canonicalize().ok()?,
            None => gitdir.clone(),
        };
        return Some((gitdir, common));
    }
    None
}

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut out = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(4096)
        .read_to_string(&mut out)
        .ok()?;
    Some(out)
}

/// Files of the git directory, and of the common one, whose mtimes make
/// the stamp (`_gitprobe.Watch`, with `config` and `refs/heads`).
const WATCH: [&str; 9] = [
    "HEAD",
    "index",
    "logs/HEAD",
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
    "rebase-merge/msgnum",
    "rebase-apply/next",
];
const SHARED: [&str; 6] = [
    "refs/stash",
    "FETCH_HEAD",
    "packed-refs",
    "worktrees",
    "refs/heads",
    "config",
];

/// The stamp of a repository for `ask`: a hash of its directories, of
/// `ask` but `since`, and of the mtimes of `WATCH` and `SHARED`. The
/// work tree is not watched: a file changed and not added keeps it.
pub fn stamp(gitdir: &Path, common: &Path, ask: &Ask) -> String {
    let mut h = DefaultHasher::new();
    gitdir.hash(&mut h);
    common.hash(&mut h);
    ask.parts.hash(&mut h);
    ask.log.hash(&mut h);
    let paths = WATCH.iter().map(|n| gitdir.join(n));
    for path in paths.chain(SHARED.iter().map(|n| common.join(n))) {
        let ns = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        ns.hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

fn mtime(path: &Path) -> Option<f64> {
    let m = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_secs_f64())
}

/// The operation left open in `gitdir`, and since when; empty when none.
pub fn progress(gitdir: &Path) -> (String, Option<f64>) {
    let number = |path: PathBuf| {
        let text = read_small(&path).unwrap_or_default();
        let text = text.trim();
        if !text.is_empty() && text.len() <= 9 && text.bytes().all(|b| b.is_ascii_digit()) {
            text.to_string()
        } else {
            "?".to_string()
        }
    };
    let (merge, apply) = (gitdir.join("rebase-merge"), gitdir.join("rebase-apply"));
    if merge.is_dir() {
        let step = format!(
            "{}/{}",
            number(merge.join("msgnum")),
            number(merge.join("end"))
        );
        return (format!("rebase {step}"), mtime(&merge.join("head-name")));
    }
    if apply.is_dir() {
        let what = if apply.join("applying").exists() {
            "am"
        } else {
            "rebase"
        };
        let step = format!(
            "{}/{}",
            number(apply.join("next")),
            number(apply.join("last"))
        );
        let since = mtime(&apply.join("head-name")).or_else(|| mtime(&apply));
        return (format!("{what} {step}"), since);
    }
    for (name, what) in [
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("BISECT_LOG", "bisect"),
    ] {
        if gitdir.join(name).exists() {
            return (what.to_string(), mtime(&gitdir.join(name)));
        }
    }
    (String::new(), None)
}

/// The last component of `path`; `/` for the root.
fn last(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| "/".to_string(), |n| n.to_string_lossy().into_owned())
}

/// Filter drivers named by the `info/attributes` of `dirs`; none when
/// one cannot be read or names too many.
fn filters(dirs: &[&Path]) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for dir in dirs {
        let text = match std::fs::read_to_string(dir.join("info/attributes")) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        };
        for name in run::filters(&text)? {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    (out.len() <= run::FILTERS).then_some(out)
}

fn read(program: &Path, dir: &Path, ask: &Ask, now: Now) -> Option<(Value, String)> {
    let mut git = Git::new(program, dir);
    let located = git.run(Cmd::Locate)?;
    let [root, gitdir, common, format] = parse::parse_locate(&located)?;
    let root = PathBuf::from(root);
    let gitdir = Path::new(gitdir).canonicalize().ok()?;
    let common = dir.join(common).canonicalize().ok()?;
    let stamp = stamp(&gitdir, &common, ask);
    git.arm(format, filters(&[&gitdir, &common]));

    let mut out = Map::new();
    out.insert("name".into(), last(&root.to_string_lossy()).into());
    out.insert("now".into(), now.at.into());

    let status = [Part::Head, Part::Changes, Part::State]
        .iter()
        .any(|p| ask.has(*p))
        .then(|| parse::parse_status(&git.run(Cmd::Status).unwrap_or_default()));
    let initial = status.as_ref().is_some_and(|(h, _)| h.initial);
    if let Some((head, changes)) = &status {
        if ask.has(Part::Head) {
            out.insert("head".into(), head_json(head));
        }
        let wants = ask.has(Part::Changes) || ask.has(Part::State);
        let counted = if wants {
            count(&git, changes, initial)
        } else {
            Vec::new()
        };
        if ask.has(Part::Changes) {
            let mut counts = Map::new();
            for kind in Kind::ALL {
                let n = changes.iter().filter(|c| c.kind == kind).count();
                counts.insert(kind.as_str().into(), n.into());
            }
            out.insert("changes".into(), counted.iter().map(change_json).collect());
            out.insert("counts".into(), counts.into());
        }
        if ask.has(Part::State) {
            let (state, since) = progress(&gitdir);
            out.insert("state".into(), state.into());
            out.insert("state_since".into(), since.into());
            out.insert("dirty_since".into(), dirty(&root, &gitdir, &counted).into());
        }
    }
    if ask.has(Part::Log) {
        let commits = if initial {
            Vec::new()
        } else {
            parse::parse_log(&git.run(Cmd::Log(ask.log)).unwrap_or_default())
        };
        let commits: Vec<Value> = commits
            .iter()
            .map(|c| {
                json!({"oid": c.oid, "at": c.at, "author": c.author, "subject": c.subject,
                       "refs": c.refs, "graph": c.graph})
            })
            .collect();
        out.insert("log".into(), commits.into());
    }
    if ask.has(Part::Days) {
        let since = now.at.floor() as i64 - (DAYS as i64 + 1) * 86400;
        let text = if initial {
            None
        } else {
            git.run(Cmd::Days(since))
        };
        let days = parse::parse_days(&text.unwrap_or_default(), now.at, now.offset, DAYS);
        out.insert("days".into(), days.into());
    }
    if ask.has(Part::Stashes) {
        let stashes = parse::parse_stashes(&git.run(Cmd::Stashes).unwrap_or_default());
        let list: Vec<Value> = stashes
            .iter()
            .take(LIST)
            .map(|s| json!({"n": s.n, "at": s.at, "subject": s.subject}))
            .collect();
        out.insert("stashes".into(), list.into());
    }
    if ask.has(Part::Branches) {
        let branches = parse::parse_branches(&git.run(Cmd::Branches).unwrap_or_default());
        let list: Vec<Value> = branches
            .iter()
            .take(LIST)
            .map(|b| {
                json!({"name": b.name, "current": b.current, "at": b.at, "upstream": b.upstream,
                       "ahead": b.ahead, "behind": b.behind, "gone": b.gone, "subject": b.subject})
            })
            .collect();
        out.insert("branches".into(), list.into());
    }
    if ask.has(Part::Worktrees) {
        let trees = parse::parse_worktrees(&git.run(Cmd::Worktrees).unwrap_or_default());
        let here = root.canonicalize().unwrap_or(root.clone());
        let list: Vec<Value> = trees
            .iter()
            .take(LIST)
            .map(|w| {
                let path = Path::new(&w.path);
                let current = path.canonicalize().unwrap_or(path.to_path_buf()) == here;
                json!({"name": last(&w.path), "oid": w.oid, "branch": w.branch,
                       "flags": w.flags, "current": current})
            })
            .collect();
        out.insert("worktrees".into(), list.into());
    }
    Some((out.into(), stamp))
}

fn head_json(h: &parse::Head) -> Value {
    json!({"oid": h.oid, "branch": h.branch, "upstream": h.upstream, "ahead": h.ahead,
           "behind": h.behind, "gone": h.gone, "initial": h.initial})
}

fn change_json(c: &Change) -> Value {
    json!({"kind": c.kind.as_str(), "code": c.code.to_string(), "path": c.path,
           "added": c.added, "deleted": c.deleted})
}

/// The first `CHANGES` of `changes`, with their lines counted.
fn count(git: &Git, changes: &[Change], initial: bool) -> Vec<Change> {
    let unstaged = parse::parse_numstat(&git.run(Cmd::DiffFiles).unwrap_or_default());
    let staged = if initial {
        HashMap::new()
    } else {
        parse::parse_numstat(&git.run(Cmd::DiffIndex).unwrap_or_default())
    };
    changes
        .iter()
        .take(CHANGES)
        .map(|c| {
            let table = match c.kind {
                Kind::Staged => Some(&staged),
                Kind::Unstaged => Some(&unstaged),
                _ => None,
            };
            let lines = table.and_then(|t| t.get(&c.path));
            Change {
                added: lines.map(|l| l.0),
                deleted: lines.map(|l| l.1),
                ..c.clone()
            }
        })
        .collect()
}

/// The oldest mtime of a tracked path changed; the index's when none
/// can be read.
fn dirty(root: &Path, gitdir: &Path, changes: &[Change]) -> Option<f64> {
    let tracked: Vec<&Change> = changes
        .iter()
        .filter(|c| c.kind != Kind::Untracked)
        .collect();
    let oldest = tracked
        .iter()
        .filter_map(|c| mtime(&root.join(&c.path)))
        .reduce(f64::min);
    match oldest {
        None if !tracked.is_empty() => mtime(&gitdir.join("index")),
        at => at,
    }
}
