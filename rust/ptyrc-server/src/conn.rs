// SPDX-License-Identifier: GPL-3.0-or-later
//! One client connection: non-blocking, line framed.

use ptyrc_proto::envelope::response;
use ptyrc_proto::{ErrorCode, LineDecoder, MAX_LINE, RpcError, encode};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

/// A client that does not read is dropped once this much is queued.
const MAX_QUEUED: usize = 64 << 20;

pub(crate) struct Conn {
    pub stream: UnixStream,
    decoder: LineDecoder,
    out: Vec<u8>,
    pub topics: HashSet<String>,
    /// To be closed: end of stream, error, or protocol violation.
    pub dead: bool,
    /// Of a paused app: it does not read, events are not queued for it.
    pub paused: bool,
    /// The pid of the peer, while it is no app of the table yet.
    pub peer: Option<u32>,
    /// The app whose token it presented (`auth`). None: the user's.
    pub app: Option<u64>,
    /// A request was read: `auth` comes first or not at all.
    pub asked: bool,
}

impl Conn {
    pub fn new(stream: UnixStream) -> io::Result<Conn> {
        stream.set_nonblocking(true)?;
        Ok(Conn {
            stream,
            decoder: LineDecoder::new(),
            out: Vec::new(),
            topics: HashSet::new(),
            dead: false,
            paused: false,
            peer: None,
            app: None,
            asked: false,
        })
    }

    pub fn wants_write(&self) -> bool {
        !self.out.is_empty() && !self.dead
    }

    /// Queue a message and send what the socket takes now.
    pub fn send(&mut self, value: &Value) {
        if self.dead {
            return;
        }
        self.out.extend_from_slice(&encode(value));
        self.flush();
    }

    pub fn flush(&mut self) {
        let mut sent = 0;
        while sent < self.out.len() && !self.dead {
            match self.stream.write(&self.out[sent..]) {
                Ok(0) => self.dead = true,
                Ok(n) => sent += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => self.dead = true,
            }
        }
        self.out.drain(..sent);
        if self.out.len() > MAX_QUEUED {
            self.dead = true;
        }
    }

    /// Read what is there; the complete lines received.
    pub fn read(&mut self) -> Vec<Vec<u8>> {
        let mut buf = [0u8; 65536];
        let mut lines = Vec::new();
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    self.dead = true;
                    break;
                }
                Ok(n) => {
                    self.decoder.push(&buf[..n]);
                    loop {
                        match self.decoder.next_line() {
                            Ok(Some(line)) => lines.push(line),
                            Ok(None) => break,
                            Err(e) => {
                                // The stream is lost: say why, then close.
                                let error = RpcError::new(ErrorCode::BadRequest, e.to_string())
                                    .with_hint(format!(
                                        "a request is one line of at most {} MiB",
                                        MAX_LINE >> 20
                                    ));
                                self.send(&response(&Value::Null, Err(error)));
                                self.dead = true;
                                return lines;
                            }
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => {
                    self.dead = true;
                    break;
                }
            }
        }
        lines
    }

    /// Last effort before closing: block briefly to drain the queue.
    pub fn drain_blocking(&mut self, timeout: std::time::Duration) {
        if self.dead || self.out.is_empty() {
            return;
        }
        let _ = self.stream.set_nonblocking(false);
        let _ = self.stream.set_write_timeout(Some(timeout));
        let _ = self.stream.write_all(&self.out);
        self.out.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_too_long_is_answered_then_closed() {
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        let mut conn = Conn::new(ours).unwrap();
        let writer = std::thread::spawn(move || {
            // The write may fail once the server closes: the reply stays.
            let _ = theirs.write_all(&vec![b'y'; MAX_LINE + 2]);
            theirs
        });
        while !conn.dead {
            assert!(conn.read().is_empty());
        }
        drop(conn);
        let mut theirs = writer.join().unwrap();
        let mut reply = String::new();
        let _ = theirs.read_to_string(&mut reply);
        let reply: Value = serde_json::from_str(reply.trim_end()).unwrap();
        assert_eq!(reply["id"], Value::Null);
        assert_eq!(reply["error"]["code"], "bad-request");
        assert_eq!(reply["error"]["message"], "line too long");
    }
}
