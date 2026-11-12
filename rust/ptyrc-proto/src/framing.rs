// SPDX-License-Identifier: GPL-3.0-or-later
//! Framing: one JSON object per line.

use serde_json::Value;
use std::fmt;

/// Longest accepted line, newline excluded.
pub const MAX_LINE: usize = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A line exceeds `MAX_LINE`. The stream cannot be resynchronised.
    TooLong,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("line too long")
    }
}

impl std::error::Error for FrameError {}

/// Incremental line splitter. Feed what was read, take complete lines.
#[derive(Debug, Default)]
pub struct LineDecoder {
    buf: Vec<u8>,
    /// Bytes of `buf` already searched for a newline.
    scanned: usize,
}

impl LineDecoder {
    pub fn new() -> LineDecoder {
        LineDecoder::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Next complete line, without its terminator (`\n` or `\r\n`). Empty
    /// lines are skipped.
    pub fn next_line(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        loop {
            match self.buf[self.scanned..].iter().position(|&b| b == b'\n') {
                Some(offset) => {
                    let end = self.scanned + offset;
                    if end > MAX_LINE {
                        return Err(FrameError::TooLong);
                    }
                    let mut line: Vec<u8> = self.buf.drain(..=end).collect();
                    self.scanned = 0;
                    line.pop();
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if line.is_empty() {
                        continue;
                    }
                    return Ok(Some(line));
                }
                None => {
                    self.scanned = self.buf.len();
                    if self.scanned > MAX_LINE {
                        return Err(FrameError::TooLong);
                    }
                    return Ok(None);
                }
            }
        }
    }

    /// Bytes held, waiting for a newline.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }
}

/// A value as one line.
pub fn encode(value: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(value).expect("a Value serialises");
    line.push(b'\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_across_partial_reads() {
        let mut d = LineDecoder::new();
        d.push(b"{\"a\"");
        assert_eq!(d.next_line(), Ok(None));
        d.push(b":1}\n{\"b\":2}\r\n\n{\"c\"");
        assert_eq!(d.next_line().unwrap().unwrap(), b"{\"a\":1}");
        assert_eq!(d.next_line().unwrap().unwrap(), b"{\"b\":2}");
        assert_eq!(d.next_line(), Ok(None));
        assert_eq!(d.pending(), 4);
        d.push(b":3}\n");
        assert_eq!(d.next_line().unwrap().unwrap(), b"{\"c\":3}");
        assert_eq!(d.next_line(), Ok(None));
        assert_eq!(d.pending(), 0);
    }

    #[test]
    fn byte_by_byte() {
        let mut d = LineDecoder::new();
        let mut lines = Vec::new();
        for b in b"ab\ncd\n" {
            d.push(&[*b]);
            while let Some(line) = d.next_line().unwrap() {
                lines.push(line);
            }
        }
        assert_eq!(lines, vec![b"ab".to_vec(), b"cd".to_vec()]);
    }

    #[test]
    fn refuses_long_lines() {
        let mut d = LineDecoder::new();
        let chunk = vec![b'x'; 1 << 20];
        for _ in 0..16 {
            d.push(&chunk);
            assert_eq!(d.next_line(), Ok(None));
        }
        d.push(b"x");
        assert_eq!(d.next_line(), Err(FrameError::TooLong));
    }

    #[test]
    fn encode_is_one_line() {
        let line = encode(&serde_json::json!({"t": "a\nb"}));
        assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
        assert_eq!(line.last(), Some(&b'\n'));
    }
}
