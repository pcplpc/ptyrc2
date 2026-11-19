// SPDX-License-Identifier: GPL-3.0-or-later
//! Store apps: manifest, origin, fetch, tree check, install, registry.
//! See `plan/store-model.md`. No engine, no build; the network only
//! through `git`.

pub mod error;
pub mod git;
pub mod install;
pub mod manifest;
pub mod origin;
pub mod registry;
pub mod tree;

pub use error::{Error, ErrorKind, Result};
pub use git::{GitTree, Repo};
pub use install::{Dirs, Installed, Lock, StoreLock};
pub use manifest::{App, AppOption, Entry, Limit, Manifest, OptionValue, Permission, Surface};
pub use origin::{Origin, Reference, Remote};
pub use registry::{Pick, Pointer, Registries, Registry, Tier, Version, Want, choose};
pub use tree::{Checked, DirTree, Kind, Node, Tree, check, check_all};
