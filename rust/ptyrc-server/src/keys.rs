// SPDX-License-Identifier: GPL-3.0-or-later
//! Key names to bytes, and base64.

/// Bytes of a key of `input.send`. A string that names no key is sent as
/// text. `app_cursor`: the app is in application cursor-key mode.
pub fn encode_key(key: &str, app_cursor: bool) -> Vec<u8> {
    match named(key, app_cursor) {
        Some(bytes) => bytes,
        None => key.as_bytes().to_vec(),
    }
}

fn named(key: &str, app_cursor: bool) -> Option<Vec<u8>> {
    let cursor = |c: u8| -> Vec<u8> { vec![0x1b, if app_cursor { b'O' } else { b'[' }, c] };
    let tilde = |n: u8| -> Vec<u8> { format!("\x1b[{n}~").into_bytes() };
    Some(match key {
        "enter" => b"\r".to_vec(),
        "tab" => b"\t".to_vec(),
        "esc" => b"\x1b".to_vec(),
        "space" => b" ".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "delete" => tilde(3),
        "up" => cursor(b'A'),
        "down" => cursor(b'B'),
        "right" => cursor(b'C'),
        "left" => cursor(b'D'),
        "home" => cursor(b'H'),
        "end" => cursor(b'F'),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "shift+tab" => b"\x1b[Z".to_vec(),
        "f1" => b"\x1bOP".to_vec(),
        "f2" => b"\x1bOQ".to_vec(),
        "f3" => b"\x1bOR".to_vec(),
        "f4" => b"\x1bOS".to_vec(),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        _ => {
            if let Some(rest) = key.strip_prefix("ctrl+") {
                return ctrl(rest).map(|b| vec![b]);
            }
            if let Some(rest) = key.strip_prefix("alt+") {
                if rest.is_empty() {
                    return None;
                }
                let mut bytes = vec![0x1b];
                bytes.extend(encode_key(rest, app_cursor));
                return Some(bytes);
            }
            return None;
        }
    })
}

fn ctrl(rest: &str) -> Option<u8> {
    if rest == "space" {
        return Some(0);
    }
    let mut chars = rest.chars();
    let (c, None) = (chars.next()?, chars.next()) else {
        return None;
    };
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | '[' | '\\' | ']' | '^' | '_' => Some(c as u8 & 0x1f),
        '?' => Some(0x7f),
        _ => None,
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64, padded.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Standard or URL-safe base64, padding optional. `None`: not base64.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u8);
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    if text.len() % 4 == 1 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(encode_key("enter", false), b"\r");
        assert_eq!(encode_key("up", false), b"\x1b[A");
        assert_eq!(encode_key("up", true), b"\x1bOA");
        assert_eq!(encode_key("end", true), b"\x1bOF");
        assert_eq!(encode_key("ctrl+c", false), b"\x03");
        assert_eq!(encode_key("ctrl+[", false), b"\x1b");
        assert_eq!(encode_key("ctrl+space", false), b"\0");
        assert_eq!(encode_key("alt+x", false), b"\x1bx");
        assert_eq!(encode_key("alt+enter", false), b"\x1b\r");
        assert_eq!(encode_key("shift+tab", false), b"\x1b[Z");
        assert_eq!(encode_key("f5", false), b"\x1b[15~");
        assert_eq!(encode_key("f12", false), b"\x1b[24~");
        assert_eq!(encode_key("pagedown", false), b"\x1b[6~");
        // Not a key: text.
        assert_eq!(encode_key(":wq", false), b":wq");
        assert_eq!(encode_key("ctrl+xx", false), b"ctrl+xx");
        assert_eq!(encode_key("Enter", false), b"Enter");
    }

    #[test]
    fn base64() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"\x00\xff\x1b[A"] {
            let enc = base64_encode(data);
            assert_eq!(base64_decode(&enc).unwrap(), data);
            assert_eq!(base64_decode(enc.trim_end_matches('=')).unwrap(), data);
        }
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert!(base64_decode("a b").is_none());
        assert!(base64_decode("A").is_none());
    }
}
