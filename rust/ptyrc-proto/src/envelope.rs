// SPDX-License-Identifier: GPL-3.0-or-later
//! Messages: request, response, event.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;

/// Protocol version.
pub const PROTOCOL: u32 = 1;

/// Server identification sent in `hello`.
pub const SERVER: &str = concat!("ptyrc/", env!("CARGO_PKG_VERSION"));

/// Topics of `subscribe`.
pub const TOPICS: [&str; 8] = [
    "screen", "cursor", "frames", "idle", "draw", "input", "size", "exit",
];

/// Client to server. `id` is echoed as given.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Error codes of the protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    BadRequest,
    UnknownMethod,
    BadParams,
    NoFrame,
    NoSprite,
    NotUsable,
    Timeout,
    Gone,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::BadRequest => "bad-request",
            ErrorCode::UnknownMethod => "unknown-method",
            ErrorCode::BadParams => "bad-params",
            ErrorCode::NoFrame => "no-frame",
            ErrorCode::NoSprite => "no-sprite",
            ErrorCode::NotUsable => "not-usable",
            ErrorCode::Timeout => "timeout",
            ErrorCode::Gone => "gone",
            ErrorCode::Internal => "internal",
        }
    }
}

/// The `error` member of a response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
    /// What to try: valid values, the nearest one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> RpcError {
        RpcError {
            code,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> RpcError {
        self.hint = Some(hint.into());
        self
    }

    pub fn bad_params(message: impl Into<String>) -> RpcError {
        RpcError::new(ErrorCode::BadParams, message)
    }

    pub fn unknown_method(method: &str) -> RpcError {
        RpcError::new(
            ErrorCode::UnknownMethod,
            format!("unknown method: {method}"),
        )
    }

    /// Exit status of a CLI verb refused with this error.
    pub fn exit_status(&self) -> u8 {
        match self.code {
            ErrorCode::BadRequest | ErrorCode::UnknownMethod | ErrorCode::BadParams => 2,
            ErrorCode::Gone | ErrorCode::Internal => 5,
            _ => 4,
        }
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for RpcError {}

/// Server to client, unsolicited.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub data: Value,
}

/// A response line.
pub fn response(id: &Value, result: Result<Value, RpcError>) -> Value {
    match result {
        Ok(result) => json!({"id": id, "result": result}),
        Err(error) => json!({"id": id, "error": error}),
    }
}

/// An event line.
pub fn event(topic: &str, data: Value) -> Value {
    json!({"event": topic, "data": data})
}

/// A line received by a client.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Response {
        id: Value,
        result: Result<Value, RpcError>,
    },
    Event(Event),
}

impl Incoming {
    /// Classify a decoded line. `None`: neither a response nor an event.
    pub fn from_value(mut value: Value) -> Option<Incoming> {
        let obj = value.as_object_mut()?;
        if obj.contains_key("event") {
            return serde_json::from_value(value).ok().map(Incoming::Event);
        }
        let id = obj.remove("id")?;
        if let Some(error) = obj.remove("error") {
            let error = serde_json::from_value(error).ok()?;
            return Some(Incoming::Response {
                id,
                result: Err(error),
            });
        }
        let result = obj.remove("result")?;
        Some(Incoming::Response {
            id,
            result: Ok(result),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_defaults() {
        let r: Request = serde_json::from_str(r#"{"method":"screen.text"}"#).unwrap();
        assert_eq!(r.id, Value::Null);
        assert_eq!(r.params, Value::Null);
        let r: Request =
            serde_json::from_str(r#"{"id":"a","method":"m","params":{"x":1}}"#).unwrap();
        assert_eq!(r.id, json!("a"));
        assert_eq!(r.params["x"], 1);
    }

    #[test]
    fn error_codes_are_kebab() {
        let e = RpcError::new(ErrorCode::UnknownMethod, "x");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"code":"unknown-method","message":"x"})
        );
        for code in [
            ErrorCode::BadRequest,
            ErrorCode::UnknownMethod,
            ErrorCode::BadParams,
            ErrorCode::NoFrame,
            ErrorCode::NoSprite,
            ErrorCode::NotUsable,
            ErrorCode::Timeout,
            ErrorCode::Gone,
            ErrorCode::Internal,
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), json!(code.as_str()));
        }
    }

    #[test]
    fn incoming_is_classified() {
        let ok = Incoming::from_value(response(&json!(3), Ok(json!({"a":1})))).unwrap();
        assert_eq!(
            ok,
            Incoming::Response {
                id: json!(3),
                result: Ok(json!({"a":1}))
            }
        );
        let err = response(&json!(4), Err(RpcError::new(ErrorCode::Gone, "bye")));
        match Incoming::from_value(err).unwrap() {
            Incoming::Response { id, result: Err(e) } => {
                assert_eq!(id, json!(4));
                assert_eq!(e.code, ErrorCode::Gone);
            }
            other => panic!("{other:?}"),
        }
        let ev = Incoming::from_value(event("exit", json!({"status":0}))).unwrap();
        assert_eq!(
            ev,
            Incoming::Event(Event {
                event: "exit".into(),
                data: json!({"status":0})
            })
        );
        assert!(Incoming::from_value(json!([1])).is_none());
        assert!(Incoming::from_value(json!({"id":1})).is_none());
    }
}
