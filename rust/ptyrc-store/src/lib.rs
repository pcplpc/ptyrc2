// SPDX-License-Identifier: GPL-3.0-or-later
//! Store apps: manifest, origin, fetch, tree check, install, registry,
//! its index, page and review.
//! See `plan/store-model.md`. No engine; the build of a Rust app by
//! `rustc` alone; the network only through `git` and, for a registry
//! server, `curl` (`http`).

pub mod build;
pub mod error;
pub mod git;
pub mod hosted;
pub mod http;
pub mod index;
pub mod install;
pub mod manifest;
pub mod options;
pub mod origin;
pub mod page;
pub mod push;
pub mod registry;
pub mod review;
pub mod shipped;
pub mod sync;
pub mod tar;
pub mod tokens;
pub mod tree;

pub use error::{Error, ErrorKind, Result};
pub use git::{GitTree, Repo};
pub use install::{Dirs, Installed, Lock, StoreLock};
pub use manifest::{App, AppOption, Entry, Limit, Manifest, OptionValue, Permission, Surface};
pub use options::{FileGiven, FileKind, Launch, Launcher, OptionType, Resolved};
pub use origin::{Origin, Reference, Remote};
pub use registry::{Pick, Pointer, Registries, Registry, Tier, Version, Want, choose};
pub use tar::{TarTree, sha256_hex};
pub use tree::{Checked, DirTree, Kind, Node, Tree, check, check_all};
