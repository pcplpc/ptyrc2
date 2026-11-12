// SPDX-License-Identifier: GPL-3.0-or-later
//! Blocking client.

use crate::envelope::{Event, Incoming, RpcError};
use crate::framing::{FrameError, LineDecoder, encode};
use crate::session::{self, SelectError};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ClientError {
    Select(SelectError),
    Io(io::Error),
    /// The server sent something that is not the protocol.
    Protocol(String),
    /// The server answered with an error.
    Rpc(RpcError),
    /// The connection was closed.
    Closed,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientError::Select(e) => e.fmt(f),
            ClientError::Io(e) => write!(f, "connection: {e}"),
            ClientError::Protocol(e) => write!(f, "protocol: {e}"),
            ClientError::Rpc(e) => e.fmt(f),
            ClientError::Closed => f.write_str("connection closed by the driver"),
        }
    }
}

impl std::error::Error for ClientError {}

impl ClientError {
    /// What to try, when there is something to say.
    pub fn hint(&self) -> Option<String> {
        match self {
            ClientError::Select(e) => Some(e.hint()),
            ClientError::Rpc(e) => e.hint.clone(),
            ClientError::Closed => Some("the session ended; list sessions: ptyrc ls".into()),
            ClientError::Io(_) | ClientError::Protocol(_) => None,
        }
    }

    /// Exit status of a CLI verb that failed with this error: 3 no
    /// session, 2 or 4 a request refused, 5 the connection.
    pub fn exit_status(&self) -> u8 {
        match self {
            ClientError::Select(_) => 3,
            ClientError::Rpc(e) => e.exit_status(),
            ClientError::Io(_) | ClientError::Protocol(_) | ClientError::Closed => 5,
        }
    }
}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> ClientError {
        ClientError::Io(e)
    }
}

impl From<FrameError> for ClientError {
    fn from(e: FrameError) -> ClientError {
        ClientError::Protocol(e.to_string())
    }
}

/// One connection to a driver. Events received while waiting for a
/// response are queued.
pub struct Client {
    stream: UnixStream,
    decoder: LineDecoder,
    events: VecDeque<Event>,
    hello: Value,
    next_id: u64,
}

impl Client {
    /// Connect to a socket and read the `hello`.
    pub fn connect(path: &Path) -> Result<Client, ClientError> {
        let stream = UnixStream::connect(path)?;
        let mut client = Client {
            stream,
            decoder: LineDecoder::new(),
            events: VecDeque::new(),
            hello: Value::Null,
            next_id: 1,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        match client.read_message(Some(deadline))? {
            Some(Incoming::Event(e)) if e.event == "hello" => client.hello = e.data,
            Some(_) => return Err(ClientError::Protocol("no hello".into())),
            None => return Err(ClientError::Protocol("hello timed out".into())),
        }
        Ok(client)
    }

    /// Connect to a session chosen by `session::select`.
    pub fn connect_session(explicit: Option<&str>) -> Result<Client, ClientError> {
        let session = session::select(explicit).map_err(ClientError::Select)?;
        Client::connect(&session.path)
    }

    /// The `hello` data.
    pub fn hello(&self) -> &Value {
        &self.hello
    }

    /// One request, its result.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = self.next_id;
        self.next_id += 1;
        let line = encode(&json!({"id": id, "method": method, "params": params}));
        self.stream.write_all(&line)?;
        loop {
            match self.read_message(None)? {
                Some(Incoming::Response { id: got, result }) if got == json!(id) => {
                    return result.map_err(ClientError::Rpc);
                }
                Some(Incoming::Response { .. }) => {}
                Some(Incoming::Event(e)) => self.events.push_back(e),
                None => unreachable!("no deadline"),
            }
        }
    }

    pub fn subscribe(&mut self, topics: &[&str]) -> Result<(), ClientError> {
        self.call("subscribe", json!({"topics": topics}))
            .map(|_| ())
    }

    /// Next event; `None` when `timeout` elapsed. No timeout: blocks.
    pub fn next_event(&mut self, timeout: Option<Duration>) -> Result<Option<Event>, ClientError> {
        if let Some(e) = self.events.pop_front() {
            return Ok(Some(e));
        }
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            match self.read_message(deadline)? {
                Some(Incoming::Event(e)) => return Ok(Some(e)),
                Some(Incoming::Response { .. }) => {}
                None => return Ok(None),
            }
        }
    }

    /// Next message; `None` at the deadline.
    fn read_message(&mut self, deadline: Option<Instant>) -> Result<Option<Incoming>, ClientError> {
        let mut buf = [0u8; 65536];
        loop {
            if let Some(line) = self.decoder.next_line()? {
                let value: Value = serde_json::from_slice(&line)
                    .map_err(|e| ClientError::Protocol(e.to_string()))?;
                return Incoming::from_value(value)
                    .map(Some)
                    .ok_or_else(|| ClientError::Protocol("neither response nor event".into()));
            }
            let timeout = match deadline {
                Some(d) => match d.checked_duration_since(Instant::now()) {
                    Some(left) if !left.is_zero() => Some(left),
                    _ => return Ok(None),
                },
                None => None,
            };
            self.stream.set_read_timeout(timeout)?;
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(ClientError::Closed),
                Ok(n) => self.decoder.push(&buf[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;

    fn temp_socket(name: &str) -> std::path::PathBuf {
        let dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../temp"))
            .join(format!("proto-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn call_and_events() {
        let path = temp_socket("a.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(&encode(&envelope::event("hello", json!({"protocol": 1}))))
                .unwrap();
            let mut reader = BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let req: crate::Request = serde_json::from_str(&line).unwrap();
            assert_eq!(req.method, "ping");
            // An event before the response, the response split in two writes.
            s.write_all(&encode(&envelope::event("screen", json!(1))))
                .unwrap();
            let resp = encode(&envelope::response(&req.id, Ok(json!({"pong": true}))));
            s.write_all(&resp[..5]).unwrap();
            s.flush().unwrap();
            std::thread::sleep(Duration::from_millis(20));
            s.write_all(&resp[5..]).unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let req: crate::Request = serde_json::from_str(&line).unwrap();
            let err = RpcError::unknown_method(&req.method);
            s.write_all(&encode(&envelope::response(&req.id, Err(err))))
                .unwrap();
        });
        let mut c = Client::connect(&path).unwrap();
        assert_eq!(c.hello()["protocol"], 1);
        assert_eq!(c.call("ping", json!({})).unwrap(), json!({"pong": true}));
        let e = c
            .next_event(Some(Duration::from_millis(10)))
            .unwrap()
            .unwrap();
        assert_eq!(e.event, "screen");
        match c.call("nope", Value::Null) {
            Err(ClientError::Rpc(e)) => assert_eq!(e.code, crate::ErrorCode::UnknownMethod),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            c.next_event(Some(Duration::from_millis(500))),
            Err(ClientError::Closed)
        ));
        server.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn event_timeout() {
        let path = temp_socket("b.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.write_all(&encode(&envelope::event("hello", json!({}))))
                .unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        let mut c = Client::connect(&path).unwrap();
        assert!(
            c.next_event(Some(Duration::from_millis(30)))
                .unwrap()
                .is_none()
        );
        server.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
