// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev test --cases`: each `tests/CASE.args` of the tree, the
//! words after `ptyrc dev test .` (run in the tree), its report
//! against `tests/CASE.txt`. `plugins/store-apps/README.md`, A case.

use crate::Fail;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

/// The words of a `.args` file: split on white space; `#` to the end
/// of a line is a comment. No quoting.
pub fn words(text: &str) -> Vec<String> {
    (text.lines())
        .map(|l| l.split('#').next().unwrap_or(""))
        .flat_map(str::split_whitespace)
        .map(str::to_string)
        .collect()
}

/// What a case came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Same,
    Blessed,
    /// The run failed: its status, its stderr.
    Failed(Option<i32>, String),
    /// No `CASE.txt`.
    Missing,
    /// The report differs: the diff.
    Differs(String),
}

impl Outcome {
    pub fn ok(&self) -> bool {
        matches!(self, Outcome::Same | Outcome::Blessed)
    }
}

/// The cases of `dir`: `tests/*.args`, sorted; `names` given, those
/// alone (`NAME`, `NAME.args` or `tests/NAME.args`).
pub fn cases(dir: &Path, names: &[String]) -> Result<Vec<PathBuf>, Fail> {
    let mut all: Vec<PathBuf> = (std::fs::read_dir(dir.join("tests")).into_iter())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "args"))
        .collect();
    all.sort();
    if all.is_empty() {
        return Err(Fail {
            status: 1,
            message: format!("{}: no case: tests/CASE.args", dir.display()),
            hint: Some("plugins/store-apps/README.md, A case".into()),
        });
    }
    let stem = |p: &Path| {
        p.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    if names.is_empty() {
        return Ok(all);
    }
    let mut chosen = Vec::new();
    for name in names {
        let name = name.strip_prefix("tests/").unwrap_or(name);
        let name = name.strip_suffix(".args").unwrap_or(name);
        match all.iter().find(|p| stem(p) == name) {
            Some(p) => chosen.push(p.clone()),
            None => {
                let known: Vec<String> = all.iter().map(|p| stem(p)).collect();
                return Err(Fail::usage(
                    format!("--cases {name}: no tests/{name}.args"),
                    format!("cases: {}", known.join(", ")),
                ));
            }
        }
    }
    Ok(chosen)
}

/// One case: `ptyrc dev test . WORDS` in `dir`, by `program`; its
/// report against `CASE.txt`, or written there (`bless`).
pub fn run(program: &Path, dir: &Path, args: &Path, bless: bool) -> Outcome {
    let text = match std::fs::read_to_string(args) {
        Ok(t) => t,
        Err(e) => return Outcome::Failed(None, format!("{}: {e}\n", args.display())),
    };
    let out = Command::new(program)
        .args(["dev", "test", "."])
        .args(words(&text))
        .current_dir(dir)
        .stdin(Stdio::null())
        .output();
    let out = match out {
        Ok(o) => o,
        Err(e) => return Outcome::Failed(None, format!("{}: {e}\n", program.display())),
    };
    if !out.status.success() {
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        return Outcome::Failed(out.status.code(), said);
    }
    let got = String::from_utf8_lossy(&out.stdout).into_owned();
    let expected = args.with_extension("txt");
    if bless {
        return match std::fs::write(&expected, &got) {
            Ok(()) => Outcome::Blessed,
            Err(e) => Outcome::Failed(None, format!("{}: {e}\n", expected.display())),
        };
    }
    match std::fs::read_to_string(&expected) {
        Ok(want) if want == got => Outcome::Same,
        Ok(want) => Outcome::Differs(diff(&want, &got)),
        Err(_) => Outcome::Missing,
    }
}

/// The lines of `want` and `got` that differ, each hunk with two
/// lines around it: ` ` kept, `-` expected, `+` got.
pub fn diff(want: &str, got: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    // The longest common subsequence, from the end.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = match a[i] == b[j] {
                true => lcs[i + 1][j + 1] + 1,
                false => lcs[i + 1][j].max(lcs[i][j + 1]),
            };
        }
    }
    let mut lines: Vec<(char, &str)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            lines.push((' ', a[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            lines.push(('-', a[i]));
            i += 1;
        } else {
            lines.push(('+', b[j]));
            j += 1;
        }
    }
    const AROUND: usize = 2;
    let near = |k: usize| {
        let lo = k.saturating_sub(AROUND);
        let hi = (k + AROUND + 1).min(lines.len());
        lines[lo..hi].iter().any(|l| l.0 != ' ')
    };
    let mut out = String::new();
    let mut gap = false;
    for (k, (mark, line)) in lines.iter().enumerate() {
        if near(k) {
            if gap {
                out.push_str("@@\n");
            }
            let _ = writeln!(out, "{mark}{line}");
            gap = false;
        } else {
            gap = true;
        }
    }
    out
}

/// `dev test --cases`: a Rust app built once, then each case, a
/// line; a diff under a failure; status 1 when one fails.
pub fn all(dir: &Path, names: &[String], bless: bool) -> Result<ExitCode, Fail> {
    let chosen = cases(dir, names)?;
    super::app::built(dir)?;
    let program = std::env::current_exe().map_err(|e| Fail::from(format!("this program: {e}")))?;
    let outcomes: Vec<Outcome> = std::thread::scope(|s| {
        let runs: Vec<_> = (chosen.iter())
            .map(|c| s.spawn(|| run(&program, dir, c, bless)))
            .collect();
        runs.into_iter().map(|r| r.join().unwrap()).collect()
    });
    let mut failed = false;
    for (case, outcome) in chosen.iter().zip(&outcomes) {
        let name = Path::new("tests").join(case.file_stem().unwrap_or_default());
        let name = name.display();
        failed |= !outcome.ok();
        match outcome {
            Outcome::Same => println!("ok   {name}"),
            Outcome::Blessed => println!("wrote {name}.txt"),
            Outcome::Missing => println!("FAIL {name}: no {name}.txt (--bless writes it)"),
            Outcome::Failed(status, said) => {
                let status = status.map_or("a signal".into(), |s| format!("status {s}"));
                println!("FAIL {name}: ptyrc dev test ended with {status}");
                print!("{said}");
            }
            Outcome::Differs(diff) => {
                println!("FAIL {name}: the report differs from {name}.txt (--bless if meant)");
                print!("{diff}");
            }
        }
    }
    Ok(match failed {
        true => ExitCode::from(1),
        false => ExitCode::SUCCESS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_of_a_case() {
        assert_eq!(
            words("--size 40x8 # the screen\n# a line\n--at 1s,2s\n"),
            ["--size", "40x8", "--at", "1s,2s"]
        );
    }

    #[test]
    fn a_diff_of_the_lines_changed() {
        let want = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let got = "a\nb\nc\nD\ne\nf\ng\nh\ni\nj\nk\n";
        assert_eq!(
            diff(want, got),
            "@@\n b\n c\n-d\n+D\n e\n f\n@@\n i\n j\n+k\n"
        );
        assert_eq!(diff("x\n", "x\n"), "");
    }
}
