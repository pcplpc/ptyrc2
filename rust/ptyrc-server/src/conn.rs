// SPDX-License-Identifier: GPL-3.0-or-later
//! One client connection: non-blocking, line framed.

use ptyrc_proto::{LineDecoder, encode};
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
                            Err(_) => {
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
