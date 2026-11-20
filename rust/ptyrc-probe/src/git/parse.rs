// SPDX-License-Identifier: GPL-3.0-or-later
//! Parsers of the output of the `git` commands of `run::Cmd`. No I/O,
//! no clock. A field that does not parse gives its zero, never an error.

/// Field separator of the `--format` strings.
pub const SEP: char = '\x1f';
/// Days of history in `days`.
pub const DAYS: usize = 112;

/// What `HEAD` is. `branch`: none when detached; `gone`: an upstream is
/// set and is no more; `initial`: no commit yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Head {
    pub oid: String,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub gone: bool,
    pub initial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Staged,
    Unstaged,
    Untracked,
    Conflict,
}

impl Kind {
    pub const ALL: [Kind; 4] = [
        Kind::Staged,
        Kind::Unstaged,
        Kind::Untracked,
        Kind::Conflict,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Staged => "staged",
            Kind::Unstaged => "unstaged",
            Kind::Untracked => "untracked",
            Kind::Conflict => "conflict",
        }
    }
}

/// A path of the status. `code`: the letter of git (`M`, `A`, `D`,
/// `R`..., `?`, `U`); `added`, `deleted`: lines, when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: Kind,
    pub code: char,
    pub path: String,
    pub added: Option<u64>,
    pub deleted: Option<u64>,
}

/// A commit of the log. `graph`: its prefix in `git log --graph`, one
/// string a row: the row of the commit, then those under it that hold
/// no commit.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub oid: String,
    pub at: f64,
    pub author: String,
    pub subject: String,
    pub refs: Vec<String>,
    pub graph: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stash {
    pub n: u64,
    pub at: f64,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub at: f64,
    pub upstream: Option<String>,
    pub ahead: u64,
    pub behind: u64,
    pub gone: bool,
    pub subject: String,
}

/// `path` as git gives it (absolute); `flags`: of `bare`, `detached`,
/// `locked`, `prunable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub oid: String,
    pub branch: Option<String>,
    pub flags: Vec<&'static str>,
}

fn int(s: &str) -> u64 {
    s.parse().unwrap_or(0)
}

fn float(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

/// The `n` fields of `s` split on `sep`, the last taking the rest;
/// missing ones empty.
fn fields(s: &str, sep: char, n: usize) -> Vec<&str> {
    let mut out: Vec<&str> = s.splitn(n, sep).collect();
    out.resize(n, "");
    out
}

/// `git status --porcelain=v2 --branch -z`.
pub fn parse_status(text: &str) -> (Head, Vec<Change>) {
    let mut head = Head::default();
    let mut changes = Vec::new();
    let mut tracked = false;
    let mut entries = text.split('\0');
    while let Some(entry) = entries.next() {
        let (kind, rest) = entry.split_once(' ').unwrap_or((entry, ""));
        match kind {
            "#" => {
                let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
                match key {
                    "branch.oid" => {
                        head.initial = value == "(initial)";
                        head.oid = if head.initial {
                            String::new()
                        } else {
                            value.chars().take(7).collect()
                        };
                    }
                    "branch.head" => {
                        head.branch = (value != "(detached)").then(|| value.to_string());
                    }
                    "branch.upstream" => {
                        head.upstream = Some(value.to_string());
                        head.gone = true;
                    }
                    "branch.ab" => {
                        let (ahead, behind) = value.split_once(' ').unwrap_or((value, ""));
                        let n = |s: &str| s.parse::<i64>().unwrap_or(0).unsigned_abs();
                        head.ahead = n(ahead);
                        head.behind = n(behind);
                        tracked = true;
                    }
                    _ => {}
                }
            }
            "1" | "2" => {
                let parts: Vec<&str> = rest.splitn(if kind == "2" { 9 } else { 8 }, ' ').collect();
                if kind == "2" {
                    entries.next(); // the path it was renamed from
                }
                let mut xy = parts[0].chars();
                let path = parts.last().copied().unwrap_or("");
                let (x, y) = (xy.next().unwrap_or('.'), xy.next().unwrap_or('.'));
                for (kind, code) in [(Kind::Staged, x), (Kind::Unstaged, y)] {
                    if code != '.' {
                        changes.push(Change {
                            kind,
                            code,
                            path: path.to_string(),
                            added: None,
                            deleted: None,
                        });
                    }
                }
            }
            "u" => changes.push(Change {
                kind: Kind::Conflict,
                code: 'U',
                path: rest.splitn(10, ' ').last().unwrap_or("").to_string(),
                added: None,
                deleted: None,
            }),
            "?" => changes.push(Change {
                kind: Kind::Untracked,
                code: '?',
                path: rest.to_string(),
                added: None,
                deleted: None,
            }),
            _ => {}
        }
    }
    if tracked {
        head.gone = false;
    }
    (head, changes)
}

/// `git diff-files --numstat -z` (or `diff-index`): lines added and
/// deleted, by path; a binary file counts none.
pub fn parse_numstat(text: &str) -> std::collections::HashMap<String, (u64, u64)> {
    let mut out = std::collections::HashMap::new();
    for record in text.split('\0') {
        let f = fields(record.trim_start_matches('\n'), '\t', 3);
        if !f[2].is_empty() {
            out.insert(f[2].to_string(), (int(f[0]), int(f[1])));
        }
    }
    out
}

/// `--format` of `parse_log`, after `--graph`.
pub fn log_format() -> String {
    ["", "%h", "%ct", "%an", "%D", "%s"].join(&SEP.to_string())
}

/// `git log --graph --format=LOG`.
pub fn parse_log(text: &str) -> Vec<Commit> {
    let mut out: Vec<Commit> = Vec::new();
    for line in text.lines() {
        let Some((graph, rest)) = line.split_once(SEP) else {
            let graph = line.trim_end();
            if let Some(last) = out.last_mut()
                && !graph.is_empty()
            {
                last.graph.push(graph.to_string());
            }
            continue;
        };
        let graph = graph.trim_end();
        let f = fields(rest, SEP, 5);
        out.push(Commit {
            oid: f[0].to_string(),
            at: float(f[1]),
            author: f[2].to_string(),
            refs: f[3]
                .split(',')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(String::from)
                .collect(),
            subject: f[4].to_string(),
            graph: vec![if graph.is_empty() { "*" } else { graph }.to_string()],
        });
    }
    out
}

/// `--format` of `parse_stashes`.
pub fn stash_format() -> String {
    ["%gd", "%ct", "%gs"].join(&SEP.to_string())
}

/// `git stash list --format=STASH`.
pub fn parse_stashes(text: &str) -> Vec<Stash> {
    let mut out = Vec::new();
    for line in text.lines() {
        let f = fields(line, SEP, 3);
        let n = f[0]
            .split_once('{')
            .map_or("", |(_, n)| n.trim_end_matches('}'));
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            out.push(Stash {
                n: int(n),
                at: float(f[1]),
                subject: f[2].to_string(),
            });
        }
    }
    out
}

/// `--format` of `parse_branches`.
pub fn branch_format() -> String {
    [
        "%(refname:short)",
        "%(HEAD)",
        "%(committerdate:unix)",
        "%(upstream:short)",
        "%(upstream:track)",
        "%(contents:subject)",
    ]
    .join(&SEP.to_string())
}

/// `git for-each-ref --format=BRANCHES refs/heads`.
pub fn parse_branches(text: &str) -> Vec<Branch> {
    let mut out = Vec::new();
    for line in text.lines() {
        let f = fields(line, SEP, 6);
        let (mut ahead, mut behind) = (0, 0);
        let track = f[4];
        for part in track.trim_matches(['[', ']']).split(',') {
            let (word, n) = part.trim().split_once(' ').unwrap_or((part.trim(), ""));
            if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
                match word {
                    "ahead" => ahead = int(n),
                    "behind" => behind = int(n),
                    _ => {}
                }
            }
        }
        out.push(Branch {
            name: f[0].to_string(),
            current: f[1] == "*",
            at: float(f[2]),
            upstream: (!f[3].is_empty()).then(|| f[3].to_string()),
            ahead,
            behind,
            gone: track.contains("gone"),
            subject: f[5].to_string(),
        });
    }
    out
}

/// `git worktree list --porcelain -z`: attributes separated by NUL, an
/// empty one between work trees.
pub fn parse_worktrees(text: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    for block in text.split("\0\0") {
        let mut seen = std::collections::HashMap::new();
        for line in block.split('\0') {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            seen.insert(key, value);
        }
        let Some(path) = seen.get("worktree") else {
            continue;
        };
        let branch = seen.get("branch").copied().unwrap_or("");
        let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
        out.push(Worktree {
            path: path.to_string(),
            oid: seen.get("HEAD").map_or("", |h| h).chars().take(7).collect(),
            branch: (!branch.is_empty()).then(|| branch.to_string()),
            flags: ["bare", "detached", "locked", "prunable"]
                .into_iter()
                .filter(|f| seen.contains_key(f))
                .collect(),
        });
    }
    out
}

/// Commit times, one a line, as commits a local day: `days` of them,
/// today last. `offset`: seconds east of UTC at `now`.
pub fn parse_days(text: &str, now: f64, offset: i64, days: usize) -> Vec<u64> {
    let mut out = vec![0; days];
    let local = now + offset as f64;
    let midnight = (local / 86400.0).floor() * 86400.0 - offset as f64;
    for word in text.split_whitespace() {
        let Ok(at) = word.parse::<f64>() else {
            continue;
        };
        let back = ((midnight + 86400.0 - at) / 86400.0).floor();
        if back >= 0.0 && (back as usize) < days {
            out[days - 1 - back as usize] += 1;
        }
    }
    out
}

/// The output of `rev-parse --show-toplevel --absolute-git-dir
/// --git-common-dir --show-object-format`: root, git directory, common
/// directory (relative to where git ran, or absolute), object format.
pub fn parse_locate(text: &str) -> Option<[&str; 4]> {
    let lines: Vec<&str> = text.lines().collect();
    match lines[..] {
        [a, b, c, d] if !a.is_empty() && !b.is_empty() => Some([a, b, c, d]),
        _ => None,
    }
}
