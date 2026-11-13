// SPDX-License-Identifier: GPL-3.0-or-later
//! The driver: owns the pty of the wrapped app, sits between it and the
//! host terminal, serves the protocol. See `plan/design.md`.
//!
//! The driver is transparent plumbing. What is drawn and what the protocol
//! methods answer is the `Engine`'s business.

mod conn;
mod demo;
pub mod driver;
pub mod engine;
mod grab;
pub use grab::{key_len, name as key_name};
mod hints;
pub mod host;
pub mod hostcolors;
pub mod keys;
pub mod mouse;
mod paging;
pub mod player;
pub mod pty;
pub mod record;
mod rpc;
pub mod screen;
#[cfg(test)]
mod screen_tests;
pub mod start;

pub use driver::{Config, Exit, replay, run};
pub use engine::{ConnId, Ctx, Engine, PassThrough, Reply, SessionInfo, Token};
pub use player::Replay;
pub use record::Tape;
pub use screen::ScreenEngine;
pub use start::{RunError, RunErrorKind};
