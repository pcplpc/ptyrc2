// SPDX-License-Identifier: GPL-3.0-or-later
//! `ptyrc dev vet DIR [--out FILE] [--force]`: what a registry server's
//! builder runs on each archive pushed (`registry/API.md`, Vetting). The
//! status is a contract: 1 evicts the archive, so 1 is the app's fault
//! alone; what is not (no toolchain, no profile, an I/O error) is 3.

use super::push::{build_temp, report, write_out};
use super::{draws, is_dir};
use crate::Fail;
use ptyrc_store::{DirTree, Entry, Error, ErrorKind, build, check_all, tree};
use std::path::Path;
use std::process::ExitCode;

/// Not vetted here: the builder's fault.
const CANNOT: u8 = 3;

fn cannot(e: Error) -> Fail {
    Fail {
        status: CANNOT,
        message: format!("not vetted: {}", e.message),
        hint: e.hint,
    }
}

/// An error of the app, or of the builder (`Io`, `Toolchain`).
fn builder(e: &Error) -> bool {
    matches!(e.kind, ErrorKind::Io | ErrorKind::Toolchain)
}

pub fn vet(dir: &Path, out: Option<&Path>, force: bool) -> Result<ExitCode, Fail> {
    is_dir(dir).map_err(|f| Fail {
        status: CANNOT,
        ..f
    })?;
    let tree = DirTree::new(dir);
    let ok = match check_all(&tree) {
        Ok(ok) => ok,
        Err(v) => match v.into_iter().partition::<Vec<_>, _>(builder) {
            (io, _) if !io.is_empty() => return Err(cannot(io.into_iter().next().unwrap())),
            (_, bad) => return Ok(report(dir, &bad)),
        },
    };
    let rust = matches!(ok.manifest.app.entry, Entry::Rust(_));
    if rust && out.is_none() {
        return Err(Fail::usage(
            format!("{}: a Rust app: --out FILE for its module", dir.display()),
            "ptyrc dev vet DIR --out FILE",
        ));
    }
    let mut problems: Vec<Error> = (ptyrc_store::manifest::todos(&ok.manifest).iter())
        .map(|t| {
            Error::new(
                ErrorKind::Manifest,
                format!("{t}: starts with TODO; the install refuses it"),
            )
            .with_hint("replace each TODO of the template")
        })
        .collect();
    problems.extend(draws(&ok, dir).0);
    problems.extend(tree::text_only(&tree, &ok));
    if rust {
        problems.extend(build::sources(&tree, &ok));
    }
    let mut built = None;
    if rust && problems.is_empty() {
        let toolchain = build::Toolchain::find().map_err(cannot)?;
        match build_temp(dir, &ok, &toolchain, force) {
            Ok(b) => built = Some(b),
            Err(e) if builder(&e) => return Err(cannot(e)),
            Err(e) => problems.push(e),
        }
    }
    if !rust || built.is_some() {
        let read = built.as_ref().map(|b| b.sources.as_slice());
        for f in tree::unused(&tree, &ok, read).map_err(cannot)? {
            problems.push(
                Error::new(ErrorKind::Tree, format!("{f}: not used by the app"))
                    .with_hint("a file the app does not use; `ptyrc dev push` leaves it out"),
            );
        }
    }
    if let Some(e) = problems.iter().find(|e| builder(e)) {
        return Err(cannot(e.clone()));
    }
    if !problems.is_empty() {
        return Ok(report(dir, &problems));
    }
    let a = &ok.manifest.app;
    match (built, out) {
        (Some(b), Some(out)) => {
            write_out(out, &b.wasm).map_err(|e| match e.kind {
                ErrorKind::Usage => Fail::from(e),
                _ => cannot(e),
            })?;
            println!(
                "ok: {} {}: vetted; {}, {} bytes",
                a.id,
                a.version,
                out.display(),
                b.wasm.len()
            );
        }
        _ => println!("ok: {} {}: vetted", a.id, a.version),
    }
    Ok(ExitCode::SUCCESS)
}
