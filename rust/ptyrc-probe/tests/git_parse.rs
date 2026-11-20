// SPDX-License-Identifier: GPL-3.0-or-later
//! The git parsers on the fixtures of `test_git.py`.

use ptyrc_probe::git::parse::*;
use ptyrc_probe::git::run::{self, Cmd, Git};
use ptyrc_probe::git::{Ask, Part};
use serde_json::json;

const NOW: f64 = 1_800_000_000.0;

fn status() -> String {
    [
        "# branch.oid 1234567890abcdef1234567890abcdef12345678",
        "# branch.head main",
        "# branch.upstream origin/main",
        "# branch.ab +2 -1",
        "1 M. N... 100644 100644 100644 aaa bbb src/staged.py",
        "1 .M N... 100644 100644 100644 aaa bbb src/changed file.py",
        "1 MM N... 100644 100644 100644 aaa bbb both.py",
        "2 R. N... 100644 100644 100644 aaa bbb R100 new name.py",
        "old name.py",
        "u UU N... 100644 100644 100644 100644 aaa bbb ccc clash.py",
        "? notes/",
        "",
    ]
    .join("\0")
}

fn log() -> String {
    let s = SEP;
    format!(
        "* {s}aaaaaaa{s}{}{s}Ada{s}HEAD -> main, tag: v1.0, origin/main{s}Merge side\n\
         |\\  \n\
         | * {s}bbbbbbb{s}{}{s}Bob{s}side{s}a side commit with a subject long enough to wrap twice\n\
         |/  \n\
         * {s}ccccccc{s}{}{s}Ada{s}{s}base\n",
        NOW - 120.0,
        NOW - 3600.0,
        NOW - 90000.0
    )
}

fn branches() -> String {
    let s = SEP;
    format!(
        "main{s}*{s}{}{s}origin/main{s}[ahead 2, behind 1]{s}Merge side\n\
         side{s} {s}{}{s}{s}{s}a side commit\n\
         old{s} {s}{}{s}origin/old{s}[gone]{s}forgotten\n",
        NOW - 120.0,
        NOW - 3600.0,
        NOW - 900000.0
    )
}

const WORKTREES: &str = "worktree /src/app\0HEAD 1234567890abcdef\0branch refs/heads/main\0\0\
worktree /src/app-fix\0HEAD abcdef1234567890\0detached\0locked\0\0\
worktree /src/gone\0HEAD abcdef1234567890\0branch refs/heads/side\0\
prunable gitdir file points to non-existent location\0\0";

#[test]
fn parse_status_fixture() {
    let (head, changes) = parse_status(&status());
    assert_eq!(
        head,
        Head {
            oid: "1234567".into(),
            branch: Some("main".into()),
            upstream: Some("origin/main".into()),
            ahead: 2,
            behind: 1,
            gone: false,
            initial: false,
        }
    );
    let got: Vec<_> = changes
        .iter()
        .map(|c| (c.kind.as_str(), c.code, c.path.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("staged", 'M', "src/staged.py"),
            ("unstaged", 'M', "src/changed file.py"),
            ("staged", 'M', "both.py"),
            ("unstaged", 'M', "both.py"),
            ("staged", 'R', "new name.py"),
            ("conflict", 'U', "clash.py"),
            ("untracked", '?', "notes/"),
        ]
    );
    let (head, changes) = parse_status("# branch.oid (initial)\0# branch.head (detached)\0");
    assert_eq!(
        head,
        Head {
            initial: true,
            ..Head::default()
        }
    );
    assert!(changes.is_empty());
    let (gone, _) = parse_status("# branch.oid abc\0# branch.upstream o/x\0");
    assert!(gone.gone && gone.upstream.as_deref() == Some("o/x"));
    assert_eq!(parse_status(""), (Head::default(), vec![]));
}

#[test]
fn parse_numstat_fixture() {
    let got = parse_numstat("3\t1\ta.py\0-\t-\timg.png\0\n12\t0\tb c.py\0");
    assert_eq!(got.len(), 3);
    assert_eq!(got["a.py"], (3, 1));
    assert_eq!(got["img.png"], (0, 0));
    assert_eq!(got["b c.py"], (12, 0));
    assert_eq!(parse_numstat("1\t2\ta\nb\0")["a\nb"], (1, 2));
}

#[test]
fn parse_log_fixture() {
    let commits = parse_log(&log());
    let [a, b, c] = &commits[..] else {
        panic!("{commits:?}")
    };
    assert_eq!(
        (a.oid.as_str(), a.at, a.author.as_str(), a.subject.as_str()),
        ("aaaaaaa", NOW - 120.0, "Ada", "Merge side")
    );
    assert_eq!(a.refs, ["HEAD -> main", "tag: v1.0", "origin/main"]);
    assert_eq!(a.graph, ["*", "|\\"]);
    assert_eq!(b.graph, ["| *", "|/"]);
    assert_eq!(c.graph, ["*"]);
    assert_eq!(b.refs, ["side"]);
    assert!(c.refs.is_empty());
    assert!(parse_log("").is_empty());
}

#[test]
fn parse_lists_fixture() {
    let s = SEP;
    let stashes = parse_stashes(&format!("stash@{{0}}{s}100{s}WIP on main: abc x\nnoise\n"));
    assert_eq!(
        stashes,
        [Stash {
            n: 0,
            at: 100.0,
            subject: "WIP on main: abc x".into()
        }]
    );
    let list = parse_branches(&branches());
    let [main, side, old] = &list[..] else {
        panic!()
    };
    assert_eq!(
        (
            main.current,
            main.upstream.as_deref(),
            main.ahead,
            main.behind
        ),
        (true, Some("origin/main"), 2, 1)
    );
    assert!(!side.current && side.upstream.is_none());
    assert!(old.gone && old.subject == "forgotten");
    let trees = parse_worktrees(WORKTREES);
    let [one, two, three] = &trees[..] else {
        panic!("{trees:?}")
    };
    assert_eq!(
        (
            one.path.as_str(),
            one.oid.as_str(),
            one.branch.as_deref(),
            one.flags.len()
        ),
        ("/src/app", "1234567", Some("main"), 0)
    );
    assert!(two.branch.is_none() && two.flags == ["detached", "locked"]);
    assert_eq!(three.flags, ["prunable"]);
    assert!(parse_worktrees("").is_empty());
}

#[test]
fn parse_days_fixture() {
    // 2026-11-13 12:00 at UTC+1.
    let offset = 3600;
    let noon = 1_794_528_000.0 - offset as f64 + 12.0 * 3600.0;
    let times = format!(
        "{}\n{}\n{}\n{}\nx\n",
        noon - 3600.0,
        noon - 7200.0,
        noon - 86400.0,
        noon - 86400.0 * 9.0
    );
    assert_eq!(parse_days(&times, noon, offset, 7), [0, 0, 0, 0, 0, 1, 2]);
    // Just past local midnight: yesterday's commit is one day back.
    let midnight = 1_794_528_000.0 - offset as f64;
    assert_eq!(
        parse_days(&format!("{}", midnight - 1.0), midnight + 1.0, offset, 2),
        [1, 0]
    );
}

#[test]
fn parse_locate_fixture() {
    assert_eq!(
        parse_locate("/r\n/r/.git\n.git\nsha1\n"),
        Some(["/r", "/r/.git", ".git", "sha1"])
    );
    assert_eq!(parse_locate("/r\n/r/.git\n"), None);
    assert_eq!(parse_locate(""), None);
}

#[test]
fn ask_from_json() {
    assert_eq!(Ask::from_json(&json!(null)), Ok(Ask::default()));
    let ask = Ask::from_json(
        &json!({"repo": "d0", "parts": ["head", "log", "head"], "log": 10000, "since": "x"}),
    )
    .unwrap();
    assert_eq!(ask.parts, [Part::Head, Part::Log]);
    assert_eq!((ask.log, ask.since.as_deref()), (200, Some("x")));
    assert_eq!(Ask::from_json(&json!({"log": 0})).unwrap().log, 1);
    assert!(
        Ask::from_json(&json!({"parts": ["head", "ci"]}))
            .unwrap_err()
            .contains("parts: of head,")
    );
    assert!(Ask::from_json(&json!({"log": "x"})).is_err());
    assert!(Ask::from_json(&json!([1])).is_err());
}

#[test]
fn commands_fixed() {
    let all = [
        Cmd::Locate,
        Cmd::Status,
        Cmd::DiffFiles,
        Cmd::DiffIndex,
        Cmd::Log(30),
        Cmd::Days(0),
        Cmd::Stashes,
        Cmd::Branches,
        Cmd::Worktrees,
    ];
    let mut git = Git::new("git", std::path::Path::new("/r"));
    assert_eq!(git.argv(Cmd::Status), None, "no attribute source yet");
    assert!(git.argv(Cmd::Locate).is_some());
    git.arm("sha1", Some(vec!["evil".into()]));
    let mut verbs = Vec::new();
    for cmd in all {
        let argv = git.argv(cmd).unwrap();
        let at = argv
            .iter()
            .rposition(|a| a == "submodule.recurse=false")
            .unwrap();
        let rest = &argv[at + 1..];
        let rest = if rest[0] == "-c" {
            &rest[rest.iter().rposition(|a| a.starts_with("filter.")).unwrap() + 1..]
        } else {
            rest
        };
        verbs.push(rest[0].clone());
        if matches!(cmd, Cmd::Stashes | Cmd::Worktrees) {
            assert_eq!(rest[1], "list");
        }
        for want in [
            "core.fsmonitor=false",
            "core.hooksPath=/dev/null",
            "core.pager=cat",
            "--no-pager",
        ] {
            assert!(argv.iter().any(|a| a == want), "{cmd:?} {want}");
        }
        let reads = matches!(cmd, Cmd::Status | Cmd::DiffFiles | Cmd::DiffIndex);
        assert_eq!(
            argv.iter().any(|a| a == "filter.evil.clean="),
            reads,
            "{cmd:?}"
        );
        if matches!(cmd, Cmd::DiffFiles | Cmd::DiffIndex) {
            assert!(argv.iter().any(|a| a == "--no-ext-diff"));
            assert!(argv.iter().any(|a| a == "--no-textconv"));
        }
    }
    verbs.dedup();
    assert_eq!(verbs, run::COMMANDS);
    git.arm("sha1", None);
    assert_eq!(
        git.argv(Cmd::Status),
        None,
        "filters unknown: no read of the tree"
    );
    assert!(git.argv(Cmd::Log(1)).is_some());
    git.arm("sha512", Some(vec![]));
    assert_eq!(
        git.argv(Cmd::Log(1)),
        None,
        "an unknown format: no attribute source"
    );
}

#[test]
fn attribute_filters() {
    assert_eq!(
        run::filters("* filter=a\n[attr]m filter=b -diff\n*.x filter=a\n"),
        Some(vec!["a".into(), "b".into()])
    );
    let many: String = (0..=run::FILTERS)
        .map(|i| format!("f{i} filter=n{i}\n"))
        .collect();
    assert_eq!(run::filters(&many), None);
}
