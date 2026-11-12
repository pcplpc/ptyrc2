// SPDX-License-Identifier: GPL-3.0-or-later
//! Wire envelope, framing, session discovery, blocking client. See
//! `plan/design.md`, "Protocol".

pub mod client;
pub mod envelope;
pub mod framing;
pub mod session;

pub use client::{Client, ClientError};
pub use envelope::{ErrorCode, Event, Incoming, PROTOCOL, Request, RpcError, SERVER};
pub use framing::{FrameError, LineDecoder, MAX_LINE, encode};
pub use session::{SelectError, Session};
