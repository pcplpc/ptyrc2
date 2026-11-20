// SPDX-License-Identifier: GPL-3.0-or-later
//! JSON: [`Value`], [`parse`], `to_string()`, and [`json!`](crate::json!).
//!
//! A number is an `f64`: an integer is exact up to 2^53, and `3.0` is
//! the integer 3. An object keeps its keys sorted. Indexing never
//! panics: `v["a"][0]` is `Null` where nothing is.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::ops::{Index, IndexMut};

/// The members of an object.
pub type Map = BTreeMap<String, Value>;

/// A JSON value.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Map),
}

static NULL: Value = Value::Null;

impl Value {
    /// The member `key` of an object, or the element `key` of an array.
    pub fn get<K: Key>(&self, key: K) -> Option<&Value> {
        key.find(self)
    }

    pub fn get_mut<K: Key>(&mut self, key: K) -> Option<&mut Value> {
        key.find_mut(self)
    }

    /// RFC 6901: `"/size/0"`; `""` is the value itself.
    pub fn pointer(&self, pointer: &str) -> Option<&Value> {
        if pointer.is_empty() {
            return Some(self);
        }
        let mut v = self;
        for part in pointer.strip_prefix('/')?.split('/') {
            let part = part.replace("~1", "/").replace("~0", "~");
            v = match v {
                Value::Object(m) => m.get(&part)?,
                Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(v)
    }

    /// The value, `Null` left in its place.
    pub fn take(&mut self) -> Value {
        std::mem::take(self)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// An integral number in range.
    pub fn as_i64(&self) -> Option<i64> {
        let n = self.as_f64()?;
        (n.fract() == 0.0 && (-9.223_372_036_854_776e18..9.223_372_036_854_776e18).contains(&n))
            .then_some(n as i64)
    }

    /// An integral number in range.
    pub fn as_u64(&self) -> Option<u64> {
        let n = self.as_f64()?;
        (n.fract() == 0.0 && (0.0..1.844_674_407_370_955_2e19).contains(&n)).then_some(n as u64)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Map> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for usize {}
    impl Sealed for &str {}
    impl Sealed for &String {}
}

/// What indexes a [`Value`]: a `&str` (an object) or a `usize` (an
/// array).
pub trait Key: sealed::Sealed {
    #[doc(hidden)]
    fn find(self, v: &Value) -> Option<&Value>;
    #[doc(hidden)]
    fn find_mut(self, v: &mut Value) -> Option<&mut Value>;
    #[doc(hidden)]
    fn place(self, v: &mut Value) -> &mut Value;
}

impl Key for &str {
    fn find(self, v: &Value) -> Option<&Value> {
        v.as_object()?.get(self)
    }
    fn find_mut(self, v: &mut Value) -> Option<&mut Value> {
        v.as_object_mut()?.get_mut(self)
    }
    /// `Null` becomes an object; the member is made `Null` if absent.
    fn place(self, v: &mut Value) -> &mut Value {
        if v.is_null() {
            *v = Value::Object(Map::new());
        }
        match v {
            Value::Object(m) => m.entry(self.to_string()).or_default(),
            _ => panic!("json: [{self:?}] of a value that is not an object"),
        }
    }
}

impl Key for &String {
    fn find(self, v: &Value) -> Option<&Value> {
        Key::find(self.as_str(), v)
    }
    fn find_mut(self, v: &mut Value) -> Option<&mut Value> {
        Key::find_mut(self.as_str(), v)
    }
    fn place(self, v: &mut Value) -> &mut Value {
        Key::place(self.as_str(), v)
    }
}

impl Key for usize {
    fn find(self, v: &Value) -> Option<&Value> {
        v.as_array()?.get(self)
    }
    fn find_mut(self, v: &mut Value) -> Option<&mut Value> {
        v.as_array_mut()?.get_mut(self)
    }
    fn place(self, v: &mut Value) -> &mut Value {
        match v {
            Value::Array(a) if self < a.len() => &mut a[self],
            _ => panic!("json: [{self}] past the end, or not an array"),
        }
    }
}

/// `Null` where nothing is.
impl<K: Key> Index<K> for Value {
    type Output = Value;
    fn index(&self, key: K) -> &Value {
        key.find(self).unwrap_or(&NULL)
    }
}

/// `v["k"] = ...`: `Null` becomes an object. Panics on another kind,
/// or an index past the end of an array.
impl<K: Key> IndexMut<K> for Value {
    fn index_mut(&mut self, key: K) -> &mut Value {
        key.place(self)
    }
}

impl PartialEq<str> for Value {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == Some(other)
    }
}

impl PartialEq<&str> for Value {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == Some(*other)
    }
}

impl PartialEq<String> for Value {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == Some(other.as_str())
    }
}

impl PartialEq<bool> for Value {
    fn eq(&self, other: &bool) -> bool {
        self.as_bool() == Some(*other)
    }
}

macro_rules! number {
    ($($t:ty),*) => {$(
        impl From<$t> for Value {
            fn from(n: $t) -> Value {
                Value::Number(n as f64)
            }
        }
        impl PartialEq<$t> for Value {
            fn eq(&self, other: &$t) -> bool {
                self.as_f64() == Some(*other as f64)
            }
        }
    )*};
}

number!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<char> for Value {
    fn from(c: char) -> Value {
        Value::String(c.to_string())
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::String(s)
    }
}

impl From<Map> for Value {
    fn from(m: Map) -> Value {
        Value::Object(m)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Value {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

impl<T: Clone + Into<Value>> From<&[T]> for Value {
    fn from(v: &[T]) -> Value {
        Value::Array(v.iter().cloned().map(Into::into).collect())
    }
}

impl<T: Into<Value>, const N: usize> From<[T; N]> for Value {
    fn from(v: [T; N]) -> Value {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

/// `None`: `Null`.
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Value {
        v.map_or(Value::Null, Into::into)
    }
}

/// A copy: `&String`, `&Value`, `&Vec<_>`, `&&str`, ...
impl<T: Clone + Into<Value>> From<&T> for Value {
    fn from(v: &T) -> Value {
        v.clone().into()
    }
}

impl<T: Into<Value>> FromIterator<T> for Value {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Value {
        Value::Array(iter.into_iter().map(Into::into).collect())
    }
}

fn write_str(f: &mut impl fmt::Write, s: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            c if (c as u32) < 0x20 || c == '\u{7f}' => write!(f, "\\u{:04x}", c as u32)?,
            c => f.write_char(c)?,
        }
    }
    f.write_char('"')
}

/// Compact JSON; a number not finite is `null`.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) if !n.is_finite() => f.write_str("null"),
            Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e17 => write!(f, "{}", *n as i64),
            Value::Number(n) => write!(f, "{n}"),
            Value::String(s) => write_str(f, s),
            Value::Array(a) => {
                f.write_char('[')?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        f.write_char(',')?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_char(']')
            }
            Value::Object(m) => {
                f.write_char('{')?;
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        f.write_char(',')?;
                    }
                    write_str(f, k)?;
                    write!(f, ":{v}")?;
                }
                f.write_char('}')
            }
        }
    }
}

/// Text that is not JSON: what, at which byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub at: usize,
    pub message: &'static str,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON: {} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for Error {}

/// Nesting refused past this depth.
pub const MAX_DEPTH: usize = 128;

/// One JSON value, white space around it.
pub fn parse(text: &str) -> Result<Value, Error> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let v = p.value(0)?;
    p.space();
    if p.i < p.s.len() {
        return Err(p.err("text after the value"));
    }
    Ok(v)
}

/// [`parse`] of UTF-8 bytes.
pub fn parse_bytes(bytes: &[u8]) -> Result<Value, Error> {
    match std::str::from_utf8(bytes) {
        Ok(text) => parse(text),
        Err(e) => Err(Error {
            at: e.valid_up_to(),
            message: "not UTF-8",
        }),
    }
}

impl std::str::FromStr for Value {
    type Err = Error;
    fn from_str(s: &str) -> Result<Value, Error> {
        parse(s)
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, message: &'static str) -> Error {
        Error {
            at: self.i,
            message,
        }
    }

    fn space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.s.get(self.i) {
            self.i += 1;
        }
    }

    fn eat(&mut self, word: &[u8], v: Value) -> Result<Value, Error> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("not a value"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        if depth > MAX_DEPTH {
            return Err(self.err("nested too deep"));
        }
        self.space();
        match self.s.get(self.i) {
            None => Err(self.err("the end, a value expected")),
            Some(b'n') => self.eat(b"null", Value::Null),
            Some(b't') => self.eat(b"true", Value::Bool(true)),
            Some(b'f') => self.eat(b"false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.space();
                if self.s.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Value::Array(a));
                }
                loop {
                    a.push(self.value(depth + 1)?);
                    self.space();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Value::Array(a));
                        }
                        _ => return Err(self.err("`,` or `]` expected")),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut m = Map::new();
                self.space();
                if self.s.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Value::Object(m));
                }
                loop {
                    self.space();
                    if self.s.get(self.i) != Some(&b'"') {
                        return Err(self.err("a key expected"));
                    }
                    let k = self.string()?;
                    self.space();
                    if self.s.get(self.i) != Some(&b':') {
                        return Err(self.err("`:` expected"));
                    }
                    self.i += 1;
                    let v = self.value(depth + 1)?;
                    m.insert(k, v);
                    self.space();
                    match self.s.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Value::Object(m));
                        }
                        _ => return Err(self.err("`,` or `}` expected")),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("not a value")),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.i;
        if self.s[self.i] == b'-' {
            self.i += 1;
        }
        match self.s.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.err("a digit expected")),
        }
        if self.s.get(self.i) == Some(&b'.') {
            self.i += 1;
            if self.digits() == 0 {
                return Err(self.err("a digit expected"));
            }
        }
        if let Some(b'e' | b'E') = self.s.get(self.i) {
            self.i += 1;
            if let Some(b'+' | b'-') = self.s.get(self.i) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return Err(self.err("a digit expected"));
            }
        }
        // ASCII only from `start` to `i`.
        let text = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("x");
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| self.err("not a number"))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let digits = self
            .s
            .get(self.i..self.i + 4)
            .ok_or(self.err("\\u: 4 digits"))?;
        let text = std::str::from_utf8(digits).map_err(|_| self.err("\\u: 4 digits"))?;
        let n = u32::from_str_radix(text, 16).map_err(|_| self.err("\\u: 4 digits"))?;
        self.i += 4;
        Ok(n)
    }

    fn string(&mut self) -> Result<String, Error> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let run = self.i;
            while let Some(&b) = self.s.get(self.i) {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.i += 1;
            }
            // Cut at ASCII bytes of a `str`: a boundary.
            out.push_str(std::str::from_utf8(&self.s[run..self.i]).unwrap_or(""));
            match self.s.get(self.i) {
                None => return Err(self.err("a string without its end")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {}
                Some(_) => return Err(self.err("a control character in a string")),
            }
            self.i += 1;
            let c = match self.s.get(self.i) {
                Some(b'"') => '"',
                Some(b'\\') => '\\',
                Some(b'/') => '/',
                Some(b'b') => '\u{8}',
                Some(b'f') => '\u{c}',
                Some(b'n') => '\n',
                Some(b'r') => '\r',
                Some(b't') => '\t',
                Some(b'u') => {
                    self.i += 1;
                    let hi = self.hex4()?;
                    let code = if (0xd800..0xdc00).contains(&hi) {
                        if !self.s[self.i..].starts_with(b"\\u") {
                            return Err(self.err("a lone surrogate"));
                        }
                        self.i += 2;
                        let lo = self.hex4()?;
                        if !(0xdc00..0xe000).contains(&lo) {
                            return Err(self.err("a lone surrogate"));
                        }
                        0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                    } else {
                        hi
                    };
                    out.push(char::from_u32(code).ok_or(self.err("a lone surrogate"))?);
                    continue;
                }
                _ => return Err(self.err("an unknown escape")),
            };
            out.push(c);
            self.i += 1;
        }
    }
}

/// A [`Value`] from JSON-like syntax; a value that is not a literal,
/// an array or an object is any Rust expression of a type with
/// `Into<Value>` (numbers, `bool`, `&str`, `String`, `Vec`, slices,
/// `Option`, `Value`, references to these). Keys are string literals,
/// or `(expr)` for one computed.
///
/// ```ignore
/// let v = json!({"anchor": {"at": [x - 1, 0], "space": "view"}, "z": 100, "fg": null});
/// ```
#[macro_export]
macro_rules! json {
    (null) => {
        $crate::json::Value::Null
    };
    ([ $($tt:tt)* ]) => {
        $crate::json::Value::Array($crate::__json_array!([] $($tt)*))
    };
    ({ $($tt:tt)* }) => {{
        #[allow(unused_mut)]
        let mut object = $crate::json::Map::new();
        $crate::__json_object!(object $($tt)*);
        $crate::json::Value::Object(object)
    }};
    ($other:expr) => {
        $crate::json::Value::from($other)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __json_array {
    ([$($done:expr,)*]) => {
        ::std::vec![$($done,)*]
    };
    ([$($done:expr,)*] null $(, $($rest:tt)*)?) => {
        $crate::__json_array!([$($done,)* $crate::json::Value::Null,] $($($rest)*)?)
    };
    ([$($done:expr,)*] [$($a:tt)*] $(, $($rest:tt)*)?) => {
        $crate::__json_array!([$($done,)* $crate::json!([$($a)*]),] $($($rest)*)?)
    };
    ([$($done:expr,)*] {$($o:tt)*} $(, $($rest:tt)*)?) => {
        $crate::__json_array!([$($done,)* $crate::json!({$($o)*}),] $($($rest)*)?)
    };
    ([$($done:expr,)*] $next:expr, $($rest:tt)*) => {
        $crate::__json_array!([$($done,)* $crate::json::Value::from($next),] $($rest)*)
    };
    ([$($done:expr,)*] $last:expr) => {
        $crate::__json_array!([$($done,)* $crate::json::Value::from($last),])
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __json_object {
    ($m:ident) => {};
    ($m:ident $k:tt : null $(, $($rest:tt)*)?) => {
        $m.insert(::std::string::String::from($k), $crate::json::Value::Null);
        $crate::__json_object!($m $($($rest)*)?);
    };
    ($m:ident $k:tt : [$($a:tt)*] $(, $($rest:tt)*)?) => {
        $m.insert(::std::string::String::from($k), $crate::json!([$($a)*]));
        $crate::__json_object!($m $($($rest)*)?);
    };
    ($m:ident $k:tt : {$($o:tt)*} $(, $($rest:tt)*)?) => {
        $m.insert(::std::string::String::from($k), $crate::json!({$($o)*}));
        $crate::__json_object!($m $($($rest)*)?);
    };
    ($m:ident $k:tt : $v:expr, $($rest:tt)*) => {
        $m.insert(::std::string::String::from($k), $crate::json::Value::from($v));
        $crate::__json_object!($m $($rest)*);
    };
    ($m:ident $k:tt : $v:expr) => {
        $m.insert(::std::string::String::from($k), $crate::json::Value::from($v));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_write_again() {
        let text = r#"{"a":[1,-2.5,1e3,true,false,null],"b":"x\"\\\n\u00e9\ud83d\ude00","c":{}}"#;
        let v = parse(text).unwrap();
        assert_eq!(v["a"][2], 1000);
        assert_eq!(v["b"], "x\"\\\né😀");
        assert_eq!(
            v.to_string(),
            r#"{"a":[1,-2.5,1000,true,false,null],"b":"x\"\\\né😀","c":{}}"#
        );
        assert_eq!(parse(&v.to_string()).unwrap(), v);
        assert_eq!(parse(" [ ] ").unwrap(), json!([]));
    }

    #[test]
    fn refused() {
        for bad in [
            "",
            "nul",
            "[1,]",
            "{\"a\" 1}",
            "01",
            "1.",
            "-",
            "\"\\x\"",
            "\"\u{1}\"",
            "[1] 2",
            "\"\\ud800\"",
            "{1:2}",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(MAX_DEPTH + 2);
        assert_eq!(parse(&deep).unwrap_err().message, "nested too deep");
        assert_eq!(parse_bytes(b"\"\xff\"").unwrap_err().message, "not UTF-8");
    }

    #[test]
    fn index_get_pointer() {
        let v = json!({"size": [80, 24], "name": "x", "f": 0.5, "n": -3});
        assert_eq!(v["size"][1].as_u64(), Some(24));
        assert_eq!(v.pointer("/size/0").and_then(Value::as_u64), Some(80));
        assert_eq!(v.get("name").and_then(Value::as_str), Some("x"));
        assert!(v["none"]["deeper"][3].is_null());
        assert_eq!(v["f"].as_u64(), None);
        assert_eq!(v["n"].as_i64(), Some(-3));
        assert_eq!(v["n"].as_u64(), None);
        assert!(v["name"] == "x" && v["size"][0] == 80);
        let mut w = Value::Null;
        w["granted"] = json!(["shape"]);
        w["granted"][0] = json!("words");
        assert_eq!(w.to_string(), r#"{"granted":["words"]}"#);
    }

    #[test]
    fn the_macro() {
        let (x, y, layer, rows) = (3i32, 4u32, "back", ["ab", "c"]);
        let keys: Vec<&str> = vec!["up", "down"];
        let style = json!({"fg": 11});
        let name = String::from("k");
        let v = json!({
            "anchor": {"at": [x - 1, y * 2], "space": "view"},
            "layer": layer, "z": 100, "none": null, "rows": &rows[..], "keys": keys,
            "style": style, "nested": [[1, {"a": true}], null, -1.5,],
            (name.as_str()): Some(1), "o": Option::<u8>::None,
        });
        assert_eq!(
            v.to_string(),
            r#"{"anchor":{"at":[2,8],"space":"view"},"k":1,"keys":["up","down"],"layer":"back","nested":[[1,{"a":true}],null,-1.5],"none":null,"o":null,"rows":["ab","c"],"style":{"fg":11},"z":100}"#
        );
        assert_eq!(json!(null), Value::Null);
        assert_eq!(json!({}), Value::Object(Map::new()));
        assert_eq!(json!(f64::NAN).to_string(), "null");
        assert_eq!(json!("\u{1}").to_string(), r#""\u0001""#);
    }
}
