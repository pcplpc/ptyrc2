// SPDX-License-Identifier: GPL-3.0-or-later
//! The tokens of pushed names: `<config>/ptyrc/tokens.toml`, 0600, a
//! table per registry host, `["host"] name = "token"`; a git app's name
//! is its origin id. Written by a rename.

use std::collections::BTreeMap;

use crate::error::{Error, ErrorKind, Result};
use crate::http::is_token;
use crate::install::{Dirs, write_private};

type Tokens = BTreeMap<String, BTreeMap<String, String>>;

fn load(dirs: &Dirs) -> Result<Tokens> {
    let p = dirs.tokens();
    let src = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Tokens::new()),
        Err(e) => return Err(Error::io(p.display(), e)),
    };
    toml::from_str(&src).map_err(|e| {
        Error::new(ErrorKind::Io, format!("{}: {}", p.display(), e.message()))
            .with_hint("[\"host\"] name = \"token\"")
    })
}

/// The token of `name` on `host`.
pub fn get(dirs: &Dirs, host: &str, name: &str) -> Result<Option<String>> {
    Ok(load(dirs)?.get(host).and_then(|t| t.get(name)).cloned())
}

/// Records the token of `name` on `host`.
pub fn set(dirs: &Dirs, host: &str, name: &str, token: &str) -> Result<()> {
    if !is_token(token) {
        return Err(Error::new(ErrorKind::Io, "a token of odd characters"));
    }
    let mut all = load(dirs)?;
    all.entry(host.to_string())
        .or_default()
        .insert(name.to_string(), token.to_string());
    let text = toml::to_string(&all)
        .map_err(|e| Error::new(ErrorKind::Io, format!("tokens.toml: {e}")))?;
    write_private(&dirs.tokens(), text.as_bytes())
}
